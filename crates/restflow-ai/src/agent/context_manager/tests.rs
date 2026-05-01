use super::*;
use crate::llm::{Message, MockLlmClient, MockStep, Role, ToolCall};
use serde_json::json;

// ======================================================================
// middle_truncate
// ======================================================================

#[test]
fn middle_truncate_short_string_unchanged() {
    let s = "hello world";
    assert_eq!(middle_truncate(s, 100), s);
}

#[test]
fn middle_truncate_exact_length_unchanged() {
    let s = "hello";
    assert_eq!(middle_truncate(s, 5), s);
}

#[test]
fn middle_truncate_empty_string() {
    assert_eq!(middle_truncate("", 10), "");
}

#[test]
fn middle_truncate_max_len_zero() {
    let s = "hello";
    let result = middle_truncate(s, 0);
    assert!(result.is_empty());
}

#[test]
fn middle_truncate_long_string() {
    let s = "a".repeat(1000);
    let result = middle_truncate(&s, 200);
    assert!(result.len() <= 200);
    assert!(result.contains("chars truncated"));
    assert!(result.starts_with('a'));
    assert!(result.ends_with('a'));
}

#[test]
fn middle_truncate_preserves_head_and_tail_content() {
    let s = format!("{}{}", "H".repeat(500), "T".repeat(500));
    let result = middle_truncate(&s, 200);
    assert!(result.starts_with('H'));
    assert!(result.ends_with('T'));
    assert!(result.contains("chars truncated"));
}

#[test]
fn middle_truncate_result_never_exceeds_max_len() {
    for max_len in [50, 100, 200, 500, 1000] {
        let s = "x".repeat(5000);
        let result = middle_truncate(&s, max_len);
        assert!(
            result.len() <= max_len,
            "max_len={max_len}, result.len()={}",
            result.len()
        );
    }
}

#[test]
fn middle_truncate_utf8_safety_chinese() {
    let s = "你好世界".repeat(100);
    let result = middle_truncate(&s, 200);
    assert!(result.len() <= 200);
    let _ = result.chars().count();
}

#[test]
fn middle_truncate_utf8_safety_emoji() {
    let s = "😀🎉🚀".repeat(50);
    let result = middle_truncate(&s, 100);
    let _ = result.chars().count();
}

#[test]
fn middle_truncate_utf8_mixed_content() {
    let s = "Hello你好😀World世界🎉".repeat(30);
    let result = middle_truncate(&s, 150);
    assert!(result.len() <= 150);
    let _ = result.chars().count();
}

#[test]
fn middle_truncate_max_len_smaller_than_marker() {
    let s = "a".repeat(100);
    let result = middle_truncate(&s, 5);
    assert_eq!(result.len(), 5);
    assert_eq!(result, "aaaaa");
}

#[test]
fn middle_truncate_marker_shows_correct_count() {
    let s = "a".repeat(1000);
    let result = middle_truncate(&s, 200);
    assert!(result.contains("800 chars truncated"));
}

// ======================================================================
// estimate_tokens
// ======================================================================

#[test]
fn estimate_tokens_basic_message() {
    let msg = Message::user("hello world");
    let tokens = estimate_message_tokens(&msg);
    assert_eq!(tokens, 6);
}

#[test]
fn estimate_tokens_empty_content() {
    let msg = Message::user("");
    let tokens = estimate_message_tokens(&msg);
    assert_eq!(tokens, ROLE_OVERHEAD_TOKENS);
}

#[test]
fn estimate_tokens_with_tool_calls() {
    let msg = Message::assistant_with_tool_calls(
        Some("thinking".to_string()),
        vec![ToolCall {
            id: "call_1".to_string(),
            name: "bash".to_string(),
            arguments: json!({"command": "ls"}),
        }],
    );
    let tokens = estimate_message_tokens(&msg);
    assert!(tokens > ROLE_OVERHEAD_TOKENS);
}

#[test]
fn estimate_tokens_tool_result_with_id() {
    let msg = Message::tool_result("call_abc123", "result content here");
    let tokens = estimate_message_tokens(&msg);
    assert_eq!(tokens, 11);
}

#[test]
fn estimate_tokens_empty_list() {
    assert_eq!(estimate_tokens(&[]), 0);
}

