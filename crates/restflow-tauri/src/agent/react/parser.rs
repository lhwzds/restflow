//! Response parsing for tool calls and final answers.

use super::AgentAction;
use anyhow::Result;
use restflow_ai::llm::ToolCall;

pub struct ResponseParser;

impl ResponseParser {
    /// Parse LLM response to determine next action
    pub fn parse(response: &str, tool_calls: Option<&[ToolCall]>) -> Result<AgentAction> {
        if let Some(calls) = tool_calls
            && let Some(call) = calls.first()
        {
            return Ok(AgentAction::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
        }

        if let Some(content) = extract_tagged_final(response) {
            return Ok(AgentAction::FinalAnswer { content });
        }

        if let Some(content) = extract_prefixed_final(response) {
            return Ok(AgentAction::FinalAnswer { content });
        }

        Ok(AgentAction::FinalAnswer {
            content: response.trim().to_string(),
        })
    }
}

fn extract_tagged_final(response: &str) -> Option<String> {
    let start_tag = "<final>";
    let end_tag = "</final>";
    let start = response.find(start_tag)? + start_tag.len();
    let end = response.find(end_tag)?;
    if end <= start {
        return None;
    }
    Some(response[start..end].trim().to_string())
}

fn extract_prefixed_final(response: &str) -> Option<String> {
    let prefix = "FINAL ANSWER:";
    let index = response.find(prefix)? + prefix.len();
    Some(response[index..].trim().to_string())
}
