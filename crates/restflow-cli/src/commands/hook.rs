use anyhow::Result;
use std::sync::Arc;

use crate::cli::HookCommands;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::{Hook, HookAction, HookEvent};

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: HookCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        HookCommands::List => list_hooks(executor, format).await,
        HookCommands::Create {
            name,
            event,
            action,
            url,
            script,
            channel,
            message,
            agent,
            input,
        } => {
            create_hook(
                executor, name, event, action, url, script, channel, message, agent, input, format,
            )
            .await
        }
        HookCommands::Update {
            id,
            name,
            event,
            action,
            url,
            script,
            channel,
            message,
            agent,
            input,
        } => {
            update_hook(
                executor, &id, name, event, action, url, script, channel, message, agent, input,
                format,
            )
            .await
        }
        HookCommands::Delete { id } => delete_hook(executor, &id, format).await,
        HookCommands::Test { id } => test_hook(executor, &id, format).await,
    }
}

async fn list_hooks(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let hooks = executor.list_hooks().await?;

    if format.is_json() {
        return print_json(&hooks);
    }

    if hooks.is_empty() {
        println!("No hooks found");
        return Ok(());
    }

    for hook in hooks {
        println!(
            "{}\t{}\t{}\t{}",
            hook.id,
            hook.name,
            hook.event.as_str(),
            if hook.enabled { "enabled" } else { "disabled" }
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_hook(
    executor: Arc<dyn CommandExecutor>,
    name: String,
    event: String,
    action: String,
    url: Option<String>,
    script: Option<String>,
    channel: Option<String>,
    message: Option<String>,
    agent: Option<String>,
    input: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let event = parse_event(&event)?;
    let action = build_action(action, url, script, channel, message, agent, input)?;

    let hook = executor.create_hook(Hook::new(name, event, action)).await?;

    if format.is_json() {
        return print_json(&hook);
    }

    println!("Hook created: {} ({})", hook.name, hook.id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_hook(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: String,
    event: String,
    action: String,
    url: Option<String>,
    script: Option<String>,
    channel: Option<String>,
    message: Option<String>,
    agent: Option<String>,
    input: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let event = parse_event(&event)?;
    let mut hook = executor
        .list_hooks()
        .await?
        .into_iter()
        .find(|hook| hook.id == id)
        .ok_or_else(|| anyhow::anyhow!("Hook not found: {}", id))?;
    let action = merge_update_action(
        &hook.action,
        build_action(action, url, script, channel, message, agent, input)?,
    );
    hook.name = name;
    hook.event = event;
    hook.action = action;
    hook.touch();

    let hook = executor.update_hook(id, hook).await?;

    if format.is_json() {
        return print_json(&hook);
    }

    println!("Hook updated: {} ({})", hook.name, hook.id);
    Ok(())
}

async fn delete_hook(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let deleted = executor.delete_hook(id).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id, "deleted": deleted }));
    }

    if deleted {
        println!("Hook deleted: {}", id);
    } else {
        println!("Hook not found: {}", id);
    }
    Ok(())
}

async fn test_hook(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.test_hook(id).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id, "tested": true }));
    }

    println!("Hook test executed: {}", id);
    Ok(())
}