#[test]
fn estimate_tokens_multiple_messages() {
    let msgs = vec![
        Message::system("You are helpful."),
        Message::user("Hello"),
        Message::assistant("Hi there!"),
    ];
    let total = estimate_tokens(&msgs);
    assert!(total > 3 * ROLE_OVERHEAD_TOKENS);
}

#[test]
fn estimate_tokens_large_message() {
    let content = "x".repeat(40_000);
    let msg = Message::user(&content);
    let tokens = estimate_message_tokens(&msg);
    assert_eq!(tokens, 10_004);
}

// ======================================================================
// TokenEstimator
// ======================================================================

#[test]
fn token_estimator_default_factor() {
    let est = TokenEstimator::default();
    assert!((est.calibration_factor - 1.0).abs() < f64::EPSILON);
    assert_eq!(est.samples, 0);
    assert!(est.compact_allowed());
}

#[test]
fn token_estimator_calibrate_adjusts_factor() {
    let mut est = TokenEstimator::default();
    est.calibrate(100, 200);
    assert!((est.calibration_factor - 1.5).abs() < 0.01);
    assert_eq!(est.samples, 1);
}

#[test]
fn token_estimator_ema_converges() {
    let mut est = TokenEstimator::default();
    for _ in 0..20 {
        est.calibrate(100, 150);
    }
    assert!((est.calibration_factor - 1.5).abs() < 0.05);
}

#[test]
fn token_estimator_ema_switches_alpha_after_5_samples() {
    let mut est = TokenEstimator::default();
    for _ in 0..5 {
        est.calibrate(100, 200);
    }
    let factor_after_5 = est.calibration_factor;
    est.calibrate(100, 100);
    let factor_after_6 = est.calibration_factor;
    let delta = (factor_after_5 - factor_after_6).abs();
    assert!(delta < 0.5, "alpha=0.2 should cause small adjustment");
}

#[test]
fn token_estimator_zero_values_ignored() {
    let mut est = TokenEstimator::default();
    est.calibrate(0, 100);
    assert!((est.calibration_factor - 1.0).abs() < f64::EPSILON);
    est.calibrate(100, 0);
    assert!((est.calibration_factor - 1.0).abs() < f64::EPSILON);
    assert_eq!(est.samples, 0);
}

#[test]
fn token_estimator_estimate_applies_factor() {
    let mut est = TokenEstimator::default();
    est.calibrate(100, 200);

    let msgs = vec![Message::user("hello world")];
    let raw = estimate_tokens(&msgs);
    let calibrated = est.estimate(&msgs);
    assert!(calibrated > raw);
    assert_eq!(
        calibrated,
        (raw as f64 * est.calibration_factor).ceil() as usize
    );
}

#[test]
fn token_estimator_cooldown() {
    let mut est = TokenEstimator::default();
    assert!(est.compact_allowed());

    est.start_compact_cooldown(3);
    assert!(!est.compact_allowed());

    est.tick_cooldown();
    assert!(!est.compact_allowed());

    est.tick_cooldown();
    assert!(!est.compact_allowed());

    est.tick_cooldown();
    assert!(est.compact_allowed());

    est.tick_cooldown();
    assert!(est.compact_allowed());
}

// ======================================================================
// find_protection_boundary
// ======================================================================

#[test]
fn protection_boundary_normal() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let boundary = find_protection_boundary(&msgs, 2);
    assert_eq!(boundary, 3);
}

#[test]
fn protection_boundary_single_user_turn() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
    ];
    let boundary = find_protection_boundary(&msgs, 1);
    assert_eq!(boundary, 1);
}

#[test]
fn protection_boundary_fewer_user_turns() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
    ];
    let boundary = find_protection_boundary(&msgs, 3);
    assert_eq!(boundary, 0);
}

#[test]
fn protection_boundary_no_user_messages() {
    let msgs = vec![Message::system("sys"), Message::assistant("a1")];
    let boundary = find_protection_boundary(&msgs, 2);
    assert_eq!(boundary, 0);
}

#[test]
fn protection_boundary_zero_protected() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
    ];
    let boundary = find_protection_boundary(&msgs, 0);
    assert_eq!(boundary, msgs.len());
}

#[test]
fn protection_boundary_empty_messages() {
    let boundary = find_protection_boundary(&[], 2);
    assert_eq!(boundary, 0);
}

