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
    Help,
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

pub fn parse_slash_command(raw: &str) -> Result<SlashCommand> {
    let mut parts = raw.split_whitespace();
    let command = parts.next().unwrap_or_default();
    match command {
        "/help" => Ok(SlashCommand::Help),
        "/task" => {
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
    use super::{SlashCommand, TaskControlAction, parse_slash_command};

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
}
