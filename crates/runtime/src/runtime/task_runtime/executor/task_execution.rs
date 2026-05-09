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
    ) -> Result<ExecutionResult> {
        let _ = (agent_id, task_id, input, steer_rx, emitter);
        anyhow::bail!("legacy task execution has been removed")
    }
}
