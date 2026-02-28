use crate::models::{
    BackgroundAgentSchedule, BackgroundAgentSpec, ChatMessage, ChatRole, ChatSession,
    ContinuationConfig, DurabilityMode, ExecutionMode, MemoryConfig, NotificationConfig,
    ResourceLimits,
};

pub const MISSING_CONVERSION_INPUT_ERROR: &str =
    "Cannot convert session: no non-empty user message found; please provide input.";

#[derive(Debug, Clone, Default)]
pub struct ConvertSessionSpecOptions {
    pub name: Option<String>,
    pub description: Option<String>,
    pub schedule: Option<BackgroundAgentSchedule>,
    pub input: Option<String>,
    pub notification: Option<NotificationConfig>,
    pub execution_mode: Option<ExecutionMode>,
    pub timeout_secs: Option<u64>,
    pub memory: Option<MemoryConfig>,
    pub durability_mode: Option<DurabilityMode>,
    pub resource_limits: Option<ResourceLimits>,
    pub prerequisites: Vec<String>,
    pub continuation: Option<ContinuationConfig>,
}

pub fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn default_conversion_schedule() -> BackgroundAgentSchedule {
    let now = chrono::Utc::now().timestamp_millis();
    BackgroundAgentSchedule::Once {
        run_at: now.saturating_add(1_000),
    }
}

pub fn derive_conversion_name(
    requested_name: Option<String>,
    session_name: &str,
    session_id: &str,
) -> String {
    if let Some(name) = normalize_optional_text(requested_name) {
        return name;
    }

    let base = session_name.trim();
    if base.is_empty() {
        format!("Background from {}", session_id)
    } else {
        format!("Background: {}", base)
    }
}

pub fn derive_conversion_input(input: Option<String>, messages: &[ChatMessage]) -> Option<String> {
    if let Some(input) = normalize_optional_text(input) {
        return Some(input);
    }

    messages
        .iter()
        .rev()
        .find(|message| message.role == ChatRole::User)
        .and_then(|message| normalize_optional_text(Some(message.content.clone())))
}

pub fn build_convert_session_spec(
    session: &ChatSession,
    options: ConvertSessionSpecOptions,
) -> Result<BackgroundAgentSpec, &'static str> {
    let name = derive_conversion_name(options.name, &session.name, &session.id);
    let input = derive_conversion_input(options.input, &session.messages)
        .ok_or(MISSING_CONVERSION_INPUT_ERROR)?;
    let description = options
        .description
        .or_else(|| Some(format!("Converted from chat session {}", session.id)));

    Ok(BackgroundAgentSpec {
        name,
        agent_id: session.agent_id.clone(),
        chat_session_id: Some(session.id.clone()),
        description,
        input: Some(input),
        input_template: None,
        schedule: options.schedule.unwrap_or_else(default_conversion_schedule),
        notification: options.notification,
        execution_mode: options.execution_mode,
        timeout_secs: options.timeout_secs,
        memory: options.memory,
        durability_mode: options.durability_mode,
        resource_limits: options.resource_limits,
        prerequisites: options.prerequisites,
        continuation: options.continuation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_name_prefers_explicit_name() {
        let name = derive_conversion_name(
            Some("  Explicit Name  ".to_string()),
            "Session Name",
            "session-1",
        );
        assert_eq!(name, "Explicit Name");
    }

    #[test]
    fn derive_name_falls_back_to_session_name() {
        let name = derive_conversion_name(None, "Session Name", "session-1");
        assert_eq!(name, "Background: Session Name");
    }

    #[test]
    fn derive_name_falls_back_to_session_id_when_name_is_empty() {
        let name = derive_conversion_name(None, "   ", "session-1");
        assert_eq!(name, "Background from session-1");
    }

    #[test]
    fn derive_input_prefers_explicit_input() {
        let messages = vec![ChatMessage::user("ignored history")];
        let input = derive_conversion_input(Some("  explicit  ".to_string()), &messages);
        assert_eq!(input.as_deref(), Some("explicit"));
    }

    #[test]
    fn derive_input_uses_latest_non_empty_user_message() {
        let messages = vec![
            ChatMessage::assistant("hello"),
            ChatMessage::user(""),
            ChatMessage::user(" latest request "),
        ];
        let input = derive_conversion_input(None, &messages);
        assert_eq!(input.as_deref(), Some("latest request"));
    }

    #[test]
    fn derive_input_returns_none_when_user_message_missing() {
        let messages = vec![ChatMessage::assistant("hello")];
        assert!(derive_conversion_input(None, &messages).is_none());
    }

    #[test]
    fn build_spec_uses_defaults_and_binds_session() {
        let mut session =
            ChatSession::new("agent-1".to_string(), "gpt-5".to_string()).with_name("Main Session");
        session.add_message(ChatMessage::assistant("hello"));
        session.add_message(ChatMessage::user("Continue this task"));

        let spec =
            build_convert_session_spec(&session, ConvertSessionSpecOptions::default()).unwrap();
        assert_eq!(spec.agent_id, "agent-1");
        assert_eq!(spec.chat_session_id.as_deref(), Some(session.id.as_str()));
        assert_eq!(spec.input.as_deref(), Some("Continue this task"));
        assert_eq!(spec.name, "Background: Main Session");
    }

    #[test]
    fn build_spec_errors_when_input_is_missing() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::assistant("hello"));

        let err = build_convert_session_spec(&session, ConvertSessionSpecOptions::default())
            .expect_err("expected missing input error");
        assert_eq!(err, MISSING_CONVERSION_INPUT_ERROR);
    }
}
