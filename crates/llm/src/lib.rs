//! LLM clients and streaming abstractions for RestFlow.

pub mod error {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum AiError {
        #[error("LLM error: {0}")]
        Llm(String),

        #[error("{provider} API error ({status}): {message}")]
        LlmHttp {
            provider: String,
            status: u16,
            message: String,
            retry_after_secs: Option<u64>,
        },

        #[error("Invalid response format: {0}")]
        InvalidFormat(String),

        #[error("HTTP error: {0}")]
        Http(#[from] reqwest::Error),

        #[error("JSON error: {0}")]
        Json(#[from] serde_json::Error),

        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),
    }

    impl AiError {
        pub fn is_retryable(&self) -> bool {
            match self {
                Self::LlmHttp { status, .. } => matches!(status, 429 | 500 | 502 | 503 | 504),
                Self::Http(err) => err.is_timeout() || err.is_connect(),
                Self::Llm(message) => {
                    let lower = message.to_lowercase();
                    lower.contains("timeout")
                        || lower.contains("rate limit")
                        || lower.contains("429")
                        || lower.contains("503")
                        || lower.contains("usage limit")
                        || lower.contains("quota")
                        || lower.contains("rollout")
                        || lower.contains("state db")
                }
                _ => false,
            }
        }

        pub fn retry_after(&self) -> Option<u64> {
            match self {
                Self::LlmHttp {
                    retry_after_secs, ..
                } => *retry_after_secs,
                _ => None,
            }
        }
    }

    pub type Result<T> = std::result::Result<T, AiError>;
}

mod client {
    //! LLM client trait and types

    use std::pin::Pin;

    use async_trait::async_trait;
    use futures::Stream;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    use crate::error::Result;
    use types::ToolSchema;

    /// Chat message role
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    pub enum Role {
        System,
        User,
        Assistant,
        Tool,
    }

    /// Chat message
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Message {
        pub role: Role,
        pub content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tool_call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        /// Tool calls made by the assistant (for assistant messages)
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tool_calls: Option<Vec<ToolCall>>,
        /// Provider-specific reasoning content (e.g. DeepSeek reasoning_content).
        /// Must be round-tripped back to the API when present on assistant messages
        /// with tool_calls, otherwise the provider returns 400.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub reasoning_content: Option<String>,
    }

    impl Message {
        /// Create a system message
        pub fn system(content: impl Into<String>) -> Self {
            Self {
                role: Role::System,
                content: content.into(),
                tool_call_id: None,
                name: None,
                tool_calls: None,
                reasoning_content: None,
            }
        }

        /// Create a user message
        pub fn user(content: impl Into<String>) -> Self {
            Self {
                role: Role::User,
                content: content.into(),
                tool_call_id: None,
                name: None,
                tool_calls: None,
                reasoning_content: None,
            }
        }

        /// Create an assistant message
        pub fn assistant(content: impl Into<String>) -> Self {
            Self {
                role: Role::Assistant,
                content: content.into(),
                tool_call_id: None,
                name: None,
                tool_calls: None,
                reasoning_content: None,
            }
        }

        /// Create an assistant message with tool calls
        pub fn assistant_with_tool_calls(
            content: Option<String>,
            tool_calls: Vec<ToolCall>,
        ) -> Self {
            Self {
                role: Role::Assistant,
                content: content.unwrap_or_default(),
                tool_call_id: None,
                name: None,
                tool_calls: Some(tool_calls),
                reasoning_content: None,
            }
        }

        /// Create an assistant message with tool calls and reasoning content
        pub fn assistant_with_tool_calls_and_reasoning(
            content: Option<String>,
            tool_calls: Vec<ToolCall>,
            reasoning_content: Option<String>,
        ) -> Self {
            Self {
                role: Role::Assistant,
                content: content.unwrap_or_default(),
                tool_call_id: None,
                name: None,
                tool_calls: Some(tool_calls),
                reasoning_content,
            }
        }

        /// Create a tool result message
        pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
            Self {
                role: Role::Tool,
                content: content.into(),
                tool_call_id: Some(tool_call_id.into()),
                name: None,
                tool_calls: None,
                reasoning_content: None,
            }
        }
    }

    /// Tool call request from LLM
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolCall {
        pub id: String,
        pub name: String,
        pub arguments: Value,
    }

    /// LLM completion response
    #[derive(Debug, Clone)]
    pub struct CompletionResponse {
        pub content: Option<String>,
        pub tool_calls: Vec<ToolCall>,
        pub finish_reason: FinishReason,
        pub usage: Option<TokenUsage>,
        /// Provider-specific reasoning content (e.g. DeepSeek reasoning_content).
        /// When present, must be stored in the assistant message so it can be
        /// sent back to the API on subsequent requests.
        pub reasoning_content: Option<String>,
    }

    /// Reason for completion
    #[derive(Debug, Clone, PartialEq)]
    pub enum FinishReason {
        Stop,
        ToolCalls,
        MaxTokens,
        Error,
    }

    /// Token usage statistics
    #[derive(Debug, Clone, Default)]
    pub struct TokenUsage {
        pub prompt_tokens: u32,
        pub completion_tokens: u32,
        pub total_tokens: u32,
        pub cost_usd: Option<f64>,
    }

    /// A chunk of streamed response
    #[derive(Debug, Clone)]
    pub struct StreamChunk {
        /// Text content in this chunk
        pub text: String,
        /// Thinking/reasoning content (for extended thinking models)
        pub thinking: Option<String>,
        /// Tool call being built incrementally
        pub tool_call_delta: Option<ToolCallDelta>,
        /// Finish reason (set on final chunk)
        pub finish_reason: Option<FinishReason>,
        /// Usage statistics (typically on final chunk)
        pub usage: Option<TokenUsage>,
    }

    impl StreamChunk {
        /// Create a text chunk
        pub fn text(text: impl Into<String>) -> Self {
            Self {
                text: text.into(),
                thinking: None,
                tool_call_delta: None,
                finish_reason: None,
                usage: None,
            }
        }

        /// Create a thinking chunk
        pub fn thinking(content: impl Into<String>) -> Self {
            Self {
                text: String::new(),
                thinking: Some(content.into()),
                tool_call_delta: None,
                finish_reason: None,
                usage: None,
            }
        }

        /// Create a final chunk with usage
        pub fn final_chunk(finish_reason: FinishReason, usage: Option<TokenUsage>) -> Self {
            Self {
                text: String::new(),
                thinking: None,
                tool_call_delta: None,
                finish_reason: Some(finish_reason),
                usage,
            }
        }

        /// Check if this is an empty chunk
        pub fn is_empty(&self) -> bool {
            self.text.is_empty()
                && self.thinking.is_none()
                && self.tool_call_delta.is_none()
                && self.finish_reason.is_none()
        }
    }

    /// Delta for incremental tool call building
    #[derive(Debug, Clone)]
    pub struct ToolCallDelta {
        /// Tool call index
        pub index: usize,
        /// Tool call ID (may be partial)
        pub id: Option<String>,
        /// Tool name (may be partial)
        pub name: Option<String>,
        /// Arguments JSON fragment
        pub arguments: Option<String>,
    }

    /// Type alias for boxed stream of chunks
    pub type StreamResult = Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>;

    /// LLM completion request
    #[derive(Debug, Clone)]
    pub struct CompletionRequest {
        pub messages: Vec<Message>,
        pub tools: Vec<ToolSchema>,
        pub temperature: Option<f32>,
        pub max_tokens: Option<u32>,
    }

    impl CompletionRequest {
        /// Create a new completion request
        pub fn new(messages: Vec<Message>) -> Self {
            Self {
                messages,
                tools: vec![],
                temperature: None,
                max_tokens: None,
            }
        }

        /// Add tools to the request
        pub fn with_tools(mut self, tools: Vec<ToolSchema>) -> Self {
            self.tools = tools;
            self
        }

        /// Set temperature
        pub fn with_temperature(mut self, temp: f32) -> Self {
            self.temperature = Some(temp);
            self
        }

        /// Set max tokens
        pub fn with_max_tokens(mut self, tokens: u32) -> Self {
            self.max_tokens = Some(tokens);
            self
        }
    }

    /// LLM client trait
    #[async_trait]
    pub trait LlmClient: Send + Sync {
        /// Get provider name
        fn provider(&self) -> &str;

        /// Get model name
        fn model(&self) -> &str;

        /// Complete a chat request (non-streaming)
        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

        /// Complete a chat request with streaming response
        ///
        /// Returns a stream of chunks that can be processed as they arrive.
        /// The final chunk will contain the finish_reason and usage statistics.
        fn complete_stream(&self, request: CompletionRequest) -> StreamResult;

        /// Check if this client supports streaming
        fn supports_streaming(&self) -> bool {
            true
        }
    }
}

pub mod cli {
    mod utils {
        //! Shared utilities for CLI-based LLM providers.

        use std::path::{Path, PathBuf};

        use crate::client::Role;
        use crate::error::{AiError, Result};

        /// Build a prompt string from messages, excluding system messages.
        pub fn build_prompt(messages: &[crate::Message]) -> String {
            messages
                .iter()
                .filter(|m| m.role != Role::System)
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        }

        /// Resolve a CLI executable by checking an env override, PATH, and fallback locations.
        pub fn resolve_executable(
            name: &str,
            override_env: &str,
            fallbacks: &[PathBuf],
        ) -> Result<PathBuf> {
            if let Ok(raw) = std::env::var(override_env)
                && !raw.trim().is_empty()
            {
                let path = PathBuf::from(raw);
                if is_executable(&path) {
                    return Ok(path);
                }
                return Err(AiError::Llm(format!(
                    "{} points to non-executable path: {}",
                    override_env,
                    path.display()
                )));
            }

            if let Some(path) = resolve_from_path(name) {
                return Ok(path);
            }

            for fallback in fallbacks {
                if is_executable(fallback) {
                    return Ok(fallback.clone());
                }
            }

            Err(AiError::Llm(format!(
                "Failed to locate '{}' executable in PATH or fallback locations",
                name
            )))
        }

        /// Search PATH for an executable by name.
        pub fn resolve_from_path(name: &str) -> Option<PathBuf> {
            let path_value = std::env::var_os("PATH")?;
            for entry in std::env::split_paths(&path_value) {
                let candidate = entry.join(name);
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
            None
        }

        /// Standard fallback paths for a CLI executable name.
        ///
        /// Checks `/opt/homebrew/bin/{name}`, `/usr/local/bin/{name}`,
        /// `/usr/bin/{name}`, and `~/.local/bin/{name}`.
        pub fn standard_fallbacks(name: &str) -> Vec<PathBuf> {
            let mut paths = vec![
                PathBuf::from(format!("/opt/homebrew/bin/{name}")),
                PathBuf::from(format!("/usr/local/bin/{name}")),
                PathBuf::from(format!("/usr/bin/{name}")),
            ];
            if let Some(home) = dirs::home_dir() {
                paths.push(home.join(".local").join("bin").join(name));
            }
            paths
        }

        /// Parse a JSON response that has a `"response"` field and an optional `"error"` field.
        ///
        /// Used by Gemini CLI and OpenCode CLI which share the same output format.
        pub fn parse_json_response(output: &str, provider: &str) -> Result<String> {
            let value: serde_json::Value = serde_json::from_str(output.trim()).map_err(|e| {
                AiError::Llm(format!("Failed to parse {} CLI output: {e}", provider))
            })?;

            if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
                return Err(AiError::Llm(format!("{} CLI error: {err}", provider)));
            }

            let response = value
                .get("response")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AiError::Llm(format!("{} CLI output missing 'response' field", provider))
                })?;

            if response.trim().is_empty() {
                return Err(AiError::Llm(format!(
                    "{} CLI returned empty output",
                    provider
                )));
            }

