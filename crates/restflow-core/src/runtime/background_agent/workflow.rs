use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;

use crate::models::{
    AgentWorkflow, BackgroundAgent, WorkflowCheckpoint, WorkflowPhase, WorkflowRetryConfig,
    WorkflowStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowExecutionResult {
    pub phase_outputs: BTreeMap<u32, String>,
}

#[async_trait]
pub trait WorkflowPhaseRunner: Send + Sync {
    async fn run_phase(
        &self,
        task: &BackgroundAgent,
        phase: &WorkflowPhase,
        input: String,
    ) -> Result<String>;
}

/// Durable multi-phase workflow executor with checkpoint persistence.
pub struct WorkflowExecutor<R: WorkflowPhaseRunner> {
    runner: Arc<R>,
    checkpoint_dir: PathBuf,
}

impl<R: WorkflowPhaseRunner> WorkflowExecutor<R> {
    pub fn new(runner: Arc<R>, checkpoint_dir: PathBuf) -> Self {
        Self {
            runner,
            checkpoint_dir,
        }
    }

    pub fn checkpoint_dir(&self) -> &Path {
        &self.checkpoint_dir
    }

    pub async fn execute_workflow(
        &self,
        task: &BackgroundAgent,
        workflow: &mut AgentWorkflow,
        context: &HashMap<String, String>,
    ) -> Result<WorkflowExecutionResult> {
        if workflow.task_id != task.id {
            bail!(
                "workflow task_id mismatch: expected {}, got {}",
                task.id,
                workflow.task_id
            );
        }

        workflow.status = WorkflowStatus::Running;

        for idx in workflow.current_phase..workflow.phases.len() {
            let phase = workflow
                .phases
                .get(idx)
                .ok_or_else(|| anyhow!("phase {} missing", idx))?;
            self.ensure_dependencies(idx, phase, workflow)?;
            let input = self.render_phase_input(phase, workflow, context);
            
            // FIX: Catch phase errors to set PhaseFailed status and persist failure checkpoint
            let (output, attempt) = match self.execute_phase_with_retry(task, phase, input).await {
                Ok(result) => result,
                Err(err) => {
                    // Set status to PhaseFailed and persist failure checkpoint before returning
                    workflow.status = WorkflowStatus::PhaseFailed {
                        phase_idx: idx,
                        error: err.to_string(),
                    };
                    let failure_checkpoint = self.build_checkpoint(
                        workflow,
                        idx,
                        0, // attempt is 0 on terminal failure
                        serde_json::json!({"status": "failed", "error": err.to_string()}),
                    );
                    // Best-effort persist; ignore errors to ensure we still return the original error
                    let _ = self.save_checkpoint(task, &failure_checkpoint).await;
                    return Err(err);
                }
            };
            
            workflow.phase_outputs.insert(idx as u32, output);
            workflow.current_phase = idx + 1;
            let checkpoint =
                self.build_checkpoint(workflow, idx, attempt, serde_json::json!({"status":"ok"}));
            self.save_checkpoint(task, &checkpoint).await?;
        }

        workflow.status = WorkflowStatus::Completed;
        Ok(WorkflowExecutionResult {
            phase_outputs: workflow.phase_outputs.clone(),
        })
    }

    pub async fn resume_from_latest_checkpoint(
        &self,
        task: &BackgroundAgent,
        workflow: &mut AgentWorkflow,
    ) -> Result<bool> {
        let Some(checkpoint) = self.load_latest_checkpoint(task.id.as_str()).await? else {
            return Ok(false);
        };

        if checkpoint.workflow_id != workflow.id {
            return Ok(false);
        }

        workflow.phase_outputs = checkpoint.phase_outputs;
        workflow.current_phase = checkpoint.phase_idx.saturating_add(1);
        workflow.status = WorkflowStatus::Running;
        Ok(true)
    }

    async fn execute_phase_with_retry(
        &self,
        task: &BackgroundAgent,
        phase: &WorkflowPhase,
        input: String,
    ) -> Result<(String, u32)> {
        let mut attempt: u32 = 0;
        let retry = &phase.retry_config;
        let max_attempts = retry.max_attempts.max(1);
        let mut backoff_ms = retry.initial_backoff_ms.min(retry.max_backoff_ms.max(1));
        
        // FIX: Get timeout from phase config
        let timeout_duration = phase.timeout_secs.map(Duration::from_secs);

        loop {
            attempt = attempt.saturating_add(1);
            
            // FIX: Wrap phase execution with optional timeout
            let run_result = if let Some(timeout) = timeout_duration {
                tokio::time::timeout(
                    timeout,
                    self.runner.run_phase(task, phase, input.clone())
                ).await
            } else {
                // No timeout configured, run directly and wrap in Ok to match timeout result type
                Ok(self.runner.run_phase(task, phase, input.clone()).await)
            };
            
            match run_result {
                Ok(Ok(output)) => return Ok((output, attempt)),
                Ok(Err(error)) => {
                    if attempt >= max_attempts || self.is_non_retryable(&error, retry) {
                        return Err(error);
                    }
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = ((backoff_ms as f32) * retry.backoff_multiplier)
                        .round()
                        .clamp(1.0, retry.max_backoff_ms as f32)
                        as u64;
                }
                // FIX: Handle timeout error
                Err(_timeout_elapsed) => {
                    let timeout_error = anyhow!(
                        "phase '{}' timed out after {}s",
                        phase.name,
                        phase.timeout_secs.unwrap_or(0)
                    );
                    if attempt >= max_attempts {
                        return Err(timeout_error);
                    }
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = ((backoff_ms as f32) * retry.backoff_multiplier)
                        .round()
                        .clamp(1.0, retry.max_backoff_ms as f32)
                        as u64;
                }
            }
        }
    }

    fn is_non_retryable(&self, error: &anyhow::Error, retry: &WorkflowRetryConfig) -> bool {
        let message = error.to_string().to_lowercase();
        retry
            .non_retryable_errors
            .iter()
            .any(|pattern| message.contains(&pattern.to_lowercase()))
    }

    fn ensure_dependencies(
        &self,
        idx: usize,
        phase: &WorkflowPhase,
        workflow: &AgentWorkflow,
    ) -> Result<()> {
        for dep in &phase.depends_on {
            if *dep >= idx {
                bail!(
                    "phase {} has invalid dependency {} (must be < current phase)",
                    idx,
                    dep
                );
            }
            if !workflow.phase_outputs.contains_key(&(*dep as u32)) {
                bail!("phase {} dependency {} has not completed", idx, dep);
            }
        }
        Ok(())
    }

    fn render_phase_input(
        &self,
        phase: &WorkflowPhase,
        workflow: &AgentWorkflow,
        context: &HashMap<String, String>,
    ) -> String {
        let mut rendered = phase.input_template.clone().unwrap_or_default();

        for (k, v) in context {
            rendered = rendered.replace(&format!("{{{{{}}}}}", k), v);
        }

        for (phase_idx, output) in &workflow.phase_outputs {
            rendered = rendered.replace(&format!("{{{{phase_{}_output}}}}", phase_idx), output);
        }

        rendered
    }

    fn build_checkpoint(
        &self,
        workflow: &AgentWorkflow,
        phase_idx: usize,
        attempt: u32,
        state: serde_json::Value,
    ) -> WorkflowCheckpoint {
        WorkflowCheckpoint {
            workflow_id: workflow.id.clone(),
            phase_idx,
            attempt,
            state,
            phase_outputs: workflow.phase_outputs.clone(),
            created_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    async fn save_checkpoint(
        &self,
        task: &BackgroundAgent,
        checkpoint: &WorkflowCheckpoint,
    ) -> Result<()> {
        let dir = self.checkpoint_dir.join(task.id.as_str());
        fs::create_dir_all(&dir).await?;
        let path = dir.join(format!(
            "phase_{}_attempt_{}.json",
            checkpoint.phase_idx, checkpoint.attempt
        ));
        let content = serde_json::to_vec_pretty(checkpoint)?;
        fs::write(path, content).await?;
        Ok(())
    }

    async fn load_latest_checkpoint(&self, task_id: &str) -> Result<Option<WorkflowCheckpoint>> {
        let dir = self.checkpoint_dir.join(task_id);
        let mut entries = match fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };

        let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified()?;
            match &latest {
                Some((_, last_modified)) if modified <= *last_modified => {}
                _ => latest = Some((entry.path(), modified)),
            }
        }

        let Some((path, _)) = latest else {
            return Ok(None);
        };

        let content = fs::read(path).await?;
        let checkpoint = serde_json::from_slice::<WorkflowCheckpoint>(&content)?;
        Ok(Some(checkpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AgentWorkflow, BackgroundAgent, TaskSchedule, WorkflowDefinition, WorkflowPhase,
        WorkflowRetryConfig, WorkflowStatus,
    };
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct MockPhaseRunner {
        responses: Mutex<HashMap<String, VecDeque<std::result::Result<String, String>>>>,
        calls: Mutex<Vec<String>>,
    }

    impl MockPhaseRunner {
        fn new(responses: HashMap<String, VecDeque<std::result::Result<String, String>>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls lock poisoned").clone()
        }
    }

    #[async_trait]
    impl WorkflowPhaseRunner for MockPhaseRunner {
        async fn run_phase(
            &self,
            _task: &BackgroundAgent,
            phase: &WorkflowPhase,
            _input: String,
        ) -> Result<String> {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .push(phase.name.clone());
            let mut responses = self.responses.lock().expect("responses lock poisoned");
            let queue = responses
                .get_mut(&phase.name)
                .expect("missing mock phase response queue");
            let next = queue.pop_front().expect("empty mock phase response queue");
            match next {
                Ok(value) => Ok(value),
                Err(err) => Err(anyhow!(err)),
            }
        }
    }

    fn temp_checkpoint_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir()
            .join("restflow-workflow-tests")
            .join(format!("{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("failed to create temp checkpoint dir");
        path
    }

    fn test_task(id: &str) -> BackgroundAgent {
        BackgroundAgent::new(
            id.to_string(),
            "wf-task".to_string(),
            "default".to_string(),
            TaskSchedule::default(),
        )
    }

    #[tokio::test]
    async fn test_execute_linear_three_phase_workflow() {
        let mut responses = HashMap::new();
        responses.insert(
            "research".to_string(),
            VecDeque::from([Ok("R".to_string())]),
        );
        responses.insert("draft".to_string(), VecDeque::from([Ok("D".to_string())]));
        responses.insert("review".to_string(), VecDeque::from([Ok("V".to_string())]));
        let runner = Arc::new(MockPhaseRunner::new(responses));
        let executor = WorkflowExecutor::new(runner.clone(), temp_checkpoint_dir("linear"));

        let definition = WorkflowDefinition {
            phases: vec![
                WorkflowPhase {
                    name: "research".to_string(),
                    description: None,
                    skill_id: None,
                    input_template: Some("Research {{topic}}".to_string()),
                    retry_config: WorkflowRetryConfig::default(),
                    depends_on: vec![],
                    timeout_secs: None,
                },
                WorkflowPhase {
                    name: "draft".to_string(),
                    description: None,
                    skill_id: None,
                    input_template: Some("Draft {{phase_0_output}}".to_string()),
                    retry_config: WorkflowRetryConfig::default(),
                    depends_on: vec![0],
                    timeout_secs: None,
                },
                WorkflowPhase {
                    name: "review".to_string(),
                    description: None,
                    skill_id: None,
                    input_template: Some("Review {{phase_1_output}}".to_string()),
                    retry_config: WorkflowRetryConfig::default(),
                    depends_on: vec![1],
                    timeout_secs: None,
                },
            ],
        };
        let mut workflow =
            AgentWorkflow::from_definition("wf-1".to_string(), "task-1".to_string(), definition);
        let mut context = HashMap::new();
        context.insert("topic".to_string(), "durable orchestration".to_string());

        let result = executor
            .execute_workflow(&test_task("task-1"), &mut workflow, &context)
            .await
            .expect("workflow should complete");

        assert_eq!(workflow.status, WorkflowStatus::Completed);
        assert_eq!(workflow.current_phase, 3);
        assert_eq!(result.phase_outputs.get(&0), Some(&"R".to_string()));
        assert_eq!(result.phase_outputs.get(&1), Some(&"D".to_string()));
        assert_eq!(result.phase_outputs.get(&2), Some(&"V".to_string()));
        assert_eq!(runner.calls(), vec!["research", "draft", "review"]);
    }

    #[tokio::test]
    async fn test_retry_uses_second_attempt() {
        let mut responses = HashMap::new();
        responses.insert(
            "phase-a".to_string(),
            VecDeque::from([Err("temporary timeout".to_string()), Ok("ok".to_string())]),
        );
        let runner = Arc::new(MockPhaseRunner::new(responses));
        let executor = WorkflowExecutor::new(runner.clone(), temp_checkpoint_dir("retry"));
        let definition = WorkflowDefinition {
            phases: vec![WorkflowPhase {
                name: "phase-a".to_string(),
                description: None,
                skill_id: None,
                input_template: Some("run".to_string()),
                retry_config: WorkflowRetryConfig {
                    max_attempts: 2,
                    initial_backoff_ms: 1,
                    max_backoff_ms: 2,
                    backoff_multiplier: 2.0,
                    non_retryable_errors: Vec::new(),
                },
                depends_on: vec![],
                timeout_secs: None,
            }],
        };
        let mut workflow =
            AgentWorkflow::from_definition("wf-2".to_string(), "task-2".to_string(), definition);
        let context = HashMap::new();

        executor
            .execute_workflow(&test_task("task-2"), &mut workflow, &context)
            .await
            .expect("workflow should retry and succeed");

        assert_eq!(runner.calls(), vec!["phase-a", "phase-a"]);
        assert_eq!(workflow.status, WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn test_non_retryable_error_stops_retry_loop() {
        let mut responses = HashMap::new();
        responses.insert(
            "phase-a".to_string(),
            VecDeque::from([Err("fatal config mismatch".to_string())]),
        );
        let runner = Arc::new(MockPhaseRunner::new(responses));
        let executor = WorkflowExecutor::new(runner.clone(), temp_checkpoint_dir("non-retryable"));
        let definition = WorkflowDefinition {
            phases: vec![WorkflowPhase {
                name: "phase-a".to_string(),
                description: None,
                skill_id: None,
                input_template: Some("run".to_string()),
                retry_config: WorkflowRetryConfig {
                    max_attempts: 3,
                    initial_backoff_ms: 1,
                    max_backoff_ms: 5,
                    backoff_multiplier: 2.0,
                    non_retryable_errors: vec!["fatal".to_string()],
                },
                depends_on: vec![],
                timeout_secs: None,
            }],
        };
        let mut workflow =
            AgentWorkflow::from_definition("wf-3".to_string(), "task-3".to_string(), definition);

        let err = executor
            .execute_workflow(&test_task("task-3"), &mut workflow, &HashMap::new())
            .await
            .expect_err("workflow should stop on non-retryable error");
        assert!(err.to_string().contains("fatal"));
        assert_eq!(runner.calls(), vec!["phase-a"]);
        // FIX: Verify status is PhaseFailed
        assert!(matches!(workflow.status, WorkflowStatus::PhaseFailed { phase_idx: 0, .. }));
    }

    #[tokio::test]
    async fn test_resume_from_latest_checkpoint() {
        let mut responses = HashMap::new();
        responses.insert(
            "phase-b".to_string(),
            VecDeque::from([Ok("done".to_string())]),
        );
        let runner = Arc::new(MockPhaseRunner::new(responses));
        let executor = WorkflowExecutor::new(runner, temp_checkpoint_dir("resume"));
        let task = test_task("task-4");

        let mut workflow = AgentWorkflow::from_definition(
            "wf-4".to_string(),
            "task-4".to_string(),
            WorkflowDefinition {
                phases: vec![WorkflowPhase {
                    name: "phase-b".to_string(),
                    description: None,
                    skill_id: None,
                    input_template: Some("run".to_string()),
                    retry_config: WorkflowRetryConfig::default(),
                    depends_on: vec![],
                    timeout_secs: None,
                }],
            },
        );
        workflow.phase_outputs.insert(0, "seed".to_string());
        let checkpoint = WorkflowCheckpoint {
            workflow_id: "wf-4".to_string(),
            phase_idx: 0,
            attempt: 1,
            state: serde_json::json!({"status":"ok"}),
            phase_outputs: workflow.phase_outputs.clone(),
            created_at: chrono::Utc::now().timestamp_millis(),
        };

        executor
            .save_checkpoint(&task, &checkpoint)
            .await
            .expect("checkpoint write should succeed");

        workflow.current_phase = 0;
        workflow.phase_outputs.clear();
        let resumed = executor
            .resume_from_latest_checkpoint(&task, &mut workflow)
            .await
            .expect("resume should succeed");
        assert!(resumed);
        assert_eq!(workflow.current_phase, 1);
        assert_eq!(workflow.phase_outputs.get(&0), Some(&"seed".to_string()));
    }
    
    // FIX: New test for PhaseFailed status and failure checkpoint persistence
    #[tokio::test]
    async fn test_phase_failed_status_and_failure_checkpoint() {
        let mut responses = HashMap::new();
        responses.insert(
            "failing-phase".to_string(),
            VecDeque::from([Err("permanent failure".to_string())]),
        );
        let runner = Arc::new(MockPhaseRunner::new(responses));
        let checkpoint_dir = temp_checkpoint_dir("phase-failed");
        let executor = WorkflowExecutor::new(runner.clone(), checkpoint_dir.clone());
        
        let definition = WorkflowDefinition {
            phases: vec![WorkflowPhase {
                name: "failing-phase".to_string(),
                description: None,
                skill_id: None,
                input_template: Some("run".to_string()),
                retry_config: WorkflowRetryConfig {
                    max_attempts: 1,
                    initial_backoff_ms: 1,
                    max_backoff_ms: 1,
                    backoff_multiplier: 1.0,
                    non_retryable_errors: vec!["permanent".to_string()],
                },
                depends_on: vec![],
                timeout_secs: None,
            }],
        };
        let mut workflow = AgentWorkflow::from_definition(
            "wf-failed".to_string(),
            "task-failed".to_string(),
            definition,
        );
        let task = test_task("task-failed");

        let err = executor
            .execute_workflow(&task, &mut workflow, &HashMap::new())
            .await
            .expect_err("workflow should fail on non-retryable error");
        
        // Verify error message
        assert!(err.to_string().contains("permanent failure"));
        
        // Verify status is PhaseFailed with correct phase_idx
        match &workflow.status {
            WorkflowStatus::PhaseFailed { phase_idx, error } => {
                assert_eq!(*phase_idx, 0);
                assert!(error.contains("permanent failure"));
            }
            _ => panic!("expected PhaseFailed status, got {:?}", workflow.status),
        }
        
        // Verify failure checkpoint was persisted
        let loaded = executor
            .load_latest_checkpoint("task-failed")
            .await
            .expect("should load checkpoint");
        let checkpoint = loaded.expect("checkpoint should exist");
        assert_eq!(checkpoint.workflow_id, "wf-failed");
        assert_eq!(checkpoint.phase_idx, 0);
        // State should contain failure info
        let state = checkpoint.state.as_object().expect("state should be object");
        assert_eq!(state.get("status").and_then(|v| v.as_str()), Some("failed"));
    }
    
    // FIX: New test for timeout enforcement
    #[tokio::test]
    async fn test_timeout_enforcement() {
        use std::sync::atomic::{AtomicU32, Ordering};
        
        struct SlowRunner {
            calls: AtomicU32,
        }
        
        #[async_trait]
        impl WorkflowPhaseRunner for SlowRunner {
            async fn run_phase(
                &self,
                _task: &BackgroundAgent,
                _phase: &WorkflowPhase,
                _input: String,
            ) -> Result<String> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                // Sleep longer than timeout (timeout is 1s, we sleep 10s)
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok("never reached".to_string())
            }
        }
        
        let runner = Arc::new(SlowRunner { calls: AtomicU32::new(0) });
        let checkpoint_dir = temp_checkpoint_dir("timeout");
        let executor = WorkflowExecutor::new(runner.clone(), checkpoint_dir);
        
        let definition = WorkflowDefinition {
            phases: vec![WorkflowPhase {
                name: "slow-phase".to_string(),
                description: None,
                skill_id: None,
                input_template: Some("run".to_string()),
                retry_config: WorkflowRetryConfig {
                    max_attempts: 1,
                    initial_backoff_ms: 1,
                    max_backoff_ms: 1,
                    backoff_multiplier: 1.0,
                    non_retryable_errors: Vec::new(),
                },
                depends_on: vec![],
                timeout_secs: Some(1), // 1 second timeout
            }],
        };
        let mut workflow = AgentWorkflow::from_definition(
            "wf-timeout".to_string(),
            "task-timeout".to_string(),
            definition,
        );
        let task = test_task("task-timeout");

        let err = executor
            .execute_workflow(&task, &mut workflow, &HashMap::new())
            .await
            .expect_err("workflow should fail on timeout");
        
        // Verify timeout error message
        assert!(err.to_string().contains("timed out"));
        
        // Verify status is PhaseFailed
        match &workflow.status {
            WorkflowStatus::PhaseFailed { phase_idx, error } => {
                assert_eq!(*phase_idx, 0);
                assert!(error.contains("timed out"));
            }
            _ => panic!("expected PhaseFailed status, got {:?}", workflow.status),
        }
        
        // Verify runner was called exactly once (no retries since max_attempts=1)
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    }
    
    // FIX: New test for timeout with retry
    #[tokio::test]
    async fn test_timeout_with_retry() {
        use std::sync::atomic::{AtomicU32, Ordering};
        
        struct SlowThenFastRunner {
            calls: AtomicU32,
        }
        
        #[async_trait]
        impl WorkflowPhaseRunner for SlowThenFastRunner {
            async fn run_phase(
                &self,
                _task: &BackgroundAgent,
                _phase: &WorkflowPhase,
                _input: String,
            ) -> Result<String> {
                let call = self.calls.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    // First call: timeout (sleep longer than timeout)
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Ok("never reached".to_string())
                } else {
                    // Second call: succeed quickly
                    Ok("success".to_string())
                }
            }
        }
        
        let runner = Arc::new(SlowThenFastRunner { calls: AtomicU32::new(0) });
        let checkpoint_dir = temp_checkpoint_dir("timeout-retry");
        let executor = WorkflowExecutor::new(runner.clone(), checkpoint_dir);
        
        let definition = WorkflowDefinition {
            phases: vec![WorkflowPhase {
                name: "phase-with-timeout".to_string(),
                description: None,
                skill_id: None,
                input_template: Some("run".to_string()),
                retry_config: WorkflowRetryConfig {
                    max_attempts: 2,
                    initial_backoff_ms: 1,
                    max_backoff_ms: 10,
                    backoff_multiplier: 1.0,
                    non_retryable_errors: Vec::new(),
                },
                depends_on: vec![],
                timeout_secs: Some(1), // 1 second timeout
            }],
        };
        let mut workflow = AgentWorkflow::from_definition(
            "wf-timeout-retry".to_string(),
            "task-timeout-retry".to_string(),
            definition,
        );
        let task = test_task("task-timeout-retry");

        let result = executor
            .execute_workflow(&task, &mut workflow, &HashMap::new())
            .await
            .expect("workflow should succeed after retry");
        
        // Verify workflow completed
        assert_eq!(workflow.status, WorkflowStatus::Completed);
        assert_eq!(result.phase_outputs.get(&0), Some(&"success".to_string()));
        
        // Verify runner was called twice (first timeout, second success)
        assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
    }
}
