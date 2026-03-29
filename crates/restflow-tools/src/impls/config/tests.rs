use super::*;
use restflow_storage::ConfigStorage;
use restflow_traits::config_types::{CliConfig, ConfigDocument, SystemConfig};
use restflow_traits::store::ConfigStore;
use std::env;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

struct EnvGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set_path(key: &'static str, path: &Path) -> Self {
        let original = env::var_os(key);
        unsafe {
            env::set_var(key, path);
        }
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            unsafe {
                env::set_var(self.key, value);
            }
        } else {
            unsafe {
                env::remove_var(self.key);
            }
        }
    }
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct TestContext {
    store: Arc<dyn ConfigStore>,
    _temp_dir: tempfile::TempDir,
    _global_guard: EnvGuard,
    _env_lock: std::sync::MutexGuard<'static, ()>,
}

struct TestConfigStore {
    storage: Arc<ConfigStorage>,
}

impl TestConfigStore {
    fn new(storage: Arc<ConfigStorage>) -> Self {
        Self { storage }
    }
}

fn config_error(e: impl std::fmt::Display) -> restflow_traits::ToolError {
    restflow_traits::ToolError::Tool(format!(
        "Config storage error: {e}. The config file may be missing, invalid, or inaccessible."
    ))
}

impl ConfigStore for TestConfigStore {
    fn get_effective_config(&self) -> restflow_traits::error::Result<ConfigDocument> {
        let system = self.storage.get_effective_config().map_err(config_error)?;
        let system = serde_json::from_value(serde_json::to_value(system).map_err(config_error)?)
            .map_err(config_error)?;
        Ok(ConfigDocument::from_system_config(
            system,
            CliConfig::default(),
        ))
    }

    fn get_writable_config(&self) -> restflow_traits::error::Result<ConfigDocument> {
        let system = self.storage.get_global_config().map_err(config_error)?;
        let system = serde_json::from_value(serde_json::to_value(system).map_err(config_error)?)
            .map_err(config_error)?;
        Ok(ConfigDocument::from_system_config(
            system,
            CliConfig::default(),
        ))
    }

    fn persist_config(&self, config: &ConfigDocument) -> restflow_traits::error::Result<()> {
        let system = serde_json::from_value(
            serde_json::to_value(config.system_config()).map_err(config_error)?,
        )
        .map_err(config_error)?;
        self.storage.update_config(system).map_err(config_error)?;
        Ok(())
    }

    fn reset_config(&self) -> restflow_traits::error::Result<ConfigDocument> {
        let doc = ConfigDocument::from_system_config(SystemConfig::default(), CliConfig::default());
        let system = serde_json::from_value(
            serde_json::to_value(doc.system_config()).map_err(config_error)?,
        )
        .map_err(config_error)?;
        self.storage.update_config(system).map_err(config_error)?;
        Ok(doc)
    }
}

fn setup_storage() -> TestContext {
    let env_guard = env_lock();
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let global_guard = EnvGuard::set_path("RESTFLOW_GLOBAL_CONFIG", &config_path);
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(redb::Database::create(db_path).unwrap());
    let storage = Arc::new(ConfigStorage::new(db).unwrap());
    let store: Arc<dyn ConfigStore> = Arc::new(TestConfigStore::new(storage));
    TestContext {
        store,
        _temp_dir: temp_dir,
        _global_guard: global_guard,
        _env_lock: env_guard,
    }
}

#[tokio::test]
async fn test_get_config() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store);

    let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
    assert!(output.success);
    assert!(
        output
            .result
            .pointer("/system/worker_count")
            .and_then(|value| value.as_u64())
            .is_some()
    );
    assert!(
        output.result.get("cli").is_none(),
        "daemon-facing config view must omit cli-local settings"
    );
}

#[tokio::test]
async fn test_set_requires_write() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store);

    let result = tool
        .execute(json!({
            "operation": "set",
            "key": "system.worker_count",
            "value": 8
        }))
        .await;
    let err = result.expect_err("expected write-guard error");
    assert!(
        err.to_string()
            .contains("Available read-only operations: get, show, list")
    );
}

