use super::*;
use crate::models::{BackgroundAgentRunMetrics, BackgroundAgentRunStatus};
use restflow_telemetry::RunHandle;

pub(super) struct BackgroundRunFinalizer<'a> {
    runner: &'a TaskRunner,
    task: Task,
    resolved_input: Option<String>,
    run_handle: RunHandle,
}

impl<'a> BackgroundRunFinalizer<'a> {
    pub(super) fn new(
        runner: &'a TaskRunner,
        task: Task,
        resolved_input: Option<String>,
        run_handle: RunHandle,
    ) -> Self {
        Self {
            runner,
            task,
            resolved_input,
            run_handle,
        }
    }

    fn build_metrics(
        duration_ms: i64,
        outcome: Option<&ExecutionResult>,
    ) -> BackgroundAgentRunMetrics {
        let duration_ms = Some(duration_ms.max(0) as u64);
        let Some(outcome) = outcome else {
            return BackgroundAgentRunMetrics {
                duration_ms,
                ..BackgroundAgentRunMetrics::default()
            };
        };

        BackgroundAgentRunMetrics {
            duration_ms,
            iterations: outcome.metrics.iterations,
            active_model: outcome.metrics.active_model.clone(),
            final_model: outcome
                .metrics
                .final_model
                .as_ref()
                .map(|model| model.as_serialized_str().to_string()),
            message_count: Some(outcome.metrics.message_count),
            compaction_events: outcome
                .metrics
                .compaction
                .as_ref()
                .map(|metrics| metrics.event_count),
        }
    }

    fn persist_run_terminal(
        &self,
        status: BackgroundAgentRunStatus,
        duration_ms: i64,
        error: Option<String>,
        outcome: Option<&ExecutionResult>,
    ) {
        if let Err(err) = self.runner.storage.mark_task_run_terminal(
            self.run_handle.run_id(),
            status,
            chrono::Utc::now().timestamp_millis(),
            error,
            Self::build_metrics(duration_ms, outcome),
        ) {
            warn!(
                task_id = %self.task.id,
                run_id = %self.run_handle.run_id(),
                error = %err,
                "Failed to persist background task run terminal state"
            );
        }
    }

    pub(super) async fn finalize_success(&self, exec_result: &ExecutionResult, duration_ms: i64) {
        self.run_handle
            .complete(Some(duration_ms.max(0) as u64))
            .await;
        self.persist_run_terminal(
            BackgroundAgentRunStatus::Completed,
            duration_ms,
            None,
            Some(exec_result),
        );

        self.runner
            .event_emitter
            .emit(TaskStreamEvent::completed(
                &self.task.id,
                &exec_result.output,
                duration_ms,
            ))
            .await;
        self.runner
            .fire_hooks(&HookContext::from_completed(
                &self.task,
                &exec_result.output,
                duration_ms,
            ))
            .await;

        if let Err(err) = self.runner.storage.complete_task_execution(
            &self.task.id,
            Some(exec_result.output.clone()),
            duration_ms,
        ) {
            error!("Failed to record task completion: {}", err);
        }

        self.runner.persist_to_chat_session(
            &self.task,
            self.resolved_input.as_deref(),
            &exec_result.output,
            false,
            duration_ms,
        );

        if let Some(compaction) = exec_result.metrics.compaction.as_ref() {
            let compaction_message = format!(
                "Compacted {} messages ({} -> {} tokens) across {} event(s)",
                compaction.messages_compacted,
                compaction.tokens_before,
                compaction.tokens_after,
                compaction.event_count
            );
            let event = crate::models::BackgroundAgentEvent::new(
                self.task.id.clone(),
                crate::models::BackgroundAgentEventType::Compaction,
            )
            .with_message(compaction_message.clone());
            if let Err(err) = self.runner.storage.add_event(&event) {
                warn!(
                    "Failed to record compaction event for '{}': {}",
                    self.task.id, err
                );
            }
            self.runner
                .event_emitter
                .emit(TaskStreamEvent::progress(
                    &self.task.id,
                    "compaction",
                    None,
                    Some(compaction_message),
                ))
                .await;
        }

        if self.task.memory.persist_on_complete {
            self.runner
                .persist_memory(&self.task, &exec_result.messages);
        }

        self.runner
            .send_notification(&self.task, true, &exec_result.output)
            .await;
    }

    pub(super) async fn finalize_failure(
        &self,
        error_msg: &str,
        duration_ms: i64,
        persist_to_session: bool,
    ) {
        self.run_handle
            .fail(error_msg, Some(duration_ms.max(0) as u64))
            .await;
        self.persist_run_terminal(
            BackgroundAgentRunStatus::Failed,
            duration_ms,
            Some(error_msg.to_string()),
            None,
        );

        self.runner
            .event_emitter
            .emit(TaskStreamEvent::failed(
                &self.task.id,
                error_msg,
                duration_ms,
                false,
            ))
            .await;
        self.runner
            .fire_hooks(&HookContext::from_failed(
                &self.task,
                error_msg,
                duration_ms,
            ))
            .await;

        if let Err(err) = self.runner.storage.fail_task_execution(
            &self.task.id,
            error_msg.to_string(),
            duration_ms,
        ) {
            error!("Failed to record task failure: {}", err);
        }

        if persist_to_session {
            self.runner.persist_to_chat_session(
                &self.task,
                self.resolved_input.as_deref(),
                error_msg,
                true,
                duration_ms,
            );
        }

        self.runner
            .send_notification(&self.task, false, error_msg)
            .await;
    }

    pub(super) async fn finalize_timeout(
        &self,
        error_msg: &str,
        timeout_secs: u64,
        duration_ms: i64,
    ) {
        self.run_handle
            .fail(error_msg, Some(duration_ms.max(0) as u64))
            .await;
        self.persist_run_terminal(
            BackgroundAgentRunStatus::TimedOut,
            duration_ms,
            Some(error_msg.to_string()),
            None,
        );

        self.runner
            .event_emitter
            .emit(TaskStreamEvent::timeout(
                self.task.id.clone(),
                timeout_secs,
                duration_ms,
            ))
            .await;
        self.runner
            .fire_hooks(&HookContext::from_failed(
                &self.task,
                error_msg,
                duration_ms,
            ))
            .await;

        if let Err(err) = self.runner.storage.fail_task_execution(
            &self.task.id,
            error_msg.to_string(),
            duration_ms,
        ) {
            error!("Failed to record task timeout: {}", err);
        }

        self.runner.persist_to_chat_session(
            &self.task,
            self.resolved_input.as_deref(),
            error_msg,
            true,
            duration_ms,
        );

        self.runner
            .send_notification(&self.task, false, error_msg)
            .await;
    }

    pub(super) async fn finalize_interrupted(&self, reason: &str, duration_ms: i64) {
        self.run_handle
            .interrupt(reason, Some(duration_ms.max(0) as u64))
            .await;
        if let Err(err) = self.runner.storage.interrupt_task_run(
            self.run_handle.run_id(),
            chrono::Utc::now().timestamp_millis(),
            reason.to_string(),
        ) {
            warn!(
                task_id = %self.task.id,
                run_id = %self.run_handle.run_id(),
                error = %err,
                "Failed to persist interrupted background task run"
            );
        }

        self.runner
            .event_emitter
            .emit(TaskStreamEvent::interrupted(
                &self.task.id,
                reason,
                duration_ms,
            ))
            .await;
        self.runner
            .fire_hooks(&HookContext::from_interrupted(
                &self.task,
                reason,
                duration_ms,
            ))
            .await;
    }
}
