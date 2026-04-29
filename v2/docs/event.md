# event

Event owns shared stream and trace event types.

Path: `crates/event`

Owns:
- text events
- tool call events
- tool result events
- error and completion events

Must Not:
- persist events directly
- render UI
- call tools

Inputs:
- runtime state changes
- tool execution updates

Outputs:
- Event

Used By:
- agent
- chat
- run

Verify:
- cargo check -p event

