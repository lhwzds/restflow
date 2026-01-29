//! Agent Task Runner - Background scheduler for agent tasks.
//!
//! The AgentTaskRunner is responsible for:
//! - Polling storage for runnable tasks
//! - Executing agents on schedule
//! - Handling task lifecycle (start, complete, fail)
//! - Sending notifications on completion/failure

use anyhow::{anyhow, Result};
use restflow_core::models::{AgentTask, AgentTaskStatus, ExecutionMode, NotificationConfig};
use restflow_core::storage::AgentTaskStorage;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use super::cli_executor::CliExecutor;
use super::pty_cli_executor::PtyCliExecutor;

/// Message types for controlling the runner
#[derive(Debug)]
pub enum RunnerCommand {
    /// Stop the runner
    Stop,
    /// Trigger immediate check for runnable tasks
    CheckNow,
    /// Run a specific task immediately (bypassing schedule)
    RunTaskNow(String),
}

/// Configuration for the AgentTaskRunner
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// How often to poll for runnable tasks (in milliseconds)
    pub poll_interval_ms: u64,
    /// Maximum concurrent task executions
    pub max_concurrent_tasks: usize,
    /// Timeout for individual task execution (in seconds)
    pub task_timeout_secs: u64,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 10_000, // 10 seconds
            max_concurrent_tasks: 5,
            task_timeout_secs: 300, // 5 minutes
        }
    }
}

/// Handle to control a running AgentTaskRunner
pub struct RunnerHandle {
    command_tx: mpsc::Sender<RunnerCommand>,
}

impl RunnerHandle {
    /// Stop the runner
    pub async fn stop(&self) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::Stop)
            .await
            .map_err(|e| anyhow!("Failed to send stop command: {}", e))
    }

    /// Trigger an immediate check for runnable tasks
    pub async fn check_now(&self) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::CheckNow)
            .await
            .map_err(|e| anyhow!("Failed to send check command: {}", e))
    }

    /// Run a specific task immediately
    pub async fn run_task_now(&self, task_id: String) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::RunTaskNow(task_id))
            .await
            .map_err(|e| anyhow!("Failed to send run task command: {}", e))
    }
}

/// Agent executor trait for dependency injection
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute an agent with the given input
    async fn execute(&self, agent_id: &str, input: Option<&str>) -> Result<String>;
}

/// Notification sender trait for dependency injection
#[async_trait::async_trait]
pub trait NotificationSender: Send + Sync {
    /// Send a notification with the given configuration
    async fn send(
        &self,
        config: &NotificationConfig,
        task: &AgentTask,
        success: bool,
        message: &str,
    ) -> Result<()>;
}

/// The main AgentTaskRunner that schedules and executes agent tasks
pub struct AgentTaskRunner {
    storage: Arc<AgentTaskStorage>,
    executor: Arc<dyn AgentExecutor>,
    notifier: Arc<dyn NotificationSender>,
    config: RunnerConfig,
    running_tasks: Arc<RwLock<HashSet<String>>>,
}

