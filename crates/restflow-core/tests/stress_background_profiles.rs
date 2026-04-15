#![cfg(feature = "test-utils")]

mod stress_support;

use stress_support::{
    FailureMode, MockLlmHttpServer, MockToolHttpServer, StressLevel, assert_no_orphan_running,
    assert_notifications_within_attempt_budget, assert_terminal_coverage,
    background_smoke_profiles, rounds_for, run_background_workload,
    run_background_workload_with_real_runtime, task_count_for,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_background_profiles_reach_terminal_states() {
    let level = StressLevel::current();
    let profiles = background_smoke_profiles();

    let stable = run_background_workload(
        &profiles[0],
        task_count_for(level, 8, 40, 140),
        FailureMode::Never,
    )
    .await;
    assert_terminal_coverage(&stable);
    assert_notifications_within_attempt_budget(&stable);
    assert_no_orphan_running(&stable);

    let flaky = run_background_workload(
        &profiles[1],
        task_count_for(level, 10, 48, 168),
        FailureMode::RetryableEvery(3),
    )
    .await;
    assert_terminal_coverage(&flaky);
    assert_notifications_within_attempt_budget(&flaky);
    assert_no_orphan_running(&flaky);
    assert!(
        flaky.failed > 0,
        "expected retryable workload to surface failures"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_background_real_runtime_handles_tool_io() {
    let level = StressLevel::current();
    let profile = background_smoke_profiles()
        .into_iter()
        .last()
        .expect("background profile");
    let llm_server = MockLlmHttpServer::start(level).await;
    let tool_server = MockToolHttpServer::start().await;

    let summary = run_background_workload_with_real_runtime(
        &profile,
        level,
        task_count_for(level, 4, 32, 96),
        &llm_server,
        &tool_server,
    )
    .await;

    assert_terminal_coverage(&summary);
    assert_notifications_within_attempt_budget(&summary);
    assert_no_orphan_running(&summary);
    assert!(
        summary.tool_calls > 0,
        "expected real runtime tool calls, summary={summary:?}"
    );

    let llm_metrics = llm_server.metrics();
    assert!(
        llm_metrics.request_count >= summary.total_runs,
        "expected mock llm backend requests for background runtime"
    );
    let tool_metrics = tool_server.metrics();
    assert!(
        summary.tool_calls >= summary.total_runs * rounds_for(level, 3, 4, 6),
        "expected multi-step real tool calls for background runtime, summary={summary:?}"
    );
    assert!(
        tool_metrics.request_count >= summary.total_runs,
        "expected at least one http_request backend call per background task, summary={summary:?}"
    );

    tool_server.shutdown().await;
    llm_server.shutdown().await;
}
