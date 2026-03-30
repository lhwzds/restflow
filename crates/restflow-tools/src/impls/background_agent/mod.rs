//! Background agent management tool.

mod batch;
mod control;
mod handlers_read;
mod handlers_write;
mod schema;
mod team;
mod types;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolError, ToolOutput};
use restflow_traits::AgentOperationAssessor;
use restflow_traits::store::{
    BackgroundAgentStore, KvStore, MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV,
};
use types::BackgroundAgentAction;

#[derive(Clone)]
pub struct BackgroundAgentTool {
    store: Arc<dyn BackgroundAgentStore>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
    allow_write: bool,
}

impl BackgroundAgentTool {
    pub fn new(store: Arc<dyn BackgroundAgentStore>) -> Self {
        Self {
            store,
            kv_store: None,
            assessor: None,
            allow_write: false,
        }
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    pub fn with_kv_store(mut self, kv_store: Arc<dyn KvStore>) -> Self {
        self.kv_store = Some(kv_store);
        self
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(crate::ToolError::Tool(
                "Write access to background agents is disabled. Available read-only operations: list, progress, list_messages, list_deliverables, list_traces, read_trace, list_teams, get_team. To modify background agents, the user must grant write permissions.".to_string(),
            ))
        }
    }

    fn team_store(&self) -> Result<Arc<dyn KvStore>> {
        self.kv_store.clone().ok_or_else(|| {
            ToolError::Tool(
                "Team storage is unavailable in this runtime. Use 'workers' directly.".to_string(),
            )
        })
    }

    fn assessor(&self) -> Result<Arc<dyn AgentOperationAssessor>> {
        self.assessor.clone().ok_or_else(|| {
            ToolError::Tool(
                "Background-agent capability assessment is unavailable in this runtime."
                    .to_string(),
            )
        })
    }
}

pub fn tool_parameters_schema() -> Value {
    schema::parameters_schema()
}

#[async_trait]
impl Tool for BackgroundAgentTool {
    fn name(&self) -> &str {
        "manage_background_agents"
    }

    fn description(&self) -> &str {
        schema::tool_description()
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: BackgroundAgentAction = match serde_json::from_value(input) {
            Ok(action) => action,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Invalid input: {e}. Supported operations: {}.",
                    MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV
                )));
            }
        };

        match action {
            BackgroundAgentAction::List { status } => handlers_read::execute_list(self, status),
            BackgroundAgentAction::RunBatch {
                agent_id,
                name,
                input,
                inputs,
                workers,
                team,
                save_as_team,
                input_template,
                chat_session_id,
                schedule,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                run_now,
                preview,
                confirmation_token,
            } => {
                batch::execute_run_batch(
                    self,
                    agent_id,
                    name,
                    input,
                    inputs,
                    workers,
                    team,
                    save_as_team,
                    input_template,
                    chat_session_id,
                    schedule,
                    timeout_secs,
                    durability_mode,
                    memory,
                    memory_scope,
                    resource_limits,
                    run_now,
                    preview,
                    confirmation_token,
                )
                .await
            }
            BackgroundAgentAction::SaveTeam {
                team,
                workers,
                preview,
                confirmation_token,
            } => {
                handlers_write::execute_save_team(self, team, workers, preview, confirmation_token)
                    .await
            }
            BackgroundAgentAction::ListTeams => handlers_read::execute_list_teams(self),
            BackgroundAgentAction::GetTeam { team } => handlers_read::execute_get_team(self, team),
            BackgroundAgentAction::DeleteTeam {
                team,
                preview,
                confirmation_token,
            } => {
                handlers_write::execute_delete_team(self, team, preview, confirmation_token).await
            }
            BackgroundAgentAction::Create {
                name,
                agent_id,
                chat_session_id,
                schedule,
                input,
                input_template,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                preview,
                confirmation_token,
            } => {
                handlers_write::execute_create(
                    self,
                    name,
                    agent_id,
                    chat_session_id,
                    schedule,
                    input,
                    input_template,
                    timeout_secs,
                    durability_mode,
                    memory,
                    memory_scope,
                    resource_limits,
                    preview,
                    confirmation_token,
                )
                .await
            }
            BackgroundAgentAction::ConvertSession {
                session_id,
                name,
                schedule,
                input,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                run_now,
                preview,
                confirmation_token,
            } => {
                handlers_write::execute_convert_session(
                    self,
                    session_id,
                    name,
                    schedule,
                    input,
                    timeout_secs,
                    durability_mode,
                    memory,
                    memory_scope,
                    resource_limits,
                    run_now,
                    preview,
                    confirmation_token,
                )
                .await
            }
            BackgroundAgentAction::PromoteToBackground {
                session_id,
                name,
                schedule,
                input,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                run_now,
                preview,
                confirmation_token,
            } => {
                handlers_write::execute_promote_to_background(
                    self,
                    session_id,
                    name,
                    schedule,
                    input,
                    timeout_secs,
                    durability_mode,
                    memory,
                    memory_scope,
                    resource_limits,
                    run_now,
                    preview,
                    confirmation_token,
                )
                .await
            }
            BackgroundAgentAction::Update {
                id,
                name,
                description,
                agent_id,
                chat_session_id,
                input,
                input_template,
                schedule,
                notification,
                execution_mode,
                timeout_secs,
                durability_mode,
                memory,
                memory_scope,
                resource_limits,
                preview,
                confirmation_token,
            } => {
                handlers_write::execute_update(
                    self,
                    id,
                    name,
                    description,
                    agent_id,
                    chat_session_id,
                    input,
                    input_template,
                    schedule,
                    notification,
                    execution_mode,
                    timeout_secs,
                    durability_mode,
                    memory,
                    memory_scope,
                    resource_limits,
                    preview,
                    confirmation_token,
                )
                .await
            }
            BackgroundAgentAction::Delete {
                id,
                preview,
                confirmation_token,
            } => handlers_write::execute_delete(self, id, preview, confirmation_token).await,
            BackgroundAgentAction::Pause { id } => control::execute_pause(self, id).await,
            BackgroundAgentAction::Start { id } => control::execute_start(self, id).await,
            BackgroundAgentAction::Resume { id } => control::execute_resume(self, id).await,
            BackgroundAgentAction::Stop { id } => control::execute_stop(self, id).await,
            BackgroundAgentAction::Run {
                id,
                preview,
                confirmation_token,
            } => control::execute_run(self, id, preview, confirmation_token).await,
            BackgroundAgentAction::Control {
                id,
                action,
                preview,
                confirmation_token,
            } => control::execute_control(self, id, action, preview, confirmation_token).await,
            BackgroundAgentAction::Progress { id, event_limit } => {
                handlers_read::execute_progress(self, id, event_limit)
            }
            BackgroundAgentAction::SendMessage {
                id,
                message,
                source,
            } => handlers_write::execute_send_message(self, id, message, source),
            BackgroundAgentAction::ListMessages { id, limit } => {
                handlers_read::execute_list_messages(self, id, limit)
            }
            BackgroundAgentAction::ListDeliverables { id } => {
                handlers_read::execute_list_deliverables(self, id)
            }
            BackgroundAgentAction::ListTraces { id, limit } => {
                handlers_read::execute_list_traces(self, id, limit)
            }
            BackgroundAgentAction::ReadTrace {
                trace_id,
                line_limit,
            } => handlers_read::execute_read_trace(self, trace_id, line_limit),
        }
    }
}
