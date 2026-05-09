use super::*;
use crate::models::{TaskRunMetrics, TaskRunStatus};

pub(super) struct TaskRunFinalizer<'a> {
    runner: &'a TaskRunner,
    task: Task,
    run_id: String,
}

impl<'a> TaskRunFinalizer<'a> {
    pub(super) fn new(runner: &'a TaskRunner, task: Task, run_id: String) -> Self {
        Self {
            runner,
            task,
            run_id,
        }
    }

    fn build_metrics(duration_ms: i64, outcome: Option<&ExecutionResult>) -> TaskRunMetrics {
        let duration_ms = Some(duration_ms.max(0) as u64);
        let Some(outcome) = outcome else {
            return TaskRunMetrics {
                duration_ms,
                ..TaskRunMetrics::default()
            };
        };

        TaskRunMetrics {
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
        status: TaskRunStatus,
        duration_ms: i64,
        error: Option<String>,
        outcome: Option<&ExecutionResult>,
    ) {
        if let Err(err) = self.runner.storage.mark_task_run_terminal(
            &self.run_id,
            status,
            chrono::Utc::now().timestamp_millis(),
            error,
            Self::build_metrics(duration_ms, outcome),
        ) {
            warn!(
                task_id = %self.task.id,
                run_id = %self.run_id,
                error = %err,
                "Failed to persist task run terminal state"
            );
        }
    }

    fn stream_event(&self, event: TaskStreamEvent) -> TaskStreamEvent {
        task_stream_event_context(event, &self.task, &self.run_id)
    }

    pub(super) async fn finalize_success(&self, exec_result: &ExecutionResult, duration_ms: i64) {
        self.persist_run_terminal(
            TaskRunStatus::Completed,
            duration_ms,
            None,
            Some(exec_result),
        );

        self.runner
            .event_emitter
            .emit(self.stream_event(TaskStreamEvent::completed(
                &self.task.id,
                &exec_result.output,
                duration_ms,
            )))
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
            None,
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
            let event = crate::models::TaskEvent::new(
                self.task.id.clone(),
                crate::models::TaskEventType::Compaction,
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
                .emit(self.stream_event(TaskStreamEvent::progress(
                    &self.task.id,
                    "compaction",
                    None,
                    Some(compaction_message),
                )))
                .await;
        }
    }

    pub(super) async fn finalize_failure(
        &self,
        error_msg: &str,
        duration_ms: i64,
        persist_to_session: bool,
    ) {
        self.persist_run_terminal(
            TaskRunStatus::Failed,
            duration_ms,
            Some(error_msg.to_string()),
            None,
        );

        self.runner
            .event_emitter
            .emit(self.stream_event(TaskStreamEvent::failed(
                &self.task.id,
                error_msg,
                duration_ms,
                false,
            )))
            .await;

        if let Err(err) = self.runner.storage.fail_task_execution(
            &self.task.id,
            error_msg.to_string(),
            duration_ms,
        ) {
            error!("Failed to record task failure: {}", err);
        }

        if persist_to_session {
            self.runner
                .persist_to_chat_session(&self.task, None, error_msg, true, duration_ms);
        }
    }

    pub(super) async fn finalize_timeout(
        &self,
        error_msg: &str,
        timeout_secs: u64,
        duration_ms: i64,
    ) {
        self.persist_run_terminal(
            TaskRunStatus::TimedOut,
            duration_ms,
            Some(error_msg.to_string()),
            None,
        );

        self.runner
            .event_emitter
            .emit(self.stream_event(TaskStreamEvent::timeout(
                self.task.id.clone(),
                timeout_secs,
                duration_ms,
            )))
            .await;

        if let Err(err) = self.runner.storage.fail_task_execution(
            &self.task.id,
            error_msg.to_string(),
            duration_ms,
        ) {
            error!("Failed to record task timeout: {}", err);
        }

        self.runner
            .persist_to_chat_session(&self.task, None, error_msg, true, duration_ms);
    }

    pub(super) async fn finalize_interrupted(&self, reason: &str, duration_ms: i64) {
        if let Err(err) = self.runner.storage.interrupt_task_run(
            &self.run_id,
            chrono::Utc::now().timestamp_millis(),
            reason.to_string(),
        ) {
            warn!(
                task_id = %self.task.id,
                run_id = %self.run_id,
                error = %err,
                "Failed to persist interrupted task run"
            );
        }

        self.runner
            .event_emitter
            .emit(self.stream_event(TaskStreamEvent::interrupted(
                &self.task.id,
                reason,
                duration_ms,
            )))
            .await;
    }
}
