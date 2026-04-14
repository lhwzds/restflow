#![cfg(feature = "test-utils")]

mod stress_support;

use stress_support::{
    assert_non_empty_outputs, assert_terminal_coverage, chat_smoke_profiles, run_chat_workload,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_chat_profiles_complete_with_non_empty_outputs() {
    for profile in chat_smoke_profiles() {
        let summary = run_chat_workload(&profile, 4).await;
        assert_terminal_coverage(&summary);
        assert_non_empty_outputs(&summary);
        assert!(
            summary.tool_calls >= profile.tool_density.min(1),
            "expected at least one tool call for profile {}",
            profile.model_id
        );
    }
}
