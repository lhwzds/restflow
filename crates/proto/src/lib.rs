//! # codocia
//!
//! Proto owns the command and response protocol model.
//!
//! ## Owns
//! - CoreCommand
//! - CoreResponse
//! - CoreSnapshot
//! - tagged JSON protocol shape
//!
//! ## Must Not
//! - execute commands
//! - own transport details
//! - own storage
//! - render UI
//!
//! ## Inputs
//! - domain records from model/auth/skill/chat/run/tool/event
//!
//! ## Outputs
//! - protocol-safe command, response, and snapshot values
//!
//! ## Depends On
//! - auth
//! - chat
//! - event
//! - model
//! - run
//! - skill
//! - tool
//!
//! ## Used By
//! - engine
//! - server
//! - Python native bridge
//!
//! ## Verify
//! - cargo check -p proto

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreCommand {
    SaveSkill {
        skill: skill::Skill,
    },
    SaveProfile {
        profile: auth::Profile,
    },
    SwitchModel {
        model: model::Model,
    },
    ChatTurn {
        session_id: String,
        message: String,
        assigned_skills: Vec<String>,
    },
    StartRun {
        task: run::Task,
        run_id: String,
        session_id: String,
    },
    RunTask {
        run_id: String,
        task: run::Task,
        message: String,
        assigned_skills: Vec<String>,
    },
    CallTool {
        call: tool::ToolCall,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreResponse {
    Saved,
    ModelSwitched { model: model::Model },
    ChatTurn { events: Vec<event::Event> },
    RunStarted { run: run::Run },
    RunTask { events: Vec<event::Event> },
    ToolEvents { events: Vec<event::Event> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreSnapshot {
    pub current_model: model::Model,
    pub models: Vec<model::ModelSpec>,
    pub skills: Vec<skill::Skill>,
    pub sessions: Vec<chat::Session>,
    pub tasks: Vec<run::Task>,
    pub runs: Vec<run::Run>,
    pub profiles: Vec<auth::Profile>,
    pub tool_specs: Vec<tool::ToolSpec>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_command_round_trips_through_json() {
        let command = CoreCommand::ChatTurn {
            session_id: "session-1".to_string(),
            message: "hello".to_string(),
            assigned_skills: vec!["team".to_string()],
        };

        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: CoreCommand = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, command);
    }

    #[test]
    fn core_snapshot_round_trips_through_json() {
        let snapshot = CoreSnapshot {
            current_model: model::Model::new("openai", "gpt-5.5"),
            models: Vec::new(),
            skills: Vec::new(),
            sessions: Vec::new(),
            tasks: Vec::new(),
            runs: Vec::new(),
            profiles: Vec::new(),
            tool_specs: Vec::new(),
        };

        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: CoreSnapshot = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, snapshot);
    }
}
