# tool

Tool owns the callable tool contract and registry.

Path: `crates/tool`

Owns:
- Tool trait
- Registry
- tool lookup
- tool call boundary

Must Not:
- decide per-turn permissions alone
- own agent loop state
- write durable run state

Inputs:
- JSON tool input
- registered tool implementations

Outputs:
- JSON tool output
- tool registry names

Used By:
- agent
- skill

Verify:
- cargo check -p tool