#[tokio::test]
async fn test_set_rejects_unknown_field_with_valid_fields_hint() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let result = tool
        .execute(json!({
            "operation": "set",
            "key": "invalid_field",
            "value": 8
        }))
        .await;
    let err = result.expect_err("expected unknown field error");
    let message = err.to_string();

    assert!(message.contains("Unknown config field: 'invalid_field'"));
    assert!(
        message
            .contains("Valid fields: system.*, agent.*, api.*, runtime.*, channel.*, registry.*.")
    );
}

#[tokio::test]
async fn test_set_experimental_features() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let output = tool
        .execute(json!({
            "operation": "set",
            "key": "system.experimental_features",
            "value": ["plan_mode", "websocket_transport"]
        }))
        .await
        .unwrap();
    assert!(output.success);
    let values = output
        .result
        .pointer("/system/experimental_features")
        .and_then(|value| value.as_array())
        .expect("experimental_features should be an array");
    assert_eq!(values.len(), 2);
}

#[tokio::test]
async fn test_set_optional_timeout_with_null() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let output = tool
        .execute(json!({
            "operation": "set",
            "key": "system.background_api_timeout_seconds",
            "value": null
        }))
        .await
        .unwrap();
    assert!(output.success);
    assert!(
        output
            .result
            .pointer("/system/background_api_timeout_seconds")
            .is_some_and(|v| v.is_null())
    );
}

#[tokio::test]
async fn test_set_agent_max_wall_clock_with_null() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let output = tool
        .execute(json!({
            "operation": "set",
            "key": "agent.max_wall_clock_secs",
            "value": null
        }))
        .await
        .unwrap();
    assert!(output.success);
    let agent = output
        .result
        .get("agent")
        .expect("agent block should exist");
    assert!(
        agent
            .get("max_wall_clock_secs")
            .is_some_and(|v| v.is_null())
    );
}

#[tokio::test]
async fn test_list_supported_fields() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store);

    let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
    assert!(output.success);
    let fields = output
        .result
        .get("fields")
        .and_then(|v| v.as_array())
        .expect("fields should be an array");
    assert!(
        fields
            .iter()
            .any(|field| field.as_str() == Some("system.log_file_retention_days")),
        "list should expose system.log_file_retention_days"
    );
}

#[tokio::test]
async fn test_listed_retention_fields_are_settable() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let updates = [
        ("system.chat_session_retention_days", json!(0)),
        ("system.background_task_retention_days", json!(14)),
        ("system.checkpoint_retention_days", json!(5)),
        ("system.memory_chunk_retention_days", json!(120)),
        ("system.log_file_retention_days", json!(30)),
    ];

    for (key, value) in updates {
        let output = tool
            .execute(json!({
                "operation": "set",
                "key": key,
                "value": value
            }))
            .await
            .unwrap_or_else(|err| panic!("set should support listed field '{key}': {err}"));
        assert!(
            output.success,
            "set should succeed for listed field '{key}'"
        );
    }
}

