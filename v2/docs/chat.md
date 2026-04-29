# chat

Chat owns sessions, turns, and message history.

Path: `crates/chat`

Owns:
- Session
- Message
- Role
- chat history composition

Must Not:
- own durable background runs
- render TUI layout
- decide model catalog policy

Inputs:
- user messages
- assistant events
- tool events

Outputs:
- session history
- message lists

Depends On:
- agent
- event
- skill
- store

Verify:
- cargo check -p chat