#[test]
fn protection_boundary_interleaved_tool_messages() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::tool_result("c1", "r1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::tool_result("c2", "r2"),
        Message::user("u3"),
    ];
    let boundary = find_protection_boundary(&msgs, 2);
    assert_eq!(boundary, 4);
}

// ======================================================================
// prune
// ======================================================================

#[test]
fn prune_truncates_old_tool_results() {
    let big_content = "x".repeat(20_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::tool_result("call_1", &big_content),
        Message::tool_result("call_2", &big_content),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 2,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    assert!(stats.applied);
    assert_eq!(stats.messages_truncated, 2);
    assert!(msgs[2].content.len() <= 2048);
    assert!(msgs[3].content.len() <= 2048);
    assert!(stats.bytes_removed > 0);
    assert!(stats.tokens_saved > 0);
}

#[test]
fn prune_protects_recent_messages() {
    let big_content = "x".repeat(20_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::tool_result("call_1", &big_content),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 2,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    assert!(!stats.applied);
    assert_eq!(msgs[4].content.len(), 20_000);
}

#[test]
fn prune_savings_below_threshold_not_applied() {
    let small_content = "x".repeat(3000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::tool_result("call_1", &small_content),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 5000,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    assert!(!stats.applied);
    assert_eq!(msgs[2].content.len(), 3000);
}

#[test]
fn prune_never_modifies_system_message() {
    let big_system = "S".repeat(20_000);
    let big_tool = "T".repeat(20_000);
    let mut msgs = vec![
        Message::system(&big_system),
        Message::tool_result("c1", &big_tool),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    prune(&mut msgs, &config);
    assert_eq!(msgs[0].content.len(), 20_000);
}

#[test]
fn prune_skips_non_tool_messages() {
    let big_content = "x".repeat(20_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user(&big_content),
        Message::assistant(&big_content),
        Message::tool_result("c1", &big_content),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 2,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    assert!(stats.applied);
    assert_eq!(stats.messages_truncated, 1);
    assert_eq!(msgs[1].content.len(), 20_000);
    assert_eq!(msgs[2].content.len(), 20_000);
    assert!(msgs[3].content.len() <= 2048);
}

#[test]
fn prune_already_small_tool_results_untouched() {
    let small_content = "small result";
    let big_content = "x".repeat(20_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::tool_result("c1", small_content),
        Message::tool_result("c2", &big_content),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    assert!(stats.applied);
    assert_eq!(stats.messages_truncated, 1);
    assert_eq!(msgs[1].content, "small result");
}

#[test]
fn prune_idempotent() {
    let big_content = "x".repeat(20_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::tool_result("c1", &big_content),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let config = ContextManagerConfig {
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats1 = prune(&mut msgs, &config);
    assert!(stats1.applied);
    let len_after_first = msgs[1].content.len();

    let stats2 = prune(&mut msgs, &config);
    assert!(!stats2.applied);
    assert_eq!(msgs[1].content.len(), len_after_first);
}

#[test]
fn prune_empty_messages() {
    let mut msgs: Vec<Message> = vec![];
    let config = ContextManagerConfig::default();
    let stats = prune(&mut msgs, &config);
    assert!(!stats.applied);
}

#[test]
fn prune_tokens_saved_is_accurate() {
    let big_content = "x".repeat(10_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::tool_result("c1", &big_content),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let tokens_before = estimate_tokens(&msgs);
    let config = ContextManagerConfig {
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    let tokens_after = estimate_tokens(&msgs);
    assert!(stats.applied);
    let actual_reduction = tokens_before - tokens_after;
    assert_eq!(actual_reduction, stats.tokens_saved);
}

// ======================================================================
// should_compact
// ======================================================================

#[test]
fn should_compact_below_threshold() {
    let config = ContextManagerConfig {
        context_window: 128_000,
        compact_trigger_ratio: 0.90,
        ..Default::default()
    };
    assert!(!should_compact(100_000, &config));
}

#[test]
fn should_compact_above_threshold() {
    let config = ContextManagerConfig {
        context_window: 128_000,
        compact_trigger_ratio: 0.90,
        ..Default::default()
    };
    assert!(should_compact(120_000, &config));
}

#[test]
fn should_compact_exactly_at_threshold() {
    let config = ContextManagerConfig {
        context_window: 100_000,
        compact_trigger_ratio: 0.90,
        ..Default::default()
    };
    assert!(!should_compact(90_000, &config));
    assert!(should_compact(90_001, &config));
}

#[test]
fn should_compact_zero_context_window() {
    let config = ContextManagerConfig {
        context_window: 0,
        compact_trigger_ratio: 0.90,
        ..Default::default()
    };
    assert!(!should_compact(100_000, &config));
}

#[test]
fn should_compact_zero_tokens() {
    let config = ContextManagerConfig::default();
    assert!(!should_compact(0, &config));
}

// ======================================================================
// find_compact_split
// ======================================================================

#[test]
fn find_compact_split_normal() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
    ];
    let split = find_compact_split(&msgs, 10);
    assert!(split >= 1);
    assert!(split <= msgs.len());
}

#[test]
fn find_compact_split_preserves_tool_call_pairs() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant_with_tool_calls(
            Some("thinking".to_string()),
            vec![ToolCall {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: json!({"command": "ls"}),
            }],
        ),
        Message::tool_result("call_1", "file1.txt\nfile2.txt"),
        Message::user("u2"),
        Message::assistant("done"),
    ];
    let split = find_compact_split(&msgs, 20);
    assert!(
        split <= 2 || split >= 4,
        "split={split} would orphan tool result at index 3"
    );
}

#[test]
fn find_compact_split_skips_consecutive_tool_results() {
    let msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant_with_tool_calls(
            None,
            vec![
                ToolCall {
                    id: "c1".to_string(),
                    name: "bash".to_string(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "c2".to_string(),
                    name: "file".to_string(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "c3".to_string(),
                    name: "http".to_string(),
                    arguments: json!({}),
                },
            ],
        ),
        Message::tool_result("c1", "r1"),
        Message::tool_result("c2", "r2"),
        Message::tool_result("c3", "r3"),
        Message::user("u2"),
        Message::assistant("done"),
    ];
    let split = find_compact_split(&msgs, 15);
    assert!(
        split <= 2 || split >= 6,
        "split={split} would orphan tool results"
    );
}

#[test]
fn find_compact_split_empty_messages() {
    assert_eq!(find_compact_split(&[], 1000), 0);
}

#[test]
fn find_compact_split_preserves_system_prompt() {
    let msgs = vec![Message::system("sys"), Message::user("u1")];
    let split = find_compact_split(&msgs, 1_000_000);
    assert_eq!(split, msgs.len());

    let msgs2 = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
    ];
    let split2 = find_compact_split(&msgs2, 5);
    assert!(split2 >= 1, "split should never remove the system prompt");
}

#[test]
fn find_compact_split_single_message_after_system() {
    let msgs = vec![Message::system("sys"), Message::user("u1")];
    let split = find_compact_split(&msgs, 5);
    assert_eq!(split, 1);
}

// ======================================================================
// format_conversation_for_summary
// ======================================================================

#[test]
fn format_conversation_basic() {
    let msgs = vec![Message::user("Hello"), Message::assistant("Hi there!")];
    let formatted = format_conversation_for_summary(&msgs);
    assert!(formatted.contains("[USER] Hello"));
    assert!(formatted.contains("[ASSISTANT] Hi there!"));
}

#[test]
fn format_conversation_truncates_long_messages() {
    let long_msg = "x".repeat(10_000);
    let msgs = vec![Message::user(&long_msg)];
    let formatted = format_conversation_for_summary(&msgs);
    assert!(formatted.contains("chars truncated"));
}

#[test]
fn format_conversation_short_messages_not_truncated() {
    let msgs = vec![Message::user("short message")];
    let formatted = format_conversation_for_summary(&msgs);
    assert!(!formatted.contains("chars truncated"));
    assert!(formatted.contains("short message"));
}

#[test]
fn format_conversation_includes_tool_calls() {
    let msgs = vec![Message::assistant_with_tool_calls(
        Some("let me check".to_string()),
        vec![ToolCall {
            id: "c1".to_string(),
            name: "bash".to_string(),
            arguments: json!({"command": "ls -la"}),
        }],
    )];
    let formatted = format_conversation_for_summary(&msgs);
    assert!(formatted.contains("tool_call: bash("));
}

#[test]
fn format_conversation_long_tool_args_truncated() {
    let long_args = json!({"data": "x".repeat(500)});
    let msgs = vec![Message::assistant_with_tool_calls(
        None,
        vec![ToolCall {
            id: "c1".to_string(),
            name: "http".to_string(),
            arguments: long_args,
        }],
    )];
    let formatted = format_conversation_for_summary(&msgs);
    assert!(formatted.contains("chars truncated"));
}

#[test]
fn format_conversation_includes_all_roles() {
    let msgs = vec![
        Message::system("system instructions"),
        Message::user("user input"),
        Message::assistant("assistant reply"),
        Message::tool_result("c1", "tool output"),
    ];
    let formatted = format_conversation_for_summary(&msgs);
    assert!(formatted.contains("[SYSTEM]"));
    assert!(formatted.contains("[USER]"));
    assert!(formatted.contains("[ASSISTANT]"));
    assert!(formatted.contains("[TOOL]"));
}

#[test]
fn format_conversation_empty() {
    let formatted = format_conversation_for_summary(&[]);
    assert!(formatted.is_empty());
}

// ======================================================================
// compact (async tests with MockLlmClient)
// ======================================================================

#[tokio::test]
async fn compact_replaces_old_messages_with_summary() {
    let mock = MockLlmClient::from_steps(
        "mock",
        vec![MockStep::text(
            "Goal: fix bug. Done: edited main.rs. Remaining: tests.",
        )],
    );
    let mut msgs = vec![
        Message::system("You are helpful."),
        Message::user("Fix the bug in main.rs"),
        Message::assistant("Looking at the file..."),
        Message::tool_result("c1", "fn main() { ... }"),
        Message::user("Good, now add tests"),
        Message::assistant("I'll add tests."),
    ];
    let config = ContextManagerConfig {
        compact_preserve_tokens: 10,
        ..Default::default()
    };

    let stats = compact(&mut msgs, &config, &mock).await.unwrap();

    assert!(stats.messages_replaced > 0);
    assert!(stats.tokens_after < stats.tokens_before);
    assert!(stats.summary_length > 0);
    assert_eq!(msgs[0].role, Role::System);
    assert_eq!(msgs[0].content, "You are helpful.");
    assert_eq!(msgs[1].role, Role::User);
    assert!(msgs[1].content.starts_with("[Session Summary]"));
}

#[tokio::test]
async fn compact_preserves_system_prompt() {
    let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("summary here")]);
    let mut msgs = vec![
        Message::system("Important system instructions"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
    ];
    let config = ContextManagerConfig {
        compact_preserve_tokens: 5,
        ..Default::default()
    };

    compact(&mut msgs, &config, &mock).await.unwrap();

    assert_eq!(msgs[0].content, "Important system instructions");
}

#[tokio::test]
async fn compact_noop_when_split_at_1() {
    let mock = MockLlmClient::new("mock");
    let mut msgs = vec![Message::system("sys"), Message::user("u1")];
    let config = ContextManagerConfig {
        compact_preserve_tokens: 1_000_000,
        ..Default::default()
    };

    let stats = compact(&mut msgs, &config, &mock).await.unwrap();

    assert_eq!(stats.messages_replaced, 0);
    assert_eq!(msgs.len(), 2);
}

#[tokio::test]
async fn compact_skips_on_empty_summary() {
    let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("")]);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
    ];
    let original_len = msgs.len();
    let config = ContextManagerConfig {
        compact_preserve_tokens: 5,
        ..Default::default()
    };

    let stats = compact(&mut msgs, &config, &mock).await.unwrap();

    assert_eq!(stats.messages_replaced, 0);
    assert_eq!(msgs.len(), original_len);
}

#[tokio::test]
async fn compact_skips_on_whitespace_only_summary() {
    let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("   \n\n  ")]);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
    ];
    let original_len = msgs.len();
    let config = ContextManagerConfig {
        compact_preserve_tokens: 5,
        ..Default::default()
    };

    let stats = compact(&mut msgs, &config, &mock).await.unwrap();

    assert_eq!(stats.messages_replaced, 0);
    assert_eq!(msgs.len(), original_len);
}

