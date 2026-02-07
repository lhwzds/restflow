//! Hook executor implementation.

use crate::channel::{ChannelRouter, ChannelType};
use crate::models::{Hook, HookAction, HookContext, HookFilter, TaskSchedule};
use crate::storage::{AgentTaskStorage, HookStorage};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::warn;

/// Trait for scheduling tasks from hook actions.
#[async_trait]
pub trait HookTaskScheduler: Send + Sync {
    async fn schedule_task(&self, agent_id: &str, input: &str) -> Result<()>;
}

/// Default task scheduler backed by `AgentTaskStorage`.
#[derive(Clone)]
pub struct AgentTaskHookScheduler {
    storage: AgentTaskStorage,
}

impl AgentTaskHookScheduler {
    pub fn new(storage: AgentTaskStorage) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl HookTaskScheduler for AgentTaskHookScheduler {
    async fn schedule_task(&self, agent_id: &str, input: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let task_name = format!("Hook follow-up: {}", agent_id);

        let mut task = self.storage.create_task(
            task_name,
            agent_id.to_string(),
            TaskSchedule::Once { run_at: now },
        )?;

        if !input.trim().is_empty() {
            task.input = Some(input.to_string());
        }
        task.description = Some("Created by hook automation".to_string());
        task.updated_at = now;

        self.storage.update_task(&task)?;
        Ok(())
    }
}

/// Executes hooks against lifecycle events.
#[derive(Clone)]
pub struct HookExecutor {
    static_hooks: Vec<Hook>,
    storage: Option<HookStorage>,
    channel_router: Option<Arc<ChannelRouter>>,
    task_scheduler: Option<Arc<dyn HookTaskScheduler>>,
    http_client: reqwest::Client,
}

impl HookExecutor {
    /// Build an executor with an in-memory hook list.
    pub fn new(hooks: Vec<Hook>) -> Self {
        Self {
            static_hooks: hooks,
            storage: None,
            channel_router: None,
            task_scheduler: None,
            http_client: reqwest::Client::new(),
        }
    }

