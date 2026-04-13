pub(super) use super::runtime::{
    build_agent_system_prompt, load_chat_max_session_history_from_core,
    persist_ipc_user_message_if_needed, steer_chat_stream, subagent_config_from_defaults,
};
pub(super) use super::*;
pub(super) use crate::models::{AgentNode, ChannelSessionBinding, Skill};
pub(super) use crate::prompt_files;
pub(super) use restflow_contracts::ToolExecutionResult;
pub(super) use restflow_traits::SteerCommand;
pub(super) use restflow_traits::store::ReplySender;
pub(super) use restflow_traits::tool::ToolErrorCategory;
pub(super) use tempfile::tempdir;
pub(super) use uuid::Uuid;

pub(super) struct AgentsDirEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl AgentsDirEnvGuard {
    fn new() -> Self {
        Self {
            _lock: prompt_files::agents_dir_env_lock(),
        }
    }
}

impl Drop for AgentsDirEnvGuard {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(prompt_files::AGENTS_DIR_ENV) };
    }
}

pub(super) struct TestCoreEnv {
    #[allow(dead_code)]
    pub db_dir: tempfile::TempDir,
    #[allow(dead_code)]
    pub agents_dir: tempfile::TempDir,
    #[allow(dead_code)]
    pub env_guard: AgentsDirEnvGuard,
}

#[allow(clippy::await_holding_lock)]
pub(super) async fn create_test_core() -> (Arc<AppCore>, TestCoreEnv) {
    let env_guard = AgentsDirEnvGuard::new();
    let temp_db = tempdir().expect("tempdir");
    let temp_agents = tempdir().expect("agents tempdir");
    unsafe { std::env::set_var(prompt_files::AGENTS_DIR_ENV, temp_agents.path()) };
    let db_path = temp_db.path().join("ipc-server-test.db");
    let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
    (
        core,
        TestCoreEnv {
            db_dir: temp_db,
            agents_dir: temp_agents,
            env_guard,
        },
    )
}

#[tokio::test]
async fn create_test_core_isolates_agents_dir_env() {
    let agents_path = {
        let (_core, env) = create_test_core().await;
        let current = std::env::var(prompt_files::AGENTS_DIR_ENV).expect("agents dir env");
        assert_eq!(current, env.agents_dir.path().to_string_lossy());
        current
    };

    assert!(
        std::env::var(prompt_files::AGENTS_DIR_ENV).is_err(),
        "agents dir env should be cleared after test env drop: {agents_path}"
    );
}

mod agents;
mod memory;
mod runtime_tools;
mod sessions;
mod system;
