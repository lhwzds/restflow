use super::*;
use restflow_ai::llm::Message;

impl BackgroundAgentRunner {
    pub(super) async fn clear_resume_intent(&self, task_id: &str) {
        let (mut states, mut checkpoint_ids) = tokio::join!(
            self.resume_states.write(),
            self.resume_checkpoint_ids.write(),
        );
        states.remove(task_id);
        checkpoint_ids.remove(task_id);
    }

    pub(super) async fn clear_task_conversation_links(&self, task_id: &str) {
        let Some(router) = self.channel_router.read().await.as_ref().cloned() else {
            return;
        };
        let cleared = router.clear_task_associations(task_id).await;
        if cleared > 0 {
            info!(
                "Cleared task association for {} conversation(s) after task {} terminal state",
                cleared, task_id
            );
        }
    }

    /// Clean up all resources associated with a background agent task.
    /// Called via scopeguard when task execution panics or fails unexpectedly.
    pub(super) fn cleanup_agent_resources(task_id: &str) {
        use std::fs;

        // Clean up tool-output directory for this task.
        if let Ok(restflow_dir) = crate::paths::resolve_restflow_dir() {
            let task_output_dir = restflow_dir.join("tool-output").join(task_id);
            if task_output_dir.exists()
                && let Err(e) = fs::remove_dir_all(&task_output_dir)
            {
                warn!(
                    "Failed to remove tool output directory {:?}: {}",
                    task_output_dir, e
                );
            } else if task_output_dir.exists() {
                debug!("Cleaned up tool output directory for task {}", task_id);
            }
        }

        debug!("Scope guard cleanup completed for task {}", task_id);
    }

    /// Remove runtime tracking entries for a task without consuming staged
    /// resume intent.
    pub(super) async fn cleanup_runtime_tracking(&self, task_id: &str) {
        // Acquire all locks concurrently to minimize inconsistency window
        let (mut running, mut senders, mut receivers) = tokio::join!(
            self.running_tasks.write(),
            self.stop_senders.write(),
            self.pending_stop_receivers.write(),
        );

        // Remove from all maps
        running.remove(task_id);
        senders.remove(task_id);
        receivers.remove(task_id);

        // Explicitly drop locks before unregister to avoid holding while calling external code
        drop((running, senders, receivers));

        // Unregister from steer registry (may fail, but maps are already cleaned)
        self.steer_registry.unregister(task_id).await;
    }

    /// Remove a task from runner tracking maps including any staged resume
    /// intent.
    pub(super) async fn cleanup_task_tracking(&self, task_id: &str) {
        self.cleanup_runtime_tracking(task_id).await;
        self.clear_resume_intent(task_id).await;
    }

    /// Take the stop receiver for a task, returning None if not found.
    /// When None, the task runs without stop support.
    pub(super) async fn take_stop_receiver(&self, task_id: &str) -> Option<oneshot::Receiver<()>> {
        self.pending_stop_receivers.write().await.remove(task_id)
    }

    /// Persist input and output as messages in the task's bound chat session.
    ///
    /// This bridges background agent execution into the chat session history so
    /// the sidebar shows execution results as regular chat messages.
    pub(super) fn persist_to_chat_session(
        &self,
        task: &BackgroundAgent,
        input: Option<&str>,
        output: &str,
        is_error: bool,
        duration_ms: i64,
    ) {
        use crate::models::{ChatExecutionStatus, ChatMessage, MessageExecution};

        let session_id = task.chat_session_id.trim();
        if session_id.is_empty() {
            debug!(
                "No chat session bound to task '{}', skipping persist",
                task.name
            );
            return;
        }

        let mut session = match self.storage.chat_sessions().get(session_id) {
            Ok(Some(s)) => s,
            Ok(None) => {
                warn!(
                    "Bound chat session '{}' not found for task '{}'",
                    session_id, task.name
                );
                return;
            }
            Err(e) => {
                warn!("Failed to load chat session '{}': {}", session_id, e);
                return;
            }
        };

        // Add user message (the input prompt)
        if let Some(input_text) = input
            && !input_text.trim().is_empty()
        {
            session.add_message(ChatMessage::user(input_text));
        }

        // Add assistant message (the output) with execution metadata
        let execution = MessageExecution {
            steps: Vec::new(),
            duration_ms: duration_ms as u64,
            tokens_used: 0,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            status: if is_error {
                ChatExecutionStatus::Failed
            } else {
                ChatExecutionStatus::Completed
            },
        };
        session.add_message(ChatMessage::assistant(output).with_execution(execution));

        // Update timestamp
        session.updated_at = chrono::Utc::now().timestamp_millis();

        if let Err(e) = self.storage.chat_sessions().save(&session) {
            warn!("Failed to save chat session '{}': {}", session_id, e);
        }
    }

