#![cfg(feature = "test-utils")]

mod stress_support;

use stress_support::{
    FailureMode, assert_non_empty_outputs, assert_notifications_within_attempt_budget,
    assert_terminal_coverage, background_smoke_profiles, chat_smoke_profiles,
    run_background_workload, run_chat_workload,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_mixed_chat_and_background_workloads_can_run_together() {
    let chat_profile = chat_smoke_profiles()
        .into_iter()
        .next()
        .expect("chat profile");
    let background_profile = background_smoke_profiles()
        .into_iter()
        .next()
        .expect("background profile");

    let (chat_summary, background_summary) = tokio::join!(
        run_chat_workload(&chat_profile, 4),
        run_background_workload(&background_profile, 6, FailureMode::Never)
    );

    assert_terminal_coverage(&chat_summary);
    assert_non_empty_outputs(&chat_summary);
    assert_terminal_coverage(&background_summary);
    assert_notifications_within_attempt_budget(&background_summary);
}
