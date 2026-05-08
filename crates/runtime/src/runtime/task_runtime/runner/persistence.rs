use super::*;

impl TaskRunner {
    pub(super) async fn clear_resume_intent(&self, task_id: &str) {
        let mut states = self.resume_states.write().await;
        states.remove(task_id);
    }

    pub(super) async fn clear_task_conversation_links(&self, task_id: &str) {
        let _ = task_id;
    }

    /// Clean up all resources associated with a task.
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
    /// This bridges scheduled task execution into the chat session history so
    /// the sidebar shows execution results as regular chat messages.
    pub(super) fn persist_to_chat_session(
        &self,
        task: &Task,
        input: Option<&str>,
        output: &str,
        is_error: bool,
        duration_ms: i64,
    ) {
        use crate::models::{ChatExecutionStatus, MessageExecution};

        let session_id = task.chat_session_id.trim();
        if session_id.is_empty() {
            debug!(
                "No chat session bound to task '{}', skipping persist",
                task.name
            );
            return;
        }

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

        if let Some(session_service) = &self.session_service {
            if let Err(e) = session_service.append_task_result(
                session_id,
                input,
                output,
                execution,
                "task_runtime",
            ) {
                warn!(
                    "Failed to persist task result to session '{}': {}",
                    session_id, e
                );
            }
            return;
        }

        warn!(
            "Task runner has no SessionService; skipping transcript persistence for task '{}' session '{}'",
            task.name, session_id
        );
    }

    pub(super) fn resolve_task_input(&self, task: &Task) -> Option<String> {
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
    pub(super) fn render_input_template(task: &Task, template: &str) -> String {
        let now = chrono::Utc::now();
        let replacement_strings = std::collections::HashMap::from([
            ("{{task.id}}", task.id.clone()),
            ("{{task.name}}", task.name.clone()),
            ("{{task.agent_id}}", task.agent_id.clone()),
            (
                "{{task.description}}",
                task.description.clone().unwrap_or_default(),
            ),
            ("{{task.input}}", task.input.clone().unwrap_or_default()),
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
}
