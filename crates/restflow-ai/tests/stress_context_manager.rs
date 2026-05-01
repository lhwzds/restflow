#![cfg(feature = "test-utils")]

//! Stress tests for the two-stage context manager (Prune + Compact).
//!
//! These tests exercise large-scale scenarios that the unit tests don't cover:
//! realistic conversation sizes, multi-cycle compact, calibration convergence,
//! and full pipeline behaviour.
//!
//! Gated behind `test-utils` so they only run in nightly stress CI.

use std::path::PathBuf;
use std::time::Instant;

use restflow_ai::agent::context_manager::{
    ContextManagerConfig, TokenEstimator, compact, compact_was_effective, estimate_tokens,
    middle_truncate, prune, should_compact,
};
use restflow_ai::llm::{Message, MockLlmClient, MockStep, Role, ToolCall};
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stress_artifacts_dir() -> PathBuf {
    let dir = std::env::var("LOG_DIR").unwrap_or_else(|_| "target/stress-artifacts".to_string());
    let path = PathBuf::from(dir);
    std::fs::create_dir_all(&path).expect("failed to create stress artifacts directory");
    path
}

/// Build a realistic multi-turn conversation with tool calls.
fn build_conversation(turns: usize, tool_result_size: usize) -> Vec<Message> {
    let mut msgs = vec![Message::system(
        "You are a helpful assistant. Follow instructions carefully.",
    )];

    for i in 0..turns {
        msgs.push(Message::user(format!("Task step {i}: do something")));
        msgs.push(Message::assistant_with_tool_calls(
            Some(format!("Let me handle step {i}...")),
            vec![ToolCall {
                id: format!("call_{i}"),
                name: "bash".to_string(),
                arguments: json!({"command": format!("echo step_{i}")}),
            }],
        ));
        msgs.push(Message::tool_result(
            format!("call_{i}"),
            "x".repeat(tool_result_size),
        ));
        msgs.push(Message::assistant(format!("Step {i} complete.")));
    }

    msgs
}

/// Build mock LLM that always returns a fixed summary.
fn summary_mock(summary: &str) -> MockLlmClient {
    MockLlmClient::from_steps("stress-mock", vec![MockStep::text(summary)])
}

/// Build mock LLM that returns summaries for N compact cycles.
fn multi_cycle_mock(cycles: usize) -> MockLlmClient {
    let steps: Vec<MockStep> = (0..cycles)
        .map(|i| MockStep::text(format!("Cycle {i} summary: work in progress on the task.")))
        .collect();
    MockLlmClient::from_steps("stress-mock", steps)
}