#[tokio::test]
async fn compact_propagates_llm_error() {
    let mock = MockLlmClient::from_steps("mock", vec![MockStep::error("LLM is down")]);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
    ];
    let config = ContextManagerConfig {
        compact_preserve_tokens: 5,
        ..Default::default()
    };

    let result = compact(&mut msgs, &config, &mock).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn compact_preserves_recent_messages() {
    let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("summary of old work")]);
    let old_content = "old work ".repeat(500);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user(&old_content),
        Message::assistant(&old_content),
        Message::user("recent question"),
        Message::assistant("recent answer"),
    ];
    let config = ContextManagerConfig {
        compact_preserve_tokens: 20,
        ..Default::default()
    };

    let stats = compact(&mut msgs, &config, &mock).await.unwrap();

    assert!(stats.messages_replaced > 0);
    let last = &msgs[msgs.len() - 1];
    assert_eq!(last.content, "recent answer");
}

// ======================================================================
// compact_was_effective
// ======================================================================

#[test]
fn compact_was_effective_good_reduction() {
    let stats = CompactStats {
        messages_replaced: 10,
        tokens_before: 100_000,
        tokens_after: 30_000,
        summary_length: 500,
    };
    assert!(compact_was_effective(&stats));
}

#[test]
fn compact_was_effective_poor_reduction() {
    let stats = CompactStats {
        messages_replaced: 10,
        tokens_before: 100_000,
        tokens_after: 90_000,
        summary_length: 500,
    };
    assert!(!compact_was_effective(&stats));
}

