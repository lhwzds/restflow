pub(super) use super::runtime::{
    build_agent_system_prompt, cancel_chat_stream, load_chat_max_session_history_from_core,
    persist_ipc_user_message_if_needed, record_turn_event_in_session_store, steer_chat_stream,
    subagent_config_from_defaults,
};
pub(super) use super::*;
pub(super) use crate::models::AgentNode;
pub(super) use crate::prompt_files;
pub(super) use crate::test_support::RestflowTestEnv;
pub(super) use restflow_contracts::ToolExecutionResult;
pub(super) use restflow_traits::SteerCommand;
pub(super) use restflow_traits::store::ReplySender;
pub(super) use restflow_traits::tool::ToolErrorCategory;
pub(super) use uuid::Uuid;

pub(super) struct TestCoreEnv {
    #[allow(dead_code)]
    pub state: RestflowTestEnv,
}

#[allow(clippy::await_holding_lock)]
pub(super) async fn create_test_core() -> (Arc<AppCore>, TestCoreEnv) {
    let state = RestflowTestEnv::new();
    let db_path = state.db_path("ipc-server-test.db");
    let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
    (core, TestCoreEnv { state })
}

#[tokio::test]
async fn create_test_core_isolates_restflow_dir_env() {
    let first_state_path = {
        let (_core, env) = create_test_core().await;
        let current = std::env::var("RESTFLOW_DIR").expect("restflow dir env");
        assert_eq!(current, env.state.root().to_string_lossy());
        assert!(std::env::var_os(prompt_files::AGENTS_DIR_ENV).is_none());
        current
    };

    let second_state_path = {
        let (_core, env) = create_test_core().await;
        let current = std::env::var("RESTFLOW_DIR").expect("restflow dir env");
        assert_eq!(current, env.state.root().to_string_lossy());
        assert!(std::env::var_os(prompt_files::AGENTS_DIR_ENV).is_none());
        current
    };

    assert_ne!(first_state_path, second_state_path);
}

mod agents;
mod runtime_tools;
mod sessions;
mod system;
