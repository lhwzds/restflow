use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

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

    let deadline = Instant::now() + std::time::Duration::from_secs(20);
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

        if terminal == task_count {
            break;
        }

        if Instant::now() >= deadline {
            panic!(
                "stress test timed out before all tasks reached terminal states: {terminal}/{task_count}"
            );
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

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
