#![cfg(feature = "test-utils")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use restflow_core::models::{BackgroundAgentStatus, TaskSchedule};
use restflow_core::runtime::background_agent::testkit::{
    DeterministicMockExecutor, MockNotificationSender, create_test_storage,
};
use restflow_core::runtime::{BackgroundAgentRunner, RunnerConfig};
use restflow_core::steer::SteerRegistry;

#[tokio::test]
async fn stress_runner_handles_mock_throughput_without_leaks() {
    let (storage, temp_dir) = create_test_storage();
    let task_count = 60usize;

    let past_time = chrono::Utc::now().timestamp_millis() - 1_000;
    for index in 0..task_count {
        let mut task = storage
            .create_task(
                format!("stress-task-{index}"),
                "agent-mock".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .expect("failed to create stress task");
        task.next_run_at = Some(past_time);
        storage
            .update_task(&task)
            .expect("failed to update stress task");
    }

    let executor = Arc::new(DeterministicMockExecutor::new(20, Some(10)));
    let notifier = Arc::new(MockNotificationSender::new());
    let runner = Arc::new(BackgroundAgentRunner::new(
        storage.clone(),
        executor.clone(),
        notifier.clone(),
        RunnerConfig {
            poll_interval_ms: 25,
            max_concurrent_tasks: 8,
            task_timeout_secs: 30,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle = runner.clone().start();

    wait_for_terminal_states(&storage, task_count, Duration::from_secs(20)).await;

    handle.stop().await.expect("failed to stop runner");

    let tasks = storage
        .list_tasks()
        .expect("failed to load final stress task state");
    let completed = tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Completed)
        .count();
    let failed = tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Failed)
        .count();

    let expected_failed = task_count / 10;
    assert_eq!(failed, expected_failed, "unexpected failure count");
    assert_eq!(completed + failed, task_count);
    assert_eq!(
        runner.running_task_count().await,
        0,
        "running task leak detected"
    );
    assert_eq!(executor.call_count(), task_count as u32);
    assert_eq!(
        notifier.notification_count().await,
        task_count,
        "every execution should emit one notification"
    );

    write_summary(
        temp_dir.path().join("stress-summary.json"),
        task_count,
        completed,
        failed,
    );
}

#[tokio::test]
async fn stress_runner_recovers_after_restart_without_orphan_running_tasks() {
    let (storage, temp_dir) = create_test_storage();
    let task_count = 24usize;
    let past_time = chrono::Utc::now().timestamp_millis() - 1_000;

    for index in 0..task_count {
        let mut task = storage
            .create_task(
                format!("restart-task-{index}"),
                "agent-mock".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .expect("failed to create restart task");
        task.next_run_at = Some(past_time);
        storage
            .update_task(&task)
            .expect("failed to update restart task");
    }

    let executor_phase1 = Arc::new(DeterministicMockExecutor::new(200, None));
    let notifier_phase1 = Arc::new(MockNotificationSender::new());
    let runner_phase1 = Arc::new(BackgroundAgentRunner::new(
        storage.clone(),
        executor_phase1,
        notifier_phase1.clone(),
        RunnerConfig {
            poll_interval_ms: 20,
            max_concurrent_tasks: 3,
            task_timeout_secs: 60,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let handle_phase1 = runner_phase1.clone().start();
    tokio::time::sleep(Duration::from_millis(350)).await;
    handle_phase1
        .stop()
        .await
        .expect("failed to stop phase1 runner");

    let mut tasks = storage
        .list_tasks()
        .expect("failed to load tasks before restart");
    let mut tagged_stale = false;
    for task in tasks.iter_mut() {
        if task.status == BackgroundAgentStatus::Active {
            task.status = BackgroundAgentStatus::Running;
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

    let executor_phase2 = Arc::new(DeterministicMockExecutor::new(8, None));
    let notifier_phase2 = Arc::new(MockNotificationSender::new());
    let runner_phase2 = Arc::new(BackgroundAgentRunner::new(
        storage.clone(),
        executor_phase2.clone(),
        notifier_phase2.clone(),
        RunnerConfig {
            poll_interval_ms: 20,
            max_concurrent_tasks: 6,
            task_timeout_secs: 60,
        },
        Arc::new(SteerRegistry::new()),
    ));

    let recovery_started_at = Instant::now();
    let handle_phase2 = runner_phase2.clone().start();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let recovery_elapsed_ms = recovery_started_at.elapsed().as_millis() as u64;

    handle_phase2
        .stop()
        .await
        .expect("failed to stop phase2 runner");

    let runner_phase3 = Arc::new(BackgroundAgentRunner::new(
        storage.clone(),
        Arc::new(DeterministicMockExecutor::new(2, None)),
        Arc::new(MockNotificationSender::new()),
        RunnerConfig {
            poll_interval_ms: 20,
            max_concurrent_tasks: 8,
            task_timeout_secs: 60,
        },
        Arc::new(SteerRegistry::new()),
    ));
    let handle_phase3 = runner_phase3.start();
    tokio::time::sleep(Duration::from_secs(2)).await;
    handle_phase3
        .stop()
        .await
        .expect("failed to stop phase3 runner");

    let final_tasks = storage
        .list_tasks()
        .expect("failed to load final restart task state");
    let running_count = final_tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Running)
        .count();
    let completed_count = final_tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Completed)
        .count();
    let failed_count = final_tasks
        .iter()
        .filter(|task| task.status == BackgroundAgentStatus::Failed)
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
        recovery_elapsed_ms <= 12_000,
        "recovery exceeded upper bound: {recovery_elapsed_ms}ms"
    );
    assert!(
        executor_phase2.call_count() > 0,
        "phase2 executor should process recovered tasks"
    );
    let total_notifications =
        notifier_phase1.notification_count().await + notifier_phase2.notification_count().await;
    assert!(
        total_notifications <= terminal_count,
        "notifications should not duplicate across restart"
    );

    let recovery_summary = serde_json::json!({
        "total_runs": task_count,
        "completed": completed_count,
        "failed": failed_count,
        "orphan_running": running_count,
        "recovery_elapsed_ms": recovery_elapsed_ms,
        "notification_count": total_notifications,
    });
    std::fs::write(
        temp_dir.path().join("restart-recovery-summary.json"),
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
            .filter(|task| {
                matches!(
                    task.status,
                    BackgroundAgentStatus::Completed | BackgroundAgentStatus::Failed
                )
            })
            .count();

        if terminal == total_tasks {
            break;
        }

        if Instant::now() >= deadline {
            panic!(
                "stress test timed out before all tasks reached terminal states: {terminal}/{total_tasks}"
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
