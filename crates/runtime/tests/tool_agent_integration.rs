//! Integration tests for the minimal tool registry surface.

use runtime::tools::{BashConfig, FileConfig, ToolRegistryBuilder};

#[test]
fn test_minimal_registry_excludes_external_capabilities() {
    let registry = ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .build();

    assert!(registry.has("bash"));
    assert!(registry.has("file"));

    for tool_name in [
        "http_request",
        "send_email",
        "telegram_send",
        "discord_send",
        "slack_send",
        "browser",
        "web_search",
        "web_fetch",
        "jina_reader",
        "transcribe",
        "vision",
    ] {
        assert!(!registry.has(tool_name), "unexpected {tool_name}");
    }
}