    /// Persist conversation messages to long-term memory.
    ///
    /// Called after successful task execution when `persist_on_complete` is enabled.
    pub(super) fn persist_memory(&self, task: &BackgroundAgent, messages: &[Message]) {
        let Some(persister) = &self.memory_persister else {
            debug!("Memory persistence not configured, skipping");
            return;
        };

        if messages.is_empty() {
            debug!("No messages to persist for task '{}'", task.name);
            return;
        }

        // Generate tags from task metadata
        // Note: BackgroundAgent doesn't have a tags field, so we use task name and agent_id
        let tags: Vec<String> = vec![
            format!("task:{}", task.id),
            format!("agent:{}", task.agent_id),
            format!(
                "memory_scope:{}",
                Self::memory_scope_label(&task.memory.memory_scope)
            ),
        ];
        let memory_agent_id = Self::resolve_memory_agent_id(task);

        match persister.persist(messages, &memory_agent_id, &task.id, &task.name, &tags) {
            Ok(result) => {
                if result.chunk_count > 0 {
                    info!(
                        "Persisted {} memory chunks for task '{}' (session: {}, namespace: {})",
                        result.chunk_count, task.name, result.session_id, memory_agent_id
                    );
                }
            }
            Err(e) => {
                warn!("Failed to persist memory for task '{}': {}", task.name, e);
            }
        }
    }

    pub(super) fn resolve_task_input(&self, task: &BackgroundAgent) -> Option<String> {
        let fallback_input = task.input.clone().filter(|value| !value.trim().is_empty());

        if let Some(template) = task.input_template.as_deref() {
            let rendered = Self::render_input_template(task, template);
            if !rendered.trim().is_empty() {
                return Some(rendered);
            }
            fallback_input
        } else {
            fallback_input
        }
    }

    /// Single-pass template renderer that prevents double-substitution.
    /// Scans for `{{...}}` placeholders left-to-right; replacement values are
    /// emitted verbatim so any `{{` inside a value will NOT be re-expanded.
    pub(super) fn render_input_template(task: &BackgroundAgent, template: &str) -> String {
        let now = chrono::Utc::now();
        // NOTE: `{{task.input}}` is preferred. `{{input}}` is kept for compatibility.
        let replacement_strings = std::collections::HashMap::from([
            ("{{task.id}}", task.id.clone()),
            ("{{task.name}}", task.name.clone()),
            ("{{task.agent_id}}", task.agent_id.clone()),
            (
                "{{task.description}}",
                task.description.clone().unwrap_or_default(),
            ),
            ("{{task.input}}", task.input.clone().unwrap_or_default()),
            ("{{input}}", task.input.clone().unwrap_or_default()),
            (
                "{{task.last_run_at}}",
                Self::format_optional_timestamp(task.last_run_at),
            ),
            (
                "{{task.next_run_at}}",
                Self::format_optional_timestamp(task.next_run_at),
            ),
            ("{{now.iso}}", now.to_rfc3339()),
            ("{{now.unix_ms}}", now.timestamp_millis().to_string()),
        ]);
        let replacements: std::collections::HashMap<&str, &str> = replacement_strings
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        crate::template::render_template_single_pass(template, &replacements)
    }

    fn format_optional_timestamp(timestamp: Option<i64>) -> String {
        timestamp.map(|value| value.to_string()).unwrap_or_default()
    }

    pub(super) fn resolve_memory_agent_id(task: &BackgroundAgent) -> String {
        match task.memory.memory_scope {
            MemoryScope::SharedAgent => task.agent_id.clone(),
            MemoryScope::PerBackgroundAgent => format!("{}::task::{}", task.agent_id, task.id),
        }
    }

    fn memory_scope_label(scope: &MemoryScope) -> &'static str {
        match scope {
            MemoryScope::SharedAgent => "shared_agent",
            MemoryScope::PerBackgroundAgent => "per_background_agent",
        }
    }
}
