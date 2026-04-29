# skill

Skill owns capability metadata and turn-level activation planning.

Path: `crates/skill`

Owns:
- skill catalog
- skill source metadata
- @skill mention parsing
- TurnPlan generation
- suggested tool activation

Must Not:
- render UI overlays
- write session history
- execute tools directly
- own durable Task or Run state

Inputs:
- user message text
- assigned skill IDs
- skill catalog

Outputs:
- TurnPlan
- activated skill IDs
- allowed tool names
- activation issues

Depends On:
- tool
- model

Used By:
- chat
- run

Verify:
- cargo check -p skill