            Ok(response.to_string())
        }

        /// Execute a CLI command and return its stdout as a string.
        ///
        /// Returns an error if the command fails to spawn or exits with non-zero status.
        pub async fn execute_cli_command(
            mut cmd: tokio::process::Command,
            provider: &str,
            install_hint: &str,
        ) -> Result<String> {
            let output = cmd.output().await.map_err(|e| {
                AiError::Llm(format!(
                    "Failed to run {} CLI: {}. {}",
                    provider, e, install_hint
                ))
            })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(AiError::Llm(format!("{} CLI error: {}", provider, stderr)));
            }
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }

        /// Return a stream that immediately yields an "unsupported" error.
        pub fn unsupported_stream(provider: &str) -> crate::StreamResult {
            let msg = format!("Streaming not supported with {}", provider);
            Box::pin(async_stream::stream! {
                yield Err(AiError::Llm(msg));
            })
        }

        /// Check whether a path points to an executable file.
        pub fn is_executable(path: &Path) -> bool {
            if !path.exists() || !path.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(path) {
                    return metadata.permissions().mode() & 0o111 != 0;
                }
                false
            }
            #[cfg(not(unix))]
            {
                true
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::client::{Message, Role};

            fn msg(role: Role, content: &str) -> Message {
                Message {
                    role,
                    content: content.to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                }
            }

            #[test]
            fn test_build_prompt_filters_system() {
                let messages = vec![
                    msg(Role::System, "You are a helper"),
                    msg(Role::User, "Hello"),
                ];
                let result = build_prompt(&messages);
                assert_eq!(result, "Hello");
                assert!(!result.contains("helper"));
            }

            #[test]
            fn test_build_prompt_joins_with_double_newline() {
                let messages = vec![msg(Role::User, "Hello"), msg(Role::Assistant, "World")];
                let result = build_prompt(&messages);
                assert_eq!(result, "Hello\n\nWorld");
            }

            #[test]
            fn test_is_executable_nonexistent() {
                assert!(!is_executable(Path::new("/nonexistent/path/to/binary")));
            }

            #[test]
            fn test_standard_fallbacks_contains_homebrew() {
                let paths = standard_fallbacks("claude");
                assert!(
                    paths
                        .iter()
                        .any(|p| p.to_str().unwrap().contains("/opt/homebrew/bin/claude"))
                );
            }

            #[test]
            fn test_parse_json_response_success() {
                let output = r#"{"response":"Hello"}"#;
                let result = parse_json_response(output, "TestCLI").unwrap();
                assert_eq!(result, "Hello");
            }

            #[test]
            fn test_parse_json_response_error() {
                let output = r#"{"error":"auth failed"}"#;
                let err = parse_json_response(output, "TestCLI").unwrap_err();
                assert!(err.to_string().contains("TestCLI CLI error"));
            }

            #[test]
            fn test_parse_json_response_missing_field() {
                let output = r#"{"data":"hello"}"#;
                let err = parse_json_response(output, "TestCLI").unwrap_err();
                assert!(err.to_string().contains("missing 'response' field"));
            }
        }
    }

    mod codex {
        //! Codex CLI LLM provider

        use async_trait::async_trait;
        use serde_json::Value;
        use std::process::Stdio;
        use tokio::process::Command;
        use tracing::{debug, info};

        use super::utils;

        use crate::client::{
            CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamResult,
        };
        use crate::error::{AiError, Result};

        const DEFAULT_MODEL: &str = "gpt-5.4";
        const DEFAULT_REASONING_EFFORT: &str = "medium";
        const DEFAULT_EXECUTION_MODE: &str = "bypass";

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum ExecutionMode {
            Safe,
            Bypass,
        }

        impl ExecutionMode {
            fn from_str(value: &str) -> Option<Self> {
                match value.trim().to_ascii_lowercase().as_str() {
                    "safe" => Some(Self::Safe),
                    "bypass" => Some(Self::Bypass),
                    _ => None,
                }
            }
        }

        /// Codex CLI client (auth via ~/.codex/auth.json)
        pub struct CodexClient {
            model: String,
            reasoning_effort: Option<String>,
            execution_mode: ExecutionMode,
        }

        impl CodexClient {
            /// Create a new Codex CLI client
            pub fn new() -> Self {
                Self {
                    model: DEFAULT_MODEL.to_string(),
                    reasoning_effort: Some(DEFAULT_REASONING_EFFORT.to_string()),
                    execution_mode: ExecutionMode::from_str(DEFAULT_EXECUTION_MODE)
                        .unwrap_or(ExecutionMode::Safe),
                }
            }
        }

        impl Default for CodexClient {
            fn default() -> Self {
                Self::new()
            }
        }

        impl CodexClient {
            /// Set the model to use
            pub fn with_model(mut self, model: impl Into<String>) -> Self {
                self.model = model.into();
                self
            }

            /// Set reasoning effort override for Codex CLI.
            pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
                let effort = effort.into();
                let normalized = effort.trim();
                if !normalized.is_empty() {
                    self.reasoning_effort = Some(normalized.to_string());
                }
                self
            }

            /// Set execution mode override for Codex CLI.
            ///
            /// Supported values:
            /// - `safe`: use `--full-auto`
            /// - `bypass`: use `--dangerously-bypass-approvals-and-sandbox`
            pub fn with_execution_mode(mut self, mode: impl AsRef<str>) -> Self {
                if let Some(parsed) = ExecutionMode::from_str(mode.as_ref()) {
                    self.execution_mode = parsed;
                }
                self
            }

            fn build_cli_args(&self, prompt: &str) -> Vec<String> {
                let mut args = vec![
                    "exec".to_string(),
                    "--json".to_string(),
                    "--color".to_string(),
                    "never".to_string(),
                    "--skip-git-repo-check".to_string(),
                ];

                match self.execution_mode {
                    ExecutionMode::Safe => args.push("--full-auto".to_string()),
                    ExecutionMode::Bypass => {
                        args.push("--dangerously-bypass-approvals-and-sandbox".to_string())
                    }
                }

                if let Some(effort) = self.reasoning_effort.as_ref() {
                    let quoted_effort =
                        serde_json::to_string(effort).unwrap_or_else(|_| "\"medium\"".to_string());
                    args.push("-c".to_string());
                    args.push(format!("model_reasoning_effort={quoted_effort}"));
                }

                args.push("--model".to_string());
                args.push(self.model.clone());
                // Ensure prompt content that starts with '-' is not parsed as CLI flags.
                args.push("--".to_string());
                args.push(prompt.to_string());
                args
            }

            fn parse_jsonl_output(output: &str) -> Result<(String, Option<String>)> {
                let mut content = String::new();
                let mut thread_id = None;

                for line in output.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                        AiError::Llm(format!("Failed to parse Codex CLI JSONL line: {e}"))
                    })?;

                    if thread_id.is_none()
                        && let Some(id) = value.get("thread_id").and_then(|v| v.as_str())
                    {
                        thread_id = Some(id.to_string());
                    }

                    if let Some(err) = extract_error(&value) {
                        return Err(AiError::Llm(format!("Codex CLI error: {err}")));
                    }

                    if let Some(text) = extract_text(&value) {
                        content.push_str(text);
                    }
                }

                if content.trim().is_empty() {
                    return Err(AiError::Llm("Codex CLI returned empty output".to_string()));
                }

                Ok((content, thread_id))
            }
        }

        fn extract_error(value: &Value) -> Option<String> {
            value
                .get("error")
                .and_then(|v| v.as_str().map(|err| err.to_string()))
                .or_else(|| {
                    value
                        .get("error")
                        .and_then(|v| v.get("message"))
                        .and_then(|v| v.as_str())
                        .map(|err| err.to_string())
                })
        }

        fn extract_text(value: &Value) -> Option<&str> {
            if let Some(item) = value.get("item") {
                let item_type = item.get("type").and_then(|v| v.as_str());
                if matches!(item_type, Some("agent_message" | "assistant_message")) {
                    return item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .or_else(|| item.get("content").and_then(|v| v.as_str()));
                }
            }

            value
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("text").and_then(|v| v.as_str()))
                .or_else(|| value.get("delta").and_then(|v| v.as_str()))
                .or_else(|| value.pointer("/message/content").and_then(|v| v.as_str()))
                .or_else(|| value.pointer("/data/content").and_then(|v| v.as_str()))
        }

        #[async_trait]
        impl LlmClient for CodexClient {
            fn provider(&self) -> &str {
                "codex-cli"
            }

            fn model(&self) -> &str {
                &self.model
            }

            async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
                info!("CodexClient: executing via CLI");

                let prompt = utils::build_prompt(&request.messages);
                let args = self.build_cli_args(&prompt);
                let executable = utils::resolve_executable(
                    "codex",
                    "RESTFLOW_CODEX_BIN",
                    &utils::standard_fallbacks("codex"),
                )?;

                let mut cmd = Command::new(executable);
                cmd.args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                let raw_output = utils::execute_cli_command(
                    cmd,
                    "Codex",
                    "Install with: npm install -g @openai/codex",
                )
                .await?;
                let (content, thread_id) = Self::parse_jsonl_output(&raw_output)?;
                debug!(
                    content_len = content.len(),
                    thread_id = thread_id.as_deref().unwrap_or("n/a"),
                    "Codex CLI response parsed"
                );

                Ok(CompletionResponse {
                    content: Some(content),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: None,
                    reasoning_content: None,
                })
            }

            fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
                utils::unsupported_stream("Codex CLI")
            }

            fn supports_streaming(&self) -> bool {
                false
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_parse_jsonl_output() {
                let output = r#"{"type":"response.output_text.delta","delta":"Hello "}
{"type":"response.output_text.delta","delta":"world"}
{"type":"response.completed","thread_id":"thread_123"}
"#;

                let (content, thread_id) = CodexClient::parse_jsonl_output(output).unwrap();
                assert_eq!(content, "Hello world");
                assert_eq!(thread_id, Some("thread_123".to_string()));
            }

            #[test]
            fn test_parse_jsonl_output_with_message_content() {
                let output = r#"{"message":{"content":"Hi"}}"#;
                let (content, thread_id) = CodexClient::parse_jsonl_output(output).unwrap();
                assert_eq!(content, "Hi");
                assert!(thread_id.is_none());
            }

            #[test]
            fn test_parse_jsonl_output_error() {
                let output = r#"{"error":"invalid"}"#;
                let err = CodexClient::parse_jsonl_output(output).unwrap_err();
                assert!(err.to_string().contains("Codex CLI error"));
            }

            #[test]
            fn test_parse_jsonl_output_with_item_text_ignores_reasoning() {
                let output = r#"{"type":"thread.started","thread_id":"thread_abc"}
{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Thinking..."}}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Hello from Codex"}}
{"type":"turn.completed"}
"#;

                let (content, thread_id) = CodexClient::parse_jsonl_output(output).unwrap();
                assert_eq!(content, "Hello from Codex");
                assert_eq!(thread_id, Some("thread_abc".to_string()));
            }

            #[test]
            fn test_build_cli_args_defaults_to_medium_reasoning_effort() {
                let client = CodexClient::new().with_model("gpt-5.4");
                let args = client.build_cli_args("hello");

                assert!(args.windows(2).any(|pair| {
                    pair[0] == "-c" && pair[1] == "model_reasoning_effort=\"medium\""
                }));
            }

            #[test]
            fn test_build_cli_args_with_reasoning_effort() {
                let client = CodexClient::new()
                    .with_model("gpt-5.4")
                    .with_reasoning_effort("xhigh");
                let args = client.build_cli_args("hello");

                assert!(args.windows(2).any(|pair| {
                    pair[0] == "-c" && pair[1] == "model_reasoning_effort=\"xhigh\""
                }));
            }

            #[test]
            fn test_build_cli_args_defaults_to_bypass_execution_mode() {
                let client = CodexClient::new().with_model("gpt-5.4");
                let args = client.build_cli_args("hello");
                assert!(
                    args.iter()
                        .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
                );
                assert!(!args.iter().any(|arg| arg == "--full-auto"));
            }

            #[test]
            fn test_build_cli_args_with_bypass_execution_mode() {
                let client = CodexClient::new()
                    .with_model("gpt-5.4")
                    .with_execution_mode("bypass");
                let args = client.build_cli_args("hello");
                assert!(
                    args.iter()
                        .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
                );
                assert!(!args.iter().any(|arg| arg == "--full-auto"));
            }

            #[test]
            fn test_build_cli_args_inserts_double_dash_before_prompt() {
                let client = CodexClient::new().with_model("gpt-5.4");
                let prompt = "- starts-with-dash";
                let args = client.build_cli_args(prompt);

                let separator_index = args
                    .iter()
                    .position(|arg| arg == "--")
                    .expect("args should include option separator");
                assert_eq!(args.get(separator_index + 1), Some(&prompt.to_string()));
            }
        }
    }

    mod gemini_cli {
        //! Gemini CLI LLM provider

        use async_trait::async_trait;
        use std::process::Stdio;
        use tokio::process::Command;
        use tracing::{debug, info};

        use crate::client::{
            CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamResult,
        };
        use crate::error::Result;

        use super::utils;

        const DEFAULT_MODEL: &str = "gemini-2.5-pro";

        /// Gemini CLI client (auth via OAuth in ~/.gemini or GEMINI_API_KEY)
        pub struct GeminiCliClient {
            model: String,
            api_key: Option<String>,
        }

        impl GeminiCliClient {
            /// Create a new Gemini CLI client
            pub fn new() -> Self {
                Self {
                    model: DEFAULT_MODEL.to_string(),
                    api_key: None,
                }
            }

            /// Set the model to use
            pub fn with_model(mut self, model: impl Into<String>) -> Self {
                self.model = model.into();
                self
            }

            /// Inject GEMINI_API_KEY for CLI execution
            pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
                self.api_key = Some(api_key.into());
                self
            }

            fn parse_json_output(output: &str) -> Result<String> {
                utils::parse_json_response(output, "Gemini")
            }
        }

        impl Default for GeminiCliClient {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait]
        impl LlmClient for GeminiCliClient {
            fn provider(&self) -> &str {
                "gemini-cli"
            }

            fn model(&self) -> &str {
                &self.model
            }

            async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
                info!("GeminiCliClient: executing via CLI");

                let prompt = utils::build_prompt(&request.messages);
                let mut cmd = Command::new("gemini");
                cmd.arg("-p")
                    .arg(&prompt)
                    .arg("-o")
                    .arg("json")
                    .arg("-y")
                    .arg("-m")
                    .arg(&self.model)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                if let Some(api_key) = &self.api_key {
                    cmd.env("GEMINI_API_KEY", api_key);
                }

                let raw_output = utils::execute_cli_command(
                    cmd,
                    "Gemini",
                    "Install with: npm install -g @google/gemini-cli",
                )
                .await?;
                let content = Self::parse_json_output(&raw_output)?;
                debug!(content_len = content.len(), "Gemini CLI response parsed");

                Ok(CompletionResponse {
                    content: Some(content),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: None,
                    reasoning_content: None,
                })
            }

            fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
                utils::unsupported_stream("Gemini CLI")
            }

            fn supports_streaming(&self) -> bool {
                false
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_parse_json_output() {
                let output = r#"{"response":"Hello from Gemini"}"#;
                let content = GeminiCliClient::parse_json_output(output).unwrap();
                assert_eq!(content, "Hello from Gemini");
            }

            #[test]
            fn test_parse_json_output_missing_response() {
                let output = r#"{"error":"auth failed"}"#;
                assert!(GeminiCliClient::parse_json_output(output).is_err());
            }

            #[test]
            fn test_parse_json_output_whitespace() {
                let output = " {\"response\": \"Hi\"} \n";
                let content = GeminiCliClient::parse_json_output(output).unwrap();
                assert_eq!(content, "Hi");
            }

            #[test]
            fn test_gemini_cli_provider_model() {
                let client = GeminiCliClient::new();
                assert_eq!(client.provider(), "gemini-cli");
                assert_eq!(client.model(), "gemini-2.5-pro");
            }

            #[test]
            fn test_gemini_cli_with_model() {
                let client = GeminiCliClient::new().with_model("gemini-2.5-flash");
                assert_eq!(client.model(), "gemini-2.5-flash");
            }
        }
    }

    mod opencode {
        //! OpenCode CLI LLM provider

        use async_trait::async_trait;
        use std::process::Stdio;
        use tokio::process::Command;
        use tracing::{debug, info};

        use crate::client::{
            CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamResult,
        };
        use crate::error::Result;

        use super::utils;

        const DEFAULT_MODEL: &str = "opencode";

        /// OpenCode CLI client (auth via env vars)
        pub struct OpenCodeClient {
            model: String,
            provider_env: Option<(String, String)>,
        }

        impl OpenCodeClient {
            /// Create a new OpenCode CLI client
            pub fn new() -> Self {
                Self {
                    model: DEFAULT_MODEL.to_string(),
                    provider_env: None,
                }
            }

            /// Set the model to use
            pub fn with_model(mut self, model: impl Into<String>) -> Self {
                self.model = model.into();
                self
            }

            /// Inject provider credentials as an env var
            pub fn with_provider_env(
                mut self,
                var_name: impl Into<String>,
                value: impl Into<String>,
            ) -> Self {
                self.provider_env = Some((var_name.into(), value.into()));
                self
            }

            fn parse_json_output(output: &str) -> Result<String> {
                utils::parse_json_response(output, "OpenCode")
            }
        }

        impl Default for OpenCodeClient {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait]
        impl LlmClient for OpenCodeClient {
            fn provider(&self) -> &str {
                "opencode-cli"
            }

            fn model(&self) -> &str {
                &self.model
            }

            async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
                info!("OpenCodeClient: executing via CLI");

                let prompt = utils::build_prompt(&request.messages);

                let mut cmd = Command::new("opencode");
                cmd.arg("-p")
                    .arg(&prompt)
                    .arg("-f")
                    .arg("json")
                    .arg("-q")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                if let Some((env_var, value)) = &self.provider_env {
                    cmd.env(env_var, value);
                }

                let raw_output = utils::execute_cli_command(
                    cmd,
                    "OpenCode",
                    "Install with: go install github.com/opencode-ai/opencode@latest",
                )
                .await?;
                let content = Self::parse_json_output(&raw_output)?;
                debug!(content_len = content.len(), "OpenCode CLI response parsed");

                Ok(CompletionResponse {
                    content: Some(content),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: None,
                    reasoning_content: None,
                })
            }

            fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
                utils::unsupported_stream("OpenCode CLI")
            }

            fn supports_streaming(&self) -> bool {
                false
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_parse_json_output() {
                let output = r#"{"response":"Hello world"}"#;
                let content = OpenCodeClient::parse_json_output(output).unwrap();
                assert_eq!(content, "Hello world");
            }

            #[test]
            fn test_parse_json_output_missing_response() {
                let output = r#"{"error":"something"}"#;
                assert!(OpenCodeClient::parse_json_output(output).is_err());
            }

            #[test]
            fn test_parse_json_output_with_whitespace() {
                let output = " {\"response\": \"Hi\"} \n";
                let content = OpenCodeClient::parse_json_output(output).unwrap();
                assert_eq!(content, "Hi");
            }

            #[test]
            fn test_build_prompt() {
                let messages = vec![
                    crate::Message::system("system"),
                    crate::Message::user("hello"),
                    crate::Message::assistant("world"),
                ];
                let prompt = super::utils::build_prompt(&messages);
                assert_eq!(prompt, "hello\n\nworld");
            }

            #[test]
            fn test_opencode_provider_model() {
                let client = OpenCodeClient::new();
                assert_eq!(client.provider(), "opencode-cli");
                assert_eq!(client.model(), "opencode");
            }
        }
    }

    pub use codex::CodexClient;
    pub use gemini_cli::GeminiCliClient;
    pub use opencode::OpenCodeClient;
}

pub mod http {
    mod anthropic {
        //! Anthropic LLM provider

        use async_trait::async_trait;
        use futures::StreamExt;
        use reqwest::Client;
        use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
        use serde::{Deserialize, Serialize};
        use serde_json::Value;

        use super::build_http_client;
        use crate::client::{
            CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamChunk,
            StreamResult, TokenUsage, ToolCall, ToolCallDelta,
        };
        use crate::error::{AiError, Result};
        use crate::pricing::calculate_cost;
        use crate::retry::response_to_error;

        /// Anthropic client
        pub struct AnthropicClient {
            client: Client,
            api_key: String,
            auth_type: AnthropicAuthType,
            model: String,
            base_url: Option<String>,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum AnthropicAuthType {
            ApiKey,
            OAuth,
        }

        impl AnthropicAuthType {
            fn from_key(key: &str) -> Self {
                if key.starts_with("sk-ant-oat") {
                    Self::OAuth
                } else {
                    Self::ApiKey
                }
            }
        }

        impl AnthropicClient {
            /// Create a new Anthropic client
            pub fn new(api_key: impl Into<String>) -> std::result::Result<Self, reqwest::Error> {
                let api_key = api_key.into();
                let auth_type = AnthropicAuthType::from_key(&api_key);
                Ok(Self {
                    client: build_http_client()?,
                    api_key,
                    auth_type,
                    model: "claude-sonnet-4-20250514".to_string(),
                    base_url: None,
                })
            }

            /// Set the model to use
            pub fn with_model(mut self, model: impl Into<String>) -> Self {
                self.model = model.into();
                self
            }

            /// Set a custom base URL (for Anthropic-compatible APIs like MiniMax)
            pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
                self.base_url = Some(base_url.into());
                self
            }

            fn api_base_url(&self) -> &str {
                self.base_url
                    .as_deref()
                    .unwrap_or("https://api.anthropic.com")
            }

            fn build_auth_headers(&self) -> HeaderMap {
                build_auth_headers(&self.api_key, self.auth_type)
            }
        }

        fn build_auth_headers(api_key: &str, auth_type: AnthropicAuthType) -> HeaderMap {
            let mut headers = HeaderMap::new();

            match auth_type {
                AnthropicAuthType::OAuth => {
                    headers.insert(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
                    );
                }
                AnthropicAuthType::ApiKey => {
                    headers.insert(
                        HeaderName::from_static("x-api-key"),
                        HeaderValue::from_str(api_key).unwrap(),
                    );
                }
            }

            headers.insert(
                HeaderName::from_static("anthropic-version"),
                HeaderValue::from_static("2023-06-01"),
            );
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

            headers
        }

        #[derive(Serialize)]
        struct AnthropicRequest {
            model: String,
            max_tokens: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            system: Option<String>,
            messages: Vec<AnthropicMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<AnthropicTool>>,
        }

        #[derive(Serialize)]
        struct AnthropicMessage {
            role: String,
            content: AnthropicContent,
        }

        #[derive(Serialize)]
        #[serde(untagged)]
        enum AnthropicContent {
            Text(String),
            Blocks(Vec<AnthropicContentBlock>),
        }

        #[derive(Serialize)]
        struct AnthropicContentBlock {
            r#type: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            text: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tool_use_id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<String>,
            // For tool_use blocks (assistant's tool calls)
            #[serde(skip_serializing_if = "Option::is_none")]
            id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            input: Option<Value>,
        }

        #[derive(Serialize)]
        struct AnthropicTool {
            name: String,
            description: String,
            input_schema: Value,
        }

        #[derive(Deserialize)]
        struct AnthropicResponse {
            content: Vec<AnthropicResponseContent>,
            stop_reason: Option<String>,
            usage: AnthropicUsage,
        }

        #[derive(Deserialize)]
        struct AnthropicResponseContent {
            r#type: String,
            #[serde(default)]
            text: Option<String>,
            #[serde(default)]
            id: Option<String>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            input: Option<Value>,
        }

        #[derive(Deserialize)]
        struct AnthropicUsage {
            input_tokens: u32,
            output_tokens: u32,
        }

