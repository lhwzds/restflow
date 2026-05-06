use super::*;
use crate::models::{
    ChatSession, Task, TaskControlAction, TaskEventType, TaskSchedule, TaskStatus,
};
use crate::runtime::task_runtime::{ChannelEventEmitter, StreamEventKind};
use crate::services::session::SessionService;
use crate::session_log::FileSessionStore;
use crate::storage::{ChatSessionStorage, ExecutionTraceStorage, SessionStorage};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;
use tempfile::tempdir;

/// Mock executor for testing
struct MockExecutor {
    call_count: AtomicU32,
    resume_call_count: AtomicU32,
    should_fail: bool,
    delay_ms: u64,
    saw_emitter: AtomicBool,
}

impl MockExecutor {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
            resume_call_count: AtomicU32::new(0),
            should_fail: false,
            delay_ms: 0,
            saw_emitter: AtomicBool::new(false),
        }
    }

    fn with_failure() -> Self {
        Self {
            call_count: AtomicU32::new(0),
            resume_call_count: AtomicU32::new(0),
            should_fail: true,
            delay_ms: 0,
            saw_emitter: AtomicBool::new(false),
        }
    }

    fn with_delay(delay_ms: u64) -> Self {
        Self {
            call_count: AtomicU32::new(0),
            resume_call_count: AtomicU32::new(0),
            should_fail: false,
            delay_ms,
            saw_emitter: AtomicBool::new(false),
        }
    }

    fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

struct FailsOnceExecutor {
    call_count: AtomicU32,
    saw_emitter: AtomicBool,
}

impl FailsOnceExecutor {
    fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
            saw_emitter: AtomicBool::new(false),
        }
    }

    fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AgentExecutor for MockExecutor {
    async fn execute(
        &self,
        agent_id: &str,
        _task_id: Option<&str>,
        input: Option<&str>,
        _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self.should_fail {
            Err(anyhow!("Mock execution failure"))
        } else {
            let output = format!("Executed agent {} with input: {:?}", agent_id, input);
            // Return empty messages for mock - real executor would return actual conversation
            Ok(ExecutionResult::success(output, Vec::new()))
        }
    }

    async fn execute_with_emitter(
        &self,
        agent_id: &str,
        task_id: Option<&str>,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        if emitter.is_some() {
            self.saw_emitter.store(true, Ordering::SeqCst);
        }
        self.execute(agent_id, task_id, input, steer_rx).await
    }

    async fn execute_from_state(
        &self,
        _agent_id: &str,
        _task_id: Option<&str>,
        state: restflow_ai::AgentState,
        _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        _emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.resume_call_count.fetch_add(1, Ordering::SeqCst);
        Ok(ExecutionResult::success(
            format!("Resumed execution {}", state.execution_id),
            state.messages,
        ))
    }
}

#[async_trait::async_trait]
impl AgentExecutor for FailsOnceExecutor {
    async fn execute(
        &self,
        agent_id: &str,
        _task_id: Option<&str>,
        input: Option<&str>,
        _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            Err(anyhow!("Mock execution failure"))
        } else {
            Ok(ExecutionResult::success(
                format!("Executed agent {} with input: {:?}", agent_id, input),
                Vec::new(),
            ))
        }
    }

    async fn execute_with_emitter(
        &self,
        agent_id: &str,
        task_id: Option<&str>,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        if emitter.is_some() {
            self.saw_emitter.store(true, Ordering::SeqCst);
        }
        self.execute(agent_id, task_id, input, steer_rx).await
    }

    async fn execute_from_state(
        &self,
        _agent_id: &str,
        _task_id: Option<&str>,
        state: restflow_ai::AgentState,
        _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        _emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        Ok(ExecutionResult::success(
            format!("Resumed execution {}", state.execution_id),
            state.messages,
        ))
    }
}

/// Mock notifier for testing
struct MockNotifier {
    notifications: Arc<RwLock<Vec<(String, bool, String)>>>,
}

