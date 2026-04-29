# agent

Agent owns the execution kernel and model/tool orchestration.

Path: `crates/agent`

Owns:
- Agent
- execution input and output
- tool registry consumption
- event production

Must Not:
- own daemon lifecycle
- write durable storage directly
- render UI
- parse UI picker state

Inputs:
- Model
- allowed tools
- user message
- prompt context

Outputs:
- Event stream
- final run output

Depends On:
- event
- model
- tool

Used By:
- chat
- run

Verify:
- cargo check -p agent

