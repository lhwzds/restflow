use anyhow::{Result, bail};
use serde_json::json;
use std::sync::Arc;

use crate::cli::TeamCommands;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};

fn emit_runtime_result(
    result: restflow_contracts::ToolExecutionResult,
    format: OutputFormat,
) -> Result<()> {
    if format.is_json() {
        return print_json(&result.result);
    }

    if result.success {
        println!(
            "{}",
            serde_json::to_string_pretty(&result.result)
                .unwrap_or_else(|_| result.result.to_string())
        );
        return Ok(());
    }

    bail!(
        "{}",
        result
            .error
            .unwrap_or_else(|| "spawn_subagent_batch execution failed".to_string())
    )
}

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: TeamCommands,
    format: OutputFormat,
) -> Result<()> {
    let input = match command {
        TeamCommands::Start {
            team,
            member,
            assignment,
            task,
            preview,
            approval_id,
        } => {
            let (task, tasks) = match (task, assignment.as_slice()) {
                (Some(task), _) => (Some(task), None),
                (None, [single]) => (Some(single.clone()), None),
                (None, []) => (None, None),
                (None, _) => (None, Some(assignment)),
            };
            let specs = if member.is_empty() {
                None
            } else {
                Some(
                    member
                        .into_iter()
                        .map(|agent| json!({ "agent": agent }))
                        .collect::<Vec<_>>(),
                )
            };
            json!({
                "operation": "spawn",
                "team": team,
                "specs": specs,
                "tasks": tasks,
                "task": task,
                "preview": preview,
                "approval_id": approval_id,
                "wait": false,
            })
        }
    };

    let result = executor
        .execute_runtime_tool("spawn_subagent_batch", input)
        .await?;
    emit_runtime_result(result, format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::CommandExecutor;
    use async_trait::async_trait;
    use restflow_contracts::{
        CleanupReportResponse, PairingApprovalResponse, PairingOwnerResponse, PairingStateResponse,
        RouteBindingResponse, request::TaskFromSessionRequest,
    };
    use restflow_core::memory::ExportResult;
    use restflow_core::models::{
        AgentNode, ChatSession, ChatSessionSummary, ExecutionTimeline, Hook, ItemQuery,
        MemoryChunk, MemorySearchResult, MemoryStats, RunListQuery, RunSummary, Secret, Skill,
        Task, TaskControlAction, TaskConversionResult, TaskPatch, TaskProgress, TaskSpec, WorkItem,
        WorkItemPatch, WorkItemSpec,
    };
    use restflow_core::storage::SystemConfig;
    use restflow_core::storage::agent::StoredAgent;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingExecutor {
        calls: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl RecordingExecutor {
        fn calls(&self) -> Vec<(String, serde_json::Value)> {
            self.calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    #[async_trait]
    impl CommandExecutor for RecordingExecutor {
        async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
            unreachable!()
        }
        async fn get_agent(&self, _id: &str) -> Result<StoredAgent> {
            unreachable!()
        }
        async fn create_agent(&self, _name: String, _agent: AgentNode) -> Result<StoredAgent> {
            unreachable!()
        }
        async fn update_agent(
            &self,
            _id: &str,
            _name: Option<String>,
            _agent: Option<AgentNode>,
        ) -> Result<StoredAgent> {
            unreachable!()
        }
        async fn delete_agent(&self, _id: &str) -> Result<()> {
            unreachable!()
        }
        async fn list_skills(&self) -> Result<Vec<Skill>> {
            unreachable!()
        }
        async fn get_skill(&self, _id: &str) -> Result<Option<Skill>> {
            unreachable!()
        }
        async fn create_skill(&self, _skill: Skill) -> Result<()> {
            unreachable!()
        }
        async fn update_skill(&self, _id: &str, _skill: Skill) -> Result<()> {
            unreachable!()
        }
        async fn delete_skill(&self, _id: &str) -> Result<()> {
            unreachable!()
        }
        async fn search_memory(
            &self,
            _query: String,
            _agent_id: Option<String>,
            _limit: Option<u32>,
        ) -> Result<MemorySearchResult> {
            unreachable!()
        }
        async fn list_memory(
            &self,
            _agent_id: Option<String>,
            _tag: Option<String>,
        ) -> Result<Vec<MemoryChunk>> {
            unreachable!()
        }
        async fn clear_memory(&self, _agent_id: Option<String>) -> Result<u32> {
            unreachable!()
        }
        async fn get_memory_stats(&self, _agent_id: Option<String>) -> Result<MemoryStats> {
            unreachable!()
        }
        async fn export_memory(&self, _agent_id: Option<String>) -> Result<ExportResult> {
            unreachable!()
        }
        async fn store_memory(
            &self,
            _agent_id: &str,
            _content: &str,
            _tags: Vec<String>,
        ) -> Result<String> {
            unreachable!()
        }
        async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
            unreachable!()
        }
        async fn get_session(&self, _id: &str) -> Result<ChatSession> {
            unreachable!()
        }
        async fn create_session(&self, _agent_id: String, _model: String) -> Result<ChatSession> {
            unreachable!()
        }
        async fn delete_session(&self, _id: &str) -> Result<bool> {
            unreachable!()
        }
        async fn search_sessions(&self, _query: String) -> Result<Vec<ChatSessionSummary>> {
            unreachable!()
        }
        async fn list_notes(&self, _query: ItemQuery) -> Result<Vec<WorkItem>> {
            unreachable!()
        }
        async fn get_note(&self, _id: &str) -> Result<Option<WorkItem>> {
            unreachable!()
        }
        async fn create_note(&self, _spec: WorkItemSpec) -> Result<WorkItem> {
            unreachable!()
        }
        async fn update_note(&self, _id: &str, _patch: WorkItemPatch) -> Result<WorkItem> {
            unreachable!()
        }
        async fn delete_note(&self, _id: &str) -> Result<()> {
            unreachable!()
        }
        async fn list_note_folders(&self) -> Result<Vec<String>> {
            unreachable!()
        }
        async fn list_secrets(&self) -> Result<Vec<Secret>> {
            unreachable!()
        }
        async fn set_secret(
            &self,
            _key: &str,
            _value: &str,
            _description: Option<String>,
        ) -> Result<()> {
            unreachable!()
        }
        async fn create_secret(
            &self,
            _key: &str,
            _value: &str,
            _description: Option<String>,
        ) -> Result<()> {
            unreachable!()
        }
        async fn update_secret(
            &self,
            _key: &str,
            _value: &str,
            _description: Option<String>,
        ) -> Result<()> {
            unreachable!()
        }
        async fn delete_secret(&self, _key: &str) -> Result<()> {
            unreachable!()
        }
        async fn has_secret(&self, _key: &str) -> Result<bool> {
            unreachable!()
        }
        async fn get_config(&self) -> Result<SystemConfig> {
            unreachable!()
        }
        async fn get_global_config(&self) -> Result<SystemConfig> {
            unreachable!()
        }
        async fn set_config(&self, _config: SystemConfig) -> Result<()> {
            unreachable!()
        }
        async fn list_hooks(&self) -> Result<Vec<Hook>> {
            unreachable!()
        }
        async fn create_hook(&self, _hook: Hook) -> Result<Hook> {
            unreachable!()
        }
        async fn update_hook(&self, _id: &str, _hook: Hook) -> Result<Hook> {
            unreachable!()
        }
        async fn delete_hook(&self, _id: &str) -> Result<bool> {
            unreachable!()
        }
        async fn test_hook(&self, _id: &str) -> Result<()> {
            unreachable!()
        }
        async fn list_pairing_state(&self) -> Result<PairingStateResponse> {
            unreachable!()
        }
        async fn approve_pairing(&self, _code: &str) -> Result<PairingApprovalResponse> {
            unreachable!()
        }
        async fn deny_pairing(&self, _code: &str) -> Result<()> {
            unreachable!()
        }
        async fn revoke_paired_peer(&self, _peer_id: &str) -> Result<bool> {
            unreachable!()
        }
        async fn get_pairing_owner(&self) -> Result<PairingOwnerResponse> {
            unreachable!()
        }
        async fn set_pairing_owner(&self, _chat_id: &str) -> Result<PairingOwnerResponse> {
            unreachable!()
        }
        async fn list_route_bindings(&self) -> Result<Vec<RouteBindingResponse>> {
            unreachable!()
        }
        async fn bind_route(
            &self,
            _binding_type: &str,
            _target_id: &str,
            _agent_id: &str,
        ) -> Result<RouteBindingResponse> {
            unreachable!()
        }
        async fn unbind_route(&self, _id: &str) -> Result<bool> {
            unreachable!()
        }
        async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
            unreachable!()
        }
        async fn list_tasks(&self, _status: Option<String>) -> Result<Vec<Task>> {
            unreachable!()
        }
        async fn get_task(&self, _id: &str) -> Result<Task> {
            unreachable!()
        }
        async fn create_task(&self, _spec: TaskSpec) -> Result<Task> {
            unreachable!()
        }
        async fn convert_session_to_task(
            &self,
            _request: TaskFromSessionRequest,
        ) -> Result<TaskConversionResult> {
            unreachable!()
        }
        async fn update_task(&self, _id: &str, _patch: TaskPatch) -> Result<Task> {
            unreachable!()
        }
        async fn delete_task(&self, _id: &str) -> Result<restflow_contracts::DeleteWithIdResponse> {
            unreachable!()
        }
        async fn control_task(&self, _id: &str, _action: TaskControlAction) -> Result<Task> {
            unreachable!()
        }
        async fn get_task_progress(
            &self,
            _id: &str,
            _event_limit: Option<usize>,
        ) -> Result<TaskProgress> {
            unreachable!()
        }
        async fn send_task_message(&self, _id: &str, _message: &str) -> Result<()> {
            unreachable!()
        }
        async fn list_execution_sessions(&self, _query: RunListQuery) -> Result<Vec<RunSummary>> {
            unreachable!()
        }
        async fn get_execution_run_timeline(&self, _run_id: &str) -> Result<ExecutionTimeline> {
            unreachable!()
        }
        async fn execute_runtime_tool(
            &self,
            name: &str,
            input: serde_json::Value,
        ) -> Result<restflow_contracts::ToolExecutionResult> {
            self.calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push((name.to_string(), input));
            Ok(restflow_contracts::ToolExecutionResult {
                success: true,
                result: json!({"ok": true}),
                error: None,
                error_category: None,
                retryable: None,
                retry_after_ms: None,
            })
        }
    }

    #[tokio::test]
    async fn team_start_routes_through_spawn_subagent_batch_tool() {
        let executor = Arc::new(RecordingExecutor::default());
        run(
            executor.clone(),
            TeamCommands::Start {
                team: Some("demo".to_string()),
                member: Vec::new(),
                assignment: vec!["Investigate".to_string()],
                task: None,
                preview: false,
                approval_id: None,
            },
            OutputFormat::Json,
        )
        .await
        .expect("team start should succeed");

        let calls = executor.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "spawn_subagent_batch");
        assert_eq!(calls[0].1["operation"], "spawn");
        assert_eq!(calls[0].1["team"], "demo");
        assert_eq!(calls[0].1["task"], "Investigate");
        assert!(calls[0].1["tasks"].is_null());
    }

    #[tokio::test]
    async fn team_start_routes_multiple_assignments_as_tasks() {
        let executor = Arc::new(RecordingExecutor::default());
        run(
            executor.clone(),
            TeamCommands::Start {
                team: Some("demo".to_string()),
                member: Vec::new(),
                assignment: vec!["First".to_string(), "Second".to_string()],
                task: None,
                preview: true,
                approval_id: Some("approval-1".to_string()),
            },
            OutputFormat::Json,
        )
        .await
        .expect("team start should succeed");

        let calls = executor.calls();
        assert_eq!(calls[0].1["task"], serde_json::Value::Null);
        assert_eq!(calls[0].1["tasks"], json!(["First", "Second"]));
        assert_eq!(calls[0].1["preview"], true);
        assert_eq!(calls[0].1["approval_id"], "approval-1");
    }
}
