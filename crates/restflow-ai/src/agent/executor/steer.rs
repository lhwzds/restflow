use std::time::Duration;

use crate::agent::context_manager;
use crate::agent::deferred::{DeferredExecutionManager, DeferredStatus};
use crate::agent::state::AgentState;
use crate::error::AiError;
use crate::llm::Message;
use crate::steer::SteerMessage;

use super::{AgentExecutor, truncate_tool_output};

impl AgentExecutor {
    /// Poll the sub-agent tracker for completions and inject notification messages.
    pub(crate) async fn poll_subagent_completions(
        &self,
        state: &mut AgentState,
        max_result_length: usize,
    ) {
        let Some(tracker) = &self.subagent_tracker else {
            return;
        };

        let completions = tracker.poll_completions().await;
        if completions.is_empty() {
            return;
        }

        for completion in completions {
            let agent_name = tracker
                .get(&completion.id)
                .map(|s| s.agent_name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let status_str = if completion.result.success {
                "completed"
            } else {
                "failed"
            };

            let mut output = completion.result.output.clone();
            if output.len() > max_result_length {
                output = context_manager::middle_truncate(&output, max_result_length);
            }

            let error_tag = match &completion.result.error {
                Some(err) => format!("\n  <error>{}</error>", err),
                None => String::new(),
            };

            let notification = format!(
                "<subagent_notification>\n  \
                 <task_id>{}</task_id>\n  \
                 <agent>{}</agent>\n  \
                 <status>{}</status>\n  \
                 <duration_ms>{}</duration_ms>\n  \
                 <output>{}</output>{}\n\
                 </subagent_notification>",
                completion.id,
                agent_name,
                status_str,
                completion.result.duration_ms,
                output,
                error_tag,
            );

            tracing::info!(
                task_id = %completion.id,
                agent = %agent_name,
                status = %status_str,
                "Injecting sub-agent completion notification"
            );

            state.add_message(Message::system(notification));
        }
    }

    pub(crate) async fn drain_steer_messages(&self) -> Vec<SteerMessage> {
        // First, drain any buffered messages from the tool-drain phase
        let mut messages = {
            let mut buffer = self.steer_buffer.lock().await;
            std::mem::take(&mut *buffer)
        };

        let Some(rx) = &self.steer_rx else {
            return messages;
        };

        let mut rx = rx.lock().await;
        loop {
            match rx.try_recv() {
                Ok(msg) => messages.push(msg),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        messages
    }

    pub(crate) async fn apply_steer_messages(
        &self,
        state: &mut AgentState,
        deferred_manager: &DeferredExecutionManager,
    ) {
        let messages = self.drain_steer_messages().await;
        if messages.is_empty() {
            return;
        }

        for steer in messages {
            match &steer.command {
                crate::steer::SteerCommand::Message { instruction } => {
                    if let Some((approval_id, approved, reason)) =
                        parse_approval_resolution(instruction)
                    {
                        let _ = deferred_manager
                            .resolve_by_approval_id(&approval_id, approved, reason.clone())
                            .await;
                        tracing::info!(
                            approval_id = %approval_id,
                            approved = approved,
                            "Received approval resolution steer message"
                        );
                        let text = if approved {
                            format!("[Approval Update]: {approval_id} approved.")
                        } else {
                            format!(
                                "[Approval Update]: {approval_id} denied. {}",
                                reason
                                    .clone()
                                    .unwrap_or_else(|| "No reason provided.".to_string())
                            )
                        };
                        let msg = Message::system(text);
                        state.add_message(msg);
                        continue;
                    }
                    tracing::info!(
                        instruction = %instruction,
                        source = ?steer.source,
                        "Received steer message, injecting into conversation"
                    );
                    let msg = Message::user(format!("[User Update]: {}", instruction));
                    state.add_message(msg);
                }
                crate::steer::SteerCommand::Interrupt { reason, .. } => {
                    tracing::info!(
                        reason = %reason,
                        source = ?steer.source,
                        "Received interrupt command"
                    );
                    state.interrupt(reason);
                }
                crate::steer::SteerCommand::CancelToolCall { tool_call_id } => {
                    if let Some((_, abort_handle)) = self.active_tool_calls.remove(tool_call_id) {
                        abort_handle.abort();
                        tracing::info!(
                            tool_call_id = %tool_call_id,
                            source = ?steer.source,
                            "Tool call cancelled via steer"
                        );
                    }
                }
            }
        }
    }

    pub(crate) async fn process_resolved_deferred_calls(
        &self,
        deferred_manager: &DeferredExecutionManager,
        state: &mut AgentState,
        tool_timeout: Duration,
        max_tool_result_length: usize,
        tool_output_dir: Option<&std::path::Path>,
    ) {
        let resolved_calls = deferred_manager.drain_resolved().await;
        if resolved_calls.is_empty() {
            return;
        }

        for deferred in resolved_calls {
            match deferred.status {
                DeferredStatus::Approved => {
                    let result = tokio::time::timeout(
                        tool_timeout,
                        self.execute_tool_call(&deferred.tool_name, deferred.args.clone(), false),
                    )
                    .await
                    .map_err(|_| AiError::Tool(format!("Tool {} timed out", deferred.tool_name)))
                    .and_then(|result| result);
                    let mut text = match result {
                        Ok(output) if output.success => {
                            let value = serde_json::to_string(&output.result).unwrap_or_default();
                            format!(
                                "Deferred tool call '{}' was approved and executed successfully. Result: {}",
                                deferred.tool_name, value
                            )
                        }
                        Ok(output) => format!(
                            "Deferred tool call '{}' was approved but failed: {}",
                            deferred.tool_name,
                            output.error.unwrap_or_else(|| "unknown error".to_string())
                        ),
                        Err(error) => format!(
                            "Deferred tool call '{}' failed after approval: {}",
                            deferred.tool_name, error
                        ),
                    };
                    text = truncate_tool_output(
                        &text,
                        max_tool_result_length,
                        tool_output_dir,
                        &deferred.call_id,
                        &deferred.tool_name,
                    );
                    let msg = Message::system(text);
                    state.add_message(msg);
                }
                DeferredStatus::Denied { reason } => {
                    let msg = Message::system(format!(
                        "Deferred tool call '{}' was denied: {}",
                        deferred.tool_name, reason
                    ));
                    state.add_message(msg);
                }
                DeferredStatus::TimedOut => {
                    let msg = Message::system(format!(
                        "Approval timed out for deferred tool call '{}'.",
                        deferred.tool_name
                    ));
                    state.add_message(msg);
                }
                DeferredStatus::Pending => {}
            }
        }
    }

    /// Process only CancelToolCall steer commands (non-blocking).
    /// Message and Interrupt variants are buffered for `apply_steer_messages()`.
    pub(crate) async fn process_cancel_steers(&self) {
        let Some(rx) = &self.steer_rx else {
            return;
        };

        let mut rx = rx.lock().await;
        let mut deferred = Vec::new();
        while let Ok(steer) = rx.try_recv() {
            match &steer.command {
                crate::steer::SteerCommand::CancelToolCall { tool_call_id } => {
                    if let Some((_, abort_handle)) = self.active_tool_calls.remove(tool_call_id) {
                        abort_handle.abort();
                        tracing::info!(
                            tool_call_id = %tool_call_id,
                            "Tool call cancelled via steer (during tool drain)"
                        );
                    }
                }
                _ => deferred.push(steer),
            }
        }
        drop(rx);

        // Buffer non-cancel messages for the next apply_steer_messages() call
        if !deferred.is_empty() {
            let mut buffer = self.steer_buffer.lock().await;
            buffer.extend(deferred);
        }
    }
}

pub(crate) fn parse_approval_resolution(
    instruction: &str,
) -> Option<(String, bool, Option<String>)> {
    let trimmed = instruction.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("approval ") {
        return None;
    }

    let mut parts = trimmed.splitn(4, ' ');
    let _ = parts.next();
    let approval_id = parts.next()?.trim();
    let action = parts.next()?.trim().to_ascii_lowercase();
    let reason = parts.next().map(|s| s.trim().to_string());

    if action == "approved" {
        Some((approval_id.to_string(), true, reason))
    } else if action == "denied" || action == "rejected" {
        Some((approval_id.to_string(), false, reason))
    } else {
        None
    }
}