        // Streaming response types

        /// Anthropic SSE event types
        #[derive(Debug, Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum AnthropicStreamEvent {
            MessageStart {
                message: MessageStartPayload,
            },
            ContentBlockStart {
                index: usize,
                content_block: ContentBlockStartPayload,
            },
            ContentBlockDelta {
                index: usize,
                delta: ContentBlockDelta,
            },
            ContentBlockStop {
                #[serde(skip, default)]
                #[allow(dead_code)]
                index: usize,
            },
            MessageDelta {
                delta: MessageDeltaPayload,
                usage: Option<OutputUsage>,
            },
            MessageStop,
            Ping,
            Error {
                error: ErrorPayload,
            },
        }

        #[derive(Debug, Deserialize)]
        struct MessageStartPayload {
            #[serde(skip, default)]
            _id: Option<String>,
            #[serde(skip, default)]
            _model: Option<String>,
            usage: Option<InputUsage>,
        }

        #[derive(Debug, Deserialize)]
        struct InputUsage {
            input_tokens: u32,
        }

        #[derive(Debug, Deserialize)]
        struct OutputUsage {
            output_tokens: u32,
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum ContentBlockStartPayload {
            Text { text: String },
            ToolUse { id: String, name: String },
            Thinking { thinking: String },
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        #[allow(clippy::enum_variant_names)]
        enum ContentBlockDelta {
            TextDelta { text: String },
            InputJsonDelta { partial_json: String },
            ThinkingDelta { thinking: String },
        }

        #[derive(Debug, Deserialize)]
        struct MessageDeltaPayload {
            stop_reason: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct ErrorPayload {
            message: String,
        }

        /// Convert a CompletionRequest into Anthropic API request parts.
        fn prepare_request_parts(
            request: &CompletionRequest,
        ) -> (
            Option<String>,
            Vec<AnthropicMessage>,
            Option<Vec<AnthropicTool>>,
        ) {
            // Extract system message
            let system = request
                .messages
                .iter()
                .find(|m| m.role == Role::System)
                .map(|m| m.content.clone());

            // Convert messages (excluding system)
            let messages: Vec<AnthropicMessage> = request
                .messages
                .iter()
                .filter(|m| m.role != Role::System)
                .map(|m| {
                    let role = match m.role {
                        Role::User | Role::Tool => "user",
                        Role::Assistant => "assistant",
                        _ => "user",
                    }
                    .to_string();

                    let content = if m.role == Role::Tool {
                        AnthropicContent::Blocks(vec![AnthropicContentBlock {
                            r#type: "tool_result".to_string(),
                            tool_use_id: m.tool_call_id.clone(),
                            content: Some(m.content.clone()),
                            text: None,
                            id: None,
                            name: None,
                            input: None,
                        }])
                    } else if let Some(tool_calls) = &m.tool_calls {
                        let mut blocks = Vec::new();
                        if !m.content.is_empty() {
                            blocks.push(AnthropicContentBlock {
                                r#type: "text".to_string(),
                                text: Some(m.content.clone()),
                                tool_use_id: None,
                                content: None,
                                id: None,
                                name: None,
                                input: None,
                            });
                        }
                        for tc in tool_calls {
                            blocks.push(AnthropicContentBlock {
                                r#type: "tool_use".to_string(),
                                text: None,
                                tool_use_id: None,
                                content: None,
                                id: Some(tc.id.clone()),
                                name: Some(tc.name.clone()),
                                input: Some(tc.arguments.clone()),
                            });
                        }
                        AnthropicContent::Blocks(blocks)
                    } else {
                        AnthropicContent::Text(m.content.clone())
                    };

                    AnthropicMessage { role, content }
                })
                .collect();

            let tools: Option<Vec<AnthropicTool>> = if request.tools.is_empty() {
                None
            } else {
                Some(
                    request
                        .tools
                        .iter()
                        .map(|t| AnthropicTool {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            input_schema: t.parameters.clone(),
                        })
                        .collect(),
                )
            };

            (system, messages, tools)
        }

        #[async_trait]
        impl LlmClient for AnthropicClient {
            fn provider(&self) -> &str {
                "anthropic"
            }

            fn model(&self) -> &str {
                &self.model
            }

            async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
                let (system, messages, tools) = prepare_request_parts(&request);

                let body = AnthropicRequest {
                    model: self.model.clone(),
                    max_tokens: request.max_tokens.unwrap_or(4096),
                    system,
                    messages,
                    tools,
                };

                let response = self
                    .client
                    .post(format!("{}/v1/messages", self.api_base_url()))
                    .headers(self.build_auth_headers())
                    .json(&body)
                    .send()
                    .await
                    .map_err(AiError::Http)?;

                if !response.status().is_success() {
                    return Err(response_to_error(response, "Anthropic").await);
                }

                let data: AnthropicResponse = response.json().await?;

                let mut content = None;
                let mut tool_calls = vec![];

                for block in data.content {
                    match block.r#type.as_str() {
                        "text" => content = block.text,
                        "tool_use" => {
                            if let (Some(id), Some(name), Some(input)) =
                                (block.id, block.name, block.input)
                            {
                                tool_calls.push(ToolCall {
                                    id,
                                    name,
                                    arguments: input,
                                });
                            }
                        }
                        _ => {}
                    }
                }

                let finish_reason = match data.stop_reason.as_deref() {
                    Some("end_turn") => FinishReason::Stop,
                    Some("tool_use") => FinishReason::ToolCalls,
                    Some("max_tokens") => FinishReason::MaxTokens,
                    _ => FinishReason::Stop,
                };

                let cost_usd = calculate_cost(
                    &self.model,
                    data.usage.input_tokens,
                    data.usage.output_tokens,
                );

                Ok(CompletionResponse {
                    content,
                    tool_calls,
                    finish_reason,
                    usage: Some(TokenUsage {
                        prompt_tokens: data.usage.input_tokens,
                        completion_tokens: data.usage.output_tokens,
                        total_tokens: data.usage.input_tokens + data.usage.output_tokens,
                        cost_usd,
                    }),
                    reasoning_content: None,
                })
            }

            fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
                let client = self.client.clone();
                let api_key = self.api_key.clone();
                let model = self.model.clone();
                let auth_type = self.auth_type;
                let base_url = self.api_base_url().to_string();

                Box::pin(async_stream::stream! {
                    // OAuth tokens can't use Anthropic API - CLI doesn't support streaming
                    if auth_type == AnthropicAuthType::OAuth {
                        yield Err(AiError::Llm(
                            "Streaming not supported with OAuth tokens. Use non-streaming mode.".to_string()
                        ));
                        return;
                    }

                    let (system, messages, tools) = prepare_request_parts(&request);

                    // Build streaming request body
                    let body = serde_json::json!({
                        "model": model,
                        "max_tokens": request.max_tokens.unwrap_or(4096),
                        "system": system,
                        "messages": messages,
                        "tools": tools,
                        "stream": true
                    });

                    let response = match client
                        .post(format!("{}/v1/messages", base_url))
                        .headers(build_auth_headers(&api_key, auth_type))
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(resp) => resp,
                        Err(e) => {
                            yield Err(AiError::Http(e));
                            return;
                        }
                    };

                    if !response.status().is_success() {
                        yield Err(response_to_error(response, "Anthropic").await);
                        return;
                    }

                    let mut byte_stream = response.bytes_stream();
                    let mut buffer = String::new();
                    let mut input_tokens = 0u32;
                    let mut output_tokens = 0u32;
                    let mut _current_tool_index: Option<usize> = None;
                    let mut current_tool_id: Option<String> = None;
                    let mut current_tool_name: Option<String> = None;

                    while let Some(chunk_result) = byte_stream.next().await {
                        let chunk = match chunk_result {
                            Ok(bytes) => bytes,
                            Err(e) => {
                                yield Err(AiError::Http(e));
                                return;
                            }
                        };

                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        // Process complete SSE events from buffer
                        while let Some(pos) = buffer.find("\n\n") {
                            let event_str = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            // Parse SSE event
                            for line in event_str.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim().is_empty() {
                                        continue;
                                    }

                                    let event: AnthropicStreamEvent = match serde_json::from_str(data) {
                                        Ok(e) => e,
                                        Err(_) => continue,
                                    };

                                    match event {
                                        AnthropicStreamEvent::MessageStart { message } => {
                                            if let Some(usage) = message.usage {
                                                input_tokens = usage.input_tokens;
                                            }
                                        }
                                        AnthropicStreamEvent::ContentBlockStart { index, content_block } => {
                                            match content_block {
                                                ContentBlockStartPayload::Text { text } => {
                                                    if !text.is_empty() {
                                                        yield Ok(StreamChunk::text(&text));
                                                    }
                                                }
                                                ContentBlockStartPayload::ToolUse { id, name } => {
                                                    _current_tool_index = Some(index);
                                                    current_tool_id = Some(id.clone());
                                                    current_tool_name = Some(name.clone());
                                                    yield Ok(StreamChunk {
                                                        text: String::new(),
                                                        thinking: None,
                                                        tool_call_delta: Some(ToolCallDelta {
                                                            index,
                                                            id: Some(id),
                                                            name: Some(name),
                                                            arguments: None,
                                                        }),
                                                        finish_reason: None,
                                                        usage: None,
                                                    });
                                                }
                                                ContentBlockStartPayload::Thinking { thinking } => {
                                                    if !thinking.is_empty() {
                                                        yield Ok(StreamChunk::thinking(&thinking));
                                                    }
                                                }
                                            }
                                        }
                                        AnthropicStreamEvent::ContentBlockDelta { index, delta } => {
                                            match delta {
                                                ContentBlockDelta::TextDelta { text } => {
                                                    yield Ok(StreamChunk::text(&text));
                                                }
                                                ContentBlockDelta::InputJsonDelta { partial_json } => {
                                                    yield Ok(StreamChunk {
                                                        text: String::new(),
                                                        thinking: None,
                                                        tool_call_delta: Some(ToolCallDelta {
                                                            index,
                                                            id: current_tool_id.clone(),
                                                            name: current_tool_name.clone(),
                                                            arguments: Some(partial_json),
                                                        }),
                                                        finish_reason: None,
                                                        usage: None,
                                                    });
                                                }
                                                ContentBlockDelta::ThinkingDelta { thinking } => {
                                                    yield Ok(StreamChunk::thinking(&thinking));
                                                }
                                            }
                                        }
                                        AnthropicStreamEvent::ContentBlockStop { index: _ } => {
                                            _current_tool_index = None;
                                            current_tool_id = None;
                                            current_tool_name = None;
                                        }
                                        AnthropicStreamEvent::MessageDelta { delta, usage } => {
                                            if let Some(u) = usage {
                                                output_tokens = u.output_tokens;
                                            }
                                            if let Some(stop_reason) = delta.stop_reason {
                                                let finish_reason = match stop_reason.as_str() {
                                                    "end_turn" => FinishReason::Stop,
                                                    "tool_use" => FinishReason::ToolCalls,
                                                    "max_tokens" => FinishReason::MaxTokens,
                                                    _ => FinishReason::Stop,
                                                };
                                                let cost_usd = calculate_cost(&model, input_tokens, output_tokens);
                                                yield Ok(StreamChunk::final_chunk(
                                                    finish_reason,
                                                    Some(TokenUsage {
                                                        prompt_tokens: input_tokens,
                                                        completion_tokens: output_tokens,
                                                        total_tokens: input_tokens + output_tokens,
                                                        cost_usd,
                                                    }),
                                                ));
                                            }
                                        }
                                        AnthropicStreamEvent::MessageStop => {
                                            // Stream complete
                                        }
                                        AnthropicStreamEvent::Ping => {
                                            // Keep-alive, ignore
                                        }
                                        AnthropicStreamEvent::Error { error } => {
                                            yield Err(AiError::Llm(format!("Stream error: {}", error.message)));
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Process any remaining data in the buffer after the stream ends.
                    // This handles the case where the last SSE event lacks a trailing \n\n
                    // (e.g., due to a network interruption).
                    let remaining = buffer.trim();
                    if !remaining.is_empty() {
                        for line in remaining.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim().is_empty() {
                                    continue;
                                }
                                if let Ok(AnthropicStreamEvent::Error { error }) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                                    yield Err(AiError::Llm(format!("Stream error: {}", error.message)));
                                }
                            }
                        }
                    }
                })
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_auth_type_detection() {
                assert_eq!(
                    AnthropicAuthType::from_key("sk-ant-oat01-xxx"),
                    AnthropicAuthType::OAuth
                );
                assert_eq!(
                    AnthropicAuthType::from_key("sk-ant-api03-xxx"),
                    AnthropicAuthType::ApiKey
                );
            }

            #[test]
            fn test_oauth_headers() {
                let headers = build_auth_headers("sk-ant-oat01-test", AnthropicAuthType::OAuth);
                assert!(headers.contains_key(AUTHORIZATION));
                assert!(!headers.contains_key("x-api-key"));
            }
        }
    }

    mod deepseek {
        //! DeepSeek LLM provider.
        //!
        //! DeepSeek is OpenAI-compatible, but thinking-mode tool calls require the
        //! provider-specific `reasoning_content` field to be preserved and sent back on
        //! subsequent assistant tool-call messages.

        use async_trait::async_trait;
        use futures::StreamExt;
        use reqwest::Client;
        use serde::{Deserialize, Serialize};
        use serde_json::Value;

        use super::build_http_client;
        use crate::client::{
            CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamChunk,
            StreamResult, TokenUsage, ToolCall, ToolCallDelta,
        };
        use crate::error::{AiError, Result};
        use crate::pricing::calculate_cost;
        use crate::retry::response_to_error;

        /// DeepSeek HTTP client.
        pub struct DeepSeekClient {
            client: Client,
            api_key: String,
            model: String,
            base_url: String,
        }

        impl DeepSeekClient {
            /// Create a new DeepSeek client.
            pub fn new(api_key: impl Into<String>) -> std::result::Result<Self, reqwest::Error> {
                Ok(Self {
                    client: build_http_client()?,
                    api_key: api_key.into(),
                    model: "deepseek-chat".to_string(),
                    base_url: "https://api.deepseek.com/v1".to_string(),
                })
            }

            /// Set the model to use.
            pub fn with_model(mut self, model: impl Into<String>) -> Self {
                self.model = model.into();
                self
            }

            /// Set custom base URL.
            #[allow(dead_code)]
            pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
                self.base_url = url.into();
                self
            }
        }

        #[derive(Serialize)]
        struct DeepSeekRequest {
            model: String,
            messages: Vec<DeepSeekMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<DeepSeekTool>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
        }

        #[derive(Serialize)]
        struct DeepSeekMessage {
            role: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tool_call_id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tool_calls: Option<Vec<DeepSeekMessageToolCall>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            reasoning_content: Option<String>,
        }

        #[derive(Serialize)]
        struct DeepSeekMessageToolCall {
            id: String,
            r#type: String,
            function: DeepSeekMessageFunction,
        }

        #[derive(Serialize)]
        struct DeepSeekMessageFunction {
            name: String,
            arguments: String,
        }

        #[derive(Serialize)]
        struct DeepSeekTool {
            r#type: String,
            function: DeepSeekFunction,
        }

        #[derive(Serialize)]
        struct DeepSeekFunction {
            name: String,
            description: String,
            parameters: Value,
        }

        #[derive(Deserialize)]
        struct DeepSeekResponse {
            choices: Vec<DeepSeekChoice>,
            usage: Option<DeepSeekUsage>,
        }

        #[derive(Deserialize)]
        struct DeepSeekChoice {
            message: DeepSeekResponseMessage,
            finish_reason: String,
        }

        #[derive(Deserialize)]
        struct DeepSeekResponseMessage {
            content: Option<String>,
            tool_calls: Option<Vec<DeepSeekToolCall>>,
            reasoning_content: Option<String>,
        }

        #[derive(Deserialize)]
        struct DeepSeekToolCall {
            id: String,
            function: DeepSeekFunctionCall,
        }

        #[derive(Deserialize)]
        struct DeepSeekFunctionCall {
            name: String,
            arguments: String,
        }

        #[derive(Deserialize, Debug)]
        struct DeepSeekUsage {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }

        #[derive(Deserialize, Debug)]
        struct DeepSeekStreamResponse {
            choices: Vec<DeepSeekStreamChoice>,
            usage: Option<DeepSeekUsage>,
        }