impl MockNotifier {
    fn new() -> Self {
        Self {
            notifications: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl NotificationSender for MockNotifier {
    async fn send(&self, task: &Task, success: bool, message: &str) -> Result<()> {
        self.notifications
            .write()
            .await
            .push((task.id.clone(), success, message.to_string()));
        Ok(())
    }

    async fn send_formatted(&self, message: &str) -> Result<()> {
        self.notifications
            .write()
            .await
            .push(("formatted".to_string(), true, message.to_string()));
        Ok(())
    }
}

struct DefaultDelegatingExecutor {
    call_count: AtomicU32,
}

#[async_trait::async_trait]
impl AgentExecutor for DefaultDelegatingExecutor {
    async fn execute(
        &self,
        _agent_id: &str,
        _task_id: Option<&str>,
        _input: Option<&str>,
        _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(ExecutionResult::success("ok".to_string(), Vec::new()))
    }
}

/// Creates test storage and returns it along with the TempDir.
/// The TempDir must be kept alive for the duration of the test to prevent
/// the database from being deleted (important on Windows).
fn create_test_storage() -> (Arc<TaskStorage>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(redb::Database::create(db_path).unwrap());
    (Arc::new(TaskStorage::new(db).unwrap()), temp_dir)
}

#[test]
fn persist_to_chat_session_uses_session_service_when_available() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(redb::Database::create(db_path).unwrap());
    let task_storage = Arc::new(TaskStorage::new(db.clone()).unwrap());
    let chat_storage = ChatSessionStorage::new(db.clone()).unwrap();
    let session_storage = SessionStorage::new(
        chat_storage.clone(),
        ExecutionTraceStorage::new(db.clone()).unwrap(),
    );
    let file_store = FileSessionStore::new(temp_dir.path().join("sessions")).unwrap();
    let session_service = SessionService::new(session_storage, None, task_storage.as_ref().clone())
        .with_file_sessions(file_store.clone());

    let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
    let session = session_service
        .create_external_session(session)
        .expect("create file session");
    let mut task = Task::new(
        "task-1".to_string(),
        "Task".to_string(),
        "agent-1".to_string(),
        TaskSchedule::default(),
    );
    task.chat_session_id = session.id.clone();

    let runner = TaskRunner::new(
        task_storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    )
    .with_session_service(session_service);
    runner.persist_to_chat_session(&task, Some("input"), "output", false, 10);

    let file_session = file_store
        .get(&session.id)
        .unwrap()
        .expect("updated file session")
        .to_chat_session();
    assert_eq!(file_session.messages.len(), 2);
    assert_eq!(file_session.messages[0].content, "input");
    assert_eq!(file_session.messages[1].content, "output");
    assert!(chat_storage.get(&session.id).unwrap().is_none());
}

#[tokio::test]
async fn test_take_stop_receiver_returns_none_when_missing() {
    let (storage, _temp_dir) = create_test_storage();
    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    let result = runner.take_stop_receiver("nonexistent-task").await;
    assert!(result.is_none());
}

#[test]
fn test_runner_config_defaults() {
    let config = TaskRunnerConfig::default();
    assert_eq!(
        config.poll_interval_ms,
        DEFAULT_TASK_RUNNER_POLL_INTERVAL_MS
    );
    assert_eq!(
        config.max_concurrent_tasks,
        DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS
    );
    assert_eq!(
        config.worker_count,
        DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS
    );
    assert_eq!(config.task_timeout_secs, None);
    assert_eq!(config.stall_timeout_secs, None);
}

#[tokio::test]
async fn test_recover_stalled_running_tasks_resets_untracked_tasks() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);
    let (channel_emitter, mut event_rx) = ChannelEventEmitter::new();
    let current_time = chrono::Utc::now().timestamp_millis();
    let past_time = current_time - 120_000;