impl AgentTaskRunner {
    /// Create a new AgentTaskRunner
    pub fn new(
        storage: Arc<AgentTaskStorage>,
        executor: Arc<dyn AgentExecutor>,
        notifier: Arc<dyn NotificationSender>,
        config: RunnerConfig,
    ) -> Self {
        Self {
            storage,
            executor,
            notifier,
            config,
            running_tasks: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Start the runner and return a handle for controlling it
    pub fn start(self: Arc<Self>) -> RunnerHandle {
        let (command_tx, command_rx) = mpsc::channel(32);
        let runner = self.clone();

        tokio::spawn(async move {
            runner.run_loop(command_rx).await;
        });

        RunnerHandle { command_tx }
    }

    /// Main run loop
    async fn run_loop(self: Arc<Self>, mut command_rx: mpsc::Receiver<RunnerCommand>) {
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));
        
        info!(
            "AgentTaskRunner started (poll_interval={}ms, max_concurrent={})",
            self.config.poll_interval_ms, self.config.max_concurrent_tasks
        );

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    self.check_and_run_tasks().await;
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(RunnerCommand::Stop) => {
                            info!("AgentTaskRunner stopping...");
                            break;
                        }
                        Some(RunnerCommand::CheckNow) => {
                            debug!("Manual check triggered");
                            self.check_and_run_tasks().await;
                        }
                        Some(RunnerCommand::RunTaskNow(task_id)) => {
                            debug!("Manual run triggered for task: {}", task_id);
                            self.run_task_immediate(&task_id).await;
                        }
                        None => {
                            info!("Command channel closed, stopping runner");
                            break;
                        }
                    }
                }
            }
        }

        info!("AgentTaskRunner stopped");
    }

    /// Check for runnable tasks and execute them
    async fn check_and_run_tasks(&self) {
        let current_time = chrono::Utc::now().timestamp_millis();

        let runnable_tasks = match self.storage.list_runnable_tasks(current_time) {
            Ok(tasks) => tasks,
            Err(e) => {
                error!("Failed to list runnable tasks: {}", e);
                return;
            }
        };

        if runnable_tasks.is_empty() {
            debug!("No runnable tasks found");
            return;
        }

        debug!("Found {} runnable tasks", runnable_tasks.len());

        // Check concurrency limit
        let running_count = self.running_tasks.read().await.len();
        let available_slots = self.config.max_concurrent_tasks.saturating_sub(running_count);

        if available_slots == 0 {
            debug!(
                "Max concurrent tasks ({}) reached, skipping this cycle",
                self.config.max_concurrent_tasks
            );
            return;
        }

        // Execute tasks up to available slots
        for task in runnable_tasks.into_iter().take(available_slots) {
            // Skip if already running
            if self.running_tasks.read().await.contains(&task.id) {
                continue;
            }

            // Add to running set BEFORE spawning to prevent race conditions
            // where the next poll cycle picks up the same task
            self.running_tasks.write().await.insert(task.id.clone());

            let runner = Arc::new(self.clone_for_task());
            let task_id = task.id.clone();

            tokio::spawn(async move {
                runner.execute_task(&task_id).await;
            });
        }
    }

    /// Run a task immediately, bypassing schedule check
    async fn run_task_immediate(&self, task_id: &str) {
        // Check if already running
        if self.running_tasks.read().await.contains(task_id) {
            warn!("Task {} is already running", task_id);
            return;
        }

        // Check concurrency limit
        let running_count = self.running_tasks.read().await.len();
        if running_count >= self.config.max_concurrent_tasks {
            warn!(
                "Cannot run task {} - max concurrent tasks ({}) reached",
                task_id, self.config.max_concurrent_tasks
            );
            return;
        }

        // Verify task exists and is not paused/completed
        match self.storage.get_task(task_id) {
            Ok(Some(task)) => {
                if task.status == AgentTaskStatus::Paused {
                    warn!("Cannot run paused task {}", task_id);
                    return;
                }
                if task.status == AgentTaskStatus::Completed {
                    warn!("Cannot run completed task {}", task_id);
                    return;
                }
            }
            Ok(None) => {
                warn!("Task {} not found", task_id);
                return;
            }
            Err(e) => {
                error!("Failed to get task {}: {}", task_id, e);
                return;
            }
        }

        // Add to running set BEFORE spawning to prevent race conditions
        self.running_tasks.write().await.insert(task_id.to_string());

        let runner = Arc::new(self.clone_for_task());
        let task_id = task_id.to_string();

        tokio::spawn(async move {
            runner.execute_task(&task_id).await;
        });
    }

    /// Execute a single task
    /// Note: Task must already be in running_tasks before calling this
    async fn execute_task(&self, task_id: &str) {

        let start_time = chrono::Utc::now().timestamp_millis();

        // Start execution in storage
        let task = match self.storage.start_task_execution(task_id) {
            Ok(task) => task,
            Err(e) => {
                error!("Failed to start task execution for {}: {}", task_id, e);
                self.running_tasks.write().await.remove(task_id);
                return;
            }
        };

        info!(
            "Executing task '{}' (id={}, agent={}, mode={:?})",
            task.name, task.id, task.agent_id, task.execution_mode
        );

        // Execute the agent based on execution mode
        let result = match &task.execution_mode {
            ExecutionMode::Api => {
                // Use the injected API executor
                debug!("Using API executor for task '{}'", task.name);
                let timeout = Duration::from_secs(self.config.task_timeout_secs);
                tokio::time::timeout(
                    timeout,
                    self.executor.execute(&task.agent_id, task.input.as_deref()),
                )
                .await
            }
            ExecutionMode::Cli(cli_config) => {
                // Use CLI executor based on use_pty flag
                debug!(
                    "Using {} executor for task '{}' (binary: {})",
                    if cli_config.use_pty { "PTY CLI" } else { "CLI" },
                    task.name,
                    cli_config.binary
                );
                let timeout = Duration::from_secs(cli_config.timeout_secs);

                if cli_config.use_pty {
                    // Use PTY-based executor for interactive CLIs
                    let executor = PtyCliExecutor::new(cli_config.clone());
                    tokio::time::timeout(
                        timeout,
                        executor.execute(&task.agent_id, task.input.as_deref()),
                    )
                    .await
                } else {
                    // Use standard CLI executor
                    let executor = CliExecutor::new(cli_config.clone());
                    tokio::time::timeout(
                        timeout,
                        executor.execute(&task.agent_id, task.input.as_deref()),
                    )
                    .await
                }
            }
        };

        let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;

        match result {
            Ok(Ok(output)) => {
                // Success
                info!(
                    "Task '{}' completed successfully (duration={}ms)",
                    task.name, duration_ms
                );

                if let Err(e) = self.storage.complete_task_execution(
                    task_id,
                    Some(output.clone()),
                    duration_ms,
                ) {
                    error!("Failed to record task completion: {}", e);
                }

                // Send notification if configured
                self.send_notification(&task, true, &output).await;
            }
            Ok(Err(e)) => {
                // Execution error
                let error_msg = format!("Execution error: {}", e);
                error!("Task '{}' failed: {}", task.name, error_msg);

                if let Err(e) = self
                    .storage
                    .fail_task_execution(task_id, error_msg.clone(), duration_ms)
                {
                    error!("Failed to record task failure: {}", e);
                }

                // Send failure notification
                self.send_notification(&task, false, &error_msg).await;
            }
            Err(_) => {
                // Timeout
                let error_msg = format!("Task timed out after {} seconds", self.config.task_timeout_secs);
                error!("Task '{}' timed out", task.name);

                if let Err(e) = self
                    .storage
                    .fail_task_execution(task_id, error_msg.clone(), duration_ms)
                {
                    error!("Failed to record task timeout: {}", e);
                }

                // Send timeout notification
                self.send_notification(&task, false, &error_msg).await;
            }
        }

        // Remove from running set
        self.running_tasks.write().await.remove(task_id);
    }

    /// Send notification for task completion/failure
    async fn send_notification(&self, task: &AgentTask, success: bool, message: &str) {
        // Check if notifications are enabled
        if !task.notification.telegram_enabled {
            return;
        }

        // Check if we should only notify on failure
        if success && task.notification.notify_on_failure_only {
            return;
        }

        let notification_message = if task.notification.include_output {
            message.to_string()
        } else if success {
            "Task completed successfully".to_string()
        } else {
            "Task failed".to_string()
        };

        match self
            .notifier
            .send(&task.notification, task, success, &notification_message)
            .await
        {
            Ok(()) => {
                if let Err(e) = self.storage.record_notification_sent(
                    &task.id,
                    format!("Notification sent: {}", if success { "success" } else { "failure" }),
                ) {
                    warn!("Failed to record notification sent event: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to send notification for task {}: {}", task.id, e);
                if let Err(e) = self
                    .storage
                    .record_notification_failed(&task.id, e.to_string())
                {
                    warn!("Failed to record notification failure event: {}", e);
                }
            }
        }
    }

    /// Create a clone for use in spawned tasks
    fn clone_for_task(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            executor: self.executor.clone(),
            notifier: self.notifier.clone(),
            config: self.config.clone(),
            running_tasks: self.running_tasks.clone(),
        }
    }

    /// Get the number of currently running tasks
    pub async fn running_task_count(&self) -> usize {
        self.running_tasks.read().await.len()
    }

    /// Get the IDs of currently running tasks
    pub async fn running_task_ids(&self) -> Vec<String> {
        self.running_tasks.read().await.iter().cloned().collect()
    }
}