fn parse_event(value: &str) -> Result<HookEvent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "task_started" | "started" => Ok(HookEvent::TaskStarted),
        "task_completed" | "completed" => Ok(HookEvent::TaskCompleted),
        "task_failed" | "failed" => Ok(HookEvent::TaskFailed),
        "task_interrupted" | "interrupted" => Ok(HookEvent::TaskInterrupted),
        _ => anyhow::bail!("Unsupported hook event: {}", value),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_action(
    action: String,
    url: Option<String>,
    script: Option<String>,
    channel: Option<String>,
    message: Option<String>,
    agent: Option<String>,
    input: Option<String>,
) -> Result<HookAction> {
    match action.trim().to_ascii_lowercase().as_str() {
        "webhook" => Ok(HookAction::Webhook {
            url: url.ok_or_else(|| anyhow::anyhow!("--url is required for webhook action"))?,
            method: None,
            headers: None,
        }),
        "script" => Ok(HookAction::Script {
            path: script
                .ok_or_else(|| anyhow::anyhow!("--script is required for script action"))?,
            args: None,
            timeout_secs: None,
        }),
        "send_message" | "message" => Ok(HookAction::SendMessage {
            channel_type: channel.unwrap_or_else(|| "telegram".to_string()),
            message_template: message
                .ok_or_else(|| anyhow::anyhow!("--message is required for send_message action"))?,
        }),
        "run_task" => Ok(HookAction::RunTask {
            agent_id: agent.ok_or_else(|| anyhow::anyhow!("--agent is required for run_task"))?,
            input_template: input.unwrap_or_default(),
        }),
        _ => anyhow::bail!("Unsupported hook action: {}", action),
    }
}