    let mut task = storage
        .create_task(
            "Stalled Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::default(),
        )
        .unwrap();
    task.status = TaskStatus::Running;
    task.updated_at = past_time;
    storage.update_task(&task).unwrap();
    storage
        .start_task_run(&task.id, "run-stalled", "exec-stalled", past_time)
        .unwrap();

    let runner = TaskRunner::new(
        storage.clone(),
        executor,
        notifier,
        TaskRunnerConfig {
            stall_timeout_secs: Some(60),
            ..Default::default()
        },
        Arc::new(SteerRegistry::new()),
    )
    .with_event_emitter(Arc::new(channel_emitter));

    runner.recover_stalled_running_tasks(current_time).await;

    let recovered_task = storage.get_task(&task.id).unwrap().unwrap();
    assert_eq!(recovered_task.status, TaskStatus::Active);

    let run = storage.get_task_run("run-stalled").unwrap().unwrap();
    assert_eq!(run.status, crate::models::TaskRunStatus::Interrupted);
    assert_eq!(
        run.error.as_deref(),
        Some("Recovered stalled task execution")
    );
    let mut interrupted_seen = false;
    while let Ok(event) = event_rx.try_recv() {
        if let StreamEventKind::Interrupted { reason, .. } = event.kind {
            interrupted_seen = reason == "Recovered stalled task execution";
        }
    }
    assert!(interrupted_seen);
}

#[tokio::test]
async fn test_take_stop_receiver_returns_receiver_when_present() {
    let (storage, _temp_dir) = create_test_storage();
    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    let (tx, rx) = oneshot::channel();
    runner
        .pending_stop_receivers
        .write()
        .await
        .insert("task-1".to_string(), rx);

    let mut result = runner.take_stop_receiver("task-1").await;
    assert!(result.is_some());
    assert!(
        !runner
            .pending_stop_receivers
            .read()
            .await
            .contains_key("task-1")
    );

    tx.send(()).unwrap();
    assert!(result.as_mut().unwrap().await.is_ok());
}

#[tokio::test]
async fn test_runner_start_stop() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage,
        executor,
        notifier,
        config,
        steer_registry,
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
    task.input = Some("Test task input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    // Wait for execution
    tokio::time::sleep(Duration::from_millis(500)).await;

    handle.stop().await.unwrap();

    // Verify execution happened
    assert_eq!(executor.call_count(), 1);

    // Verify task status updated
    let updated_task = storage.get_task(&task.id).unwrap().unwrap();
    assert_eq!(updated_task.status, TaskStatus::Completed);
    assert_eq!(updated_task.success_count, 1);
}

#[tokio::test]
async fn test_runner_emits_stream_events_with_custom_emitter() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);
    let (channel_emitter, mut event_rx) = ChannelEventEmitter::new();

    let past_time = chrono::Utc::now().timestamp_millis() - 1000;
    let mut task = storage
        .create_task(
            "Event Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once { run_at: past_time },
        )
        .unwrap();
    task.input = Some("Event task input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(
        TaskRunner::new(storage, executor, notifier, config, steer_registry)
            .with_event_emitter(Arc::new(channel_emitter)),
    );

    let handle = runner.clone().start();
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle.stop().await.unwrap();

    let mut started_seen = false;
    let mut completed_seen = false;
    while let Ok(event) = event_rx.try_recv() {
        if let StreamEventKind::Started { .. } = event.kind {
            started_seen = true;
        }
        if let StreamEventKind::Completed { .. } = event.kind {
            completed_seen = true;
        }
    }

    assert!(started_seen);
    assert!(completed_seen);
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

    task.input = Some("Failing task input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor,
        notifier.clone(),
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    tokio::time::sleep(Duration::from_millis(500)).await;

    handle.stop().await.unwrap();

    // Verify task failed
    let updated_task = storage.get_task(&task.id).unwrap().unwrap();
    assert_eq!(updated_task.status, TaskStatus::Failed);
    assert_eq!(updated_task.failure_count, 1);
    assert!(updated_task.last_error.is_some());
}