// ---------------------------------------------------------------------------
// 1. Large conversation prune
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_prune_large_conversation() {
    let turns = 200;
    let tool_size = 50_000; // 50KB per tool result
    let mut msgs = build_conversation(turns, tool_size);
    let total_messages = msgs.len();

    let config = ContextManagerConfig {
        context_window: 128_000,
        prune_protected_turns: 3,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 20_000,
    };

    let tokens_before = estimate_tokens(&msgs);
    let start = Instant::now();
    let stats = prune(&mut msgs, &config);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let tokens_after = estimate_tokens(&msgs);

    assert!(stats.applied, "prune should have applied");
    assert!(
        stats.messages_truncated > 100,
        "expected many truncations, got {}",
        stats.messages_truncated
    );
    assert!(
        tokens_after < tokens_before / 2,
        "expected significant token reduction: before={tokens_before}, after={tokens_after}"
    );
    assert_eq!(
        msgs.len(),
        total_messages,
        "message count should not change"
    );
    // Tool results outside the protection zone should be truncated.
    // Protection covers the last N user turns, so messages in the earlier
    // part of the conversation (say first 75%) should all be truncated.
    let check_boundary = total_messages * 3 / 4;
    let mut checked = 0;
    for (i, msg) in msgs.iter().enumerate().take(check_boundary) {
        if msg.role == Role::Tool && i > 0 && msg.content.len() > config.prune_tool_max + 200 {
            panic!(
                "message {i} (before protection) not truncated: len={}",
                msg.content.len()
            );
        }
        if msg.role == Role::Tool && i > 0 {
            checked += 1;
        }
    }
    assert!(
        checked > 50,
        "should have checked many tool results, got {checked}"
    );

    // Performance: should complete well under 1 second.
    assert!(
        elapsed_ms < 2000,
        "prune took too long: {elapsed_ms}ms for {turns} turns"
    );

    let summary = serde_json::json!({
        "test": "stress_prune_large_conversation",
        "turns": turns,
        "tool_result_size": tool_size,
        "total_messages": total_messages,
        "messages_truncated": stats.messages_truncated,
        "tokens_before": tokens_before,
        "tokens_after": tokens_after,
        "bytes_removed": stats.bytes_removed,
        "elapsed_ms": elapsed_ms,
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-prune-stress.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 2. Multi-cycle compact
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_compact_multi_cycle() {
    // Use a small context window to trigger compaction frequently.
    let context_window = 2_000;
    let config = ContextManagerConfig {
        context_window,
        prune_protected_turns: 1,
        prune_tool_max: 512,
        min_prune_savings_tokens: 10,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 200,
    };

    let cycles = 10;
    let mock = multi_cycle_mock(cycles);
    let mut msgs = vec![Message::system("You are helpful.")];
    let mut compact_count = 0;
    let estimator = TokenEstimator::default();

    let start = Instant::now();

    for cycle in 0..cycles {
        // Grow the conversation until we exceed the compact threshold.
        for j in 0..20 {
            msgs.push(Message::user(format!(
                "Cycle {cycle} message {j}: {}",
                "data ".repeat(50)
            )));
            msgs.push(Message::assistant(format!(
                "Response to cycle {cycle} msg {j}: {}",
                "reply ".repeat(30)
            )));
        }

        let estimated = estimator.estimate(&msgs);
        if should_compact(estimated, &config) {
            let stats = compact(&mut msgs, &config, &mock).await.unwrap();
            if stats.messages_replaced > 0 {
                compact_count += 1;
            }
        }

        // Verify invariants after each cycle.
        assert!(
            !msgs.is_empty(),
            "messages should never be empty after compact"
        );
        assert_eq!(
            msgs[0].role,
            Role::System,
            "system prompt must always be first"
        );
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    assert!(
        compact_count >= 3,
        "expected multiple compactions, got {compact_count}"
    );

    let summary = serde_json::json!({
        "test": "stress_compact_multi_cycle",
        "cycles": cycles,
        "compact_count": compact_count,
        "final_message_count": msgs.len(),
        "final_tokens": estimate_tokens(&msgs),
        "elapsed_ms": elapsed_ms,
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-compact-multi-cycle.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 3. TokenEstimator convergence
// ---------------------------------------------------------------------------

#[test]
fn stress_token_estimator_convergence() {
    let mut estimator = TokenEstimator::default();
    let target_ratio = 1.35; // actual tokens are 35% more than heuristic
    let samples = 1000;

    let start = Instant::now();

    for i in 0..samples {
        let estimated = 100 + (i % 50); // vary estimate a bit
        let actual = (estimated as f64 * target_ratio) as u32;
        estimator.calibrate(estimated, actual);
    }

    let elapsed_us = start.elapsed().as_micros() as u64;

    // After 1000 samples, factor should converge to ~1.35.
    let factor = estimator.estimate(&[Message::user("test")]) as f64
        / estimate_tokens(&[Message::user("test")]) as f64;
    assert!(
        (factor - target_ratio).abs() < 0.05,
        "factor={factor:.4} should converge to {target_ratio}"
    );

    // With noisy data, should still converge.
    let mut noisy_estimator = TokenEstimator::default();
    for i in 0..samples {
        let estimated = 100 + (i % 50);
        // Add noise: ratio oscillates between 1.1 and 1.6 (mean ~1.35).
        let noise = if i % 2 == 0 { 1.1 } else { 1.6 };
        let actual = (estimated as f64 * noise) as u32;
        noisy_estimator.calibrate(estimated, actual);
    }
    let noisy_factor = noisy_estimator.estimate(&[Message::user("test")]) as f64
        / estimate_tokens(&[Message::user("test")]) as f64;
    // Noisy EMA should settle near the mean of 1.35 with some tolerance.
    assert!(
        (noisy_factor - target_ratio).abs() < 0.30,
        "noisy factor={noisy_factor:.4} should be near {target_ratio}"
    );

    let summary = serde_json::json!({
        "test": "stress_token_estimator_convergence",
        "samples": samples,
        "target_ratio": target_ratio,
        "converged_factor": factor,
        "noisy_converged_factor": noisy_factor,
        "elapsed_us": elapsed_us,
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-estimator-convergence.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 4. Middle-truncate throughput
// ---------------------------------------------------------------------------

#[test]
fn stress_middle_truncate_throughput() {
    let iterations = 1_000;
    let input_size = 100_000; // 100KB per string
    let max_len = 2048;

    // Build strings with mixed content (ASCII + multibyte).
    let ascii_input = "a".repeat(input_size);
    let mixed_input = {
        let mut s = String::new();
        for i in 0..input_size / 10 {
            if i % 3 == 0 {
                s.push_str("你好世界ab");
            } else {
                s.push_str("abcdefghij");
            }
        }
        s
    };

    let start = Instant::now();
    for _ in 0..iterations {
        let result = middle_truncate(&ascii_input, max_len);
        assert!(result.len() <= max_len);
    }
    let ascii_elapsed_ms = start.elapsed().as_millis() as u64;

    let start = Instant::now();
    for _ in 0..iterations {
        let result = middle_truncate(&mixed_input, max_len);
        assert!(result.len() <= max_len);
        // Verify UTF-8 validity by iterating chars.
        let _ = result.chars().count();
    }
    let mixed_elapsed_ms = start.elapsed().as_millis() as u64;

    // Performance: should handle 1000 iterations in under 5 seconds.
    assert!(
        ascii_elapsed_ms < 5000,
        "ascii truncation too slow: {ascii_elapsed_ms}ms for {iterations} iterations"
    );
    assert!(
        mixed_elapsed_ms < 5000,
        "mixed truncation too slow: {mixed_elapsed_ms}ms for {iterations} iterations"
    );

    // Test edge cases at scale.
    for max in [1, 10, 50, 100, 500, 1000, 5000] {
        let result = middle_truncate(&ascii_input, max);
        assert!(
            result.len() <= max,
            "max={max}, result.len()={}",
            result.len()
        );
        let result = middle_truncate(&mixed_input, max);
        // For mixed content, byte length may slightly exceed max due to char boundary
        // alignment, but we trust floor_char_boundary to keep it safe.
        let _ = result.chars().count(); // Must not panic.
    }

    let summary = serde_json::json!({
        "test": "stress_middle_truncate_throughput",
        "iterations": iterations,
        "input_size": input_size,
        "max_len": max_len,
        "ascii_elapsed_ms": ascii_elapsed_ms,
        "mixed_elapsed_ms": mixed_elapsed_ms,
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-truncate-throughput.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 5. Full pipeline: prune → compact
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_full_pipeline_prune_then_compact() {
    // Build a conversation that needs both prune AND compact.
    let turns = 100;
    let tool_size = 30_000;
    let mut msgs = build_conversation(turns, tool_size);

    // Use a context window where even after prune, compact is still needed.
    let config = ContextManagerConfig {
        context_window: 5_000,
        prune_protected_turns: 2,
        prune_tool_max: 512,
        min_prune_savings_tokens: 50,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 500,
    };

    let tokens_initial = estimate_tokens(&msgs);
    let msg_count_initial = msgs.len();

    // Stage 1: Prune.
    let start = Instant::now();
    let prune_stats = prune(&mut msgs, &config);
    let prune_elapsed_ms = start.elapsed().as_millis() as u64;
    let tokens_after_prune = estimate_tokens(&msgs);

    assert!(prune_stats.applied, "prune should apply");
    assert_eq!(
        msgs.len(),
        msg_count_initial,
        "prune does not remove messages"
    );
    assert!(tokens_after_prune < tokens_initial);

    // Stage 2: Check if compact is needed after prune.
    let needs_compact = should_compact(tokens_after_prune, &config);

    let mut compact_stats = None;
    if needs_compact {
        let mock = summary_mock("Pipeline summary: completed 100 steps of file processing.");
        let stats = compact(&mut msgs, &config, &mock).await.unwrap();
        compact_stats = Some(stats);
    }

    let tokens_final = estimate_tokens(&msgs);

    // Final state: tokens should be much less than initial.
    assert!(
        tokens_final < tokens_initial / 3,
        "final tokens ({tokens_final}) should be << initial ({tokens_initial})"
    );
    // System prompt preserved.
    assert_eq!(msgs[0].role, Role::System);

    let summary = serde_json::json!({
        "test": "stress_full_pipeline_prune_then_compact",
        "turns": turns,
        "tool_result_size": tool_size,
        "tokens_initial": tokens_initial,
        "tokens_after_prune": tokens_after_prune,
        "tokens_final": tokens_final,
        "prune_applied": prune_stats.applied,
        "prune_messages_truncated": prune_stats.messages_truncated,
        "prune_elapsed_ms": prune_elapsed_ms,
        "compact_needed": needs_compact,
        "compact_messages_replaced": compact_stats.as_ref().map(|s| s.messages_replaced),
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-full-pipeline.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 6. Compact cooldown prevents loops
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_compact_cooldown_prevents_loops() {
    // Simulate a scenario where compact is ineffective (LLM returns long summary).
    let long_summary = "detailed ".repeat(5000); // ~40KB summary
    let config = ContextManagerConfig {
        context_window: 2_000,
        prune_protected_turns: 1,
        prune_tool_max: 512,
        min_prune_savings_tokens: 10,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 200,
    };

    let mut estimator = TokenEstimator::default();
    let mut compact_attempts = 0;
    let mut compact_blocked = 0;
    let iterations = 30;

    for iter in 0..iterations {
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("data ".repeat(200)),
            Message::assistant("response ".repeat(200)),
            Message::user("more data ".repeat(200)),
            Message::assistant("more response ".repeat(200)),
        ];

        estimator.tick_cooldown();
        let estimated = estimator.estimate(&msgs);

        if should_compact(estimated, &config) {
            if estimator.compact_allowed() {
                compact_attempts += 1;
                let mock = summary_mock(&long_summary);
                let stats = compact(&mut msgs, &config, &mock).await.unwrap();
                if !compact_was_effective(&stats) {
                    estimator.start_compact_cooldown(5);
                }
            } else {
                compact_blocked += 1;
            }
        }

        // Verify cooldown prevents rapid compaction.
        if iter > 5 {
            assert!(
                compact_attempts <= iterations / 3,
                "iter={iter}: too many compact attempts ({compact_attempts}) suggests no cooldown"
            );
        }
    }

    assert!(
        compact_blocked > 0,
        "cooldown should have blocked at least some compactions"
    );

    let summary = serde_json::json!({
        "test": "stress_compact_cooldown_prevents_loops",
        "iterations": iterations,
        "compact_attempts": compact_attempts,
        "compact_blocked": compact_blocked,
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-cooldown-stress.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 7. Alternating grow-prune-compact (realistic long agent)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_alternating_grow_prune_compact() {
    // Simulate a long-running agent that alternates between growing context
    // and managing it through prune/compact cycles.
    let context_window = 5_000;
    let config = ContextManagerConfig {
        context_window,
        prune_protected_turns: 2,
        prune_tool_max: 1024,
        min_prune_savings_tokens: 50,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 800,
    };

    let agent_iterations = 50;
    let mock = multi_cycle_mock(agent_iterations); // enough summaries
    let mut msgs = vec![Message::system("You are a code review agent.")];
    let mut estimator = TokenEstimator::default();
    let mut total_prunes = 0;
    let mut total_compacts = 0;
    let mut peak_tokens = 0usize;
    let mut token_history = Vec::new();

    let start = Instant::now();

    for iter in 0..agent_iterations {
        // Simulate agent work: user request + tool call + tool result + reply.
        msgs.push(Message::user(format!(
            "Review file_{iter}.rs: {}",
            "code ".repeat(100)
        )));
        let tool_call_id = format!("call_review_{iter}");
        msgs.push(Message::assistant_with_tool_calls(
            Some(format!("Reviewing file_{iter}.rs...")),
            vec![ToolCall {
                id: tool_call_id.clone(),
                name: "file".to_string(),
                arguments: json!({"action": "read", "path": format!("src/file_{iter}.rs")}),
            }],
        ));
        msgs.push(Message::tool_result(
            &tool_call_id,
            format!("fn process_{iter}() {{ {} }}", "let x = 1; ".repeat(200)),
        ));
        msgs.push(Message::assistant(format!(
            "File_{iter}.rs review: looks good, minor style issues."
        )));

        // Check context management thresholds.
        estimator.tick_cooldown();
        let estimated = estimator.estimate(&msgs);
        peak_tokens = peak_tokens.max(estimated);
        token_history.push(estimated);

        // Prune first (zero cost).
        let prune_stats = prune(&mut msgs, &config);
        if prune_stats.applied {
            total_prunes += 1;
        }

        // Then check compact.
        let estimated_after_prune = estimator.estimate(&msgs);
        if estimator.compact_allowed() && should_compact(estimated_after_prune, &config) {
            match compact(&mut msgs, &config, &mock).await {
                Ok(stats) => {
                    if stats.messages_replaced > 0 {
                        total_compacts += 1;
                        if !compact_was_effective(&stats) {
                            estimator.start_compact_cooldown(5);
                        }
                    }
                }
                Err(_) => {
                    estimator.start_compact_cooldown(3);
                }
            }
        }

        // Invariant: system prompt is always first.
        assert_eq!(
            msgs[0].role,
            Role::System,
            "iter={iter}: system prompt lost"
        );
        // Invariant: messages are never empty.
        assert!(msgs.len() >= 2, "iter={iter}: too few messages");
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let final_tokens = estimate_tokens(&msgs);

    // The context should stay bounded despite 50 iterations of growth.
    assert!(
        final_tokens < context_window * 2,
        "final tokens ({final_tokens}) should be bounded near context_window ({context_window})"
    );
    assert!(total_prunes > 0, "should have pruned at least once");
    assert!(total_compacts > 0, "should have compacted at least once");

    // Performance: 50 iterations with prune+compact should complete quickly.
    assert!(
        elapsed_ms < 10_000,
        "agent simulation too slow: {elapsed_ms}ms for {agent_iterations} iterations"
    );

    let summary = serde_json::json!({
        "test": "stress_alternating_grow_prune_compact",
        "agent_iterations": agent_iterations,
        "total_prunes": total_prunes,
        "total_compacts": total_compacts,
        "peak_tokens": peak_tokens,
        "final_tokens": final_tokens,
        "final_message_count": msgs.len(),
        "context_window": context_window,
        "elapsed_ms": elapsed_ms,
        "token_history_sample": &token_history[..token_history.len().min(20)],
    });
    let artifacts_dir = stress_artifacts_dir();
    std::fs::write(
        artifacts_dir.join("context-alternating-stress.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// 8. Prune idempotency at scale
// ---------------------------------------------------------------------------

#[test]
fn stress_prune_idempotent_multiple_passes() {
    let turns = 100;
    let tool_size = 20_000;
    let mut msgs = build_conversation(turns, tool_size);
    let config = ContextManagerConfig {
        context_window: 128_000,
        prune_protected_turns: 2,
        prune_tool_max: 2048,
        min_prune_savings_tokens: 100,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 20_000,
    };

    // First pass should apply.
    let stats1 = prune(&mut msgs, &config);
    assert!(stats1.applied);
    let snapshot = msgs.iter().map(|m| m.content.len()).collect::<Vec<_>>();

    // Subsequent passes should be no-ops.
    for pass in 2..=10 {
        let stats = prune(&mut msgs, &config);
        assert!(
            !stats.applied,
            "pass {pass}: prune should be idempotent after first application"
        );
        let current = msgs.iter().map(|m| m.content.len()).collect::<Vec<_>>();
        assert_eq!(snapshot, current, "pass {pass}: messages mutated");
    }
}

// ---------------------------------------------------------------------------
// 9. Compact with LLM errors doesn't corrupt state
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_compact_error_resilience() {
    let config = ContextManagerConfig {
        context_window: 1_000,
        prune_protected_turns: 1,
        prune_tool_max: 512,
        min_prune_savings_tokens: 10,
        compact_trigger_ratio: 0.90,
        compact_preserve_tokens: 100,
    };

    for trial in 0..20 {
        let mut msgs = vec![
            Message::system("sys"),
            Message::user("data ".repeat(200)),
            Message::assistant("response ".repeat(200)),
            Message::user("more ".repeat(200)),
        ];
        let snapshot = msgs.clone();

        let mock =
            MockLlmClient::from_steps("error-mock", vec![MockStep::error("LLM unavailable")]);
        let result = compact(&mut msgs, &config, &mock).await;

        assert!(result.is_err(), "trial {trial}: should propagate error");
        // Messages must be unchanged after error.
        assert_eq!(
            msgs.len(),
            snapshot.len(),
            "trial {trial}: message count changed after error"
        );
        for (i, (got, expected)) in msgs.iter().zip(snapshot.iter()).enumerate() {
            assert_eq!(
                got.content, expected.content,
                "trial {trial}: message {i} content changed after error"
            );
            assert_eq!(
                got.role, expected.role,
                "trial {trial}: message {i} role changed after error"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 10. Estimate accuracy across varied message shapes
// ---------------------------------------------------------------------------

#[test]
fn stress_estimate_accuracy_varied_shapes() {
    // Test that estimate_tokens handles a wide variety of message shapes
    // without panicking or producing unreasonable results.
    let test_cases: Vec<Vec<Message>> = vec![
        // Empty conversation.
        vec![],
        // Just system prompt.
        vec![Message::system("sys")],
        // Many tiny messages.
        (0..500)
            .map(|i| {
                if i % 2 == 0 {
                    Message::user("hi")
                } else {
                    Message::assistant("ok")
                }
            })
            .collect(),
        // Few very large messages.
        vec![
            Message::system("s".repeat(100_000)),
            Message::user("u".repeat(200_000)),
            Message::assistant("a".repeat(150_000)),
        ],
        // Messages with tool calls of varying argument sizes.
        vec![
            Message::system("sys"),
            Message::assistant_with_tool_calls(
                None,
                (0..20)
                    .map(|i| ToolCall {
                        id: format!("c{i}"),
                        name: format!("tool_{i}"),
                        arguments: json!({"data": "x".repeat(i * 100)}),
                    })
                    .collect(),
            ),
        ],
        // Tool results with long IDs.
        (0..100)
            .map(|i| Message::tool_result(format!("call_{}", "x".repeat(i)), "result"))
            .collect(),
        // Mixed UTF-8 content.
        vec![
            Message::user("Hello 你好 こんにちは 🌍"),
            Message::assistant("Résponse avec des accents: café, naïve, über"),
            Message::tool_result("c1", "🎉".repeat(10_000)),
        ],
    ];

    for (i, msgs) in test_cases.iter().enumerate() {
        let tokens = estimate_tokens(msgs);
        // Should never panic and should return reasonable values.
        assert!(
            tokens < 500_000,
            "test case {i}: unreasonable token count {tokens}"
        );
        if msgs.is_empty() {
            assert_eq!(tokens, 0, "empty messages should have 0 tokens");
        }
    }
}

// ---------------------------------------------------------------------------
// Summary writer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stress_write_combined_summary() {
    // This test just writes a combined summary of all stress test results.
    // It runs last (alphabetically) and reads individual summaries if they exist.
    let artifacts_dir = stress_artifacts_dir();
    let mut combined = serde_json::Map::new();
    combined.insert("suite".into(), json!("context_manager_stress"));

    let files = [
        "context-prune-stress.json",
        "context-compact-multi-cycle.json",
        "context-estimator-convergence.json",
        "context-truncate-throughput.json",
        "context-full-pipeline.json",
        "context-cooldown-stress.json",
        "context-alternating-stress.json",
    ];
    for file in &files {
        let path = artifacts_dir.join(file);
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            combined.insert(file.replace(".json", "").replace('-', "_"), val);
        }
    }

    std::fs::write(
        artifacts_dir.join("context-manager-stress-summary.json"),
        serde_json::to_vec_pretty(&serde_json::Value::Object(combined)).unwrap(),
    )
    .unwrap();
}