/// No-op notification sender for when notifications are not configured
pub struct NoopNotificationSender;

#[async_trait::async_trait]
impl NotificationSender for NoopNotificationSender {
    async fn send(
        &self,
        _config: &NotificationConfig,
        _task: &AgentTask,
        _success: bool,
        _message: &str,
    ) -> Result<()> {
        // No-op: notifications are handled elsewhere or disabled
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::models::TaskSchedule;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::tempdir;

    /// Mock executor for testing
    struct MockExecutor {
        call_count: AtomicU32,
        should_fail: bool,
        delay_ms: u64,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
                should_fail: false,
                delay_ms: 0,
            }
        }

        fn with_failure() -> Self {
            Self {
                call_count: AtomicU32::new(0),
                should_fail: true,
                delay_ms: 0,
            }
        }

        fn with_delay(delay_ms: u64) -> Self {
            Self {
                call_count: AtomicU32::new(0),
                should_fail: false,
                delay_ms,
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl AgentExecutor for MockExecutor {
        async fn execute(&self, agent_id: &str, input: Option<&str>) -> Result<String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }

            if self.should_fail {
                Err(anyhow!("Mock execution failure"))
            } else {
                Ok(format!(
                    "Executed agent {} with input: {:?}",
                    agent_id, input
                ))
            }
        }
    }

