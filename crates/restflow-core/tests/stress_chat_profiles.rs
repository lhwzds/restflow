#![cfg(feature = "test-utils")]

mod stress_support;

use stress_support::{
    ModelProfile, ProviderFamily, StreamMode, StressLevel, assert_non_empty_outputs,
    assert_terminal_coverage, assert_tool_call_result_pairing, chat_smoke_profiles,
    coordination_tool_profiles, rounds_for, run_chat_workload, run_chat_workload_with_tools,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_chat_profiles_complete_with_non_empty_outputs() {
    let level = StressLevel::current();
    let rounds = rounds_for(level, 4, 24, 96);
    for profile in chat_smoke_profiles() {
        let summary = run_chat_workload(&profile, rounds).await;
        assert_terminal_coverage(&summary);
        assert_non_empty_outputs(&summary);
        assert_tool_call_result_pairing(&summary);
        assert!(
            summary.tool_calls >= profile.tool_density.min(1),
            "expected at least one tool call for profile {}",
            profile.model_id
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_chat_coordination_profiles_cover_subagent_and_switch_model() {
    let level = StressLevel::current();
    let coordination_profile = ModelProfile {
        provider: ProviderFamily::Anthropic,
        model_id: "claude-sonnet-4-5",
        stream_mode: StreamMode::Streaming,
        tool_density: 3,
    };

    let summary = run_chat_workload_with_tools(
        &coordination_profile,
        rounds_for(level, 3, 12, 36),
        coordination_tool_profiles(),
    )
    .await;

    assert_terminal_coverage(&summary);
    assert_non_empty_outputs(&summary);
    assert_tool_call_result_pairing(&summary);
    assert!(
        summary.provider_switches > 0,
        "expected switch_model activity"
    );
    assert!(summary.tool_calls >= 3, "expected coordination tool calls");
}