#[tokio::test]
async fn test_runner_reschedules_interval_task_after_failure() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(FailsOnceExecutor::new());
    let notifier = Arc::new(MockNotifier::new());

    let past_time = chrono::Utc::now().timestamp_millis() - 1_000;
    let mut task = storage
        .create_task(
            "Retrying Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Interval {
                interval_ms: 100,
                start_at: Some(past_time),
            },
        )
        .unwrap();

    task.input = Some("Retry this task".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 50,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let updated_task = storage.get_task(&task.id).unwrap().unwrap();
        if executor.call_count() >= 2 && updated_task.success_count >= 1 {
            assert_eq!(updated_task.failure_count, 1);
            assert_eq!(updated_task.status, TaskStatus::Active);
            assert!(updated_task.next_run_at.is_some());
            break;
        }

        if Instant::now() >= deadline {
            panic!(
                "interval task was not retried after failure; call_count={}, task={:?}",
                executor.call_count(),
                storage.get_task(&task.id).unwrap()
            );
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    handle.stop().await.unwrap();
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
        task.input = Some(format!("Concurrent task input {}", i));
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();
    }

    let config = TaskRunnerConfig {
        poll_interval_ms: 50,
        max_concurrent_tasks: 2, // Only 2 at a time
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    // Check running count shortly after start
    tokio::time::sleep(Duration::from_millis(100)).await;
    let running = runner.running_task_count().await;
    assert!(running <= 2, "Should respect concurrency limit");

    // Keep sampling while the first batch runs so we assert the limit over time
    // without depending on the full scheduler throughput under heavy suite load.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut max_running = running;
    loop {
        let current_running = runner.running_task_count().await;
        max_running = max_running.max(current_running);

        if executor.call_count() >= 2 {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "concurrency test did not make progress; call_count={}, running={}",
                executor.call_count(),
                current_running
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    handle.stop().await.unwrap();

    assert!(max_running <= 2, "Should never exceed concurrency limit");
    assert!(
        executor.call_count() >= 2,
        "Runner should execute at least one batch"
    );
}

#[tokio::test]
async fn test_runner_check_now() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10000, // Very long poll interval
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
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
    task.input = Some("Immediate check input".to_string());
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

    let config = TaskRunnerConfig {
        poll_interval_ms: 10000, // Very long poll interval
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    // Create a task with future run time (shouldn't run automatically)
    let future_time = chrono::Utc::now().timestamp_millis() + 3600000;
    let mut task = storage
        .create_task(
            "Future Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once {
                run_at: future_time,
            },
        )
        .unwrap();
    task.input = Some("Run now input".to_string());
    storage.update_task(&task).unwrap();

    // Run it immediately
    handle.run_task_now(task.id.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    handle.stop().await.unwrap();

    // Should have executed despite future schedule
    assert_eq!(executor.call_count(), 1);
}

#[tokio::test]
async fn test_runner_run_task_now_deduplicates_same_task() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::with_delay(500));
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10_000,
        max_concurrent_tasks: 4,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    let future_time = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let mut task = storage
        .create_task(
            "Dedup Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once {
                run_at: future_time,
            },
        )
        .unwrap();
    task.input = Some("dedup input".to_string());
    storage.update_task(&task).unwrap();

    // Fire duplicate run-now commands while the first execution is still running.
    let task_id = task.id.clone();
    let first = handle.run_task_now(task_id.clone());
    let second = handle.run_task_now(task_id);
    let _ = tokio::join!(first, second);

    tokio::time::sleep(Duration::from_millis(800)).await;
    handle.stop().await.unwrap();

    assert_eq!(executor.call_count(), 1);
}

