//! Typed IPC bindings setup for Tauri commands.
//!
//! This module centralizes command collection for tauri-specta export and invoke handler wiring.

use crate::commands;
use specta_typescript::{BigIntExportBehavior, Typescript};

const WEB_BINDINGS_PATH: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/src/api/bindings.ts");

macro_rules! collect_ipc_commands {
    () => {
        tauri_specta::collect_commands![
            commands::add_chat_message,
            commands::archive_chat_session,
            commands::auth_add_profile,
            commands::auth_clear,
            commands::auth_disable_profile,
            commands::auth_discover,
            commands::auth_enable_profile,
            commands::auth_get_api_key,
            commands::auth_get_available_profiles,
            commands::auth_get_profile,
            commands::auth_get_profiles_for_provider,
            commands::auth_get_summary,
            commands::auth_initialize,
            commands::auth_list_profiles,
            commands::auth_mark_failure,
            commands::auth_mark_success,
            commands::auth_remove_profile,
            commands::auth_update_profile,
            commands::cancel_background_agent,
            commands::cancel_chat_stream,
            commands::convert_session_to_background_agent,
            commands::create_agent,
            commands::create_chat_session,
            commands::create_hook,
            commands::create_memory_chunk,
            commands::create_memory_session,
            commands::create_secret,
            commands::create_skill,
            commands::delete_agent,
            commands::delete_background_agent,
            commands::delete_chat_session,
            commands::delete_hook,
            commands::delete_memory_chunk,
            commands::delete_memory_chunks_for_agent,
            commands::delete_memory_session,
            commands::delete_secret,
            commands::delete_skill,
            commands::execute_chat_session,
            commands::export_memory_advanced,
            commands::export_memory_markdown,
            commands::export_memory_session_markdown,
            commands::export_skill,
            commands::get_agent,
            commands::get_available_models,
            commands::get_available_providers,
            commands::get_available_tools,
            commands::get_model_catalog,
            commands::get_background_agent_events,
            commands::get_background_agent_stream_event_name,
            commands::get_chat_session,
            commands::get_cli_daemon_status,
            commands::get_config,
            commands::get_heartbeat_event_name,
            commands::get_memory_chunk,
            commands::get_memory_session,
            commands::get_memory_stats,
            commands::get_session_change_event_name,
            commands::get_skill,
            commands::has_secret,
            commands::import_skill,
            commands::list_agents,
            commands::list_background_agents,
            commands::list_chat_session_summaries,
            commands::list_chat_sessions,
            commands::list_chat_sessions_by_agent,
            commands::list_chat_sessions_by_skill,
            commands::list_hooks,
            commands::list_memory_chunks,
            commands::list_memory_chunks_for_session,
            commands::list_memory_chunks_by_tag,
            commands::list_memory_sessions,
            commands::list_secrets,
            commands::list_skills,
            commands::list_tool_traces,
            commands::marketplace_check_gating,
            commands::marketplace_get_content,
            commands::marketplace_get_skill,
            commands::marketplace_get_versions,
            commands::marketplace_install_skill,
            commands::marketplace_list_installed,
            commands::marketplace_search,
            commands::marketplace_uninstall_skill,
            commands::pause_background_agent,
            commands::read_media_file,
            commands::rebuild_external_chat_session,
            commands::rename_chat_session,
            commands::resume_background_agent,
            commands::run_background_agent_streaming,
            commands::save_voice_message,
            commands::search_memory,
            commands::search_memory_advanced,
            commands::send_chat_message,
            commands::send_chat_message_stream,
            commands::send_live_audio_chunk,
            commands::start_cli_daemon,
            commands::start_live_transcription,
            commands::steer_chat_stream,
            commands::steer_task,
            commands::stop_cli_daemon,
            commands::stop_live_transcription,
            commands::test_hook,
            commands::transcribe_audio,
            commands::transcribe_audio_stream,
            commands::update_agent,
            commands::update_background_agent,
            commands::update_chat_session,
            commands::update_config,
            commands::update_hook,
            commands::update_secret,
            commands::update_skill,
            commands::restart_cli_daemon,
        ]
    };
}

pub fn build_ipc_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(collect_ipc_commands!())
}

