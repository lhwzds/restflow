use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskControlAction {
    Pause,
    Resume,
    Stop,
}

impl TaskControlAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Daemon,
    Start,
    Stop,
    Help,
    ListSessions,
    ListTasks,
    ListRuns,
    ListApprovals,
    ListTeams,
    TaskControl {
        action: TaskControlAction,
        task_id: String,
    },
    OpenRun {
        run_id: String,
    },
    TeamState {
        team_run_id: String,
    },
    TeamStart {
        saved_team: String,
    },
    Approve {
        approval_id: String,
    },
    Reject {
        approval_id: String,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashCommandSpec {
    pub command: &'static str,
    pub args: &'static str,
    pub description: &'static str,
}

pub const SLASH_COMMAND_SPECS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        command: "/daemon",
        args: "",
        description: "Control the local daemon",
    },
    SlashCommandSpec {
        command: "/help",
        args: "",
        description: "Show slash command help",
    },
    SlashCommandSpec {
        command: "/resume",
        args: "",
        description: "Resume a previous session",
    },
    SlashCommandSpec {
        command: "/task",
        args: "",
        description: "Select a task action",
    },
    SlashCommandSpec {
        command: "/team",
        args: "",
        description: "Open team actions",
    },
];

pub fn parse_slash_command(raw: &str) -> Result<SlashCommand> {
    let mut parts = raw.split_whitespace();
    let command = parts.next().unwrap_or_default();
    match command {
        "/daemon" => match parts.next().unwrap_or_default() {
            "" => Ok(SlashCommand::Daemon),
            "start" => Ok(SlashCommand::Start),
            "stop" => Ok(SlashCommand::Stop),
            _ => bail!("Usage: /daemon [start|stop]"),
        },
        "/start" => Ok(SlashCommand::Start),
        "/stop" => Ok(SlashCommand::Stop),
        "/help" => Ok(SlashCommand::Help),
        "/resume" | "/session" | "/sessions" => Ok(SlashCommand::ListSessions),
        "/runs" => Ok(SlashCommand::ListRuns),
        "/approvals" => Ok(SlashCommand::ListApprovals),
        "/task" => {
            if parts.clone().next().is_none() {
                return Ok(SlashCommand::ListTasks);
            }
            let action = match parts.next().unwrap_or_default() {
                "pause" => TaskControlAction::Pause,
                "resume" => TaskControlAction::Resume,
                "stop" => TaskControlAction::Stop,
                _ => bail!("Usage: /task pause|resume|stop <id>"),
            };
            let task_id = parts.next().unwrap_or_default();
            if task_id.is_empty() {
                bail!("Usage: /task pause|resume|stop <id>");
            }
            Ok(SlashCommand::TaskControl {
                action,
                task_id: task_id.to_string(),
            })
        }
        "/run" => {
            let action = parts.next().unwrap_or_default();
            let run_id = parts.next().unwrap_or_default();
            if action != "open" || run_id.is_empty() {
                bail!("Usage: /run open <run_id>");
            }
            Ok(SlashCommand::OpenRun {
                run_id: run_id.to_string(),
            })
        }
        "/team" => {
            let action = parts.next().unwrap_or_default();
            if action.is_empty() {
                return Ok(SlashCommand::ListTeams);
            }
            match action {
                "state" => {
                    let team_run_id = parts.next().unwrap_or_default();
                    if team_run_id.is_empty() {
                        bail!("Usage: /team state <team_run_id>");
                    }
                    Ok(SlashCommand::TeamState {
                        team_run_id: team_run_id.to_string(),
                    })
                }
                "start" => {
                    let saved_team = parts.next().unwrap_or_default();
                    if saved_team.is_empty() {
                        bail!("Usage: /team start <saved_team>");
                    }
                    Ok(SlashCommand::TeamStart {
                        saved_team: saved_team.to_string(),
                    })
                }
                _ => bail!("Unsupported /team action"),
            }
        }
        "/approve" => {
            let approval_id = parts.next().unwrap_or_default();
            if approval_id.is_empty() {
                bail!("Usage: /approve <approval_id>");
            }
            Ok(SlashCommand::Approve {
                approval_id: approval_id.to_string(),
            })
        }
        "/reject" => {
            let approval_id = parts.next().unwrap_or_default();
            if approval_id.is_empty() {
                bail!("Usage: /reject <approval_id> [reason]");
            }
            let reason = parts.collect::<Vec<_>>().join(" ");
            Ok(SlashCommand::Reject {
                approval_id: approval_id.to_string(),
                reason: (!reason.is_empty()).then_some(reason),
            })
        }
        _ => bail!("Unknown command: {command}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{SLASH_COMMAND_SPECS, SlashCommand, TaskControlAction, parse_slash_command};

    #[test]
    fn parses_task_control_command() {
        let command = parse_slash_command("/task pause task-1").expect("parse");
        assert_eq!(
            command,
            SlashCommand::TaskControl {
                action: TaskControlAction::Pause,
                task_id: "task-1".to_string(),
            }
        );
    }

    #[test]
    fn parses_all_task_control_actions() {
        assert_eq!(
            parse_slash_command("/task resume task-1").expect("parse"),
            SlashCommand::TaskControl {
                action: TaskControlAction::Resume,
                task_id: "task-1".to_string(),
            }
        );
        assert_eq!(
            parse_slash_command("/task stop task-1").expect("parse"),
            SlashCommand::TaskControl {
                action: TaskControlAction::Stop,
                task_id: "task-1".to_string(),
            }
        );
    }

    #[test]
    fn parses_reject_reason() {
        let command = parse_slash_command("/reject approval-1 not-now").expect("parse");
        assert_eq!(
            command,
            SlashCommand::Reject {
                approval_id: "approval-1".to_string(),
                reason: Some("not-now".to_string()),
            }
        );
    }

    #[test]
    fn rejects_invalid_run_command() {
        let error = parse_slash_command("/run close run-1").expect_err("invalid");
        assert!(error.to_string().contains("Usage: /run open <run_id>"));
    }

    #[test]
    fn parses_run_open_command() {
        assert_eq!(
            parse_slash_command("/run open run-1").expect("parse"),
            SlashCommand::OpenRun {
                run_id: "run-1".to_string(),
            }
        );
    }

    #[test]
    fn parses_list_runs_and_approvals_commands() {
        assert_eq!(
            parse_slash_command("/runs").expect("parse"),
            SlashCommand::ListRuns
        );
        assert_eq!(
            parse_slash_command("/approvals").expect("parse"),
            SlashCommand::ListApprovals
        );
    }

    #[test]
    fn parses_session_listing_commands() {
        assert_eq!(
            parse_slash_command("/sessions").expect("parse"),
            SlashCommand::ListSessions
        );
        assert_eq!(
            parse_slash_command("/session").expect("parse"),
            SlashCommand::ListSessions
        );
    }

    #[test]
    fn bare_team_command_lists_teams() {
        assert_eq!(
            parse_slash_command("/team").expect("parse"),
            SlashCommand::ListTeams
        );
    }

    #[test]
    fn bare_task_command_lists_tasks() {
        assert_eq!(
            parse_slash_command("/task").expect("parse"),
            SlashCommand::ListTasks
        );
    }

    #[test]
    fn parses_team_state_and_start_commands() {
        assert_eq!(
            parse_slash_command("/team state team-run-1").expect("parse"),
            SlashCommand::TeamState {
                team_run_id: "team-run-1".to_string(),
            }
        );
        assert_eq!(
            parse_slash_command("/team start saved-team").expect("parse"),
            SlashCommand::TeamStart {
                saved_team: "saved-team".to_string(),
            }
        );
    }

    #[test]
    fn parses_approve_command() {
        assert_eq!(
            parse_slash_command("/approve approval-1").expect("parse"),
            SlashCommand::Approve {
                approval_id: "approval-1".to_string(),
            }
        );
    }

    #[test]
    fn parses_start_command() {
        assert_eq!(
            parse_slash_command("/start").expect("parse"),
            SlashCommand::Start
        );
        assert_eq!(
            parse_slash_command("/daemon start").expect("parse"),
            SlashCommand::Start
        );
    }

    #[test]
    fn parses_stop_command() {
        assert_eq!(
            parse_slash_command("/stop").expect("parse"),
            SlashCommand::Stop
        );
        assert_eq!(
            parse_slash_command("/daemon stop").expect("parse"),
            SlashCommand::Stop
        );
    }

    #[test]
    fn parses_daemon_menu_command() {
        assert_eq!(
            parse_slash_command("/daemon").expect("parse"),
            SlashCommand::Daemon
        );
    }

    #[test]
    fn parses_help_command() {
        assert_eq!(
            parse_slash_command("/help").expect("parse"),
            SlashCommand::Help
        );
    }

    #[test]
    fn command_specs_include_all_supported_entrypoints() {
        let specs = SLASH_COMMAND_SPECS
            .iter()
            .map(|spec| (spec.command, spec.args))
            .collect::<Vec<_>>();

        assert!(specs.contains(&("/daemon", "")));
        assert!(!specs.contains(&("/start", "")));
        assert!(!specs.contains(&("/stop", "")));
        assert!(specs.contains(&("/help", "")));
        assert!(specs.contains(&("/resume", "")));
        assert!(specs.contains(&("/task", "")));
        assert!(specs.contains(&("/team", "")));
        assert!(!specs.contains(&("/session", "open <session_id>")));
        assert!(!specs.contains(&("/runs", "")));
        assert!(!specs.contains(&("/run", "open <run_id>")));
        assert!(!specs.contains(&("/task", "pause <id>")));
        assert!(!specs.contains(&("/task", "resume <id>")));
        assert!(!specs.contains(&("/task", "stop <id>")));
        assert!(!specs.contains(&("/team", "start <saved_team>")));
        assert!(!specs.contains(&("/approve", "<approval_id>")));
        assert!(!specs.contains(&("/reject", "<approval_id> [reason]")));
    }
}
