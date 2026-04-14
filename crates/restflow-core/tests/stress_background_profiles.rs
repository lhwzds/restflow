#![cfg(feature = "test-utils")]

mod stress_support;

use stress_support::{
    FailureMode, StressLevel, assert_no_orphan_running, assert_notifications_within_attempt_budget,
    assert_terminal_coverage, background_smoke_profiles, run_background_workload, task_count_for,
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