#[test]
fn compact_was_effective_no_messages_replaced() {
    let stats = CompactStats {
        messages_replaced: 0,
        tokens_before: 100_000,
        tokens_after: 100_000,
        summary_length: 0,
    };
    assert!(!compact_was_effective(&stats));
}

#[test]
fn compact_was_effective_zero_tokens_before() {
    let stats = CompactStats {
        messages_replaced: 5,
        tokens_before: 0,
        tokens_after: 0,
        summary_length: 100,
    };
    assert!(!compact_was_effective(&stats));
}

// ======================================================================
// Integration: prune reduces estimate
// ======================================================================

#[test]
fn prune_reduces_token_estimate() {
    let big_content = "x".repeat(40_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::tool_result("c1", &big_content),
        Message::tool_result("c2", &big_content),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];
    let before = estimate_tokens(&msgs);
    let config = ContextManagerConfig {
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        ..Default::default()
    };
    let stats = prune(&mut msgs, &config);
    let after = estimate_tokens(&msgs);

    assert!(stats.applied);
    assert!(after < before);
    assert!(before - after > 15_000);
}

#[tokio::test]
async fn compact_then_estimate_shows_reduction() {
    let mock = MockLlmClient::from_steps("mock", vec![MockStep::text("brief summary")]);
    let big_content = "x".repeat(10_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user(&big_content),
        Message::assistant(&big_content),
        Message::user("u2"),
        Message::assistant("a2"),
    ];
    let before = estimate_tokens(&msgs);
    let config = ContextManagerConfig {
        compact_preserve_tokens: 20,
        ..Default::default()
    };
    let stats = compact(&mut msgs, &config, &mock).await.unwrap();
    let after = estimate_tokens(&msgs);

    assert!(stats.messages_replaced > 0);
    assert!(after < before);
}

// ======================================================================
// End-to-end scenario
// ======================================================================

#[tokio::test]
async fn full_scenario_prune_avoids_compact() {
    let big_tool = "x".repeat(100_000);
    let mut msgs = vec![
        Message::system("sys"),
        Message::user("u1"),
        Message::assistant_with_tool_calls(
            None,
            vec![ToolCall {
                id: "c1".to_string(),
                name: "bash".to_string(),
                arguments: json!({"cmd": "cat big_file.txt"}),
            }],
        ),
        Message::tool_result("c1", &big_tool),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];

    let config = ContextManagerConfig {
        context_window: 128_000,
        compact_trigger_ratio: 0.90,
        prune_protected_turns: 1,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        compact_preserve_tokens: 20_000,
    };

    let est_before = estimate_tokens(&msgs);
    let stats = prune(&mut msgs, &config);
    assert!(stats.applied);
    let est_after = estimate_tokens(&msgs);

    assert!(est_after < est_before);
    assert!(
        !should_compact(est_after, &config),
        "after prune, should not need compact"
    );
}
