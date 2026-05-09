use super::*;
use async_trait::async_trait;

#[async_trait]
impl AgentExecutor for AgentRuntimeExecutor {
    /// Execute a background task as a bound chat session turn.
    async fn execute(
        &self,
        agent_id: &str,
        task_id: Option<&str>,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<ai::telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        let task_id = task_id.ok_or_else(|| anyhow!("task session execution requires task_id"))?;
        let Some(input) = input else {
            anyhow::bail!("task session execution requires input");
        };
        let task = self
            .storage
            .tasks
            .get_task(task_id)?
            .ok_or_else(|| anyhow!("Task '{}' not found", task_id))?;
        if task.agent_id != agent_id {
            anyhow::bail!(
                "task '{}' is bound to agent '{}', expected '{}'",
                task_id,
                task.agent_id,
                agent_id
            );
        }
        let session_id = task.chat_session_id.trim();
        if session_id.is_empty() {
            anyhow::bail!("task '{}' is not bound to a chat session", task_id);
        }
        let mut session = self.load_chat_session(session_id)?;
        self.validate_prerequisites(&task.prerequisites)?;
        let max_history = self
            .storage
            .config
            .get_effective_config_for_workspace(None)
            .ok()
            .map(|config| config.runtime_defaults.chat_max_session_history)
            .unwrap_or(types::DEFAULT_CHAT_MAX_SESSION_HISTORY);
        let result = self
            .execute_session_turn_with_emitter_and_steer(
                &mut session,
                input,
                max_history,
                SessionInputMode::EphemeralInput,
                emitter,
                SessionTurnRuntimeOptions {
                    steer_rx,
                    telemetry_context,
                    stream_display_mode: ai::StreamDisplayMode::Buffered,
                    workspace_root: None,
                },
            )
            .await?;
        Ok(ExecutionResult::success(result.output, Vec::new()).with_metrics(result.metrics))
    }
}
