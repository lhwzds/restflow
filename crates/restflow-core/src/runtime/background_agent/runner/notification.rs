use super::*;

impl TaskRunner {
    pub(super) async fn fire_hooks(&self, context: &HookContext) {
        if let Some(executor) = &self.hook_executor {
            executor.fire(context).await;
        }
    }

    /// Send notification for task completion/failure.
    ///
    /// Prefers broadcasting through ChannelRouter when available. Falls
    /// back to the dedicated Telegram sender only when router delivery
    /// does not succeed, avoiding duplicate notifications.
    pub(super) async fn send_notification(
        &self,
        task: &Task,
        success: bool,
        message: &str,
    ) {
        // Check if we should only notify on failure
        if success && task.notification.notify_on_failure_only {
            return;
        }

        let operation = format!(
            "Executed background agent '{}' ({}) and prepared a {} notification payload.",
            task.name,
            task.id,
            if success { "success" } else { "failure" }
        );
        let verification = if success {
            "Execution completed without runtime errors. Delivery was attempted through configured notification sinks."
        } else {
            "Execution failed. Operators should inspect logs/events and retry after fixing the identified issue."
        };
        let notification_message = if success {
            ensure_success_output(message, &operation, verification)
        } else {
            let detail = if message.trim().is_empty() {
                "Background agent execution failed without explicit error detail."
            } else {
                message
            };
            format_error_output(detail, &operation, verification)
        };
        let level = if success {
            MessageLevel::Plain
        } else {
            MessageLevel::Error
        };

        let mut sent_via: Vec<&'static str> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        let router_sink = ChannelRouterNotificationSink {
            router: self.channel_router.clone(),
        };
        match router_sink.send(task, level, &notification_message).await {
            Ok(NotificationDispatchStatus::Sent) => sent_via.push(router_sink.name()),
            Ok(NotificationDispatchStatus::Skipped) => {}
            Err(err) => {
                let detail = format!("{}: {}", router_sink.name(), err);
                error!(
                    task_id = %task.id,
                    sink = router_sink.name(),
                    error = %err,
                    "Failed to dispatch notification"
                );
                failures.push(detail);
            }
        }

        // Avoid duplicate sends: only try direct Telegram fallback when no
        // router delivery has succeeded.
        if sent_via.is_empty() {
            let telegram_sink = TelegramNotificationSink {
                notifier: self.notifier.clone(),
            };
            match telegram_sink.send(task, level, &notification_message).await {
                Ok(NotificationDispatchStatus::Sent) => sent_via.push(telegram_sink.name()),
                Ok(NotificationDispatchStatus::Skipped) => {}
                Err(err) => {
                    let detail = format!("{}: {}", telegram_sink.name(), err);
                    error!(
                        task_id = %task.id,
                        sink = telegram_sink.name(),
                        error = %err,
                        "Failed to dispatch notification"
                    );
                    failures.push(detail);
                }
            }
        }

        if !sent_via.is_empty() {
            let summary = format!(
                "Notification sent via [{}]: {}",
                sent_via.join(","),
                if success { "success" } else { "failure" }
            );
            if let Err(err) = self
                .storage
                .record_notification_sent(&task.id, summary.clone())
            {
                warn!("Failed to record notification sent event: {}", err);
            }
            self.event_emitter
                .emit(TaskStreamEvent::progress(
                    &task.id,
                    "notification",
                    None,
                    Some(summary),
                ))
                .await;
            return;
        }

        if !failures.is_empty() {
            let detail = failures.join(" | ");
            if let Err(err) = self
                .storage
                .record_notification_failed(&task.id, detail.clone())
            {
                warn!("Failed to record notification failure event: {}", err);
            }
            self.event_emitter
                .emit(TaskStreamEvent::progress(
                    &task.id,
                    "notification_failed",
                    None,
                    Some(detail),
                ))
                .await;
            return;
        }

        self.event_emitter
            .emit(TaskStreamEvent::progress(
                &task.id,
                "notification_skipped",
                None,
                Some("No enabled notification sinks".to_string()),
            ))
            .await;
    }
}