fn merge_update_action(existing: &HookAction, updated: HookAction) -> HookAction {
    match (existing, updated) {
        (
            HookAction::Webhook {
                method, headers, ..
            },
            HookAction::Webhook { url, .. },
        ) => HookAction::Webhook {
            url,
            method: method.clone(),
            headers: headers.clone(),
        },
        (
            HookAction::Script {
                args, timeout_secs, ..
            },
            HookAction::Script { path, .. },
        ) => HookAction::Script {
            path,
            args: args.clone(),
            timeout_secs: *timeout_secs,
        },
        (_, updated) => updated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormat;
    use async_trait::async_trait;
    use restflow_contracts::{
        CleanupReportResponse, PairingApprovalResponse, PairingOwnerResponse, PairingStateResponse,
        RouteBindingResponse, SessionSourceMigrationResponse,
    };
    use restflow_core::memory::ExportResult;
    use restflow_core::models::{
        AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentConversionResult,
        BackgroundAgentPatch, BackgroundAgentSpec, BackgroundProgress, ChatSession,
        ChatSessionSummary, Deliverable, ExecutionSessionListQuery, ExecutionSessionSummary,
        ExecutionTimeline, ItemQuery, MemoryChunk, MemorySearchResult, MemoryStats, Secret,
        SharedEntry, Skill, WorkItem, WorkItemPatch, WorkItemSpec,
    };
    use restflow_core::storage::SystemConfig;
    use restflow_core::storage::agent::StoredAgent;
    use restflow_traits::BackgroundAgentCommandOutcome;
    use restflow_traits::store::BackgroundAgentConvertSessionRequest;
    use std::sync::Mutex;

    use crate::executor::CommandExecutor;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum HookCall {
        List,
        Create(Hook),
        Update(String, Hook),
        Delete(String),
        Test(String),
    }

    struct RecordingExecutor {
        calls: Mutex<Vec<HookCall>>,
        hooks: Vec<Hook>,
        created_hook: Hook,
        delete_result: bool,
    }

    impl RecordingExecutor {
        fn new(hooks: Vec<Hook>, created_hook: Hook, delete_result: bool) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                hooks,
                created_hook,
                delete_result,
            }
        }

        fn calls(&self) -> Vec<HookCall> {
            self.calls.lock().expect("lock poisoned").clone()
        }

        fn record(&self, call: HookCall) {
            self.calls.lock().expect("lock poisoned").push(call);
        }
    }

    #[async_trait]
    impl CommandExecutor for RecordingExecutor {
        async fn list_agents(&self) -> anyhow::Result<Vec<StoredAgent>> {
            panic!("unexpected executor call")
        }

        async fn get_agent(&self, _id: &str) -> anyhow::Result<StoredAgent> {
            panic!("unexpected executor call")
        }

        async fn create_agent(
            &self,
            _name: String,
            _agent: AgentNode,
        ) -> anyhow::Result<StoredAgent> {
            panic!("unexpected executor call")
        }

        async fn update_agent(
            &self,
            _id: &str,
            _name: Option<String>,
            _agent: Option<AgentNode>,
        ) -> anyhow::Result<StoredAgent> {
            panic!("unexpected executor call")
        }

        async fn delete_agent(&self, _id: &str) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn list_skills(&self) -> anyhow::Result<Vec<Skill>> {
            panic!("unexpected executor call")
        }

        async fn get_skill(&self, _id: &str) -> anyhow::Result<Option<Skill>> {
            panic!("unexpected executor call")
        }

        async fn create_skill(&self, _skill: Skill) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn update_skill(&self, _id: &str, _skill: Skill) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn delete_skill(&self, _id: &str) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn search_memory(
            &self,
            _query: String,
            _agent_id: Option<String>,
            _limit: Option<u32>,
        ) -> anyhow::Result<MemorySearchResult> {
            panic!("unexpected executor call")
        }

        async fn list_memory(
            &self,
            _agent_id: Option<String>,
            _tag: Option<String>,
        ) -> anyhow::Result<Vec<MemoryChunk>> {
            panic!("unexpected executor call")
        }

        async fn clear_memory(&self, _agent_id: Option<String>) -> anyhow::Result<u32> {
            panic!("unexpected executor call")
        }

        async fn get_memory_stats(&self, _agent_id: Option<String>) -> anyhow::Result<MemoryStats> {
            panic!("unexpected executor call")
        }

        async fn export_memory(&self, _agent_id: Option<String>) -> anyhow::Result<ExportResult> {
            panic!("unexpected executor call")
        }

        async fn store_memory(
            &self,
            _agent_id: &str,
            _content: &str,
            _tags: Vec<String>,
        ) -> anyhow::Result<String> {
            panic!("unexpected executor call")
        }

        async fn list_sessions(&self) -> anyhow::Result<Vec<ChatSessionSummary>> {
            panic!("unexpected executor call")
        }

        async fn get_session(&self, _id: &str) -> anyhow::Result<ChatSession> {
            panic!("unexpected executor call")
        }

        async fn create_session(
            &self,
            _agent_id: String,
            _model: String,
        ) -> anyhow::Result<ChatSession> {
            panic!("unexpected executor call")
        }

        async fn delete_session(&self, _id: &str) -> anyhow::Result<bool> {
            panic!("unexpected executor call")
        }

        async fn search_sessions(&self, _query: String) -> anyhow::Result<Vec<ChatSessionSummary>> {
            panic!("unexpected executor call")
        }

        async fn list_notes(&self, _query: ItemQuery) -> anyhow::Result<Vec<WorkItem>> {
            panic!("unexpected executor call")
        }

        async fn get_note(&self, _id: &str) -> anyhow::Result<Option<WorkItem>> {
            panic!("unexpected executor call")
        }

        async fn create_note(&self, _spec: WorkItemSpec) -> anyhow::Result<WorkItem> {
            panic!("unexpected executor call")
        }

        async fn update_note(&self, _id: &str, _patch: WorkItemPatch) -> anyhow::Result<WorkItem> {
            panic!("unexpected executor call")
        }

        async fn delete_note(&self, _id: &str) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn list_note_folders(&self) -> anyhow::Result<Vec<String>> {
            panic!("unexpected executor call")
        }

        async fn list_secrets(&self) -> anyhow::Result<Vec<Secret>> {
            panic!("unexpected executor call")
        }

        async fn set_secret(
            &self,
            _key: &str,
            _value: &str,
            _description: Option<String>,
        ) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn create_secret(
            &self,
            _key: &str,
            _value: &str,
            _description: Option<String>,
        ) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn update_secret(
            &self,
            _key: &str,
            _value: &str,
            _description: Option<String>,
        ) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn delete_secret(&self, _key: &str) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn has_secret(&self, _key: &str) -> anyhow::Result<bool> {
            panic!("unexpected executor call")
        }

        async fn get_config(&self) -> anyhow::Result<SystemConfig> {
            panic!("unexpected executor call")
        }

        async fn get_global_config(&self) -> anyhow::Result<SystemConfig> {
            panic!("unexpected executor call")
        }

        async fn set_config(&self, _config: SystemConfig) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn list_hooks(&self) -> anyhow::Result<Vec<Hook>> {
            self.record(HookCall::List);
            Ok(self.hooks.clone())
        }

        async fn create_hook(&self, hook: Hook) -> anyhow::Result<Hook> {
            self.record(HookCall::Create(hook));
            Ok(self.created_hook.clone())
        }

        async fn update_hook(&self, id: &str, hook: Hook) -> anyhow::Result<Hook> {
            self.record(HookCall::Update(id.to_string(), hook));
            Ok(self.created_hook.clone())
        }

        async fn delete_hook(&self, id: &str) -> anyhow::Result<bool> {
            self.record(HookCall::Delete(id.to_string()));
            Ok(self.delete_result)
        }

        async fn test_hook(&self, id: &str) -> anyhow::Result<()> {
            self.record(HookCall::Test(id.to_string()));
            Ok(())
        }

        async fn list_pairing_state(&self) -> anyhow::Result<PairingStateResponse> {
            panic!("unexpected executor call")
        }

        async fn approve_pairing(&self, _code: &str) -> anyhow::Result<PairingApprovalResponse> {
            panic!("unexpected executor call")
        }

        async fn deny_pairing(&self, _code: &str) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn revoke_paired_peer(&self, _peer_id: &str) -> anyhow::Result<bool> {
            panic!("unexpected executor call")
        }

        async fn get_pairing_owner(&self) -> anyhow::Result<PairingOwnerResponse> {
            panic!("unexpected executor call")
        }

        async fn set_pairing_owner(&self, _chat_id: &str) -> anyhow::Result<PairingOwnerResponse> {
            panic!("unexpected executor call")
        }

        async fn list_route_bindings(&self) -> anyhow::Result<Vec<RouteBindingResponse>> {
            panic!("unexpected executor call")
        }

        async fn bind_route(
            &self,
            _binding_type: &str,
            _target_id: &str,
            _agent_id: &str,
        ) -> anyhow::Result<RouteBindingResponse> {
            panic!("unexpected executor call")
        }

        async fn unbind_route(&self, _id: &str) -> anyhow::Result<bool> {
            panic!("unexpected executor call")
        }

        async fn run_cleanup(&self) -> anyhow::Result<CleanupReportResponse> {
            panic!("unexpected executor call")
        }

        async fn migrate_session_sources(
            &self,
            _dry_run: bool,
        ) -> anyhow::Result<SessionSourceMigrationResponse> {
            panic!("unexpected executor call")
        }

        async fn list_background_agents(
            &self,
            _status: Option<String>,
        ) -> anyhow::Result<Vec<BackgroundAgent>> {
            panic!("unexpected executor call")
        }

        async fn get_background_agent(&self, _id: &str) -> anyhow::Result<BackgroundAgent> {
            panic!("unexpected executor call")
        }

        async fn create_background_agent(
            &self,
            _spec: BackgroundAgentSpec,
            _preview: bool,
            _confirmation_token: Option<String>,
        ) -> anyhow::Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
            panic!("unexpected executor call")
        }

        async fn convert_session_to_background_agent(
            &self,
            _request: BackgroundAgentConvertSessionRequest,
        ) -> anyhow::Result<BackgroundAgentCommandOutcome<BackgroundAgentConversionResult>>
        {
            panic!("unexpected executor call")
        }

        async fn update_background_agent(
            &self,
            _id: &str,
            _patch: BackgroundAgentPatch,
            _preview: bool,
            _confirmation_token: Option<String>,
        ) -> anyhow::Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
            panic!("unexpected executor call")
        }

        async fn delete_background_agent(
            &self,
            _id: &str,
            _preview: bool,
            _confirmation_token: Option<String>,
        ) -> anyhow::Result<BackgroundAgentCommandOutcome<restflow_contracts::DeleteWithIdResponse>>
        {
            panic!("unexpected executor call")
        }

        async fn control_background_agent(
            &self,
            _id: &str,
            _action: BackgroundAgentControlAction,
            _preview: bool,
            _confirmation_token: Option<String>,
        ) -> anyhow::Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
            panic!("unexpected executor call")
        }

        async fn get_background_agent_progress(
            &self,
            _id: &str,
            _event_limit: Option<usize>,
        ) -> anyhow::Result<BackgroundProgress> {
            panic!("unexpected executor call")
        }

        async fn send_background_agent_message(
            &self,
            _id: &str,
            _message: &str,
        ) -> anyhow::Result<()> {
            panic!("unexpected executor call")
        }

        async fn list_execution_sessions(
            &self,
            _query: ExecutionSessionListQuery,
        ) -> anyhow::Result<Vec<ExecutionSessionSummary>> {
            panic!("unexpected executor call")
        }

        async fn get_execution_run_timeline(
            &self,
            _run_id: &str,
        ) -> anyhow::Result<ExecutionTimeline> {
            panic!("unexpected executor call")
        }

        async fn list_kv_store(
            &self,
            _namespace: Option<&str>,
        ) -> anyhow::Result<Vec<SharedEntry>> {
            panic!("unexpected executor call")
        }

        async fn get_kv_store(&self, _key: &str) -> anyhow::Result<Option<SharedEntry>> {
            panic!("unexpected executor call")
        }

        async fn set_kv_store(
            &self,
            _key: &str,
            _value: &str,
            _visibility: &str,
        ) -> anyhow::Result<SharedEntry> {
            panic!("unexpected executor call")
        }

        async fn delete_kv_store(&self, _key: &str) -> anyhow::Result<bool> {
            panic!("unexpected executor call")
        }

        async fn list_deliverables(&self, _task_id: &str) -> anyhow::Result<Vec<Deliverable>> {
            panic!("unexpected executor call")
        }
    }

    #[test]
    fn test_parse_event() {
        assert!(matches!(
            parse_event("task_completed").expect("parse"),
            HookEvent::TaskCompleted
        ));
        assert!(parse_event("tool_executed").is_err());
        assert!(parse_event("unknown").is_err());
    }

    #[test]
    fn test_build_run_task_action() {
        let action = build_action(
            "run_task".to_string(),
            None,
            None,
            None,
            None,
            Some("agent-1".to_string()),
            Some("input".to_string()),
        )
        .expect("build action");

        match action {
            HookAction::RunTask {
                agent_id,
                input_template,
            } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(input_template, "input");
            }
            _ => panic!("Expected run_task action"),
        }
    }

    #[tokio::test]
    async fn hook_list_dispatches_through_executor() {
        let existing_hook = Hook::new(
            "existing".to_string(),
            HookEvent::TaskCompleted,
            HookAction::SendMessage {
                channel_type: "telegram".to_string(),
                message_template: "done".to_string(),
            },
        );
        let executor = Arc::new(RecordingExecutor::new(
            vec![existing_hook.clone()],
            existing_hook,
            true,
        ));

        run(executor.clone(), HookCommands::List, OutputFormat::Json)
            .await
            .expect("hook list should succeed");

        assert_eq!(executor.calls(), vec![HookCall::List]);
    }

    #[tokio::test]
    async fn hook_create_dispatches_through_executor() {
        let created_hook = Hook::new(
            "notify".to_string(),
            HookEvent::TaskStarted,
            HookAction::Webhook {
                url: "https://example.com/hook".to_string(),
                method: None,
                headers: None,
            },
        );
        let executor = Arc::new(RecordingExecutor::new(
            Vec::new(),
            created_hook.clone(),
            true,
        ));

        run(
            executor.clone(),
            HookCommands::Create {
                name: "notify".to_string(),
                event: "task_started".to_string(),
                action: "webhook".to_string(),
                url: Some("https://example.com/hook".to_string()),
                script: None,
                channel: None,
                message: None,
                agent: None,
                input: None,
            },
            OutputFormat::Json,
        )
        .await
        .expect("hook create should succeed");

        let calls = executor.calls();
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            HookCall::Create(hook) => {
                assert_eq!(hook.name, "notify");
                assert_eq!(hook.event, HookEvent::TaskStarted);
                assert_eq!(
                    hook.action,
                    HookAction::Webhook {
                        url: "https://example.com/hook".to_string(),
                        method: None,
                        headers: None,
                    }
                );
            }
            other => panic!("unexpected call: {other:?}"),
        }
    }

    #[tokio::test]
    async fn hook_delete_dispatches_through_executor() {
        let executor = Arc::new(RecordingExecutor::new(
            Vec::new(),
            Hook::new(
                "unused".to_string(),
                HookEvent::TaskStarted,
                HookAction::RunTask {
                    agent_id: "agent-1".to_string(),
                    input_template: String::new(),
                },
            ),
            true,
        ));

        run(
            executor.clone(),
            HookCommands::Delete {
                id: "hook-123".to_string(),
            },
            OutputFormat::Json,
        )
        .await
        .expect("hook delete should succeed");

        assert_eq!(
            executor.calls(),
            vec![HookCall::Delete("hook-123".to_string())]
        );
    }

    #[tokio::test]
    async fn hook_update_dispatches_through_executor() {
        let mut existing_hook = Hook::new(
            "existing".to_string(),
            HookEvent::TaskCompleted,
            HookAction::SendMessage {
                channel_type: "telegram".to_string(),
                message_template: "done".to_string(),
            },
        );
        existing_hook.id = "hook-456".to_string();
        existing_hook.description = Some("keep-me".to_string());
        existing_hook.filter = Some(restflow_core::models::HookFilter {
            task_name_pattern: Some("deploy-*".to_string()),
            agent_id: Some("agent-1".to_string()),
            success_only: Some(true),
        });
        existing_hook.enabled = false;
        existing_hook.created_at = 123;
        existing_hook.updated_at = 456;

        let mut updated_hook = existing_hook.clone();
        updated_hook.name = "updated".to_string();
        updated_hook.event = HookEvent::TaskFailed;
        updated_hook.action = HookAction::Script {
            path: "/tmp/update-hook.sh".to_string(),
            args: None,
            timeout_secs: None,
        };
        updated_hook.updated_at = 789;
        let executor = Arc::new(RecordingExecutor::new(
            vec![existing_hook.clone()],
            updated_hook.clone(),
            true,
        ));

        run(
            executor.clone(),
            HookCommands::Update {
                id: "hook-456".to_string(),
                name: "updated".to_string(),
                event: "task_failed".to_string(),
                action: "script".to_string(),
                url: None,
                script: Some("/tmp/update-hook.sh".to_string()),
                channel: None,
                message: None,
                agent: None,
                input: None,
            },
            OutputFormat::Json,
        )
        .await
        .expect("hook update should succeed");

        let calls = executor.calls();
        assert_eq!(calls.len(), 2);
        assert!(matches!(calls[0], HookCall::List));
        match &calls[1] {
            HookCall::Update(id, hook) => {
                assert_eq!(id, "hook-456");
                assert_eq!(hook.name, updated_hook.name);
                assert_eq!(hook.event, updated_hook.event);
                assert_eq!(hook.action, updated_hook.action);
                assert_eq!(hook.id, existing_hook.id);
                assert_eq!(hook.description, existing_hook.description);
                assert_eq!(hook.filter, existing_hook.filter);
                assert_eq!(hook.enabled, existing_hook.enabled);
                assert_eq!(hook.created_at, existing_hook.created_at);
                assert!(hook.updated_at >= existing_hook.updated_at);
            }
            other => panic!("unexpected call: {other:?}"),
        }
    }

    #[test]
    fn merge_update_action_preserves_webhook_advanced_fields() {
        let existing = HookAction::Webhook {
            url: "https://example.com/original".to_string(),
            method: Some("PATCH".to_string()),
            headers: Some(std::collections::BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer token".to_string(),
            )])),
        };

        let updated = merge_update_action(
            &existing,
            HookAction::Webhook {
                url: "https://example.com/updated".to_string(),
                method: None,
                headers: None,
            },
        );

        assert_eq!(
            updated,
            HookAction::Webhook {
                url: "https://example.com/updated".to_string(),
                method: Some("PATCH".to_string()),
                headers: Some(std::collections::BTreeMap::from([(
                    "Authorization".to_string(),
                    "Bearer token".to_string(),
                )])),
            }
        );
    }

    #[test]
    fn merge_update_action_preserves_script_advanced_fields() {
        let existing = HookAction::Script {
            path: "/tmp/original.sh".to_string(),
            args: Some(vec!["--flag".to_string(), "value".to_string()]),
            timeout_secs: Some(45),
        };

        let updated = merge_update_action(
            &existing,
            HookAction::Script {
                path: "/tmp/updated.sh".to_string(),
                args: None,
                timeout_secs: None,
            },
        );

        assert_eq!(
            updated,
            HookAction::Script {
                path: "/tmp/updated.sh".to_string(),
                args: Some(vec!["--flag".to_string(), "value".to_string()]),
                timeout_secs: Some(45),
            }
        );
    }

    #[tokio::test]
    async fn hook_update_preserves_existing_webhook_advanced_fields() {
        let mut existing_hook = Hook::new(
            "existing".to_string(),
            HookEvent::TaskCompleted,
            HookAction::Webhook {
                url: "https://example.com/original".to_string(),
                method: Some("PATCH".to_string()),
                headers: Some(std::collections::BTreeMap::from([(
                    "Authorization".to_string(),
                    "Bearer token".to_string(),
                )])),
            },
        );
        existing_hook.id = "hook-advanced".to_string();

        let mut updated_hook = existing_hook.clone();
        updated_hook.name = "updated".to_string();
        updated_hook.event = HookEvent::TaskFailed;
        updated_hook.action = HookAction::Webhook {
            url: "https://example.com/updated".to_string(),
            method: Some("PATCH".to_string()),
            headers: Some(std::collections::BTreeMap::from([(
                "Authorization".to_string(),
                "Bearer token".to_string(),
            )])),
        };

        let executor = Arc::new(RecordingExecutor::new(
            vec![existing_hook.clone()],
            updated_hook,
            true,
        ));

        run(
            executor.clone(),
            HookCommands::Update {
                id: "hook-advanced".to_string(),
                name: "updated".to_string(),
                event: "task_failed".to_string(),
                action: "webhook".to_string(),
                url: Some("https://example.com/updated".to_string()),
                script: None,
                channel: None,
                message: None,
                agent: None,
                input: None,
            },
            OutputFormat::Json,
        )
        .await
        .expect("hook update should preserve advanced webhook fields");

        let calls = executor.calls();
        assert_eq!(calls.len(), 2);
        match &calls[1] {
            HookCall::Update(id, hook) => {
                assert_eq!(id, "hook-advanced");
                assert_eq!(
                    hook.action,
                    HookAction::Webhook {
                        url: "https://example.com/updated".to_string(),
                        method: Some("PATCH".to_string()),
                        headers: Some(std::collections::BTreeMap::from([(
                            "Authorization".to_string(),
                            "Bearer token".to_string(),
                        )])),
                    }
                );
            }
            other => panic!("unexpected call: {other:?}"),
        }
    }

    #[tokio::test]
    async fn hook_test_dispatches_through_executor() {
        let executor = Arc::new(RecordingExecutor::new(
            Vec::new(),
            Hook::new(
                "unused".to_string(),
                HookEvent::TaskStarted,
                HookAction::RunTask {
                    agent_id: "agent-1".to_string(),
                    input_template: String::new(),
                },
            ),
            true,
        ));

        run(
            executor.clone(),
            HookCommands::Test {
                id: "hook-xyz".to_string(),
            },
            OutputFormat::Json,
        )
        .await
        .expect("hook test should succeed");

        assert_eq!(
            executor.calls(),
            vec![HookCall::Test("hook-xyz".to_string())]
        );
    }
}