#[tokio::test]
async fn test_runner_run_task_now_missing_task_does_not_leak_tracking() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10_000,
        max_concurrent_tasks: 2,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage,
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();
    handle
        .run_task_now("missing-task-id".to_string())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;
    handle.stop().await.unwrap();

    assert_eq!(executor.call_count(), 0);
    assert_eq!(runner.running_task_count().await, 0);
    assert!(runner.running_task_ids().await.is_empty());
}

#[tokio::test]
async fn test_runner_run_task_now_paused_task_does_not_leak_tracking() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10_000,
        max_concurrent_tasks: 2,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    let task = storage
        .create_task(
            "Paused Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::default(),
        )
        .unwrap();
    storage.pause_task(&task.id).unwrap();

    handle.run_task_now(task.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    handle.stop().await.unwrap();

    assert_eq!(executor.call_count(), 0);
    assert_eq!(runner.running_task_count().await, 0);
    assert!(runner.running_task_ids().await.is_empty());
}

#[tokio::test]
async fn test_runner_run_task_now_completed_task_does_not_leak_tracking() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10_000,
        max_concurrent_tasks: 2,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    let task = storage
        .create_task(
            "Completed Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::default(),
        )
        .unwrap();
    storage
        .complete_task_execution(&task.id, Some("done".to_string()), 10)
        .unwrap();

    handle.run_task_now(task.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    handle.stop().await.unwrap();

    assert_eq!(executor.call_count(), 0);
    assert_eq!(runner.running_task_count().await, 0);
    assert!(runner.running_task_ids().await.is_empty());
}

#[tokio::test]
async fn test_runner_run_task_now_respects_concurrency_guard() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::with_delay(500));
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10_000,
        max_concurrent_tasks: 1,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    let future_time = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let mut task_a = storage
        .create_task(
            "Concurrency Task A".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once {
                run_at: future_time,
            },
        )
        .unwrap();
    task_a.input = Some("A".to_string());
    storage.update_task(&task_a).unwrap();

    let mut task_b = storage
        .create_task(
            "Concurrency Task B".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once {
                run_at: future_time,
            },
        )
        .unwrap();
    task_b.input = Some("B".to_string());
    storage.update_task(&task_b).unwrap();

    handle.run_task_now(task_a.id.clone()).await.unwrap();
    handle.run_task_now(task_b.id.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(800)).await;
    handle.stop().await.unwrap();

    // Task B is intentionally dropped by run-now guard when max concurrency is reached.
    assert_eq!(executor.call_count(), 1);
}
#[tokio::test]
async fn test_execute_task_without_stop_receiver_fails_before_commit() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let (channel_emitter, mut event_rx) = ChannelEventEmitter::new();

    let runner = TaskRunner::new(
        storage.clone(),
        executor.clone(),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    )
    .with_event_emitter(Arc::new(channel_emitter));

    let mut task = storage
        .create_task(
            "Missing Stop Receiver".to_string(),
            "agent-001".to_string(),
            TaskSchedule::default(),
        )
        .unwrap();
    task.input = Some("Run me".to_string());
    storage.update_task(&task).unwrap();

    let result = runner.execute_task(&task.id, None).await;
    assert!(result.is_err());
    assert_eq!(executor.call_count(), 0);
    assert!(storage.get_active_task_run(&task.id).unwrap().is_none());
    assert_eq!(
        storage.get_task(&task.id).unwrap().unwrap().status,
        TaskStatus::Active
    );
    assert_eq!(runner.running_task_count().await, 0);

    let mut started_seen = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event.kind, StreamEventKind::Started { .. }) {
            started_seen = true;
        }
    }
    assert!(!started_seen);
}

