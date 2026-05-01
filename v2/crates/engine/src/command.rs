//! # codocia
//!
//! Engine command dispatch maps protocol commands onto engine operations.
//!
//! ## Owns
//! - CoreCommand routing
//! - CoreResponse construction
//! - protocol-level command grouping
//!
//! ## Must Not
//! - own transport encoding
//! - own domain storage formats
//! - duplicate session, task, or tool execution logic
//!
//! ## Inputs
//! - CoreCommand
//!
//! ## Outputs
//! - CoreResponse
//!
//! ## Depends On
//! - proto
//! - engine Core
//!
//! ## Verify
//! - cargo test -p engine

use anyhow::Result;
use proto::{CoreCommand, CoreResponse};

use crate::Core;

impl Core {
    pub async fn handle(&mut self, command: CoreCommand) -> Result<CoreResponse> {
        match command {
            CoreCommand::SaveSkill { skill } => {
                self.save_skill(skill).await?;
                Ok(CoreResponse::Saved)
            }
            CoreCommand::SaveProfile { profile } => {
                self.save_profile(profile).await?;
                Ok(CoreResponse::Saved)
            }
            CoreCommand::SwitchModel { model } => {
                self.set_model(model.clone());
                Ok(CoreResponse::ModelSwitched { model })
            }
            CoreCommand::ChatTurn {
                session_id,
                message,
                assigned_skills,
            } => {
                let request = chat::TurnRequest::new(message).with_assigned_skills(assigned_skills);
                let output = self.chat_turn(&session_id, request).await?;
                Ok(CoreResponse::ChatTurn {
                    events: output.events,
                })
            }
            CoreCommand::StartRun {
                task,
                run_id,
                session_id,
            } => {
                let run = self.start_run(task, run_id, session_id).await?;
                Ok(CoreResponse::RunStarted { run })
            }
            CoreCommand::RunTask {
                run_id,
                task,
                message,
                assigned_skills,
            } => {
                let request =
                    run::TaskRequest::new(task, message).with_assigned_skills(assigned_skills);
                let output = self.run_task(&run_id, request).await?;
                Ok(CoreResponse::RunTask {
                    events: output.events,
                })
            }
            CoreCommand::CallTool { call } => Ok(CoreResponse::ToolEvents {
                events: self.call_tool_events(call).await,
            }),
        }
    }
}