#[tokio::test]
async fn test_set_agent_defaults() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let updates = [
        ("agent.tool_timeout_secs", json!(180)),
        ("agent.llm_timeout_secs", json!(900)),
        ("agent.bash_timeout_secs", json!(600)),
        ("agent.python_timeout_secs", json!(60)),
        ("agent.browser_timeout_secs", json!(240)),
        ("agent.process_session_ttl_secs", json!(5400)),
        ("agent.approval_timeout_secs", json!(420)),
        ("agent.max_iterations", json!(50)),
        ("agent.max_depth", json!(4)),
        ("agent.subagent_timeout_secs", json!(900)),
        ("agent.max_parallel_subagents", json!(12)),
        ("agent.max_tool_calls", json!(300)),
        ("agent.max_tool_concurrency", json!(24)),
        ("agent.max_tool_result_length", json!(8192)),
        ("agent.prune_tool_max_chars", json!(4096)),
        ("agent.compact_preserve_tokens", json!(16000)),
        ("agent.max_wall_clock_secs", json!(3600)),
        ("agent.default_task_timeout_secs", json!(3600)),
        ("agent.default_max_duration_secs", json!(3600)),
        ("agent.fallback_models", json!(["glm-5", "gpt-5"])),
    ];

    for (key, value) in updates {
        let output = tool
            .execute(json!({
                "operation": "set",
                "key": key,
                "value": value
            }))
            .await
            .unwrap_or_else(|err| panic!("set should support agent field '{key}': {err}"));
        assert!(output.success, "set should succeed for agent field '{key}'");
    }

    // Verify the values persisted
    let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
    let agent = output
        .result
        .get("agent")
        .expect("agent block should exist");
    assert_eq!(
        agent.get("tool_timeout_secs").and_then(|v| v.as_u64()),
        Some(180)
    );
    assert_eq!(
        agent.get("llm_timeout_secs").and_then(|v| v.as_u64()),
        Some(900)
    );
    assert_eq!(
        agent.get("bash_timeout_secs").and_then(|v| v.as_u64()),
        Some(600)
    );
    assert_eq!(
        agent.get("max_iterations").and_then(|v| v.as_u64()),
        Some(50)
    );
    assert_eq!(agent.get("max_depth").and_then(|v| v.as_u64()), Some(4));
    assert_eq!(
        agent.get("browser_timeout_secs").and_then(|v| v.as_u64()),
        Some(240)
    );
    assert_eq!(
        agent
            .get("process_session_ttl_secs")
            .and_then(|v| v.as_u64()),
        Some(5400)
    );
    assert_eq!(
        agent.get("approval_timeout_secs").and_then(|v| v.as_u64()),
        Some(420)
    );
    assert_eq!(
        agent.get("max_parallel_subagents").and_then(|v| v.as_u64()),
        Some(12)
    );
    assert_eq!(
        agent.get("max_tool_concurrency").and_then(|v| v.as_u64()),
        Some(24)
    );
    assert_eq!(
        agent.get("max_tool_result_length").and_then(|v| v.as_u64()),
        Some(8192)
    );
    assert_eq!(
        agent.get("prune_tool_max_chars").and_then(|v| v.as_u64()),
        Some(4096)
    );
    assert_eq!(
        agent
            .get("compact_preserve_tokens")
            .and_then(|v| v.as_u64()),
        Some(16000)
    );
    assert_eq!(
        agent.get("fallback_models"),
        Some(&json!(["glm-5", "gpt-5"]))
    );
}

#[tokio::test]
async fn test_set_agent_unknown_field() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let result = tool
        .execute(json!({
            "operation": "set",
            "key": "agent.nonexistent",
            "value": 42
        }))
        .await;
    let err = result.expect_err("expected unknown agent field error");
    assert!(err.to_string().contains("Unknown agent config field"));
}

#[tokio::test]
async fn test_get_includes_agent_defaults() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store);

    let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
    assert!(output.success);
    let agent = output
        .result
        .get("agent")
        .expect("agent block should exist");
    assert!(agent.get("tool_timeout_secs").is_some());
    assert!(agent.get("llm_timeout_secs").is_some());
    assert!(agent.get("bash_timeout_secs").is_some());
    assert!(agent.get("browser_timeout_secs").is_some());
    assert!(agent.get("process_session_ttl_secs").is_some());
    assert!(agent.get("approval_timeout_secs").is_some());
    assert!(agent.get("max_iterations").is_some());
    assert!(agent.get("max_tool_concurrency").is_some());
    assert!(agent.get("max_tool_result_length").is_some());
    assert!(agent.get("prune_tool_max_chars").is_some());
    assert!(agent.get("compact_preserve_tokens").is_some());
    assert!(agent.get("fallback_models").is_some());
}