    /// Build an executor backed by persistent storage.
    pub fn with_storage(storage: HookStorage) -> Self {
        Self {
            static_hooks: Vec::new(),
            storage: Some(storage),
            channel_router: None,
            task_scheduler: None,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn with_channel_router(mut self, router: Arc<ChannelRouter>) -> Self {
        self.channel_router = Some(router);
        self
    }

    pub fn with_task_scheduler(mut self, scheduler: Arc<dyn HookTaskScheduler>) -> Self {
        self.task_scheduler = Some(scheduler);
        self
    }

    /// Fire all matching hooks for an event context.
    pub async fn fire(&self, context: &HookContext) {
        let hooks = match self.load_hooks() {
            Ok(hooks) => hooks,
            Err(error) => {
                warn!(error = %error, "Failed to load hooks");
                return;
            }
        };

        for hook in hooks {
            if !hook.enabled || hook.event != context.event {
                continue;
            }
            if !self.matches_filter(hook.filter.as_ref(), context) {
                continue;
            }

            if let Err(error) = self.execute_hook(&hook, context).await {
                warn!(hook = %hook.name, error = %error, "Hook execution failed");
            }
        }
    }

    /// Execute one specific hook.
    pub async fn execute_hook(&self, hook: &Hook, context: &HookContext) -> Result<()> {
        self.execute_action(&hook.action, context).await
    }

    fn load_hooks(&self) -> Result<Vec<Hook>> {
        if let Some(storage) = &self.storage {
            return storage.list();
        }
        Ok(self.static_hooks.clone())
    }

    fn matches_filter(&self, filter: Option<&HookFilter>, context: &HookContext) -> bool {
        let Some(filter) = filter else {
            return true;
        };

        if let Some(pattern) = filter.task_name_pattern.as_deref()
            && !glob_match::glob_match(pattern, &context.task_name)
        {
            return false;
        }

        if let Some(agent_id) = filter.agent_id.as_deref()
            && agent_id != context.agent_id
        {
            return false;
        }

        if filter.success_only == Some(true) && context.success != Some(true) {
            return false;
        }

        true
    }

    async fn execute_action(&self, action: &HookAction, context: &HookContext) -> Result<()> {
        match action {
            HookAction::Webhook {
                url,
                method,
                headers,
            } => {
                let method =
                    reqwest::Method::from_bytes(method.as_deref().unwrap_or("POST").as_bytes())?;

                let mut request = self.http_client.request(method, url);
                if let Some(headers) = headers {
                    for (key, value) in headers {
                        request = request.header(key, value);
                    }
                }

                request.json(context).send().await?.error_for_status()?;
                Ok(())
            }
            HookAction::Script {
                path,
                args,
                timeout_secs,
            } => {
                let timeout = Duration::from_secs(timeout_secs.unwrap_or(30));
                let mut command = tokio::process::Command::new(path);
                if let Some(args) = args {
                    command.args(args);
                }

                for (key, value) in self.context_env(context) {
                    command.env(key, value);
                }

                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());

                let status = tokio::time::timeout(timeout, command.status()).await??;
                if !status.success() {
                    anyhow::bail!("Script hook failed with status: {}", status);
                }
                Ok(())
            }
            HookAction::SendMessage {
                channel_type,
                message_template,
            } => {
                let Some(router) = &self.channel_router else {
                    warn!("SendMessage hook skipped: no channel router available");
                    return Ok(());
                };

                let channel = parse_channel_type(channel_type)?;
                let content = self.render_template(message_template, context);
                router.send_to_default(channel, &content).await
            }
            HookAction::RunTask {
                agent_id,
                input_template,
            } => {
                let Some(scheduler) = &self.task_scheduler else {
                    warn!("RunTask hook skipped: no task scheduler available");
                    return Ok(());
                };

                let input = self.render_template(input_template, context);
                scheduler.schedule_task(agent_id, &input).await
            }
        }
    }

    fn render_template(&self, template: &str, context: &HookContext) -> String {
        let replacements = [
            ("{{event}}", context.event.as_str()),
            ("{{task_id}}", context.task_id.as_str()),
            ("{{task_name}}", context.task_name.as_str()),
            ("{{agent_id}}", context.agent_id.as_str()),
            (
                "{{success}}",
                if context.success == Some(true) {
                    "true"
                } else {
                    "false"
                },
            ),
            ("{{output}}", context.output.as_deref().unwrap_or("")),
            ("{{error}}", context.error.as_deref().unwrap_or("")),
        ];

        let mut rendered = template.to_string();
        for (key, value) in replacements {
            rendered = rendered.replace(key, value);
        }

        if rendered.contains("{{duration}}") {
            rendered = rendered.replace(
                "{{duration}}",
                &context.duration_ms.unwrap_or_default().to_string(),
            );
        }

        rendered
    }

    fn context_env(&self, context: &HookContext) -> HashMap<&'static str, String> {
        let mut env = HashMap::new();
        env.insert("HOOK_EVENT", context.event.as_str().to_string());
        env.insert("HOOK_TASK_ID", context.task_id.clone());
        env.insert("HOOK_TASK_NAME", context.task_name.clone());
        env.insert("HOOK_AGENT_ID", context.agent_id.clone());
        env.insert(
            "HOOK_DURATION_MS",
            context.duration_ms.unwrap_or_default().to_string(),
        );
        env.insert(
            "HOOK_SUCCESS",
            if context.success == Some(true) {
                "true".to_string()
            } else {
                "false".to_string()
            },
        );

        if let Some(output) = &context.output {
            env.insert("HOOK_OUTPUT", output.clone());
        }
        if let Some(error) = &context.error {
            env.insert("HOOK_ERROR", error.clone());
        }

        env
    }
}

fn parse_channel_type(input: &str) -> Result<ChannelType> {
    match input.trim().to_ascii_lowercase().as_str() {
        "telegram" => Ok(ChannelType::Telegram),
        "discord" => Ok(ChannelType::Discord),
        "slack" => Ok(ChannelType::Slack),
        "email" => Ok(ChannelType::Email),
        "webhook" => Ok(ChannelType::Webhook),
        _ => anyhow::bail!("Unsupported channel type: {}", input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, OutboundMessage};
    use crate::models::HookEvent;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::Mutex;

    struct MockScheduler {
        call_count: AtomicU32,
    }

    impl MockScheduler {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }

        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl HookTaskScheduler for MockScheduler {
        async fn schedule_task(&self, _agent_id: &str, _input: &str) -> Result<()> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct TestChannel {
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
    }

    impl TestChannel {
        fn new(sent: Arc<Mutex<Vec<OutboundMessage>>>) -> Self {
            Self { sent }
        }
    }

    #[async_trait]
    impl Channel for TestChannel {
        fn channel_type(&self) -> ChannelType {
            ChannelType::Telegram
        }

        fn is_configured(&self) -> bool {
            true
        }

        async fn send(&self, message: OutboundMessage) -> Result<()> {
            self.sent.lock().await.push(message);
            Ok(())
        }

        fn start_receiving(
            &self,
        ) -> Option<Pin<Box<dyn futures::Stream<Item = crate::channel::InboundMessage> + Send>>>
        {
            None
        }
    }

    fn sample_context() -> HookContext {
        HookContext {
            event: HookEvent::TaskCompleted,
            task_id: "task-1".to_string(),
            task_name: "daily-report".to_string(),
            agent_id: "agent-1".to_string(),
            success: Some(true),
            output: Some("summary".to_string()),
            error: None,
            duration_ms: Some(1200),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[test]
    fn test_matches_filter_task_name_pattern() {
        let executor = HookExecutor::new(Vec::new());
        let context = sample_context();
        let filter = HookFilter {
            task_name_pattern: Some("daily-*".to_string()),
            agent_id: None,
            success_only: None,
        };

        assert!(executor.matches_filter(Some(&filter), &context));

        let mismatch = HookFilter {
            task_name_pattern: Some("weekly-*".to_string()),
            agent_id: None,
            success_only: None,
        };
        assert!(!executor.matches_filter(Some(&mismatch), &context));
    }

    #[test]
    fn test_render_template() {
        let executor = HookExecutor::new(Vec::new());
        let context = sample_context();

        let rendered = executor.render_template(
            "Task {{task_name}} done in {{duration}}ms with {{output}}",
            &context,
        );

        assert_eq!(rendered, "Task daily-report done in 1200ms with summary");
    }

    #[tokio::test]
    async fn test_run_task_hook_calls_scheduler() {
        let scheduler = Arc::new(MockScheduler::new());
        let executor = HookExecutor::new(Vec::new()).with_task_scheduler(scheduler.clone());

        let hook = Hook {
            id: "hook-1".to_string(),
            name: "run task".to_string(),
            description: None,
            event: HookEvent::TaskCompleted,
            action: HookAction::RunTask {
                agent_id: "agent-next".to_string(),
                input_template: "Input {{output}}".to_string(),
            },
            filter: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };

        executor
            .execute_hook(&hook, &sample_context())
            .await
            .expect("execute run task hook");

        assert_eq!(scheduler.calls(), 1);
    }

    #[tokio::test]
    async fn test_send_message_hook_uses_default_channel() {
        let sent = Arc::new(Mutex::new(Vec::<OutboundMessage>::new()));
        let mut router = ChannelRouter::new();
        router.register_with_default(TestChannel::new(sent.clone()), "chat-1");

        let executor = HookExecutor::new(Vec::new()).with_channel_router(Arc::new(router));

        let hook = Hook {
            id: "hook-1".to_string(),
            name: "send".to_string(),
            description: None,
            event: HookEvent::TaskCompleted,
            action: HookAction::SendMessage {
                channel_type: "telegram".to_string(),
                message_template: "Task {{task_id}}".to_string(),
            },
            filter: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        };

        executor
            .execute_hook(&hook, &sample_context())
            .await
            .expect("execute send hook");

        let messages = sent.lock().await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].conversation_id, "chat-1");
        assert_eq!(messages[0].content, "Task task-1");
    }
}