    /// Mock notifier for testing
    struct MockNotifier {
        notifications: Arc<RwLock<Vec<(String, bool)>>>,
    }

    impl MockNotifier {
        fn new() -> Self {
            Self {
                notifications: Arc::new(RwLock::new(Vec::new())),
            }
        }

        async fn notification_count(&self) -> usize {
            self.notifications.read().await.len()
        }
    }

    #[async_trait::async_trait]
    impl NotificationSender for MockNotifier {
        async fn send(
            &self,
            _config: &NotificationConfig,
            task: &AgentTask,
            success: bool,
            _message: &str,
        ) -> Result<()> {
            self.notifications
                .write()
                .await
                .push((task.id.clone(), success));
            Ok(())
        }
    }

    /// Creates test storage and returns it along with the TempDir.
    /// The TempDir must be kept alive for the duration of the test to prevent
    /// the database from being deleted (important on Windows).
    fn create_test_storage() -> (Arc<AgentTaskStorage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        (Arc::new(AgentTaskStorage::new(db).unwrap()), temp_dir)
    }

    #[tokio::test]
    async fn test_runner_start_stop() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            executor,
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Stop it
        handle.stop().await.unwrap();

        // Give it time to stop
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_runner_executes_runnable_task() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(MockNotifier::new());

        // Create a task that should run immediately
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();

        // Update next_run_at to be in the past
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        // Wait for execution
        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Verify execution happened
        assert_eq!(executor.call_count(), 1);

