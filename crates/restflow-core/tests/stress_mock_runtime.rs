#![cfg(feature = "test-utils")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use restflow_core::models::{TaskSchedule, TaskStatus};
use restflow_core::runtime::background_agent::testkit::{
    DeterministicMockExecutor, MockNotificationSender, create_test_storage,
};
use restflow_core::runtime::{TaskRunner, TaskRunnerConfig};
use restflow_core::steer::SteerRegistry;

fn stress_level() -> &'static str {
    match std::env::var("RESTFLOW_STRESS_LEVEL") {
        Ok(value) if value == "soak" => "soak",
        Ok(value) if value == "stress" => "stress",
        _ => "smoke",
    }
}

fn scaled_usize(smoke: usize, stress: usize, soak: usize) -> usize {
    match stress_level() {
        "smoke" => smoke,
        "stress" => stress,
        "soak" => soak,
        _ => smoke,
    }
}

fn scaled_u64(smoke: u64, stress: u64, soak: u64) -> u64 {
    match stress_level() {
        "smoke" => smoke,
        "stress" => stress,
        "soak" => soak,
        _ => smoke,
    }
}

fn scaled_duration(smoke: Duration, stress: Duration, soak: Duration) -> Duration {
    match stress_level() {
        "smoke" => smoke,
        "stress" => stress,
        "soak" => soak,
        _ => smoke,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_runner_handles_mock_throughput_without_leaks() {
    let (storage, _temp_dir) = create_test_storage();
    let task_count = scaled_usize(60, 180, 420);

    let past_time = chrono::Utc::now().timestamp_millis() - 1_000;
    for index in 0..task_count {
        let mut task = storage
            .create_task(
                format!("stress-task-{index}"),
                "agent-mock".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .expect("failed to create stress task");
        task.input = Some(format!("stress-input-{index}"));
        task.next_run_at = Some(past_time);
        storage
            .update_task(&task)
            .expect("failed to update stress task");
    }

    let executor = Arc::new(DeterministicMockExecutor::new(
        scaled_u64(20, 35, 45),
        Some(10),
    ));
    let notifier = Arc::new(MockNotificationSender::new());
    let runner = Arc::new(TaskRunner::new(
        storage.clone(),
        executor.clone(),
        notifier.clone(),
        TaskRunnerConfig {
            poll_interval_ms: scaled_u64(25, 15, 10),
            max_concurrent_tasks: scaled_usize(8, 16, 24),
            worker_count: scaled_usize(8, 16, 24),
            task_timeout_secs: Some(30),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle = runner.clone().start();

    wait_for_terminal_states(
        &storage,
        task_count,
        scaled_duration(
            Duration::from_secs(20),
            Duration::from_secs(60),
            Duration::from_secs(180),
        ),
    )
    .await;

    handle.stop().await.expect("failed to stop runner");

    let tasks = storage
        .list_tasks()
        .expect("failed to load final stress task state");
    let completed = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .count();
    let failed = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Failed)
        .count();

    let expected_failed = task_count / 10;
    assert_eq!(failed, expected_failed, "unexpected failure count");
    assert_eq!(completed + failed, task_count);
    let drain_deadline = Instant::now()
        + scaled_duration(
            Duration::from_secs(2),
            Duration::from_secs(6),
            Duration::from_secs(12),
        );
    loop {
        let running = runner.running_task_count().await;
        if running == 0 {
            break;
        }
        if Instant::now() >= drain_deadline {
            panic!("running task leak detected: {running}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(executor.call_count(), task_count as u32);
    assert_eq!(
        notifier.notification_count().await,
        task_count,
        "every execution should emit one notification"
    );

    let artifacts_dir = stress_artifacts_dir();
    write_summary(
        artifacts_dir.join("stress-summary.json"),
        task_count,
        completed,
        failed,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_runner_recovers_after_restart_without_orphan_running_tasks() {
    let (storage, _temp_dir) = create_test_storage();
    let task_count = scaled_usize(24, 72, 180);
    let past_time = chrono::Utc::now().timestamp_millis() - 1_000;

    for index in 0..task_count {
        let mut task = storage
            .create_task(
                format!("restart-task-{index}"),
                "agent-mock".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .expect("failed to create restart task");
        task.input = Some(format!("restart-input-{index}"));
        task.next_run_at = Some(past_time);
        storage
            .update_task(&task)
            .expect("failed to update restart task");
    }

    let executor_phase1 = Arc::new(DeterministicMockExecutor::new(
        scaled_u64(200, 300, 450),
        None,
    ));
    let notifier_phase1 = Arc::new(MockNotificationSender::new());
    let runner_phase1 = Arc::new(TaskRunner::new(
        storage.clone(),
        executor_phase1.clone(),
        notifier_phase1.clone(),
        TaskRunnerConfig {
            poll_interval_ms: scaled_u64(20, 15, 10),
            max_concurrent_tasks: scaled_usize(3, 6, 10),
            worker_count: scaled_usize(3, 6, 10),
            task_timeout_secs: Some(60),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle_phase1 = runner_phase1.clone().start();
    tokio::time::sleep(scaled_duration(
        Duration::from_millis(350),
        Duration::from_millis(900),
        Duration::from_millis(2_000),
    ))
    .await;
    handle_phase1
        .stop()
        .await
        .expect("failed to stop phase1 runner");

    let mut tasks = storage
        .list_tasks()
        .expect("failed to load tasks before restart");
    let mut tagged_stale = false;
    for task in tasks.iter_mut() {
        if task.status == TaskStatus::Active {
            task.status = TaskStatus::Running;
            storage
                .update_task(task)
                .expect("failed to mark stale running task");
            tagged_stale = true;
            break;
        }
    }
    assert!(
        tagged_stale,
        "expected at least one active task to be marked as stale running"
    );

    let executor_phase2 = Arc::new(DeterministicMockExecutor::new(
        scaled_u64(8, 12, 18),
        None,
    ));
    let notifier_phase2 = Arc::new(MockNotificationSender::new());
    let runner_phase2 = Arc::new(TaskRunner::new(
        storage.clone(),
        executor_phase2.clone(),
        notifier_phase2.clone(),
        TaskRunnerConfig {
            poll_interval_ms: scaled_u64(20, 15, 10),
            max_concurrent_tasks: scaled_usize(6, 12, 18),
            worker_count: scaled_usize(6, 12, 18),
            task_timeout_secs: Some(60),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let recovery_started_at = Instant::now();
    let handle_phase2 = runner_phase2.clone().start();
    tokio::time::sleep(scaled_duration(
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(12),
    ))
    .await;
    let recovery_elapsed_ms = recovery_started_at.elapsed().as_millis() as u64;

    handle_phase2
        .stop()
        .await
        .expect("failed to stop phase2 runner");

    let executor_phase3 = Arc::new(DeterministicMockExecutor::new(
        scaled_u64(2, 4, 6),
        None,
    ));
    let runner_phase3 = Arc::new(TaskRunner::new(
        storage.clone(),
        executor_phase3.clone(),
        Arc::new(MockNotificationSender::new()),
        TaskRunnerConfig {
            poll_interval_ms: scaled_u64(20, 15, 10),
            max_concurrent_tasks: scaled_usize(8, 16, 24),
            worker_count: scaled_usize(8, 16, 24),
            task_timeout_secs: Some(60),
            stall_timeout_secs: None,
        },
        Arc::new(SteerRegistry::new()),
    ));
    let handle_phase3 = runner_phase3.start();
    tokio::time::sleep(scaled_duration(
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(12),
    ))
    .await;
    handle_phase3
        .stop()
        .await
        .expect("failed to stop phase3 runner");

    let final_tasks = storage
        .list_tasks()
        .expect("failed to load final restart task state");
    let running_count = final_tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Running)
        .count();
    let completed_count = final_tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .count();
    let failed_count = final_tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Failed)
        .count();
    let terminal_count = completed_count + failed_count;

    assert!(
        running_count <= task_count / 2,
        "unexpected running task count after restart recovery: {}",
        running_count
    );
    assert!(
        terminal_count > 0,
        "expected at least one terminal task after restart"
    );
    assert!(
        recovery_elapsed_ms <= scaled_u64(12_000, 25_000, 60_000),
        "recovery exceeded upper bound: {recovery_elapsed_ms}ms"
    );
    assert!(
        executor_phase2.call_count() > 0,
        "phase2 executor should process recovered tasks"
    );
    let total_notifications =
        notifier_phase1.notification_count().await + notifier_phase2.notification_count().await;
    let total_execution_attempts = executor_phase1.call_count() + executor_phase2.call_count();
    assert!(
        total_notifications <= total_execution_attempts as usize,
        "notifications should not exceed completed execution attempts"
    );

    let recovery_summary = serde_json::json!({
        "total_runs": task_count,
        "completed": completed_count,
        "failed": failed_count,
        "orphan_running": running_count,
        "recovery_elapsed_ms": recovery_elapsed_ms,
        "notification_count": total_notifications,
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("restart-recovery-summary.json"),
        serde_json::to_vec_pretty(&recovery_summary).expect("failed to serialize restart summary"),
    )
    .expect("failed to write restart summary file");
}

async fn wait_for_terminal_states(
    storage: &Arc<restflow_core::storage::BackgroundAgentStorage>,
    total_tasks: usize,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let tasks = storage.list_tasks().expect("failed to list tasks");
        let terminal = tasks
            .iter()
            .filter(|task| matches!(task.status, TaskStatus::Completed | TaskStatus::Failed))
            .count();

        if terminal == total_tasks {
            break;
        }

        if Instant::now() >= deadline {
            let active = tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Active)
                .count();
            let running = tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Running)
                .count();
            let completed = tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Completed)
                .count();
            let failed = tasks
                .iter()
                .filter(|task| task.status == TaskStatus::Failed)
                .count();
            panic!(
                "stress test timed out before all tasks reached terminal states: {terminal}/{total_tasks} (active={active}, running={running}, completed={completed}, failed={failed})"
            );
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn write_summary(path: PathBuf, total_runs: usize, success: usize, failure: usize) {
    let success_rate = if total_runs == 0 {
        0.0
    } else {
        success as f64 / total_runs as f64
    };

    let summary = serde_json::json!({
        "total_runs": total_runs,
        "success": success,
        "failure": failure,
        "timeout": 0,
        "success_rate": success_rate,
        "panic_count": 0,
    });

    std::fs::write(
        path,
        serde_json::to_vec_pretty(&summary).expect("failed to serialize stress summary"),
    )
    .expect("failed to write stress summary file");
}

fn stress_artifacts_dir() -> PathBuf {
    let dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "target/stress-artifacts".to_string());
    let path = PathBuf::from(dir);
    std::fs::create_dir_all(&path).expect("failed to create stress artifacts directory");
    path
}