pub fn export_ipc_bindings() -> Result<(), specta_typescript::ExportError> {
    build_ipc_builder().export(
        Typescript::default()
            .header("// @ts-nocheck\n/* eslint-disable */")
            .bigint(BigIntExportBehavior::Number),
        WEB_BINDINGS_PATH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    const COVERAGE_BASELINE: &str = include_str!("ipc_command_coverage.baseline.txt");

    fn extract_tauri_command_names(commands_dir: &Path) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let mut files: Vec<PathBuf> = fs::read_dir(commands_dir)
            .expect("read commands directory")
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .collect();
        files.sort();

        for path in files {
            let content = fs::read_to_string(&path).expect("read command source file");
            let mut expect_fn_signature = false;

            for raw_line in content.lines() {
                let line = raw_line.trim();
                if line.starts_with("#[tauri::command") {
                    expect_fn_signature = true;
                    continue;
                }

                if !expect_fn_signature {
                    continue;
                }

                if let Some((_, right)) = line.split_once("fn ")
                    && let Some(name) = right.split('(').next().map(str::trim)
                    && !name.is_empty()
                {
                    names.insert(name.to_string());
                    expect_fn_signature = false;
                }
            }
        }

        names
    }

    fn extract_bound_command_names(ipc_bindings_source: &str) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let mut inside_collect_macro = false;

        for raw_line in ipc_bindings_source.lines() {
            let line = raw_line.trim();
            if line.starts_with("tauri_specta::collect_commands![") {
                inside_collect_macro = true;
                continue;
            }
            if inside_collect_macro && line.starts_with("]") {
                break;
            }
            if !inside_collect_macro {
                continue;
            }

            if let Some(command_ref) = line.strip_prefix("commands::") {
                let name = command_ref.trim_end_matches(',').trim();
                if !name.is_empty() {
                    names.insert(name.to_string());
                }
            }
        }

        names
    }

    fn format_coverage_snapshot(
        tauri_commands: &BTreeSet<String>,
        bound_commands: &BTreeSet<String>,
    ) -> String {
        let unbound_commands: Vec<&String> = tauri_commands.difference(bound_commands).collect();
        let dangling_bound_commands: Vec<&String> =
            bound_commands.difference(tauri_commands).collect();

        let mut out = String::new();
        out.push_str("# tauri-ipc-command-coverage baseline\n");
        out.push_str("# format-version: 1\n");
        out.push_str(&format!("tauri_total={}\n", tauri_commands.len()));
        out.push_str(&format!("bound_total={}\n", bound_commands.len()));
        out.push_str(&format!(
            "dangling_total={}\n",
            dangling_bound_commands.len()
        ));
        out.push_str(&format!("unbound_total={}\n", unbound_commands.len()));
        out.push('\n');
        out.push_str("[unbound_commands]\n");
        if unbound_commands.is_empty() {
            out.push_str("(none)\n");
        } else {
            for command in unbound_commands {
                out.push_str(command);
                out.push('\n');
            }
        }
        out.push('\n');
        out.push_str("[dangling_bound_commands]\n");
        if dangling_bound_commands.is_empty() {
            out.push_str("(none)\n");
        } else {
            for command in dangling_bound_commands {
                out.push_str(command);
                out.push('\n');
            }
        }
        out
    }

    fn normalize_newlines(value: &str) -> String {
        value.replace("\r\n", "\n")
    }

    #[test]
    fn export_ipc_bindings_for_web() {
        export_ipc_bindings().expect("export tauri ipc bindings");
    }

    #[test]
    fn bound_commands_do_not_reference_missing_tauri_commands() {
        let tauri_commands = extract_tauri_command_names(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands"
        )));
        let bound_commands = extract_bound_command_names(include_str!("ipc_bindings.rs"));
        let dangling_bound: Vec<_> = bound_commands
            .difference(&tauri_commands)
            .map(String::as_str)
            .collect();

        assert!(
            dangling_bound.is_empty(),
            "Found dangling bound commands in collect_commands!: {:?}",
            dangling_bound
        );
    }

    #[test]
    fn critical_frontend_ipc_commands_are_bound() {
        let bound_commands = extract_bound_command_names(include_str!("ipc_bindings.rs"));
        let required = ["get_config", "update_config", "has_secret", "import_skill"];

        for command in required {
            assert!(
                bound_commands.contains(command),
                "Required IPC command '{}' is not bound in collect_commands!().",
                command
            );
        }
    }

    #[test]
    fn ipc_command_coverage_matches_baseline_snapshot() {
        let tauri_commands = extract_tauri_command_names(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/commands"
        )));
        let bound_commands = extract_bound_command_names(include_str!("ipc_bindings.rs"));
        let snapshot = format_coverage_snapshot(&tauri_commands, &bound_commands);

        assert_eq!(
            normalize_newlines(COVERAGE_BASELINE),
            normalize_newlines(&snapshot),
            "IPC command coverage baseline changed. Run scripts/check_tauri_ipc_command_coverage.sh to inspect diff and update baseline intentionally."
        );
    }
}