#[tokio::test]
async fn test_runner_uses_input_template_when_running_task() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let config = TaskRunnerConfig {
        poll_interval_ms: 10_000,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    let future_time = chrono::Utc::now().timestamp_millis() + 3_600_000;
    let mut task = storage
        .create_task(
            "Template Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once {
                run_at: future_time,
            },
        )
        .unwrap();
    task.input = Some("fallback input".to_string());
    task.input_template = Some("Task {{task.id}} for {{task.name}}".to_string());
    storage.update_task(&task).unwrap();

    handle.run_task_now(task.id.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    handle.stop().await.unwrap();

    assert_eq!(executor.call_count(), 1);

    let events = storage.list_events_for_task(&task.id).unwrap();
    let completed = events
        .iter()
        .find(|event| event.event_type == TaskEventType::Completed)
        .and_then(|event| event.output.as_deref())
        .unwrap_or_default()
        .to_string();

    assert!(completed.contains(&format!("Task {} for Template Task", task.id)));
    assert!(!completed.contains("fallback input"));
}

#[tokio::test]
async fn test_runner_fails_fast_when_input_and_template_missing() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::new());
    let notifier = Arc::new(NoopNotificationSender);

    let past_time = chrono::Utc::now().timestamp_millis() - 1000;
    let mut task = storage
        .create_task(
            "Missing Input Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once { run_at: past_time },
        )
        .unwrap();
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle.stop().await.unwrap();

    assert_eq!(executor.call_count(), 0);

    let updated_task = storage.get_task(&task.id).unwrap().unwrap();
    assert_eq!(updated_task.status, TaskStatus::Failed);
    assert!(
        updated_task
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("requires non-empty input or input_template")
    );
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
    task.input = Some("Paused task input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();
    storage.pause_task(&task.id).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage,
        executor.clone(),
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    tokio::time::sleep(Duration::from_millis(300)).await;

    handle.stop().await.unwrap();

    // Should not have executed paused task
    assert_eq!(executor.call_count(), 0);
}

#[test]
fn test_render_input_template_replaces_known_placeholders() {
    let mut task = Task::new(
        "task-123".to_string(),
        "Template Unit Test".to_string(),
        "agent-456".to_string(),
        TaskSchedule::default(),
    );
    task.description = Some("description".to_string());
    task.input = Some("input".to_string());

    let rendered = TaskRunner::render_input_template(
        &task,
        "ID={{task.id}}, NAME={{task.name}}, AGENT={{task.agent_id}}, DESC={{task.description}}, INPUT={{task.input}}, NOW={{now.unix_ms}}",
    );

    assert!(rendered.contains("ID=task-123"));
    assert!(rendered.contains("NAME=Template Unit Test"));
    assert!(rendered.contains("AGENT=agent-456"));
    assert!(rendered.contains("DESC=description"));
    assert!(rendered.contains("INPUT=input"));
    assert!(!rendered.contains("{{now.unix_ms}}"));
}

#[test]
fn test_render_input_template_keeps_unknown_input_placeholder() {
    let mut task = Task::new(
        "task-123".to_string(),
        "Template Unit Test".to_string(),
        "agent-456".to_string(),
        TaskSchedule::default(),
    );
    task.input = Some("input".to_string());

    let rendered =
        TaskRunner::render_input_template(&task, "UNKNOWN={{input}}, REQUIRED={{task.input}}");

    assert!(rendered.contains("UNKNOWN={{input}}"));
    assert!(rendered.contains("REQUIRED=input"));
}

#[test]
fn test_render_input_template_no_double_substitution() {
    let task = Task::new(
        "task-123".to_string(),
        "Process {{task.id}}".to_string(), // name contains a placeholder
        "agent-456".to_string(),
        TaskSchedule::default(),
    );

    let rendered = TaskRunner::render_input_template(&task, "Name: {{task.name}}, ID: {{task.id}}");

    // Name should be literal "Process {{task.id}}", NOT "Process task-123"
    assert_eq!(rendered, "Name: Process {{task.id}}, ID: task-123");
}

#[test]
fn test_resolve_task_input_keeps_plain_input_unchanged() {
    let (storage, _temp_dir) = create_test_storage();
    let mut task = Task::new(
        "task-plain".to_string(),
        "Plain Input Task".to_string(),
        "agent-456".to_string(),
        TaskSchedule::default(),
    );
    task.input = Some("Collect latest news.".to_string());

    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    let resolved = runner
        .resolve_task_input(&task)
        .expect("resolved input should exist");

    assert_eq!(resolved, "Collect latest news.");
}

