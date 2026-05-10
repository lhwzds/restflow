use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Daemon,
    NewChat,
    Quit,
    Start,
    Stop,
    Help,
    ListSessions,
    ListSkills,
    ListModels,
    ListModelsForProvider { provider: String },
    ListRuns,
    SwitchModel { model: String },
    OpenRun { run_id: String },
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
        command: "/new",
        args: "",
        description: "Start a new chat",
    },
    SlashCommandSpec {
        command: "/quit",
        args: "",
        description: "Exit RestFlow",
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
        command: "/skill",
        args: "",
        description: "View skills",
    },
    SlashCommandSpec {
        command: "/model",
        args: "",
        description: "Switch the current session model",
    },
    SlashCommandSpec {
        command: "/runs",
        args: "",
        description: "Show work, runs, and subagents",
    },
];

pub const HELP_TEXT: &str = "RestFlow terminal shell\n\n\
Use /daemon when the daemon is offline.\n\
\
Enter sends the current draft.\n\
Ctrl-J inserts a newline.\n\
Ctrl-P resumes a previous session.\n\
Ctrl-L clears and redraws the screen.\n\
Ctrl-C exits.\n\n\
Slash commands:\n\
/daemon\n\
/new\n\
/quit\n\
/help\n\
/resume\n\
/skill\n\
/model\n\
	/runs";

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
        "/new" | "/clear" => Ok(SlashCommand::NewChat),
        "/quit" | "/exit" => Ok(SlashCommand::Quit),
        "/start" => Ok(SlashCommand::Start),
        "/stop" => Ok(SlashCommand::Stop),
        "/help" => Ok(SlashCommand::Help),
        "/resume" | "/session" | "/sessions" => Ok(SlashCommand::ListSessions),
        "/skill" => Ok(SlashCommand::ListSkills),
        "/model" => {
            let first = parts.next().unwrap_or_default();
            if first.is_empty() {
                return Ok(SlashCommand::ListModels);
            }
            let second = parts.next().unwrap_or_default();
            if second.is_empty() {
                return Ok(SlashCommand::ListModelsForProvider {
                    provider: first.to_string(),
                });
            }
            Ok(SlashCommand::SwitchModel {
                model: format!("{first}:{second}"),
            })
        }
        "/runs" => Ok(SlashCommand::ListRuns),
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
        _ => bail!("Unknown command: {command}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{SLASH_COMMAND_SPECS, SlashCommand, parse_slash_command};

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
    fn parses_list_runs_command() {
        assert_eq!(
            parse_slash_command("/runs").expect("parse"),
            SlashCommand::ListRuns
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
    fn parses_model_commands() {
        assert_eq!(
            parse_slash_command("/model").expect("parse"),
            SlashCommand::ListModels
        );
        assert_eq!(
            parse_slash_command("/model gpt-5.4").expect("parse"),
            SlashCommand::ListModelsForProvider {
                provider: "gpt-5.4".to_string(),
            }
        );
        assert_eq!(
            parse_slash_command("/model codex").expect("parse"),
            SlashCommand::ListModelsForProvider {
                provider: "codex".to_string(),
            }
        );
        assert_eq!(
            parse_slash_command("/model codex gpt-5.4").expect("parse"),
            SlashCommand::SwitchModel {
                model: "codex:gpt-5.4".to_string(),
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
    fn parses_quit_aliases() {
        assert_eq!(
            parse_slash_command("/quit").expect("parse"),
            SlashCommand::Quit
        );
        assert_eq!(
            parse_slash_command("/exit").expect("parse"),
            SlashCommand::Quit
        );
    }

    #[test]
    fn rejects_team_as_slash_command() {
        let error = parse_slash_command("/team").expect_err("team is a skill mention");
        assert!(error.to_string().contains("Unknown command: /team"));
    }

    #[test]
    fn parses_new_chat_aliases() {
        assert_eq!(
            parse_slash_command("/new").expect("parse"),
            SlashCommand::NewChat
        );
        assert_eq!(
            parse_slash_command("/clear").expect("parse"),
            SlashCommand::NewChat
        );
    }

    #[test]
    fn command_specs_include_all_supported_entrypoints() {
        let specs = SLASH_COMMAND_SPECS
            .iter()
            .map(|spec| (spec.command, spec.args))
            .collect::<Vec<_>>();

        assert!(specs.contains(&("/daemon", "")));
        assert!(specs.contains(&("/new", "")));
        assert!(specs.contains(&("/quit", "")));
        assert!(!specs.contains(&("/clear", "")));
        assert!(!specs.contains(&("/exit", "")));
        assert!(!specs.contains(&("/start", "")));
        assert!(!specs.contains(&("/stop", "")));
        assert!(specs.contains(&("/help", "")));
        assert!(specs.contains(&("/resume", "")));
        assert!(specs.contains(&("/skill", "")));
        assert!(specs.contains(&("/model", "")));
        assert!(!specs.contains(&("/task", "")));
        assert!(!specs.contains(&("/team", "")));
        assert!(!specs.contains(&("/session", "open <session_id>")));
        assert!(specs.contains(&("/runs", "")));
        assert!(!specs.contains(&("/run", "open <run_id>")));
        assert!(!specs.contains(&("/task", "pause <id>")));
        assert!(!specs.contains(&("/task", "resume <id>")));
        assert!(!specs.contains(&("/task", "stop <id>")));
        assert!(!specs.contains(&("/approve", "<approval_id>")));
        assert!(!specs.contains(&("/reject", "<approval_id> [reason]")));
    }
}
