//! Response parsing for tool calls and final answers.

use anyhow::Result;

use super::AgentAction;
use restflow_ai::llm::ToolCall;

/// Parse LLM responses into agent actions.
pub struct ResponseParser;

impl ResponseParser {
    /// Parse an LLM response to determine the next action.
    pub fn parse(response: &str, tool_calls: Option<&[ToolCall]>) -> Result<AgentAction> {
        if let Some(calls) = tool_calls {
            if !calls.is_empty() {
                return Ok(AgentAction::ToolCalls {
                    calls: calls.to_vec(),
                });
            }
        }

        let trimmed = response.trim();
        if trimmed.is_empty() {
            return Ok(AgentAction::Continue);
        }

        if let Some(content) = Self::extract_final_answer(trimmed) {
            return Ok(AgentAction::FinalAnswer { content });
        }

        Ok(AgentAction::FinalAnswer {
            content: trimmed.to_string(),
        })
    }

    fn extract_final_answer(response: &str) -> Option<String> {
        let marker = "FINAL ANSWER:";
        let start = response.find(marker)?;
        let content = response[start + marker.len()..].trim();
        if content.is_empty() {
            None
        } else {
            Some(content.to_string())
        }
    }
}
