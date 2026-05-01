//! # codocia
//!
//! Engine tooling wraps tool registry calls into streamable events.
//!
//! ## Owns
//! - tool call event creation
//! - tool result event creation
//! - tool error event conversion
//!
//! ## Must Not
//! - define concrete tools
//! - own tool schemas
//! - apply UI formatting
//!
//! ## Inputs
//! - ToolCall
//!
//! ## Outputs
//! - Event list
//!
//! ## Depends On
//! - event
//! - tool
//!
//! ## Verify
//! - cargo test -p engine core_tool_calls_emit_call_and_result_events

use crate::Core;

impl Core {
    pub async fn call_tool_events(&self, call: tool::ToolCall) -> Vec<event::Event> {
        let mut events = vec![event::Event::tool_call(
            call.id.clone(),
            call.name.clone(),
            call.input.clone(),
        )];

        match self.tools.call(call).await {
            Ok(output) => events.push(event::Event::tool_result(output.call_id, output.value)),
            Err(error) => events.push(event::Event::error(error.to_string())),
        }

        events
    }
}
