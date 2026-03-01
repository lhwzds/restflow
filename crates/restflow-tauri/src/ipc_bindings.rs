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
            commands::create_secret,
            commands::create_skill,
            commands::delete_agent,
            commands::delete_background_agent,
            commands::delete_chat_session,
            commands::delete_secret,
            commands::delete_skill,
            commands::execute_chat_session,
            commands::export_skill,
            commands::get_agent,
            commands::get_available_models,
            commands::get_background_agent_events,
            commands::get_background_agent_stream_event_name,
            commands::get_chat_session,
            commands::get_heartbeat_event_name,
            commands::get_session_change_event_name,
            commands::get_skill,
            commands::list_agents,
            commands::list_background_agents,
            commands::list_chat_session_summaries,
            commands::list_chat_sessions,
            commands::list_chat_sessions_by_agent,
            commands::list_chat_sessions_by_skill,
            commands::list_memory_chunks_for_session,
            commands::list_memory_chunks_by_tag,
            commands::list_memory_sessions,
            commands::list_secrets,
            commands::list_skills,
            commands::list_tool_traces,
            commands::pause_background_agent,
            commands::read_media_file,
            commands::rebuild_external_chat_session,
            commands::rename_chat_session,
            commands::resume_background_agent,
            commands::run_background_agent_streaming,
            commands::save_voice_message,
            commands::send_chat_message,
            commands::send_chat_message_stream,
            commands::send_live_audio_chunk,
            commands::start_live_transcription,
            commands::steer_chat_stream,
            commands::steer_task,
            commands::stop_live_transcription,
            commands::transcribe_audio,
            commands::transcribe_audio_stream,
            commands::update_agent,
            commands::update_chat_session,
            commands::update_secret,
            commands::update_skill,
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

    #[test]
    fn export_ipc_bindings_for_web() {
        export_ipc_bindings().expect("export tauri ipc bindings");
    }
}