        #[derive(Deserialize, Debug)]
        struct DeepSeekStreamChoice {
            delta: DeepSeekStreamDelta,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize, Debug)]
        struct DeepSeekStreamDelta {
            content: Option<String>,
            tool_calls: Option<Vec<DeepSeekStreamToolCall>>,
            reasoning_content: Option<String>,
        }

        #[derive(Deserialize, Debug)]
        struct DeepSeekStreamToolCall {
            index: usize,
            id: Option<String>,
            function: Option<DeepSeekStreamFunction>,
        }

        #[derive(Deserialize, Debug)]
        struct DeepSeekStreamFunction {
            name: Option<String>,
            arguments: Option<String>,
        }

        fn convert_messages(request: &CompletionRequest) -> Vec<DeepSeekMessage> {
            request
                .messages
                .iter()
                .map(|message| {
                    let role = match message.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                    }
                    .to_string();

                    let tool_calls = message.tool_calls.as_ref().map(|calls| {
                        calls
                            .iter()
                            .map(|call| DeepSeekMessageToolCall {
                                id: call.id.clone(),
                                r#type: "function".to_string(),
                                function: DeepSeekMessageFunction {
                                    name: call.name.clone(),
                                    arguments: serde_json::to_string(&call.arguments)
                                        .unwrap_or_default(),
                                },
                            })
                            .collect()
                    });

                    let content = if message.tool_calls.is_some() && message.content.is_empty() {
                        None
                    } else {
                        Some(message.content.clone())
                    };

                    let reasoning_content = if message.role == Role::Assistant {
                        message.reasoning_content.clone()
                    } else {
                        None
                    };

                    DeepSeekMessage {
                        role,
                        content,
                        tool_call_id: message.tool_call_id.clone(),
                        tool_calls,
                        reasoning_content,
                    }
                })
                .collect()
        }

        fn convert_tools(request: &CompletionRequest) -> Option<Vec<DeepSeekTool>> {
            if request.tools.is_empty() {
                None
            } else {
                Some(
                    request
                        .tools
                        .iter()
                        .map(|tool| DeepSeekTool {
                            r#type: "function".to_string(),
                            function: DeepSeekFunction {
                                name: tool.name.clone(),
                                description: tool.description.clone(),
                                parameters: tool.parameters.clone(),
                            },
                        })
                        .collect(),
                )
            }
        }

        fn map_finish_reason(reason: &str) -> FinishReason {
            match reason {
                "stop" => FinishReason::Stop,
                "tool_calls" => FinishReason::ToolCalls,
                "length" => FinishReason::MaxTokens,
                _ => FinishReason::Error,
            }
        }

        #[async_trait]
        impl LlmClient for DeepSeekClient {
            fn provider(&self) -> &str {
                "deepseek"
            }

            fn model(&self) -> &str {
                &self.model
            }

            async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
                let body = DeepSeekRequest {
                    model: self.model.clone(),
                    messages: convert_messages(&request),
                    tools: convert_tools(&request),
                    temperature: request.temperature,
                    max_tokens: request.max_tokens,
                };

                let response = self
                    .client
                    .post(format!("{}/chat/completions", self.base_url))
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await
                    .map_err(AiError::Http)?;

                if !response.status().is_success() {
                    return Err(response_to_error(response, "DeepSeek").await);
                }

                let data: DeepSeekResponse = response.json().await?;
                let choice = data
                    .choices
                    .into_iter()
                    .next()
                    .ok_or_else(|| AiError::Llm("No response from DeepSeek".to_string()))?;

                let tool_calls = choice
                    .message
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .map(|call| ToolCall {
                        id: call.id,
                        name: call.function.name,
                        arguments: serde_json::from_str(&call.function.arguments)
                            .unwrap_or(Value::Null),
                    })
                    .collect();

                let usage = data.usage.map(|usage| {
                    let cost_usd =
                        calculate_cost(&self.model, usage.prompt_tokens, usage.completion_tokens);
                    TokenUsage {
                        prompt_tokens: usage.prompt_tokens,
                        completion_tokens: usage.completion_tokens,
                        total_tokens: usage.total_tokens,
                        cost_usd,
                    }
                });

                Ok(CompletionResponse {
                    content: choice.message.content,
                    tool_calls,
                    finish_reason: map_finish_reason(&choice.finish_reason),
                    usage,
                    reasoning_content: choice.message.reasoning_content,
                })
            }

            fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
                let client = self.client.clone();
                let api_key = self.api_key.clone();
                let base_url = self.base_url.clone();
                let model = self.model.clone();

                Box::pin(async_stream::stream! {
                    let mut body = serde_json::json!({
                        "model": model,
                        "messages": convert_messages(&request),
                        "tools": convert_tools(&request),
                        "temperature": request.temperature,
                        "stream": true,
                        "stream_options": { "include_usage": true }
                    });
                    if let Some(max_tokens) = request.max_tokens {
                        body["max_tokens"] = Value::from(max_tokens);
                    }

                    let response = match client
                        .post(format!("{}/chat/completions", base_url))
                        .header("Authorization", format!("Bearer {}", api_key))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(response) => response,
                        Err(error) => {
                            yield Err(AiError::Llm(format!("Request failed: {}", error)));
                            return;
                        }
                    };

                    if !response.status().is_success() {
                        yield Err(response_to_error(response, "DeepSeek").await);
                        return;
                    }

                    let mut byte_stream = response.bytes_stream();
                    let mut buffer = String::new();
                    let mut tool_call_ids = std::collections::HashMap::<usize, String>::new();
                    let mut tool_call_names = std::collections::HashMap::<usize, String>::new();

                    while let Some(chunk_result) = byte_stream.next().await {
                        let chunk = match chunk_result {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                yield Err(AiError::Llm(format!("Stream error: {}", error)));
                                return;
                            }
                        };

                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        while let Some(pos) = buffer.find("\n\n") {
                            let event_str = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            for line in event_str.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" {
                                        continue;
                                    }

                                    let parsed: DeepSeekStreamResponse = match serde_json::from_str(data) {
                                        Ok(parsed) => parsed,
                                        Err(_) => continue,
                                    };

                                    if let Some(usage) = parsed.usage {
                                        yield Ok(StreamChunk::final_chunk(
                                            FinishReason::Stop,
                                            Some(TokenUsage {
                                                prompt_tokens: usage.prompt_tokens,
                                                completion_tokens: usage.completion_tokens,
                                                total_tokens: usage.total_tokens,
                                                cost_usd: calculate_cost(
                                                    &model,
                                                    usage.prompt_tokens,
                                                    usage.completion_tokens,
                                                ),
                                            }),
                                        ));
                                        continue;
                                    }

                                    for choice in parsed.choices {
                                        if let Some(finish_reason) = choice.finish_reason {
                                            yield Ok(StreamChunk::final_chunk(
                                                map_finish_reason(&finish_reason),
                                                None,
                                            ));
                                            continue;
                                        }

                                        if let Some(reasoning) = &choice.delta.reasoning_content
                                            && !reasoning.is_empty()
                                        {
                                            yield Ok(StreamChunk::thinking(reasoning));
                                        }

                                        if let Some(content) = &choice.delta.content
                                            && !content.is_empty()
                                        {
                                            yield Ok(StreamChunk::text(content));
                                        }

                                        if let Some(tool_calls) = choice.delta.tool_calls {
                                            for call in tool_calls {
                                                if let Some(id) = &call.id {
                                                    tool_call_ids.insert(call.index, id.clone());
                                                }
                                                if let Some(function) = &call.function
                                                    && let Some(name) = &function.name
                                                {
                                                    tool_call_names.insert(call.index, name.clone());
                                                }

                                                let arguments = call
                                                    .function
                                                    .as_ref()
                                                    .and_then(|function| function.arguments.clone());

                                                yield Ok(StreamChunk {
                                                    text: String::new(),
                                                    thinking: None,
                                                    tool_call_delta: Some(ToolCallDelta {
                                                        index: call.index,
                                                        id: tool_call_ids.get(&call.index).cloned(),
                                                        name: tool_call_names.get(&call.index).cloned(),
                                                        arguments,
                                                    }),
                                                    finish_reason: None,
                                                    usage: None,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    let remaining = buffer.trim();
                    if !remaining.is_empty() {
                        for line in remaining.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim() == "[DONE]" || data.trim().is_empty() {
                                    continue;
                                }
                                if let Ok(parsed) = serde_json::from_str::<DeepSeekStreamResponse>(data)
                                    && let Some(usage) = parsed.usage
                                {
                                    yield Ok(StreamChunk::final_chunk(
                                        FinishReason::Stop,
                                        Some(TokenUsage {
                                            prompt_tokens: usage.prompt_tokens,
                                            completion_tokens: usage.completion_tokens,
                                            total_tokens: usage.total_tokens,
                                            cost_usd: calculate_cost(
                                                &model,
                                                usage.prompt_tokens,
                                                usage.completion_tokens,
                                            ),
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                })
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::client::{Message, ToolCall};
            use serde_json::json;

            #[test]
            fn request_serializes_reasoning_content_on_assistant_messages() {
                let request = CompletionRequest::new(vec![
                    Message::user("hello"),
                    Message::assistant_with_tool_calls_and_reasoning(
                        None,
                        vec![ToolCall {
                            id: "call_1".to_string(),
                            name: "bash".to_string(),
                            arguments: json!({"command": "ls"}),
                        }],
                        Some("Let me think about this...".to_string()),
                    ),
                    Message::tool_result("call_1", "file1.txt\nfile2.txt"),
                ]);

                let messages = convert_messages(&request);
                assert!(messages[0].reasoning_content.is_none());
                assert_eq!(
                    messages[1].reasoning_content.as_deref(),
                    Some("Let me think about this...")
                );
                assert!(messages[2].reasoning_content.is_none());

                let serialized = serde_json::to_value(&messages[1]).unwrap();
                assert_eq!(
                    serialized["reasoning_content"],
                    "Let me think about this..."
                );
                assert!(serialized["tool_calls"].is_array());
            }

            #[test]
            fn request_omits_reasoning_content_when_none() {
                let request = CompletionRequest::new(vec![
                    Message::user("hello"),
                    Message::assistant("hi there"),
                ]);

                let messages = convert_messages(&request);
                let serialized = serde_json::to_value(&messages[1]).unwrap();
                assert!(serialized.get("reasoning_content").is_none());
            }

            #[test]
            fn request_ignores_reasoning_content_on_non_assistant_messages() {
                let mut user_msg = Message::user("hello");
                user_msg.reasoning_content = Some("should be ignored".to_string());

                let request = CompletionRequest::new(vec![user_msg]);
                let messages = convert_messages(&request);
                assert!(messages[0].reasoning_content.is_none());
            }

            #[test]
            fn stream_response_deserializes_reasoning_content() {
                let json = r#"{"choices":[{"delta":{"reasoning_content":"Hmm, let me think..."},"finish_reason":null}]}"#;
                let parsed: DeepSeekStreamResponse = serde_json::from_str(json).unwrap();
                assert_eq!(
                    parsed.choices[0].delta.reasoning_content.as_deref(),
                    Some("Hmm, let me think...")
                );
            }

            #[test]
            fn response_deserializes_reasoning_content() {
                let json = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{"id":"c1","function":{"name":"bash","arguments":"{}"}}],
                    "reasoning_content": "Step 1: analyze..."
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}
        }"#;
                let parsed: DeepSeekResponse = serde_json::from_str(json).unwrap();
                assert_eq!(
                    parsed.choices[0].message.reasoning_content.as_deref(),
                    Some("Step 1: analyze...")
                );
                assert_eq!(parsed.choices[0].finish_reason, "tool_calls");
            }

            #[test]
            fn finish_reason_mapping() {
                assert_eq!(map_finish_reason("stop"), FinishReason::Stop);
                assert_eq!(map_finish_reason("tool_calls"), FinishReason::ToolCalls);
                assert_eq!(map_finish_reason("length"), FinishReason::MaxTokens);
                assert_eq!(map_finish_reason("unknown"), FinishReason::Error);
            }
        }
    }

    mod openai {
        //! OpenAI LLM provider

        use async_trait::async_trait;
        use futures::StreamExt;
        use reqwest::Client;
        use serde::{Deserialize, Serialize};
        use serde_json::Value;

        use super::build_http_client;
        use crate::client::{
            CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamChunk,
            StreamResult, TokenUsage, ToolCall, ToolCallDelta,
        };
        use crate::error::{AiError, Result};
        use crate::pricing::calculate_cost;
        use crate::retry::response_to_error;

        /// OpenAI client
        pub struct OpenAIClient {
            client: Client,
            api_key: String,
            model: String,
            base_url: String,
        }

        impl OpenAIClient {
            /// Create a new OpenAI client
            pub fn new(api_key: impl Into<String>) -> std::result::Result<Self, reqwest::Error> {
                Ok(Self {
                    client: build_http_client()?,
                    api_key: api_key.into(),
                    model: "gpt-4o".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                })
            }

            /// Set the model to use
            pub fn with_model(mut self, model: impl Into<String>) -> Self {
                self.model = model.into();
                self
            }

            /// Set custom base URL (for API-compatible services)
            pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
                self.base_url = url.into();
                self
            }
        }

        #[derive(Serialize)]
        struct OpenAIRequest {
            model: String,
            messages: Vec<OpenAIMessage>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<OpenAITool>>,
            #[serde(skip_serializing_if = "Option::is_none")]
            temperature: Option<f32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_tokens: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_completion_tokens: Option<u32>,
        }

        #[derive(Serialize)]
        struct OpenAIMessage {
            role: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            content: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tool_call_id: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tool_calls: Option<Vec<OpenAIMessageToolCall>>,
        }

        #[derive(Serialize)]
        struct OpenAIMessageToolCall {
            id: String,
            r#type: String,
            function: OpenAIMessageFunction,
        }

        #[derive(Serialize)]
        struct OpenAIMessageFunction {
            name: String,
            arguments: String,
        }

        #[derive(Serialize)]
        struct OpenAITool {
            r#type: String,
            function: OpenAIFunction,
        }

        #[derive(Serialize)]
        struct OpenAIFunction {
            name: String,
            description: String,
            parameters: Value,
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
            choices: Vec<OpenAIChoice>,
            usage: Option<OpenAIUsage>,
        }

        #[derive(Deserialize)]
        struct OpenAIChoice {
            message: OpenAIResponseMessage,
            finish_reason: String,
        }

        #[derive(Deserialize)]
        struct OpenAIResponseMessage {
            content: Option<String>,
            tool_calls: Option<Vec<OpenAIToolCall>>,
        }

        #[derive(Deserialize)]
        struct OpenAIToolCall {
            id: String,
            function: OpenAIFunctionCall,
        }

        #[derive(Deserialize)]
        struct OpenAIFunctionCall {
            name: String,
            arguments: String,
        }

        #[derive(Deserialize, Debug)]
        struct OpenAIUsage {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }

        fn uses_max_completion_tokens(model: &str) -> bool {
            let normalized = model.trim().to_ascii_lowercase();
            normalized == "gpt-5"
                || normalized.starts_with("gpt-5-")
                || normalized.starts_with("gpt-5.")
        }

        fn token_limit_fields(model: &str, max_tokens: Option<u32>) -> (Option<u32>, Option<u32>) {
            if uses_max_completion_tokens(model) {
                (None, max_tokens)
            } else {
                (max_tokens, None)
            }
        }

        fn apply_token_limit(body: &mut Value, model: &str, max_tokens: Option<u32>) {
            let (max_tokens, max_completion_tokens) = token_limit_fields(model, max_tokens);
            if let Some(max_tokens) = max_tokens {
                body["max_tokens"] = Value::from(max_tokens);
            }
            if let Some(max_completion_tokens) = max_completion_tokens {
                body["max_completion_tokens"] = Value::from(max_completion_tokens);
            }
        }

        // Streaming types

        #[derive(Deserialize, Debug)]
        struct OpenAIStreamResponse {
            choices: Vec<OpenAIStreamChoice>,
            usage: Option<OpenAIUsage>,
        }

        #[derive(Deserialize, Debug)]
        struct OpenAIStreamChoice {
            delta: OpenAIStreamDelta,
            finish_reason: Option<String>,
        }

        #[derive(Deserialize, Debug)]
        struct OpenAIStreamDelta {
            content: Option<String>,
            tool_calls: Option<Vec<OpenAIStreamToolCall>>,
        }

        #[derive(Deserialize, Debug)]
        struct OpenAIStreamToolCall {
            index: usize,
            id: Option<String>,
            function: Option<OpenAIStreamFunction>,
        }

        #[derive(Deserialize, Debug)]
        struct OpenAIStreamFunction {
            name: Option<String>,
            arguments: Option<String>,
        }

        #[async_trait]
        impl LlmClient for OpenAIClient {
            fn provider(&self) -> &str {
                "openai"
            }

            fn model(&self) -> &str {
                &self.model
            }

            async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
                let messages: Vec<OpenAIMessage> = request
                    .messages
                    .iter()
                    .map(|m| {
                        let role = match m.role {
                            Role::System => "system",
                            Role::User => "user",
                            Role::Assistant => "assistant",
                            Role::Tool => "tool",
                        }
                        .to_string();

                        // Convert tool_calls if present
                        let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                            tcs.iter()
                                .map(|tc| OpenAIMessageToolCall {
                                    id: tc.id.clone(),
                                    r#type: "function".to_string(),
                                    function: OpenAIMessageFunction {
                                        name: tc.name.clone(),
                                        arguments: serde_json::to_string(&tc.arguments)
                                            .unwrap_or_default(),
                                    },
                                })
                                .collect()
                        });

                        // For assistant messages with tool_calls, content can be null
                        let content = if m.tool_calls.is_some() && m.content.is_empty() {
                            None
                        } else {
                            Some(m.content.clone())
                        };

                        OpenAIMessage {
                            role,
                            content,
                            tool_call_id: m.tool_call_id.clone(),
                            tool_calls,
                        }
                    })
                    .collect();

                let tools: Option<Vec<OpenAITool>> = if request.tools.is_empty() {
                    None
                } else {
                    Some(
                        request
                            .tools
                            .iter()
                            .map(|t| OpenAITool {
                                r#type: "function".to_string(),
                                function: OpenAIFunction {
                                    name: t.name.clone(),
                                    description: t.description.clone(),
                                    parameters: t.parameters.clone(),
                                },
                            })
                            .collect(),
                    )
                };

                let (max_tokens, max_completion_tokens) =
                    token_limit_fields(&self.model, request.max_tokens);
                let body = OpenAIRequest {
                    model: self.model.clone(),
                    messages,
                    tools,
                    temperature: request.temperature,
                    max_tokens,
                    max_completion_tokens,
                };

                let response = self
                    .client
                    .post(format!("{}/chat/completions", self.base_url))
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await
                    .map_err(AiError::Http)?;

                if !response.status().is_success() {
                    return Err(response_to_error(response, "OpenAI").await);
                }

                let data: OpenAIResponse = response.json().await?;
                let choice = data
                    .choices
                    .into_iter()
                    .next()
                    .ok_or_else(|| AiError::Llm("No response from OpenAI".to_string()))?;

                let tool_calls = choice
                    .message
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.id,
                        name: tc.function.name,
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(Value::Null),
                    })
                    .collect();

                let finish_reason = match choice.finish_reason.as_str() {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolCalls,
                    "length" => FinishReason::MaxTokens,
                    _ => FinishReason::Error,
                };

                let usage = data.usage.map(|u| {
                    let cost_usd =
                        calculate_cost(&self.model, u.prompt_tokens, u.completion_tokens);
                    TokenUsage {
                        prompt_tokens: u.prompt_tokens,
                        completion_tokens: u.completion_tokens,
                        total_tokens: u.total_tokens,
                        cost_usd,
                    }
                });

                Ok(CompletionResponse {
                    content: choice.message.content,
                    tool_calls,
                    finish_reason,
                    usage,
                    reasoning_content: None,
                })
            }

            fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
                let client = self.client.clone();
                let api_key = self.api_key.clone();
                let base_url = self.base_url.clone();
                let model = self.model.clone();

                Box::pin(async_stream::stream! {
                    let messages: Vec<OpenAIMessage> = request
                        .messages
                        .iter()
                        .map(|m| {
                            let role = match m.role {
                                Role::System => "system",
                                Role::User => "user",
                                Role::Assistant => "assistant",
                                Role::Tool => "tool",
                            }
                            .to_string();

                            let tool_calls = m.tool_calls.as_ref().map(|tcs| {
                                tcs.iter()
                                    .map(|tc| OpenAIMessageToolCall {
                                        id: tc.id.clone(),
                                        r#type: "function".to_string(),
                                        function: OpenAIMessageFunction {
                                            name: tc.name.clone(),
                                            arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                                        },
                                    })
                                    .collect()
                            });

                            let content = if m.tool_calls.is_some() && m.content.is_empty() {
                                None
                            } else {
                                Some(m.content.clone())
                            };

                            OpenAIMessage {
                                role,
                                content,
                                tool_call_id: m.tool_call_id.clone(),
                                tool_calls,
                            }
                        })
                        .collect();

                    let tools: Option<Vec<OpenAITool>> = if request.tools.is_empty() {
                        None
                    } else {
                        Some(
                            request
                                .tools
                                .iter()
                                .map(|t| OpenAITool {
                                    r#type: "function".to_string(),
                                    function: OpenAIFunction {
                                        name: t.name.clone(),
                                        description: t.description.clone(),
                                        parameters: t.parameters.clone(),
                                    },
                                })
                                .collect(),
                        )
                    };

                    let mut body = serde_json::json!({
                        "model": model,
                        "messages": messages,
                        "tools": tools,
                        "temperature": request.temperature,
                        "stream": true,
                        "stream_options": { "include_usage": true }
                    });
                    apply_token_limit(&mut body, &model, request.max_tokens);

                    let response = match client
                        .post(format!("{}/chat/completions", base_url))
                        .header("Authorization", format!("Bearer {}", api_key))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(resp) => resp,
                        Err(e) => {
                            yield Err(AiError::Llm(format!("Request failed: {}", e)));
                            return;
                        }
                    };

                    if !response.status().is_success() {
                        yield Err(response_to_error(response, "OpenAI").await);
                        return;
                    }

                    let mut byte_stream = response.bytes_stream();
                    let mut buffer = String::new();
                    let mut tool_call_ids: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
                    let mut tool_call_names: std::collections::HashMap<usize, String> = std::collections::HashMap::new();

                    while let Some(chunk_result) = byte_stream.next().await {
                        let chunk = match chunk_result {
                            Ok(bytes) => bytes,
                            Err(e) => {
                                yield Err(AiError::Llm(format!("Stream error: {}", e)));
                                return;
                            }
                        };

                        buffer.push_str(&String::from_utf8_lossy(&chunk));

                        // Process complete SSE events from buffer
                        while let Some(pos) = buffer.find("\n\n") {
                            let event_str = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            for line in event_str.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if data.trim() == "[DONE]" {
                                        continue;
                                    }

                                    let parsed: OpenAIStreamResponse = match serde_json::from_str(data) {
                                        Ok(p) => p,
                                        Err(_) => continue,
                                    };

                                    // Handle usage (at the end of stream)
                                    if let Some(usage) = parsed.usage {
                                        yield Ok(StreamChunk::final_chunk(
                                            FinishReason::Stop,
                                            Some(TokenUsage {
                                                prompt_tokens: usage.prompt_tokens,
                                                completion_tokens: usage.completion_tokens,
                                                total_tokens: usage.total_tokens,
                                                cost_usd: calculate_cost(
                                                    &model,
                                                    usage.prompt_tokens,
                                                    usage.completion_tokens,
                                                ),
                                            }),
                                        ));
                                        continue;
                                    }

                                    for choice in parsed.choices {
                                        // Handle finish reason
                                        if let Some(finish_reason) = choice.finish_reason {
                                            let reason = match finish_reason.as_str() {
                                                "stop" => FinishReason::Stop,
                                                "tool_calls" => FinishReason::ToolCalls,
                                                "length" => FinishReason::MaxTokens,
                                                _ => FinishReason::Error,
                                            };
                                            // Final chunk with reason but no usage yet (usage comes separately)
                                            yield Ok(StreamChunk::final_chunk(reason, None));
                                            continue;
                                        }

                                        // Handle content delta
                                        if let Some(content) = choice.delta.content
                                            && !content.is_empty()
                                        {
                                            yield Ok(StreamChunk::text(&content));
                                        }

                                        // Handle tool calls delta
                                        if let Some(tool_calls) = choice.delta.tool_calls {
                                            for tc in tool_calls {
                                                // Store id and name when they first appear
                                                if let Some(id) = &tc.id {
                                                    tool_call_ids.insert(tc.index, id.clone());
                                                }
                                                if let Some(func) = &tc.function
                                                    && let Some(name) = &func.name
                                                {
                                                    tool_call_names.insert(tc.index, name.clone());
                                                }

                                                let arguments = tc.function.as_ref().and_then(|f| f.arguments.clone());

                                                yield Ok(StreamChunk {
                                                    text: String::new(),
                                                    thinking: None,
                                                    tool_call_delta: Some(ToolCallDelta {
                                                        index: tc.index,
                                                        id: tool_call_ids.get(&tc.index).cloned(),
                                                        name: tool_call_names.get(&tc.index).cloned(),
                                                        arguments,
                                                    }),
                                                    finish_reason: None,
                                                    usage: None,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Process any remaining data in the buffer after the stream ends.
                    // This handles the case where the last SSE event lacks a trailing \n\n
                    // (e.g., due to a network interruption).
                    let remaining = buffer.trim();
                    if !remaining.is_empty() {
                        for line in remaining.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim() == "[DONE]" || data.trim().is_empty() {
                                    continue;
                                }
                                // Best effort: try to parse final event
                                if let Ok(parsed) = serde_json::from_str::<OpenAIStreamResponse>(data)
                                    && let Some(usage) = parsed.usage
                                {
                                    yield Ok(StreamChunk::final_chunk(
                                        FinishReason::Stop,
                                        Some(TokenUsage {
                                            prompt_tokens: usage.prompt_tokens,
                                            completion_tokens: usage.completion_tokens,
                                            total_tokens: usage.total_tokens,
                                            cost_usd: calculate_cost(
                                                &model,
                                                usage.prompt_tokens,
                                                usage.completion_tokens,
                                            ),
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                })
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_stream_response_deserializes_stop_finish_reason() {
                let json = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
                let parsed: OpenAIStreamResponse = serde_json::from_str(json).unwrap();
                assert_eq!(parsed.choices.len(), 1);
                assert_eq!(parsed.choices[0].finish_reason, Some("stop".to_string()));
            }

            #[test]
            fn test_finish_reason_mapping() {
                // Verify the mapping from string to FinishReason enum
                let map = |s: &str| match s {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolCalls,
                    "length" => FinishReason::MaxTokens,
                    _ => FinishReason::Error,
                };
                assert_eq!(map("stop"), FinishReason::Stop);
                assert_eq!(map("tool_calls"), FinishReason::ToolCalls);
                assert_eq!(map("length"), FinishReason::MaxTokens);
                assert_eq!(map("unknown"), FinishReason::Error);
            }

            #[test]
            fn gpt5_models_use_max_completion_tokens() {
                let (max_tokens, max_completion_tokens) = token_limit_fields("gpt-5-2", Some(128));

                assert_eq!(max_tokens, None);
                assert_eq!(max_completion_tokens, Some(128));
            }

            #[test]
            fn gpt5_api_names_use_max_completion_tokens() {
                let (max_tokens, max_completion_tokens) = token_limit_fields("gpt-5.2", Some(128));

                assert_eq!(max_tokens, None);
                assert_eq!(max_completion_tokens, Some(128));
            }

            #[test]
            fn legacy_models_use_max_tokens() {
                let (max_tokens, max_completion_tokens) = token_limit_fields("gpt-4o", Some(128));

                assert_eq!(max_tokens, Some(128));
                assert_eq!(max_completion_tokens, None);
            }

            #[test]
            fn openai_request_serializes_only_supported_token_limit_field() {
                let (max_tokens, max_completion_tokens) = token_limit_fields("gpt-5-2", Some(128));
                let request = OpenAIRequest {
                    model: "gpt-5-2".to_string(),
                    messages: Vec::new(),
                    tools: None,
                    temperature: None,
                    max_tokens,
                    max_completion_tokens,
                };
                let value = serde_json::to_value(request).unwrap();

                assert_eq!(value["max_completion_tokens"], 128);
                assert!(value.get("max_tokens").is_none());
            }

            #[test]
            fn streaming_body_uses_max_completion_tokens_for_gpt5_models() {
                let mut body = serde_json::json!({
                    "model": "gpt-5-2",
                    "messages": [],
                    "stream": true
                });

                apply_token_limit(&mut body, "gpt-5-2", Some(128));

                assert_eq!(body["max_completion_tokens"], 128);
                assert!(body.get("max_tokens").is_none());
            }
        }
    }

    use reqwest::Client;

    pub use anthropic::AnthropicClient;
    pub use deepseek::DeepSeekClient;
    pub use openai::OpenAIClient;

    const DISABLE_SYSTEM_PROXY_ENV: &str = "RESTFLOW_DISABLE_SYSTEM_PROXY";

    fn build_http_client() -> Result<Client, reqwest::Error> {
        if should_disable_system_proxy() {
            Client::builder().no_proxy().build()
        } else {
            Client::builder().build()
        }
    }

    fn should_disable_system_proxy() -> bool {
        if std::env::var_os(DISABLE_SYSTEM_PROXY_ENV).is_some() {
            return true;
        }

        cfg!(test)
    }
}

mod factory {
    //! LLM client factory for dynamic model creation

    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::error::{AiError, Result};
    use crate::retry::RetryingLlmClient;
    use crate::{
        AnthropicClient, CodexClient, DeepSeekClient, GeminiCliClient, LlmClient, OpenAIClient,
        OpenCodeClient,
    };
    use types::{ClientKind, LlmProvider, ModelSpec};

    pub trait LlmClientFactory: Send + Sync {
        fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>>;
        fn available_models(&self) -> Vec<String>;
        fn resolve_api_key(&self, provider: LlmProvider) -> Option<String>;
        fn provider_for_model(&self, model: &str) -> Option<LlmProvider>;
        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind>;
    }

    pub struct DefaultLlmClientFactory {
        api_keys: HashMap<LlmProvider, String>,
        models: HashMap<String, ModelSpec>,
    }

    impl DefaultLlmClientFactory {
        pub fn new(api_keys: HashMap<LlmProvider, String>, models: Vec<ModelSpec>) -> Self {
            let mut map = HashMap::new();
            for spec in models {
                map.insert(normalize_model_name(&spec.name), spec);
            }
            Self {
                api_keys,
                models: map,
            }
        }

        fn model_spec(&self, model: &str) -> Result<ModelSpec> {
            let key = normalize_model_name(model);
            self.models
                .get(&key)
                .cloned()
                .ok_or_else(|| AiError::Llm(format!("Unknown model '{model}'")))
        }
    }

    impl LlmClientFactory for DefaultLlmClientFactory {
        fn create_client(&self, model: &str, api_key: Option<&str>) -> Result<Arc<dyn LlmClient>> {
            let spec = self.model_spec(model)?;

            let client: Arc<dyn LlmClient> = match spec.client_kind {
                ClientKind::OpenCodeCli => {
                    let mut c = OpenCodeClient::new().with_model(spec.client_model);
                    if let Some(key) = api_key {
                        let env_var = detect_env_var(key);
                        c = c.with_provider_env(env_var, key.to_string());
                    }
                    Arc::new(c)
                }
                ClientKind::CodexCli => Arc::new(CodexClient::new().with_model(spec.client_model)),
                ClientKind::GeminiCli => {
                    let mut c = GeminiCliClient::new().with_model(spec.client_model);
                    if let Some(key) = api_key {
                        c = c.with_api_key(key.to_string());
                    }
                    Arc::new(c)
                }
                ClientKind::ClaudeCodeCli => {
                    return Err(AiError::Llm(
                        "Claude Code CLI support has been removed".to_string(),
                    ));
                }
                ClientKind::Http => {
                    let key = api_key.ok_or_else(|| {
                        AiError::Llm(format!("{} API key is required", spec.provider.as_str()))
                    })?;

                    match spec.provider {
                        LlmProvider::Anthropic => {
                            Arc::new(AnthropicClient::new(key)?.with_model(spec.client_model))
                        }
                        LlmProvider::DeepSeek => {
                            Arc::new(DeepSeekClient::new(key)?.with_model(spec.client_model))
                        }
                        LlmProvider::MiniMax | LlmProvider::MiniMaxCodingPlan => Arc::new(
                            AnthropicClient::new(key)?
                                .with_model(spec.client_model)
                                .with_base_url("https://api.minimax.io/anthropic"),
                        ),
                        provider => {
                            let base_url = spec.base_url.as_deref().unwrap_or(provider.base_url());
                            Arc::new(
                                OpenAIClient::new(key)?
                                    .with_model(spec.client_model)
                                    .with_base_url(base_url),
                            )
                        }
                    }
                }
            };

            Ok(Arc::new(RetryingLlmClient::with_default_config(client)))
        }

        fn available_models(&self) -> Vec<String> {
            let mut models: Vec<String> =
                self.models.values().map(|spec| spec.name.clone()).collect();
            models.sort();
            models
        }

        fn resolve_api_key(&self, provider: LlmProvider) -> Option<String> {
            self.api_keys.get(&provider).cloned()
        }

        fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
            let key = normalize_model_name(model);
            self.models.get(&key).map(|spec| spec.provider)
        }

        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
            let key = normalize_model_name(model);
            self.models.get(&key).map(|spec| spec.client_kind)
        }
    }

    fn normalize_model_name(model: &str) -> String {
        model.trim().to_lowercase()
    }

    fn detect_env_var(api_key: &str) -> &'static str {
        let normalized = api_key.trim();
        if normalized.starts_with("sk-ant-") {
            "ANTHROPIC_API_KEY"
        } else if normalized.starts_with("ghp_") || normalized.starts_with("gho_") {
            "GITHUB_TOKEN"
        } else if normalized.starts_with("xai-") {
            "XAI_API_KEY"
        } else if normalized.starts_with("sk-or-") {
            "OPENROUTER_API_KEY"
        } else if normalized.starts_with("gsk_") {
            "GROQ_API_KEY"
        } else if normalized.starts_with("AIza") {
            "GEMINI_API_KEY"
        } else {
            "OPENAI_API_KEY"
        }
    }

    #[cfg(test)]
    mod tests {
        use std::collections::HashMap;

        use super::{DefaultLlmClientFactory, LlmClientFactory, LlmProvider};
        use types::{ClientKind, ModelSpec};

        #[test]
        fn zai_uses_api_z_ai_endpoint() {
            assert_eq!(LlmProvider::Zai.base_url(), "https://api.z.ai/api/paas/v4");
        }

        #[test]
        fn factory_reports_client_kind_for_known_models() {
            let factory = DefaultLlmClientFactory::new(
                HashMap::new(),
                vec![
                    ModelSpec::new("gpt-5", LlmProvider::OpenAI, "gpt-5"),
                    ModelSpec::codex("gpt-5.3-codex", "gpt-5.3-codex"),
                    ModelSpec::claude_code("claude-code-opus", "opus"),
                ],
            );

            assert_eq!(
                factory.client_kind_for_model("gpt-5"),
                Some(ClientKind::Http)
            );
            assert_eq!(
                factory.client_kind_for_model("gpt-5.3-codex"),
                Some(ClientKind::CodexCli)
            );
            assert_eq!(
                factory.client_kind_for_model("claude-code-opus"),
                Some(ClientKind::ClaudeCodeCli)
            );
            assert_eq!(factory.client_kind_for_model("missing"), None);
        }

        #[test]
        fn deepseek_provider_creates_deepseek_client() {
            let mut api_keys: HashMap<LlmProvider, String> = HashMap::new();
            api_keys.insert(LlmProvider::DeepSeek, "sk-test-deepseek-key".to_string());

            let factory = DefaultLlmClientFactory::new(
                api_keys,
                vec![ModelSpec::new(
                    "deepseek-v4-pro",
                    LlmProvider::DeepSeek,
                    "deepseek-v4-pro",
                )],
            );

            let client = factory
                .create_client("deepseek-v4-pro", Some("sk-test-deepseek-key"))
                .expect("DeepSeek client creation should succeed");

            // Verify the client is wired correctly (provider() returns "deepseek")
            assert_eq!(client.provider(), "deepseek");
            assert_eq!(client.model(), "deepseek-v4-pro");
        }

        #[test]
        fn openai_provider_still_uses_openai_client() {
            let mut api_keys: HashMap<LlmProvider, String> = HashMap::new();
            api_keys.insert(LlmProvider::OpenAI, "sk-test-openai-key".to_string());

            let factory = DefaultLlmClientFactory::new(
                api_keys,
                vec![ModelSpec::new("gpt-5", LlmProvider::OpenAI, "gpt-5")],
            );

            let client = factory
                .create_client("gpt-5", Some("sk-test-openai-key"))
                .expect("OpenAI client creation should succeed");

            assert_eq!(client.provider(), "openai");
        }

        #[test]
        fn create_client_rejects_claude_code_cli_models() {
            let factory = DefaultLlmClientFactory::new(
                HashMap::new(),
                vec![ModelSpec::claude_code("claude-code-opus", "opus")],
            );

            let err = match factory.create_client("claude-code-opus", Some("sk-ant-oat01-test")) {
                Ok(_) => panic!("claude-code client should be rejected"),
                Err(err) => err,
            };

            assert!(
                err.to_string()
                    .contains("Claude Code CLI support has been removed")
            );
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod mock_client {
    //! Deterministic mock LLM client for stress and reliability tests.

    use std::collections::VecDeque;
    use std::sync::Arc;

    use async_stream::try_stream;
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use tokio::time::{Duration, sleep};

    use crate::error::{AiError, Result};

    use super::{
        CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamChunk, StreamResult,
        TokenUsage, ToolCall,
    };

    /// Deterministic step for scripted mock completions.
    #[derive(Debug, Clone)]
    pub enum MockStepKind {
        /// Return a plain assistant message.
        Text(String),
        /// Return a tool call response.
        ToolCall {
            id: String,
            name: String,
            arguments: serde_json::Value,
        },
        /// Return an LLM error.
        Error(String),
        /// Return a timeout-like error after optional delay.
        Timeout,
    }

    /// Scripted completion step with optional delay.
    #[derive(Debug, Clone)]
    pub struct MockStep {
        pub delay_ms: u64,
        pub kind: MockStepKind,
    }

    impl MockStep {
        pub fn text(content: impl Into<String>) -> Self {
            Self {
                delay_ms: 0,
                kind: MockStepKind::Text(content.into()),
            }
        }

        pub fn tool_call(
            id: impl Into<String>,
            name: impl Into<String>,
            arguments: serde_json::Value,
        ) -> Self {
            Self {
                delay_ms: 0,
                kind: MockStepKind::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                },
            }
        }

        pub fn error(message: impl Into<String>) -> Self {
            Self {
                delay_ms: 0,
                kind: MockStepKind::Error(message.into()),
            }
        }

        pub fn timeout(delay_ms: u64) -> Self {
            Self {
                delay_ms,
                kind: MockStepKind::Timeout,
            }
        }

        pub fn with_delay(mut self, delay_ms: u64) -> Self {
            self.delay_ms = delay_ms;
            self
        }
    }

    /// A deterministic mock LLM client driven by scripted steps.
    #[derive(Debug, Clone, Default)]
    pub struct MockLlmClient {
        model: String,
        script: Arc<Mutex<VecDeque<MockStep>>>,
    }

    impl MockLlmClient {
        pub fn new(model: impl Into<String>) -> Self {
            Self {
                model: model.into(),
                script: Arc::new(Mutex::new(VecDeque::new())),
            }
        }

        pub fn from_steps(model: impl Into<String>, steps: Vec<MockStep>) -> Self {
            Self {
                model: model.into(),
                script: Arc::new(Mutex::new(VecDeque::from(steps))),
            }
        }

        async fn next_step(&self) -> Option<MockStep> {
            self.script.lock().await.pop_front()
        }

        fn usage_for(content_len: usize) -> TokenUsage {
            let completion_tokens = content_len as u32;
            TokenUsage {
                prompt_tokens: 1,
                completion_tokens,
                total_tokens: 1 + completion_tokens,
                cost_usd: Some(0.0),
            }
        }

        fn fallback_response(request: &CompletionRequest) -> CompletionResponse {
            let text = request
                .messages
                .iter()
                .rev()
                .find(|msg| matches!(msg.role, super::Role::User))
                .map(|msg| format!("mock-echo: {}", msg.content))
                .unwrap_or_else(|| "mock-ok".to_string());

            CompletionResponse {
                content: Some(text.clone()),
                tool_calls: Vec::new(),
                finish_reason: FinishReason::Stop,
                usage: Some(Self::usage_for(text.len())),
                reasoning_content: None,
            }
        }
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        fn provider(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            &self.model
        }

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            let step = self.next_step().await;
            let Some(step) = step else {
                return Ok(Self::fallback_response(&request));
            };

            if step.delay_ms > 0 {
                sleep(Duration::from_millis(step.delay_ms)).await;
            }

            match step.kind {
                MockStepKind::Text(content) => Ok(CompletionResponse {
                    usage: Some(Self::usage_for(content.len())),
                    content: Some(content),
                    tool_calls: Vec::new(),
                    finish_reason: FinishReason::Stop,
                    reasoning_content: None,
                }),
                MockStepKind::ToolCall {
                    id,
                    name,
                    arguments,
                } => Ok(CompletionResponse {
                    usage: Some(Self::usage_for(0)),
                    content: None,
                    tool_calls: vec![ToolCall {
                        id,
                        name,
                        arguments,
                    }],
                    finish_reason: FinishReason::ToolCalls,
                    reasoning_content: None,
                }),
                MockStepKind::Error(message) => Err(AiError::Llm(message)),
                MockStepKind::Timeout => Err(AiError::Llm("mock timeout".to_string())),
            }
        }

        fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
            let client = self.clone();
            Box::pin(try_stream! {
                let response = client.complete(request).await?;

                if let Some(content) = response.content
                    && !content.is_empty()
                {
                    yield StreamChunk::text(content);
                }

                yield StreamChunk::final_chunk(response.finish_reason, response.usage);
            })
        }

        fn supports_streaming(&self) -> bool {
            true
        }
    }

    #[cfg(test)]
    mod tests {
        use futures::TryStreamExt;

        use super::*;
        use crate::{CompletionRequest, Message};

        #[tokio::test]
        async fn mock_client_returns_scripted_text() {
            let client = MockLlmClient::from_steps("mock-model", vec![MockStep::text("hello")]);

            let response = client
                .complete(CompletionRequest::new(vec![Message::user("ping")]))
                .await
                .expect("mock response should succeed");

            assert_eq!(response.content.as_deref(), Some("hello"));
            assert_eq!(response.finish_reason, FinishReason::Stop);
        }

        #[tokio::test]
        async fn mock_client_returns_scripted_tool_call() {
            let client = MockLlmClient::from_steps(
                "mock-model",
                vec![MockStep::tool_call(
                    "call-1",
                    "search",
                    serde_json::json!({"q": "restflow"}),
                )],
            );

            let response = client
                .complete(CompletionRequest::new(vec![Message::user("use tool")]))
                .await
                .expect("tool call response should succeed");

            assert_eq!(response.finish_reason, FinishReason::ToolCalls);
            assert_eq!(response.tool_calls.len(), 1);
            assert_eq!(response.tool_calls[0].name, "search");
        }

        #[tokio::test]
        async fn mock_client_supports_streaming() {
            let client = MockLlmClient::from_steps("mock-model", vec![MockStep::text("stream")]);

            let chunks = client
                .complete_stream(CompletionRequest::new(vec![Message::user("hi")]))
                .try_collect::<Vec<_>>()
                .await
                .expect("stream should succeed");

            assert!(!chunks.is_empty());
            assert_eq!(chunks[0].text, "stream");
            assert!(
                chunks
                    .last()
                    .and_then(|chunk| chunk.finish_reason.as_ref())
                    .is_some()
            );
        }
    }
}

pub mod pricing {
    //! Model pricing and cost calculation for LLM API calls.

    use once_cell::sync::Lazy;
    use serde::Deserialize;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::RwLock;

    /// Pricing per 1 million tokens (USD).
    #[derive(Debug, Clone, Copy)]
    pub struct ModelPricing {
        pub cost_per_1m_input: f64,
        pub cost_per_1m_output: f64,
        pub cache_read_per_1m: Option<f64>,
        pub cache_write_per_1m: Option<f64>,
    }

    #[derive(Default)]
    struct DynamicPricingCache {
        loaded: bool,
        by_model: HashMap<String, ModelPricing>,
    }

    #[derive(Debug, Deserialize)]
    struct ModelsDevProvider {
        models: HashMap<String, ModelsDevModel>,
    }

    #[derive(Debug, Deserialize)]
    struct ModelsDevModel {
        id: Option<String>,
        cost: Option<ModelsDevCost>,
    }

    #[derive(Debug, Deserialize)]
    struct ModelsDevCost {
        input: f64,
        output: f64,
    }

    static DYNAMIC_PRICING_CACHE: Lazy<RwLock<DynamicPricingCache>> =
        Lazy::new(|| RwLock::new(DynamicPricingCache::default()));

    fn normalize(value: &str) -> String {
        value.trim().to_ascii_lowercase()
    }

    fn pricing_candidates(model_name: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        let mut push = |value: String| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return;
            }
            let key = normalize(trimmed);
            if seen.insert(key) {
                candidates.push(trimmed.to_string());
            }
        };

        push(model_name.to_string());
        push(model_name.replace('.', "-"));
        push(model_name.replace('-', "."));

        if let Some(base) = model_name.strip_suffix("-preview") {
            push(base.to_string());
        }
        if let Some((_, tail)) = model_name.split_once('/') {
            push(tail.to_string());
            push(tail.replace('.', "-"));
            push(tail.replace('-', "."));
        }

        candidates
    }

    fn resolve_models_cache_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("RESTFLOW_MODELS_PATH")
            && !path.trim().is_empty()
        {
            return Some(PathBuf::from(path));
        }

        if let Ok(dir) = std::env::var("RESTFLOW_DIR")
            && !dir.trim().is_empty()
        {
            return Some(PathBuf::from(dir).join("cache").join("models.json"));
        }

        dirs::home_dir().map(|home| home.join(".restflow").join("cache").join("models.json"))
    }

    /// Canonical providers whose pricing should take precedence over third-party resellers.
    const CANONICAL_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "deepseek",
        "google",
        "azure",
        "amazon-bedrock",
    ];

    fn is_canonical_provider(provider_name: &str) -> bool {
        let name = provider_name.to_ascii_lowercase();
        CANONICAL_PROVIDERS.iter().any(|&p| name == p)
    }

    fn load_dynamic_pricing_cache() -> HashMap<String, ModelPricing> {
        let Some(path) = resolve_models_cache_path() else {
            return HashMap::new();
        };

        let Ok(raw) = std::fs::read_to_string(path) else {
            return HashMap::new();
        };

        let Ok(root) = serde_json::from_str::<HashMap<String, ModelsDevProvider>>(&raw) else {
            return HashMap::new();
        };

        // Track which keys were set by canonical providers so we don't overwrite them.
        let mut canonical_keys: HashSet<String> = HashSet::new();
        let mut by_model = HashMap::new();

        // Two-pass: canonical providers first, then others fill gaps.
        for (provider_name, provider) in &root {
            if !is_canonical_provider(provider_name) {
                continue;
            }
            for (model_key, model) in &provider.models {
                let Some(ref cost) = model.cost else {
                    continue;
                };
                if cost.input == 0.0 && cost.output == 0.0 {
                    continue;
                }
                let pricing = ModelPricing {
                    cost_per_1m_input: cost.input,
                    cost_per_1m_output: cost.output,
                    cache_read_per_1m: None,
                    cache_write_per_1m: None,
                };
                let key = normalize(model_key);
                by_model.insert(key.clone(), pricing);
                canonical_keys.insert(key);
                if let Some(id) = model.id.as_deref() {
                    let id_key = normalize(id);
                    by_model.insert(id_key.clone(), pricing);
                    canonical_keys.insert(id_key);
                }
            }
        }

        // Second pass: non-canonical providers fill in models not yet covered.
        for (provider_name, provider) in &root {
            if is_canonical_provider(provider_name) {
                continue;
            }
            for (model_key, model) in &provider.models {
                let Some(ref cost) = model.cost else {
                    continue;
                };
                if cost.input == 0.0 && cost.output == 0.0 {
                    continue;
                }
                let pricing = ModelPricing {
                    cost_per_1m_input: cost.input,
                    cost_per_1m_output: cost.output,
                    cache_read_per_1m: None,
                    cache_write_per_1m: None,
                };
                let key = normalize(model_key);
                if !canonical_keys.contains(&key) {
                    by_model.entry(key).or_insert(pricing);
                }
                if let Some(id) = model.id.as_deref() {
                    let id_key = normalize(id);
                    if !canonical_keys.contains(&id_key) {
                        by_model.entry(id_key).or_insert(pricing);
                    }
                }
            }
        }

        by_model
    }

    fn dynamic_pricing(model_name: &str) -> Option<ModelPricing> {
        {
            let cache = DYNAMIC_PRICING_CACHE.read().ok()?;
            if cache.loaded {
                for candidate in pricing_candidates(model_name) {
                    if let Some(pricing) = cache.by_model.get(&normalize(&candidate)) {
                        return Some(*pricing);
                    }
                }
                return None;
            }
        }

        {
            let mut cache = DYNAMIC_PRICING_CACHE.write().ok()?;
            if !cache.loaded {
                cache.by_model = load_dynamic_pricing_cache();
                cache.loaded = true;
            }
            for candidate in pricing_candidates(model_name) {
                if let Some(pricing) = cache.by_model.get(&normalize(&candidate)) {
                    return Some(*pricing);
                }
            }
        }

        None
    }

    /// Get pricing for a model by API name.
    /// Returns None for CLI-based models where cost is tracked externally.
    pub fn get_pricing(model_name: &str) -> Option<ModelPricing> {
        // Match model name prefixes to handle versioned model names
        // e.g., "claude-sonnet-4-20250514" should match ClaudeSonnet4_5

        // CLI-based models (cost tracked externally) - check first to avoid prefix matching
        // codex-cli, claude-code CLI aliases
        if model_name.contains("codex") || model_name == "gpt-5.3-codex" {
            return None;
        }
        if model_name == "opus" || model_name == "sonnet" || model_name == "haiku" {
            return None;
        }

        if let Some(pricing) = dynamic_pricing(model_name) {
            return Some(pricing);
        }

        // OpenAI
        if model_name.starts_with("gpt-5.4-mini") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.75,
                cost_per_1m_output: 4.50,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("gpt-5.4-nano") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.20,
                cost_per_1m_output: 1.25,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("gpt-5.4") {
            return Some(ModelPricing {
                cost_per_1m_input: 2.50,
                cost_per_1m_output: 15.0,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("gpt-5-pro") {
            return Some(ModelPricing {
                cost_per_1m_input: 10.0,
                cost_per_1m_output: 40.0,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("gpt-5-mini") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.4,
                cost_per_1m_output: 1.6,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("gpt-5-nano") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.1,
                cost_per_1m_output: 0.4,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("gpt-5") || model_name == "gpt-5" {
            return Some(ModelPricing {
                cost_per_1m_input: 1.25,
                cost_per_1m_output: 10.0,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }

        // Anthropic
        if model_name.starts_with("claude-opus-4-6") || model_name.starts_with("claude-opus-4") {
            return Some(ModelPricing {
                cost_per_1m_input: 15.0,
                cost_per_1m_output: 75.0,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("claude-sonnet-4") {
            return Some(ModelPricing {
                cost_per_1m_input: 3.0,
                cost_per_1m_output: 15.0,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("claude-haiku-4") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.8,
                cost_per_1m_output: 4.0,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }

        // DeepSeek
        if model_name.starts_with("deepseek-reasoner") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.55,
                cost_per_1m_output: 2.19,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }
        if model_name.starts_with("deepseek-chat") || model_name.starts_with("deepseek") {
            return Some(ModelPricing {
                cost_per_1m_input: 0.27,
                cost_per_1m_output: 1.10,
                cache_read_per_1m: None,
                cache_write_per_1m: None,
            });
        }

        // Unknown model - return None
        None
    }

    /// Calculate cost in USD from token usage and model name.
    pub fn calculate_cost(model_name: &str, input_tokens: u32, output_tokens: u32) -> Option<f64> {
        let pricing = get_pricing(model_name)?;
        let cost = (input_tokens as f64 / 1_000_000.0) * pricing.cost_per_1m_input
            + (output_tokens as f64 / 1_000_000.0) * pricing.cost_per_1m_output;
        Some(cost)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_pricing_anthropic_sonnet() {
            let pricing = get_pricing("claude-sonnet-4-20250514").unwrap();
            // Hardcoded fallback: input=3.0, output=15.0
            // Dynamic cache may override if models.json exists; canonical provider pricing is preferred.
            assert!(
                pricing.cost_per_1m_input > 0.0 && pricing.cost_per_1m_input <= 5.0,
                "Sonnet input price {} out of expected range",
                pricing.cost_per_1m_input
            );
            assert!(
                pricing.cost_per_1m_output > 0.0 && pricing.cost_per_1m_output <= 20.0,
                "Sonnet output price {} out of expected range",
                pricing.cost_per_1m_output
            );
        }

        #[test]
        fn test_pricing_openai_gpt5() {
            let pricing = get_pricing("gpt-5").unwrap();
            // Hardcoded fallback: input=1.25, output=10.0
            // Dynamic cache may override if models.json exists; canonical provider pricing is preferred.
            assert!(
                pricing.cost_per_1m_input > 0.0 && pricing.cost_per_1m_input <= 5.0,
                "GPT-5 input price {} out of expected range",
                pricing.cost_per_1m_input
            );
            assert!(
                pricing.cost_per_1m_output > 0.0 && pricing.cost_per_1m_output <= 20.0,
                "GPT-5 output price {} out of expected range",
                pricing.cost_per_1m_output
            );
        }

        #[test]
        fn test_pricing_openai_gpt54() {
            let pricing = get_pricing("gpt-5.4").unwrap();
            assert_eq!(pricing.cost_per_1m_input, 2.50);
            assert_eq!(pricing.cost_per_1m_output, 15.0);

            let mini = get_pricing("gpt-5.4-mini").unwrap();
            assert_eq!(mini.cost_per_1m_input, 0.75);
            assert_eq!(mini.cost_per_1m_output, 4.50);

            let nano = get_pricing("gpt-5.4-nano").unwrap();
            assert_eq!(nano.cost_per_1m_input, 0.20);
            assert_eq!(nano.cost_per_1m_output, 1.25);
        }

        #[test]
        fn test_pricing_cli_models_none() {
            assert!(get_pricing("opus").is_none());
            assert!(get_pricing("sonnet").is_none());
            assert!(get_pricing("gpt-5.3-codex").is_none());
        }

        #[test]
        fn test_calculate_cost() {
            // Use the actual pricing returned by get_pricing (may come from dynamic cache)
            let pricing = get_pricing("claude-sonnet-4-20250514").unwrap();
            let expected = (1000.0 / 1_000_000.0) * pricing.cost_per_1m_input
                + (500.0 / 1_000_000.0) * pricing.cost_per_1m_output;
            let cost = calculate_cost("claude-sonnet-4-20250514", 1000, 500).unwrap();
            assert!(
                (cost - expected).abs() < 1e-10,
                "cost={cost}, expected={expected}"
            );
        }

        #[test]
        fn test_calculate_cost_zero_tokens() {
            let cost = calculate_cost("claude-sonnet-4-20250514", 0, 0).unwrap();
            assert_eq!(cost, 0.0);
        }

        #[test]
        fn test_canonical_provider_priority() {
            // Verify the canonical provider list is reasonable
            assert!(is_canonical_provider("anthropic"));
            assert!(is_canonical_provider("openai"));
            assert!(is_canonical_provider("deepseek"));
            assert!(is_canonical_provider("google"));
            assert!(!is_canonical_provider("aihubmix"));
            assert!(!is_canonical_provider("jiekou"));
        }

        #[test]
        fn test_pricing_candidates_generation() {
            let candidates = pricing_candidates("claude-sonnet-4-20250514");
            assert!(candidates.contains(&"claude-sonnet-4-20250514".to_string()));

            let candidates = pricing_candidates("gpt-5");
            assert!(candidates.contains(&"gpt-5".to_string()));
        }
    }
}

mod retry {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use futures::StreamExt;
    use reqwest::Response;

    use crate::client::{CompletionRequest, CompletionResponse, LlmClient, StreamResult};
    use crate::error::AiError;

    #[derive(Debug, Clone)]
    pub struct LlmRetryConfig {
        pub max_retries: u32,
        pub initial_delay_ms: u64,
        pub max_delay_ms: u64,
        pub backoff_multiplier: f64,
    }

    impl Default for LlmRetryConfig {
        fn default() -> Self {
            Self {
                max_retries: 3,
                initial_delay_ms: 200,
                max_delay_ms: 5_000,
                backoff_multiplier: 2.0,
            }
        }
    }

    impl LlmRetryConfig {
        pub fn delay_for(&self, attempt: u32, retry_after_secs: Option<u64>) -> Duration {
            if let Some(seconds) = retry_after_secs {
                return Duration::from_secs(seconds);
            }

            let multiplier = self
                .backoff_multiplier
                .powi(attempt.saturating_sub(1) as i32);
            let delay = (self.initial_delay_ms as f64 * multiplier) as u64;
            Duration::from_millis(delay.min(self.max_delay_ms))
        }
    }

    pub fn parse_retry_after(response: &Response) -> Option<u64> {
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
    }

    pub async fn response_to_error(response: Response, provider: &str) -> AiError {
        let status = response.status().as_u16();
        let retry_after = parse_retry_after(&response);
        let body = response.text().await.unwrap_or_default();

        // Truncate error body to prevent leaking large or sensitive responses.
        const MAX_ERROR_BODY: usize = 512;
        let message = if body.len() > MAX_ERROR_BODY {
            // Find safe character boundary to avoid panic on multi-byte UTF-8
            let truncate_at = body
                .char_indices()
                .take_while(|(idx, _)| *idx < MAX_ERROR_BODY)
                .last()
                .map(|(idx, c)| idx + c.len_utf8())
                .unwrap_or(0);
            format!("{}... [truncated]", &body[..truncate_at])
        } else {
            body
        };

        AiError::LlmHttp {
            provider: provider.to_string(),
            status,
            message,
            retry_after_secs: retry_after,
        }
    }

    /// Decorator that adds retry logic around any `LlmClient`.
    ///
    /// Wraps a `complete()` call with exponential backoff and retryable-error
    /// detection. `complete_stream()` retries only when failure happens before
    /// any chunk is received.
    pub struct RetryingLlmClient {
        inner: Arc<dyn LlmClient>,
        config: LlmRetryConfig,
    }

    impl RetryingLlmClient {
        pub fn new(inner: Arc<dyn LlmClient>, config: LlmRetryConfig) -> Self {
            Self { inner, config }
        }

        pub fn with_default_config(inner: Arc<dyn LlmClient>) -> Self {
            Self::new(inner, LlmRetryConfig::default())
        }
    }

    #[async_trait]
    impl LlmClient for RetryingLlmClient {
        fn provider(&self) -> &str {
            self.inner.provider()
        }

        fn model(&self) -> &str {
            self.inner.model()
        }

        fn supports_streaming(&self) -> bool {
            self.inner.supports_streaming()
        }

        async fn complete(
            &self,
            request: CompletionRequest,
        ) -> crate::error::Result<CompletionResponse> {
            let mut last_error = None;

            for attempt in 0..=self.config.max_retries {
                let req = request.clone();
                match self.inner.complete(req).await {
                    Ok(response) => return Ok(response),
                    Err(error) => {
                        if !error.is_retryable() || attempt == self.config.max_retries {
                            return Err(error);
                        }
                        let delay = self.config.delay_for(attempt + 1, error.retry_after());
                        tracing::warn!(
                            provider = self.inner.provider(),
                            model = self.inner.model(),
                            attempt = attempt + 1,
                            delay_ms = delay.as_millis() as u64,
                            error = %error,
                            "Retrying LLM request"
                        );
                        tokio::time::sleep(delay).await;
                        last_error = Some(error);
                    }
                }
            }

            Err(last_error.unwrap_or_else(|| {
                AiError::Llm(format!(
                    "{}/{} request failed after retries",
                    self.inner.provider(),
                    self.inner.model()
                ))
            }))
        }

        fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
            let inner = Arc::clone(&self.inner);
            let config = self.config.clone();

            Box::pin(async_stream::stream! {
                let mut retry_attempts = 0u32;

                'retry_loop: loop {
                    let mut saw_any_chunk = false;
                    let mut stream = inner.complete_stream(request.clone());

                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(chunk) => {
                                saw_any_chunk = true;
                                yield Ok(chunk);
                            }
                            Err(error) => {
                                let can_retry = !saw_any_chunk
                                    && error.is_retryable()
                                    && retry_attempts < config.max_retries;

                                if can_retry {
                                    retry_attempts += 1;
                                    let delay = config.delay_for(retry_attempts, error.retry_after());
                                    tracing::warn!(
                                        provider = inner.provider(),
                                        model = inner.model(),
                                        attempt = retry_attempts,
                                        delay_ms = delay.as_millis() as u64,
                                        error = %error,
                                        "Retrying LLM streaming request before first chunk"
                                    );
                                    tokio::time::sleep(delay).await;
                                    continue 'retry_loop;
                                }

                                yield Err(error);
                                return;
                            }
                        }
                    }

                    return;
                }
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::client::{FinishReason, Message, StreamChunk, TokenUsage};
        use crate::error::Result;
        use futures::stream;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct MockRetryClient {
            stream_calls: AtomicUsize,
            stream_results: Mutex<Vec<Vec<Result<StreamChunk>>>>,
        }

        impl MockRetryClient {
            fn new(stream_results: Vec<Vec<Result<StreamChunk>>>) -> Self {
                Self {
                    stream_calls: AtomicUsize::new(0),
                    stream_results: Mutex::new(stream_results.into_iter().rev().collect()),
                }
            }

            fn stream_call_count(&self) -> usize {
                self.stream_calls.load(Ordering::SeqCst)
            }
        }

        #[async_trait]
        impl LlmClient for MockRetryClient {
            fn provider(&self) -> &str {
                "mock"
            }

            fn model(&self) -> &str {
                "mock-model"
            }

            async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
                Ok(CompletionResponse {
                    content: Some("ok".to_string()),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: Some(TokenUsage {
                        prompt_tokens: 1,
                        completion_tokens: 1,
                        total_tokens: 2,
                        cost_usd: None,
                    }),
                    reasoning_content: None,
                })
            }

            fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
                self.stream_calls.fetch_add(1, Ordering::SeqCst);
                let next = self
                    .stream_results
                    .lock()
                    .unwrap()
                    .pop()
                    .unwrap_or_default();
                Box::pin(stream::iter(next))
            }
        }

        #[test]
        fn test_delay_progression() {
            let config = LlmRetryConfig::default();
            assert_eq!(config.delay_for(1, None), Duration::from_millis(200));
            assert_eq!(config.delay_for(2, None), Duration::from_millis(400));
            assert_eq!(config.delay_for(3, None), Duration::from_millis(800));
            assert_eq!(config.delay_for(4, None), Duration::from_millis(1600));
            assert_eq!(config.delay_for(5, None), Duration::from_millis(3200));
            assert_eq!(config.delay_for(6, None), Duration::from_millis(5000));
        }

        #[test]
        fn test_retry_after_overrides_backoff() {
            let config = LlmRetryConfig::default();
            assert_eq!(config.delay_for(3, Some(10)), Duration::from_secs(10));
        }

        #[test]
        fn test_ai_error_is_retryable() {
            let retryable = AiError::LlmHttp {
                provider: "Test".to_string(),
                status: 429,
                message: "rate limit".to_string(),
                retry_after_secs: None,
            };
            let non_retryable = AiError::LlmHttp {
                provider: "Test".to_string(),
                status: 401,
                message: "unauthorized".to_string(),
                retry_after_secs: None,
            };
            assert!(retryable.is_retryable());
            assert!(!non_retryable.is_retryable());
        }

        #[test]
        fn test_ai_error_llm_string_fallback() {
            let retryable = AiError::Llm("rate limit".to_string());
            let non_retryable = AiError::Llm("bad request".to_string());
            assert!(retryable.is_retryable());
            assert!(!non_retryable.is_retryable());
        }

        #[tokio::test]
        async fn test_complete_stream_retries_before_first_chunk() {
            let client = Arc::new(MockRetryClient::new(vec![
                vec![Err(AiError::Llm("timeout while connecting".to_string()))],
                vec![Ok(StreamChunk::text("hello"))],
            ]));
            let config = LlmRetryConfig {
                max_retries: 1,
                initial_delay_ms: 0,
                max_delay_ms: 0,
                backoff_multiplier: 1.0,
            };
            let retrying = RetryingLlmClient::new(client.clone(), config);
            let request = CompletionRequest::new(vec![Message::user("ping")]);

            let mut stream = retrying.complete_stream(request);
            let first = stream
                .next()
                .await
                .expect("first stream item")
                .expect("chunk");
            assert_eq!(first.text, "hello");
            assert!(stream.next().await.is_none());
            assert_eq!(client.stream_call_count(), 2);
        }

        #[tokio::test]
        async fn test_complete_stream_does_not_retry_after_first_chunk() {
            let client = Arc::new(MockRetryClient::new(vec![vec![
                Ok(StreamChunk::text("partial")),
                Err(AiError::Llm("timeout while reading stream".to_string())),
            ]]));
            let config = LlmRetryConfig {
                max_retries: 3,
                initial_delay_ms: 0,
                max_delay_ms: 0,
                backoff_multiplier: 1.0,
            };
            let retrying = RetryingLlmClient::new(client.clone(), config);
            let request = CompletionRequest::new(vec![Message::user("ping")]);

            let mut stream = retrying.complete_stream(request);
            let first = stream
                .next()
                .await
                .expect("first stream item")
                .expect("chunk");
            assert_eq!(first.text, "partial");

            let second = stream.next().await.expect("second stream item");
            assert!(second.is_err());
            assert_eq!(client.stream_call_count(), 1);
        }
    }
}

mod operation_retry {
    //! Operation-level retry state for agent/session execution.

    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ClassifiedErrorKind {
        Authentication,
        RateLimited,
        Timeout,
        Validation,
        Unknown,
    }

    /// Configuration for operation-level retry.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RetryConfig {
        /// Maximum number of retry attempts (0 = no retries).
        pub max_retries: u32,
        /// Initial delay between retries in seconds.
        pub initial_delay_secs: u64,
        /// Maximum delay between retries in seconds.
        pub max_delay_secs: u64,
        /// Multiplier for exponential backoff.
        pub backoff_multiplier: f64,
        /// Whether to add deterministic jitter to delays.
        pub jitter_enabled: bool,
        /// Maximum jitter as a fraction of delay.
        pub jitter_factor: f64,
    }

    impl Default for RetryConfig {
        fn default() -> Self {
            Self {
                max_retries: 3,
                initial_delay_secs: 60,
                max_delay_secs: 3600,
                backoff_multiplier: 2.0,
                jitter_enabled: true,
                jitter_factor: 0.25,
            }
        }
    }

    impl RetryConfig {
        pub fn new(max_retries: u32, initial_delay_secs: u64) -> Self {
            Self {
                max_retries,
                initial_delay_secs,
                ..Default::default()
            }
        }

        pub fn conservative() -> Self {
            Self {
                max_retries: 2,
                initial_delay_secs: 120,
                max_delay_secs: 7200,
                backoff_multiplier: 3.0,
                jitter_enabled: true,
                jitter_factor: 0.3,
            }
        }
    }

    /// Retry state for one operation.
    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct RetryState {
        pub attempt: u32,
        pub last_error: Option<String>,
        pub next_retry_at: Option<i64>,
        pub last_failure_at: Option<i64>,
        pub total_failures: u32,
    }

    impl RetryState {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn should_retry(&self, config: &RetryConfig, error: &str) -> bool {
            self.attempt < config.max_retries && is_transient_error(error)
        }

        pub fn calculate_delay(&self, config: &RetryConfig) -> Duration {
            let base_delay = config.initial_delay_secs as f64
                * config.backoff_multiplier.powi(self.attempt as i32);
            let capped_delay = base_delay.min(config.max_delay_secs as f64);
            let final_delay = if config.jitter_enabled {
                let jitter_range = capped_delay * config.jitter_factor;
                let jitter = jitter_range * ((self.attempt as f64 * 0.37).sin().abs());
                capped_delay + jitter
            } else {
                capped_delay
            };

            Duration::from_secs(final_delay as u64)
        }

        pub fn record_failure(&mut self, error: &str, config: &RetryConfig) {
            let now = chrono::Utc::now().timestamp_millis();

            self.attempt += 1;
            self.total_failures += 1;
            self.last_error = Some(error.to_string());
            self.last_failure_at = Some(now);

            if self.attempt < config.max_retries && is_transient_error(error) {
                let delay = self.calculate_delay(config);
                self.next_retry_at = Some(now + delay.as_millis() as i64);
            } else {
                self.next_retry_at = None;
            }
        }

        pub fn time_until_retry(&self) -> Option<i64> {
            self.next_retry_at
                .map(|retry_at| (retry_at - chrono::Utc::now().timestamp_millis()).max(0))
        }

        pub fn reset(&mut self) {
            self.attempt = 0;
            self.last_error = None;
            self.next_retry_at = None;
            self.last_failure_at = None;
        }
    }

    /// Categorize an operation error for logging and metrics.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorCategory {
        Transient,
        AuthError,
        ClientError,
        NotFound,
        Unknown,
    }

    impl ErrorCategory {
        pub fn from_error(error: &str) -> Self {
            let lower = error.to_lowercase();
            if lower.contains("404") || lower.contains("not found") {
                return Self::NotFound;
            }

            match classify_error(error) {
                ClassifiedErrorKind::Authentication => Self::AuthError,
                ClassifiedErrorKind::RateLimited | ClassifiedErrorKind::Timeout => Self::Transient,
                ClassifiedErrorKind::Validation => Self::ClientError,
                ClassifiedErrorKind::Unknown => Self::Unknown,
            }
        }

        pub fn should_retry(&self) -> bool {
            matches!(self, Self::Transient)
        }
    }

    pub fn is_transient_error(error: &str) -> bool {
        matches!(
            classify_error(error),
            ClassifiedErrorKind::RateLimited | ClassifiedErrorKind::Timeout
        )
    }

    pub(crate) fn is_authentication_error(error: &str) -> bool {
        matches!(classify_error(error), ClassifiedErrorKind::Authentication)
    }

    fn classify_error(error: &str) -> ClassifiedErrorKind {
        let lower = error.to_lowercase();

        if contains_any(
            &lower,
            &[
                "unauthorized",
                "forbidden",
                "authentication",
                "auth failed",
                "invalid api key",
                "invalid token",
                "api key",
                "api_key",
                "secret",
                "credential",
                "401",
                "403",
                "billing",
            ],
        ) {
            return ClassifiedErrorKind::Authentication;
        }

        if contains_any(
            &lower,
            &[
                "rate limit",
                "rate-limit",
                "too many requests",
                "retry after",
                "retry-after",
                "quota",
                "429",
            ],
        ) {
            return ClassifiedErrorKind::RateLimited;
        }

        if contains_any(
            &lower,
            &[
                "timeout",
                "timed out",
                "connection refused",
                "connection reset",
                "connection aborted",
                "broken pipe",
                "transport error",
                "connection closed",
                "network error",
                "network unreachable",
                "error sending request",
                "request failed",
                "temporary failure",
                "temporarily unavailable",
                "service unavailable",
                "internal server error",
                "500",
                "503",
                "504",
                "502",
                "bad gateway",
                "gateway timeout",
                "overloaded",
                "capacity",
                "please try again",
            ],
        ) {
            return ClassifiedErrorKind::Timeout;
        }

        if contains_any(
            &lower,
            &[
                "bad request",
                "invalid request",
                "validation error",
                "invalid model",
                "model not found",
                "configuration error",
                "not found",
                "404",
                "400",
            ],
        ) {
            return ClassifiedErrorKind::Validation;
        }

        ClassifiedErrorKind::Unknown
    }

    fn contains_any(haystack: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| haystack.contains(needle))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn detects_transient_errors() {
            assert!(is_transient_error("Connection timeout"));
            assert!(is_transient_error("Rate limit exceeded"));
            assert!(is_transient_error("503 Service Unavailable"));
            assert!(is_transient_error("error sending request for url"));
            assert!(!is_transient_error("401 Unauthorized"));
            assert!(!is_transient_error("Invalid API key"));
            assert!(!is_transient_error("404 Not Found"));
        }

        #[test]
        fn records_retry_state() {
            let config = RetryConfig::default();
            let mut state = RetryState::new();
            state.record_failure("Connection timeout", &config);

            assert_eq!(state.attempt, 1);
            assert_eq!(state.total_failures, 1);
            assert!(state.next_retry_at.is_some());

            state.reset();
            assert_eq!(state.attempt, 0);
            assert_eq!(state.total_failures, 1);
        }

        #[test]
        fn categorizes_errors() {
            assert_eq!(
                ErrorCategory::from_error("Connection timeout"),
                ErrorCategory::Transient
            );
            assert_eq!(
                ErrorCategory::from_error("401 Unauthorized"),
                ErrorCategory::AuthError
            );
            assert_eq!(
                ErrorCategory::from_error("404 Not Found"),
                ErrorCategory::NotFound
            );
            assert!(ErrorCategory::Transient.should_retry());
            assert!(!ErrorCategory::ClientError.should_retry());
        }
    }
}

mod failover {
    //! Model failover policy for agent/session execution.

    use anyhow::Result;
    use serde::{Deserialize, Serialize};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tracing::{debug, info, warn};
    use types::{ModelId, Provider};

    use super::operation_retry::is_authentication_error;

    /// Configuration for model failover.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FailoverConfig {
        pub primary: ModelId,
        pub fallbacks: Vec<ModelId>,
        pub cooldown_secs: u64,
        pub failure_threshold: u32,
        pub auto_recover: bool,
    }

    impl Default for FailoverConfig {
        fn default() -> Self {
            Self {
                primary: ModelId::ClaudeSonnet4_5,
                fallbacks: vec![ModelId::Gpt5, ModelId::DeepseekChat],
                cooldown_secs: 300,
                failure_threshold: 3,
                auto_recover: true,
            }
        }
    }

    impl FailoverConfig {
        pub fn with_fallbacks(primary: ModelId, fallbacks: Vec<ModelId>) -> Self {
            Self {
                primary,
                fallbacks,
                ..Default::default()
            }
        }

        pub fn build_smart(
            primary: ModelId,
            _available_providers: &HashSet<Provider>,
            manual_fallbacks: Option<Vec<ModelId>>,
        ) -> Self {
            if primary.is_cli_model() {
                return Self {
                    primary,
                    fallbacks: vec![],
                    ..Default::default()
                };
            }

            let mut fallbacks = Vec::new();
            let mut seen = HashSet::new();
            seen.insert(primary);

            let mut current = primary;
            while let Some(fallback) = current.same_provider_fallback() {
                if seen.insert(fallback) {
                    fallbacks.push(fallback);
                }
                current = fallback;
            }

            if let Some(manual) = manual_fallbacks {
                for model in manual {
                    if seen.insert(model) {
                        fallbacks.push(model);
                    }
                }
            }

            Self {
                primary,
                fallbacks,
                ..Default::default()
            }
        }

        pub fn all_models(&self) -> Vec<ModelId> {
            let mut models = vec![self.primary];
            models.extend(self.fallbacks.iter().copied());
            models
        }

        pub fn contains(&self, model: ModelId) -> bool {
            self.primary == model || self.fallbacks.contains(&model)
        }
    }

    #[derive(Debug, Clone, Default)]
    struct ModelHealth {
        consecutive_failures: u32,
        total_failures: u32,
        total_successes: u32,
        cooldown_until: Option<i64>,
        last_error: Option<String>,
        last_failure_at: Option<i64>,
        last_success_at: Option<i64>,
    }

    impl ModelHealth {
        fn is_available(&self, now: i64) -> bool {
            self.cooldown_until.is_none_or(|until| now >= until)
        }

        fn remaining_cooldown_ms(&self, now: i64) -> Option<i64> {
            self.cooldown_until
                .and_then(|until| (until - now).gt(&0).then_some(until - now))
        }

        fn success_rate(&self) -> f64 {
            let total = self.total_successes + self.total_failures;
            if total == 0 {
                1.0
            } else {
                self.total_successes as f64 / total as f64
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ModelStatus {
        pub model: ModelId,
        pub available: bool,
        pub consecutive_failures: u32,
        pub success_rate: f64,
        pub cooldown_remaining_secs: Option<u64>,
        pub last_error: Option<String>,
    }

    pub struct FailoverManager {
        config: FailoverConfig,
        health: Arc<RwLock<HashMap<ModelId, ModelHealth>>>,
    }

    impl FailoverManager {
        pub fn new(config: FailoverConfig) -> Self {
            Self {
                config,
                health: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        pub async fn get_available_model(&self) -> Option<ModelId> {
            let health = self.health.read().await;
            let now = chrono::Utc::now().timestamp_millis();

            if self.is_model_available(&health, self.config.primary, now) {
                return Some(self.config.primary);
            }

            debug!(
                "Primary model {:?} unavailable, checking fallbacks",
                self.config.primary
            );

            for &model in &self.config.fallbacks {
                if self.is_model_available(&health, model, now) {
                    info!("Failing over to model {:?}", model);
                    return Some(model);
                }
            }

            warn!("All models are in cooldown or unavailable");
            None
        }

        fn is_model_available(
            &self,
            health: &HashMap<ModelId, ModelHealth>,
            model: ModelId,
            now: i64,
        ) -> bool {
            health
                .get(&model)
                .map(|model_health| model_health.is_available(now))
                .unwrap_or(true)
        }

        pub async fn record_success(&self, model: ModelId) {
            let mut health = self.health.write().await;
            let now = chrono::Utc::now().timestamp_millis();
            let entry = health.entry(model).or_default();
            entry.consecutive_failures = 0;
            entry.total_successes += 1;
            entry.last_success_at = Some(now);
            if self.config.auto_recover {
                entry.cooldown_until = None;
            }
        }

        pub async fn record_failure(&self, model: ModelId) {
            self.record_failure_with_error(model, None).await
        }

        pub async fn record_failure_with_error(&self, model: ModelId, error: Option<&str>) {
            let mut health = self.health.write().await;
            let now = chrono::Utc::now().timestamp_millis();
            let entry = health.entry(model).or_default();
            entry.consecutive_failures += 1;
            entry.total_failures += 1;
            entry.last_failure_at = Some(now);
            if let Some(error) = error {
                entry.last_error = Some(error.to_string());
            }

            if entry.consecutive_failures >= self.config.failure_threshold {
                entry.cooldown_until = Some(now + (self.config.cooldown_secs * 1000) as i64);
                warn!(
                    "Model {:?} placed in cooldown for {}s after {} consecutive failures",
                    model, self.config.cooldown_secs, entry.consecutive_failures
                );
            }
        }

        pub async fn force_cooldown(&self, model: ModelId) {
            let mut health = self.health.write().await;
            let now = chrono::Utc::now().timestamp_millis();
            let entry = health.entry(model).or_default();
            entry.cooldown_until = Some(now + (self.config.cooldown_secs * 1000) as i64);
            info!(
                "Manually placed model {:?} in cooldown for {}s",
                model, self.config.cooldown_secs
            );
        }

        pub async fn get_status(&self, model: ModelId) -> ModelStatus {
            let health = self.health.read().await;
            let now = chrono::Utc::now().timestamp_millis();
            self.model_status(&health, model, now)
        }

        fn model_status(
            &self,
            health: &HashMap<ModelId, ModelHealth>,
            model: ModelId,
            now: i64,
        ) -> ModelStatus {
            match health.get(&model) {
                Some(model_health) => ModelStatus {
                    model,
                    available: model_health.is_available(now),
                    consecutive_failures: model_health.consecutive_failures,
                    success_rate: model_health.success_rate(),
                    cooldown_remaining_secs: model_health
                        .remaining_cooldown_ms(now)
                        .map(|ms| (ms / 1000) as u64),
                    last_error: model_health.last_error.clone(),
                },
                None => ModelStatus {
                    model,
                    available: true,
                    consecutive_failures: 0,
                    success_rate: 1.0,
                    cooldown_remaining_secs: None,
                    last_error: None,
                },
            }
        }

        pub async fn reset(&self) {
            self.health.write().await.clear();
            info!("Failover manager reset - all models marked healthy");
        }

        pub fn config(&self) -> &FailoverConfig {
            &self.config
        }
    }

    pub async fn execute_with_failover<F, Fut, T>(
        manager: &FailoverManager,
        mut execute_fn: F,
    ) -> Result<(T, ModelId)>
    where
        F: FnMut(ModelId) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;

        for model in manager.config().all_models() {
            let status = manager.get_status(model).await;
            if !status.available {
                debug!("Skipping model {:?} (in cooldown)", model);
                continue;
            }

            match execute_fn(model).await {
                Ok(result) => {
                    manager.record_success(model).await;
                    return Ok((result, model));
                }
                Err(error) => {
                    let error_str = error.to_string();
                    if is_authentication_error(&error_str) {
                        warn!("Model {:?} auth error (skipping): {}", model, error_str);
                        manager.force_cooldown(model).await;
                    } else {
                        warn!("Model {:?} failed: {}", model, error_str);
                        manager
                            .record_failure_with_error(model, Some(&error_str))
                            .await;
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No models available")))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn test_config() -> FailoverConfig {
            FailoverConfig {
                primary: ModelId::ClaudeSonnet4_5,
                fallbacks: vec![ModelId::Gpt5, ModelId::DeepseekChat],
                cooldown_secs: 60,
                failure_threshold: 2,
                auto_recover: true,
            }
        }

        #[tokio::test]
        async fn falls_back_when_primary_is_in_cooldown() {
            let manager = FailoverManager::new(test_config());
            manager.record_failure(ModelId::ClaudeSonnet4_5).await;
            manager.record_failure(ModelId::ClaudeSonnet4_5).await;

            assert_eq!(manager.get_available_model().await, Some(ModelId::Gpt5));
        }

        #[tokio::test]
        async fn execute_uses_first_successful_model() {
            let manager = FailoverManager::new(test_config());
            let result = execute_with_failover(&manager, |model| async move {
                if model == ModelId::Gpt5 {
                    Ok("fallback success")
                } else {
                    Err(anyhow::anyhow!("primary failed"))
                }
            })
            .await
            .unwrap();

            assert_eq!(result, ("fallback success", ModelId::Gpt5));
        }

        #[tokio::test]
        async fn auth_errors_skip_to_next_model() {
            let config = FailoverConfig {
                primary: ModelId::ClaudeSonnet4_5,
                fallbacks: vec![ModelId::Gpt5],
                cooldown_secs: 60,
                failure_threshold: 3,
                auto_recover: true,
            };
            let manager = FailoverManager::new(config);
            let result = execute_with_failover(&manager, |model| async move {
                if model == ModelId::ClaudeSonnet4_5 {
                    Err(anyhow::anyhow!("No API key configured for provider"))
                } else {
                    Ok("success")
                }
            })
            .await
            .unwrap();

            assert_eq!(result, ("success", ModelId::Gpt5));
            assert!(!manager.get_status(ModelId::ClaudeSonnet4_5).await.available);
        }

        #[test]
        fn smart_config_uses_same_provider_and_manual_fallbacks() {
            let mut providers = HashSet::new();
            providers.insert(Provider::Anthropic);
            let config = FailoverConfig::build_smart(
                ModelId::ClaudeSonnet4_5,
                &providers,
                Some(vec![ModelId::Gpt5]),
            );

            assert!(config.fallbacks.contains(&ModelId::ClaudeHaiku4_5));
            assert!(config.fallbacks.contains(&ModelId::Gpt5));
        }

        #[test]
        fn cli_models_disable_fallbacks() {
            let providers = HashSet::new();
            let config = FailoverConfig::build_smart(ModelId::CodexCli, &providers, None);
            assert!(config.fallbacks.is_empty());
        }
    }
}

mod swappable {
    //! Swappable LLM wrapper for dynamic model switching

    use async_trait::async_trait;
    use parking_lot::RwLock;
    use std::sync::Arc;

    use crate::client::{CompletionRequest, CompletionResponse, LlmClient, StreamResult};
    use crate::error::Result;

    /// LLM wrapper that supports hot-swapping the underlying client.
    pub struct SwappableLlm {
        inner: RwLock<Arc<dyn LlmClient>>,
    }

    impl SwappableLlm {
        /// Create a new swappable LLM wrapper.
        pub fn new(inner: Arc<dyn LlmClient>) -> Self {
            Self {
                inner: RwLock::new(inner),
            }
        }

        /// Swap the underlying LLM client, returning the previous client.
        pub fn swap(&self, new_client: Arc<dyn LlmClient>) -> Arc<dyn LlmClient> {
            let mut guard = self.inner.write();
            std::mem::replace(&mut *guard, new_client)
        }

        /// Get the current provider name.
        pub fn current_provider(&self) -> String {
            let guard = self.inner.read();
            guard.provider().to_string()
        }

        /// Get the current model name.
        pub fn current_model(&self) -> String {
            let guard = self.inner.read();
            guard.model().to_string()
        }
    }

    #[async_trait]
    impl LlmClient for SwappableLlm {
        fn provider(&self) -> &str {
            "swappable"
        }

        fn model(&self) -> &str {
            "dynamic"
        }

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            let client = {
                let guard = self.inner.read();
                guard.clone()
            };
            client.complete(request).await
        }

        fn complete_stream(&self, request: CompletionRequest) -> StreamResult {
            let client = {
                let guard = self.inner.read();
                guard.clone()
            };
            client.complete_stream(request)
        }
    }
}

mod switcher {
    //! Concrete [`LlmSwitcher`] implementation wrapping `SwappableLlm` + `LlmClientFactory`.

    use std::sync::Arc;

    use types::ToolError;
    use types::llm::{ClientKind, LlmProvider, LlmSwitcher, SwapResult};

    use super::factory::LlmClientFactory;
    use super::swappable::SwappableLlm;

    /// Concrete implementation of [`LlmSwitcher`].
    pub struct LlmSwitcherImpl {
        llm: Arc<SwappableLlm>,
        factory: Arc<dyn LlmClientFactory>,
    }

    impl LlmSwitcherImpl {
        pub fn new(llm: Arc<SwappableLlm>, factory: Arc<dyn LlmClientFactory>) -> Self {
            Self { llm, factory }
        }
    }

    impl LlmSwitcher for LlmSwitcherImpl {
        fn current_model(&self) -> String {
            self.llm.current_model()
        }

        fn current_provider(&self) -> String {
            self.llm.current_provider()
        }

        fn available_models(&self) -> Vec<String> {
            self.factory.available_models()
        }

        fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
            self.factory.provider_for_model(model)
        }

        fn resolve_api_key(&self, provider: LlmProvider) -> Option<String> {
            self.factory.resolve_api_key(provider)
        }

        fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
            self.factory.client_kind_for_model(model)
        }

        fn create_and_swap(
            &self,
            model: &str,
            api_key: Option<&str>,
        ) -> std::result::Result<SwapResult, ToolError> {
            let new_runtime_provider = self.factory.provider_for_model(model).ok_or_else(|| {
                ToolError::Tool(format!("Unknown runtime provider for model '{model}'"))
            })?;
            let client = self
                .factory
                .create_client(model, api_key)
                .map_err(|e| ToolError::Tool(e.to_string()))?;

            let previous = self.llm.swap(client.clone());
            let previous_runtime_provider = self.factory.provider_for_model(previous.model());

            Ok(SwapResult {
                previous_provider: previous.provider().to_string(),
                previous_model: previous.model().to_string(),
                previous_runtime_provider,
                new_provider: client.provider().to_string(),
                new_model: client.model().to_string(),
                new_runtime_provider,
            })
        }
    }
}

pub use cli::{CodexClient, GeminiCliClient, OpenCodeClient};
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Message, Role, StreamChunk,
    StreamResult, TokenUsage, ToolCall, ToolCallDelta,
};
pub use error::{AiError, Result};
pub use factory::{DefaultLlmClientFactory, LlmClientFactory};
pub use failover::{FailoverConfig, FailoverManager, ModelStatus, execute_with_failover};
pub use http::{AnthropicClient, DeepSeekClient, OpenAIClient};
#[cfg(any(test, feature = "test-utils"))]
pub use mock_client::{MockLlmClient, MockStep, MockStepKind};
pub use operation_retry::{ErrorCategory, RetryConfig, RetryState, is_transient_error};
pub use retry::{LlmRetryConfig, RetryingLlmClient};
pub use swappable::SwappableLlm;
pub use switcher::LlmSwitcherImpl;
pub use types::{ClientKind, LlmProvider, ModelSpec};
