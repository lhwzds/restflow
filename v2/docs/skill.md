# skill

Skill owns skill metadata and AI-facing context resolution.

Path: `crates/skill`

Owns:
- skill catalog
- skill source metadata
- @skill mention parsing
- SkillContext resolution
- suggested tool metadata

Must Not:
- render UI overlays
- write session history
- decide tool permissions
- execute tools directly
- own durable Task or Run state

Inputs:
- user message text
- assigned skill IDs
- skill catalog

Outputs:
- SkillContext
- assigned skill summaries
- mentioned skill content
- context issues

Used By:
- chat
- run

Verify:
- cargo check -p skill

