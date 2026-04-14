#![cfg(feature = "test-utils")]

mod stress_support;

use stress_support::{
    FailureMode, StressLevel, assert_no_orphan_running, assert_non_empty_outputs,
    assert_notifications_within_attempt_budget, assert_terminal_coverage,
    assert_tool_call_result_pairing, background_smoke_profiles, chat_smoke_profiles, rounds_for,
    run_background_workload, run_chat_workload, task_count_for,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_mixed_chat_and_background_workloads_can_run_together() {
    let level = StressLevel::current();
    let chat_profile = chat_smoke_profiles()
        .into_iter()
        .next()
        .expect("chat profile");
    let background_profile = background_smoke_profiles()
        .into_iter()
        .next()
        .expect("background profile");

    let (chat_summary, background_summary) = tokio::join!(
        run_chat_workload(&chat_profile, rounds_for(level, 4, 20, 72)),
        run_background_workload(
            &background_profile,
            task_count_for(level, 6, 24, 96),
            FailureMode::Never
        )
    );

    assert_terminal_coverage(&chat_summary);
    assert_non_empty_outputs(&chat_summary);
    assert_tool_call_result_pairing(&chat_summary);
    assert_terminal_coverage(&background_summary);
    assert_notifications_within_attempt_budget(&background_summary);
    assert_no_orphan_running(&background_summary);
}
