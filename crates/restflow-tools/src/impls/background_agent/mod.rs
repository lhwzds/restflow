//! Task management tool with a legacy background-agent alias.

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
use restflow_traits::store::{
    BackgroundAgentStore, KvStore, MANAGE_BACKGROUND_AGENTS_TOOL_NAME, MANAGE_TASK_OPERATIONS_CSV,
    MANAGE_TASKS_TOOL_NAME, TaskStore,
};
use restflow_traits::{AgentOperationAssessor, normalize_legacy_approval_replay};
use types::TaskAction;

#[derive(Clone)]
pub struct TaskTool {
    store: Arc<dyn BackgroundAgentStore>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
    allow_write: bool,
}

#[derive(Clone)]
/// Legacy compatibility wrapper that preserves the historical
/// `manage_background_agents` tool surface while delegating to `TaskTool`.
pub struct BackgroundAgentTool(TaskTool);

impl TaskTool {
    pub fn new(store: Arc<dyn BackgroundAgentStore>) -> Self {
        Self {
            store,
            kv_store: None,
            assessor: None,
            allow_write: false,
        }
    }

    pub fn from_task_store(store: Arc<dyn TaskStore>) -> Self {
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
                "Write access to tasks is disabled. Available read-only operations: list, progress, list_messages, list_deliverables, list_traces, read_trace, list_teams, get_team. To modify tasks, the user must grant write permissions.".to_string(),
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
                "Task capability assessment is unavailable in this runtime.".to_string(),
            )
        })
    }
}

impl BackgroundAgentTool {
    pub fn new(store: Arc<dyn BackgroundAgentStore>) -> Self {
        Self(TaskTool::new(store))
    }

    pub fn from_task_store(store: Arc<dyn TaskStore>) -> Self {
        Self(TaskTool::from_task_store(store))
    }

    pub fn from_task_tool(tool: TaskTool) -> Self {
        Self(tool)
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.0 = self.0.with_assessor(assessor);
        self
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.0 = self.0.with_write(allow_write);
        self
    }

    pub fn with_kv_store(mut self, kv_store: Arc<dyn KvStore>) -> Self {
        self.0 = self.0.with_kv_store(kv_store);
        self
    }
}

pub fn tool_parameters_schema() -> Value {
    schema::parameters_schema()
}

pub fn tool_description() -> &'static str {
    schema::tool_description()
}

pub fn legacy_tool_description() -> &'static str {
    schema::legacy_tool_description()
}

#[async_trait]
impl Tool for TaskTool {
    fn name(&self) -> &str {
        MANAGE_TASKS_TOOL_NAME
    }

    fn description(&self) -> &str {
        tool_description()
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema()
    }

    async fn execute(&self, mut input: Value) -> Result<ToolOutput> {
        normalize_legacy_approval_replay(&mut input);
        let action: TaskAction = match serde_json::from_value(input) {
            Ok(action) => action,
            Err(e) => {
                return Ok(ToolOutput::error(format!(
                    "Invalid input: {e}. Supported operations: {}.",
                    MANAGE_TASK_OPERATIONS_CSV
                )));
            }
        };

        match action {
            TaskAction::List { status } => handlers_read::execute_list(self, status),
            TaskAction::RunBatch {
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
                approval_id,
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
                    approval_id,
                )
                .await
            }
            TaskAction::SaveTeam {
                team,
                workers,
                preview,
                approval_id,
            } => handlers_write::execute_save_team(self, team, workers, preview, approval_id).await,
            TaskAction::ListTeams => handlers_read::execute_list_teams(self),
            TaskAction::GetTeam { team } => handlers_read::execute_get_team(self, team),
            TaskAction::DeleteTeam {
                team,
                preview,
                approval_id,
            } => handlers_write::execute_delete_team(self, team, preview, approval_id).await,
            TaskAction::Create {
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
                approval_id,
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
                    approval_id,
                )
                .await
            }
            TaskAction::ConvertSession {
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
                approval_id,
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
                    approval_id,
                )
                .await
            }
            TaskAction::PromoteToBackground {
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
                approval_id,
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
                    approval_id,
                )
                .await
            }
            TaskAction::Update {
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
                approval_id,
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
                    approval_id,
                )
                .await
            }
            TaskAction::Delete {
                id,
                preview,
                approval_id,
            } => handlers_write::execute_delete(self, id, preview, approval_id).await,
            TaskAction::Pause { id } => control::execute_pause(self, id).await,
            TaskAction::Start { id } => control::execute_start(self, id).await,
            TaskAction::Resume { id } => control::execute_resume(self, id).await,
            TaskAction::Stop { id } => control::execute_stop(self, id).await,
            TaskAction::Run {
                id,
                preview,
                approval_id,
            } => control::execute_run(self, id, preview, approval_id).await,
            TaskAction::Control {
                id,
                action,
                preview,
                approval_id,
            } => control::execute_control(self, id, action, preview, approval_id).await,
            TaskAction::Progress { id, event_limit } => {
                handlers_read::execute_progress(self, id, event_limit)
            }
            TaskAction::SendMessage {
                id,
                message,
                source,
            } => handlers_write::execute_send_message(self, id, message, source),
            TaskAction::ListMessages { id, limit } => {
                handlers_read::execute_list_messages(self, id, limit)
            }
            TaskAction::ListDeliverables { id } => {
                handlers_read::execute_list_deliverables(self, id)
            }
            TaskAction::ListTraces { id, limit } => {
                handlers_read::execute_list_traces(self, id, limit)
            }
            TaskAction::ReadTrace {
                trace_id,
                line_limit,
            } => handlers_read::execute_read_trace(self, trace_id, line_limit),
        }
    }
}

#[async_trait]
impl Tool for BackgroundAgentTool {
    fn name(&self) -> &str {
        MANAGE_BACKGROUND_AGENTS_TOOL_NAME
    }

    fn description(&self) -> &str {
        legacy_tool_description()
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        self.0.execute(input).await
    }
}