#[test]
fn test_resolve_task_input_renders_template_without_injection() {
    let (storage, _temp_dir) = create_test_storage();
    let mut task = Task::new(
        "task-template".to_string(),
        "Template Input Task".to_string(),
        "agent-789".to_string(),
        TaskSchedule::default(),
    );
    task.input = Some("fallback".to_string());
    task.input_template = Some("Template for {{task.name}}".to_string());

    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    let resolved = runner
        .resolve_task_input(&task)
        .expect("resolved input should exist");

    assert_eq!(resolved, "Template for Template Input Task");
}

#[test]
fn test_resolve_task_input_falls_back_when_template_renders_empty() {
    let (storage, _temp_dir) = create_test_storage();
    let mut task = Task::new(
        "task-template-empty".to_string(),
        "Template Empty Task".to_string(),
        "agent-789".to_string(),
        TaskSchedule::default(),
    );
    task.input = Some("fallback".to_string());
    task.input_template = Some("   ".to_string());

    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    let resolved = runner
        .resolve_task_input(&task)
        .expect("resolved input should fallback");

    assert_eq!(resolved, "fallback");
}

#[test]
fn test_resolve_task_input_returns_none_for_empty_template_without_fallback() {
    let (storage, _temp_dir) = create_test_storage();
    let mut task = Task::new(
        "task-template-empty-none".to_string(),
        "Template Empty No Fallback Task".to_string(),
        "agent-789".to_string(),
        TaskSchedule::default(),
    );
    task.input_template = Some("{{task.input}}".to_string());

    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    assert!(runner.resolve_task_input(&task).is_none());
}

#[test]
fn test_resolve_task_input_returns_none_when_no_input_or_template() {
    let (storage, _temp_dir) = create_test_storage();
    let task = Task::new(
        "task-empty".to_string(),
        "Empty Input Task".to_string(),
        "agent-000".to_string(),
        TaskSchedule::default(),
    );

    let runner = TaskRunner::new(
        storage,
        Arc::new(MockExecutor::new()),
        Arc::new(NoopNotificationSender),
        TaskRunnerConfig::default(),
        Arc::new(SteerRegistry::new()),
    );

    assert!(runner.resolve_task_input(&task).is_none());
}

#[tokio::test]
async fn test_agent_executor_default_execute_with_emitter_delegates_to_execute() {
    let executor = DefaultDelegatingExecutor {
        call_count: AtomicU32::new(0),
    };
    let result = executor
        .execute_with_emitter(
            "agent-001",
            None,
            Some("hello"),
            None,
            Some(Box::new(restflow_ai::agent::NullEmitter)),
        )
        .await
        .expect("execution should succeed");

    assert!(result.success);
    assert_eq!(executor.call_count.load(Ordering::SeqCst), 1);
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
    task.input = Some("Slow task input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage,
        executor,
        notifier,
        config,
        steer_registry,
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

#[tokio::test]
async fn test_runner_interrupts_running_task_when_paused() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::with_delay(3_000));
    let notifier = Arc::new(NoopNotificationSender);

    let past_time = chrono::Utc::now().timestamp_millis() - 1000;
    let mut task = storage
        .create_task(
            "Pause Interrupt Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once { run_at: past_time },
        )
        .unwrap();
    task.input = Some("Pause interrupt input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor,
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    // Wait for task to start running
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(runner.running_task_count().await, 1);

    // Simulate pause control while the task is running
    storage.pause_task(&task.id).unwrap();

    // Runner should notice pause and stop execution early
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(runner.running_task_count().await, 0);

    let updated_task = storage.get_task(&task.id).unwrap().unwrap();
    assert_eq!(updated_task.status, TaskStatus::Paused);

    handle.stop().await.unwrap();
}

#[tokio::test]
async fn test_runner_interrupts_running_task_when_stopped() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::with_delay(3_000));
    let notifier = Arc::new(NoopNotificationSender);

    let past_time = chrono::Utc::now().timestamp_millis() - 1000;
    let mut task = storage
        .create_task(
            "Stop Interrupt Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once { run_at: past_time },
        )
        .unwrap();
    task.input = Some("Stop interrupt input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor,
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(runner.running_task_count().await, 1);

    storage
        .control_task(&task.id, TaskControlAction::Stop)
        .unwrap();

    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(runner.running_task_count().await, 0);

    let updated_task = storage.get_task(&task.id).unwrap().unwrap();
    assert_eq!(updated_task.status, TaskStatus::Interrupted);

    handle.stop().await.unwrap();
}