#[tokio::test]
async fn test_set_runtime_channel_and_registry_defaults() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let updates = [
        ("runtime.background_runner_poll_interval_ms", json!(15000)),
        ("runtime.background_runner_max_concurrent_tasks", json!(8)),
        ("runtime.chat_max_session_history", json!(40)),
        ("channel.telegram_api_timeout_secs", json!(45)),
        ("channel.telegram_polling_timeout_secs", json!(55)),
        ("registry.github_cache_ttl_secs", json!(900)),
        ("registry.marketplace_cache_ttl_secs", json!(450)),
    ];

    for (key, value) in updates {
        let output = tool
            .execute(json!({
                "operation": "set",
                "key": key,
                "value": value
            }))
            .await
            .unwrap_or_else(|err| panic!("set should support config field '{key}': {err}"));
        assert!(
            output.success,
            "set should succeed for config field '{key}'"
        );
    }

    let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
    assert_eq!(
        output
            .result
            .pointer("/runtime/background_runner_poll_interval_ms")
            .and_then(|value| value.as_u64()),
        Some(15000)
    );
    assert_eq!(
        output
            .result
            .pointer("/runtime/background_runner_max_concurrent_tasks")
            .and_then(|value| value.as_u64()),
        Some(8)
    );
    assert_eq!(
        output
            .result
            .pointer("/runtime/chat_max_session_history")
            .and_then(|value| value.as_u64()),
        Some(40)
    );
    assert_eq!(
        output
            .result
            .pointer("/channel/telegram_api_timeout_secs")
            .and_then(|value| value.as_u64()),
        Some(45)
    );
    assert_eq!(
        output
            .result
            .pointer("/channel/telegram_polling_timeout_secs")
            .and_then(|value| value.as_u64()),
        Some(55)
    );
    assert_eq!(
        output
            .result
            .pointer("/registry/github_cache_ttl_secs")
            .and_then(|value| value.as_u64()),
        Some(900)
    );
    assert_eq!(
        output
            .result
            .pointer("/registry/marketplace_cache_ttl_secs")
            .and_then(|value| value.as_u64()),
        Some(450)
    );
}

#[tokio::test]
async fn test_set_agent_fallback_models_allows_null_clear() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    tool.execute(json!({
        "operation": "set",
        "key": "agent.fallback_models",
        "value": ["glm-5", "gpt-5"]
    }))
    .await
    .expect("initial fallback_models set should succeed");

    let output = tool
        .execute(json!({
            "operation": "set",
            "key": "agent.fallback_models",
            "value": null
        }))
        .await
        .expect("clearing fallback_models should succeed");

    assert!(output.success);
    let agent = output
        .result
        .get("agent")
        .expect("agent block should exist");
    assert!(
        agent
            .get("fallback_models")
            .is_some_and(|value| value.is_null())
    );
}

#[tokio::test]
async fn test_set_rejects_cli_fields() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let err = tool
        .execute(json!({
            "operation": "set",
            "key": "cli.model",
            "value": "gpt-5"
        }))
        .await
        .expect_err("cli fields should be rejected by daemon-facing config");

    assert!(
        err.to_string()
            .contains("Unknown config field: 'cli.model'")
    );
}

#[tokio::test]
async fn test_set_rejects_cli_block_in_full_config_payload() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let err = tool
        .execute(json!({
            "operation": "set",
            "config": {
                "system": {
                    "worker_count": 6
                },
                "cli": {
                    "model": "gpt-5"
                }
            }
        }))
        .await
        .expect_err("cli block should be rejected for daemon-facing config writes");

    assert!(
        err.to_string()
            .contains("CLI-local config fields are not available through manage_config")
    );
}

#[tokio::test]
async fn test_set_rejects_default_cli_block_in_full_config_payload() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let err = tool
        .execute(json!({
            "operation": "set",
            "config": {
                "system": {
                    "worker_count": 6
                },
                "cli": {}
            }
        }))
        .await
        .expect_err("default cli block should still be rejected");

    assert!(
        err.to_string()
            .contains("CLI-local config fields are not available through manage_config")
    );
}

#[tokio::test]
async fn test_set_api_defaults() {
    let ctx = setup_storage();
    let tool = ConfigTool::new(ctx.store).with_write(true);

    let updates = [
        ("api.memory_search_limit", json!(25)),
        ("api.session_list_limit", json!(30)),
        ("api.background_progress_event_limit", json!(12)),
        ("api.background_message_list_limit", json!(60)),
        ("api.background_trace_list_limit", json!(80)),
        ("api.background_trace_line_limit", json!(300)),
        ("api.web_search_num_results", json!(7)),
        ("api.diagnostics_timeout_ms", json!(9000)),
    ];

    for (key, value) in updates {
        let output = tool
            .execute(json!({
                "operation": "set",
                "key": key,
                "value": value
            }))
            .await
            .unwrap_or_else(|err| panic!("set should support api field '{key}': {err}"));
        assert!(output.success, "set should succeed for api field '{key}'");
    }

    let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
    let api = output.result.get("api").expect("api block should exist");
    assert_eq!(
        api.get("web_search_num_results").and_then(|v| v.as_u64()),
        Some(7)
    );
    assert_eq!(
        api.get("diagnostics_timeout_ms").and_then(|v| v.as_u64()),
        Some(9000)
    );
}
