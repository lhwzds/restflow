//! Integration tests for the AI agent

use restflow_ai::{AgentConfig, AgentExecutor, HttpTool, OpenAIClient, ToolRegistry};
use std::sync::Arc;

fn disable_system_proxy_for_tests() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // Safety: set once for the process before any HTTP clients are built.
        unsafe {
            std::env::set_var("RESTFLOW_DISABLE_SYSTEM_PROXY", "1");
        }
    });
}

#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY environment variable
async fn test_agent_with_http_tool() {
    disable_system_proxy_for_tests();
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");

    let llm = Arc::new(OpenAIClient::new(api_key));
    let mut tools = ToolRegistry::new();
    tools.register(HttpTool::new());

    let mut executor = AgentExecutor::new(llm, Arc::new(tools));
    let config =
        AgentConfig::new("What is my IP address? Use the http tool to check httpbin.org/ip")
            .with_max_iterations(5);

    let result = executor.run(config).await.unwrap();

    assert!(result.success);
    assert!(result.answer.is_some());
    println!("Answer: {}", result.answer.unwrap());
}

#[tokio::test]
#[ignore] // Requires ANTHROPIC_API_KEY environment variable
async fn test_agent_with_anthropic() {
    use restflow_ai::AnthropicClient;

    disable_system_proxy_for_tests();
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY required");

    let llm = Arc::new(AnthropicClient::new(api_key));
    let mut tools = ToolRegistry::new();
    tools.register(HttpTool::new());

    let mut executor = AgentExecutor::new(llm, Arc::new(tools));
    let config = AgentConfig::new(
        "Use the http tool to fetch https://httpbin.org/get and tell me what the origin IP is",
    )
    .with_max_iterations(5);

    let result = executor.run(config).await.unwrap();

    assert!(result.success);
    assert!(result.answer.is_some());
    println!("Answer: {}", result.answer.unwrap());
}

#[tokio::test]
async fn test_tool_registry() {
    use restflow_ai::{EmailTool, PythonTool};

    disable_system_proxy_for_tests();
    let mut registry = ToolRegistry::new();
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());

    assert!(registry.has("http_request"));
    assert!(registry.has("run_python"));
    assert!(registry.has("send_email"));
    assert!(!registry.has("unknown"));

    let schemas = registry.schemas();
    assert_eq!(schemas.len(), 3);
}

#[tokio::test]
async fn test_python_tool_execution() {
    use restflow_ai::PythonTool;
    use restflow_ai::Tool;
    use serde_json::json;

    let tool = PythonTool::new();
    let input = json!({
        "code": "print(2 + 2)"
    });

    let result = tool.execute(input).await.unwrap();
    assert!(result.success);
    assert_eq!(result.result["stdout"], "4");
}

#[tokio::test]
async fn test_email_tool_dry_run() {
    use restflow_ai::EmailTool;
    use restflow_ai::Tool;
    use serde_json::json;

    let tool = EmailTool::new();
    let input = json!({
        "to": "test@example.com",
        "subject": "Test Subject",
        "body": "Test body content"
    });

    let result = tool.execute(input).await.unwrap();
    assert!(result.success);
    assert_eq!(result.result["dry_run"], true);
}