#[tokio::test]
async fn test_runner_interrupts_running_task_when_deleted() {
    let (storage, _temp_dir) = create_test_storage();
    let executor = Arc::new(MockExecutor::with_delay(3_000));
    let notifier = Arc::new(NoopNotificationSender);

    let past_time = chrono::Utc::now().timestamp_millis() - 1000;
    let mut task = storage
        .create_task(
            "Delete Interrupt Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once { run_at: past_time },
        )
        .unwrap();
    task.input = Some("Delete interrupt input".to_string());
    task.next_run_at = Some(past_time);
    storage.update_task(&task).unwrap();

    let config = TaskRunnerConfig {
        poll_interval_ms: 100,
        ..Default::default()
    };

    let steer_registry = Arc::new(SteerRegistry::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor,
        notifier,
        config,
        steer_registry,
    ));

    let handle = runner.clone().start();

    // Wait for task to start running
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(runner.running_task_count().await, 1);

    // Delete task while execution is running.
    storage.delete_task(&task.id).unwrap();

    // Runner should notice deletion and stop execution early.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(runner.running_task_count().await, 0);
    assert!(storage.get_task(&task.id).unwrap().is_none());

    handle.stop().await.unwrap();
}

#[test]
fn test_cleanup_agent_resources_removes_orphan_files() {
    use std::fs;
    use tempfile::tempdir;

    let _lock = crate::paths::restflow_dir_env_lock();

    // Create a temporary directory to act as RESTFLOW_DIR
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let tool_output_base_dir = temp_dir.path().join("tool-output");
    fs::create_dir_all(&tool_output_base_dir).expect("Failed to create tool output dir");

    // Create per-task output directories and files.
    let task_id = "test-task-123";
    let task_output_dir = tool_output_base_dir.join(task_id);
    let other_output_dir = tool_output_base_dir.join("other-task-456");
    fs::create_dir_all(&task_output_dir).expect("Failed to create task output dir");
    fs::create_dir_all(&other_output_dir).expect("Failed to create other output dir");
    let orphan_file = task_output_dir.join("tool-call.txt");
    let other_file = other_output_dir.join("tool-call.txt");

    fs::write(&orphan_file, "orphan content").expect("Failed to write orphan file");
    fs::write(&other_file, "other content").expect("Failed to write other file");

    assert!(orphan_file.exists());
    assert!(other_file.exists());

    // Set RESTFLOW_DIR env var temporarily
    unsafe {
        std::env::set_var("RESTFLOW_DIR", temp_dir.path());
    }

    // Call cleanup
    TaskRunner::cleanup_agent_resources(task_id);

    // Target task output dir should be removed, other dir should remain.
    assert!(
        !task_output_dir.exists(),
        "Task output dir should be removed"
    );
    assert!(
        other_output_dir.exists(),
        "Other task output dir should remain"
    );
    assert!(!orphan_file.exists(), "Task output file should be removed");
    assert!(other_file.exists(), "Other task file should remain");

    // Cleanup env var
    unsafe {
        std::env::remove_var("RESTFLOW_DIR");
    }
}