        // Verify task status updated
        let updated_task = storage.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated_task.status, AgentTaskStatus::Completed);
        assert_eq!(updated_task.success_count, 1);
    }

    #[tokio::test]
    async fn test_runner_handles_failure() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_failure());
        let notifier = Arc::new(MockNotifier::new());

        // Create a task that should run immediately
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Failing Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();

        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage.clone(),
            executor,
            notifier.clone(),
            config,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Verify task failed
        let updated_task = storage.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated_task.status, AgentTaskStatus::Failed);
        assert_eq!(updated_task.failure_count, 1);
        assert!(updated_task.last_error.is_some());
    }

    #[tokio::test]
    async fn test_runner_respects_concurrency_limit() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_delay(500)); // 500ms delay
        let notifier = Arc::new(NoopNotificationSender);

        // Create multiple tasks
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        for i in 0..5 {
            let mut task = storage
                .create_task(
                    format!("Task {}", i),
                    "agent-001".to_string(),
                    TaskSchedule::Once { run_at: past_time },
                )
                .unwrap();
            task.next_run_at = Some(past_time);
            storage.update_task(&task).unwrap();
        }

        let config = RunnerConfig {
            poll_interval_ms: 50,
            max_concurrent_tasks: 2, // Only 2 at a time
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            executor.clone(),
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        // Check running count shortly after start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let running = runner.running_task_count().await;
        assert!(running <= 2, "Should respect concurrency limit");

        // Wait for all to complete (5 tasks * 500ms each / 2 concurrent = 1250ms min)
        // Add generous buffer for Windows CI
        tokio::time::sleep(Duration::from_millis(4000)).await;

        handle.stop().await.unwrap();

        // All tasks should have run eventually
        assert_eq!(executor.call_count(), 5);
    }

    #[tokio::test]
    async fn test_runner_check_now() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 10000, // Very long poll interval
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        // Create a runnable task
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        // Without check_now, it wouldn't run for 10 seconds
        // Trigger immediate check
        handle.check_now().await.unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        handle.stop().await.unwrap();

        // Should have executed despite long poll interval
        assert_eq!(executor.call_count(), 1);
    }

    #[tokio::test]
    async fn test_runner_run_task_now() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 10000, // Very long poll interval
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        // Create a task with future run time (shouldn't run automatically)
        let future_time = chrono::Utc::now().timestamp_millis() + 3600000;
        let task = storage
            .create_task(
                "Future Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: future_time },
            )
            .unwrap();

        // Run it immediately
        handle.run_task_now(task.id.clone()).await.unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        handle.stop().await.unwrap();

        // Should have executed despite future schedule
        assert_eq!(executor.call_count(), 1);
    }

    #[tokio::test]
    async fn test_runner_skips_paused_tasks() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        // Create a runnable task and pause it
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Paused Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();
        storage.pause_task(&task.id).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            executor.clone(),
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(300)).await;

        handle.stop().await.unwrap();

        // Should not have executed paused task
        assert_eq!(executor.call_count(), 0);
    }

    #[tokio::test]
    async fn test_runner_sends_notifications() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(MockNotifier::new());

        // Create a task with notifications enabled
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Notified Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        task.notification.telegram_enabled = true;
        task.notification.telegram_chat_id = Some("123456".to_string());
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            executor,
            notifier.clone(),
            config,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Should have sent notification
        assert_eq!(notifier.notification_count().await, 1);
    }

    #[tokio::test]
    async fn test_runner_notify_on_failure_only() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new()); // This succeeds
        let notifier = Arc::new(MockNotifier::new());

        // Create a task with notify_on_failure_only
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Success No Notify".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        task.notification.telegram_enabled = true;
        task.notification.telegram_chat_id = Some("123456".to_string());
        task.notification.notify_on_failure_only = true;
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            executor,
            notifier.clone(),
            config,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Should NOT have sent notification (success with notify_on_failure_only)
        assert_eq!(notifier.notification_count().await, 0);
    }

    #[tokio::test]
    async fn test_running_task_tracking() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_delay(500));
        let notifier = Arc::new(NoopNotificationSender);

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Slow Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            executor,
            notifier,
            config,
        ));

        let handle = runner.clone().start();

        // Wait for task to start (allow extra time for Windows CI)
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should show as running
        let running_ids = runner.running_task_ids().await;
        assert_eq!(running_ids.len(), 1);
        assert_eq!(running_ids[0], task.id);

        // Wait for completion (500ms task + generous buffer for Windows CI)
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Should no longer be running
        assert_eq!(runner.running_task_count().await, 0);

        handle.stop().await.unwrap();
    }
}
