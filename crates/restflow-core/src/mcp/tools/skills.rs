//! Skill-related MCP tools
//!
//! The actual tool implementations are in server.rs using the #[tool] macro.
//! This module exists for organizational purposes and potential future expansion.

// Tool implementations are in server.rs using rmcp's #[tool] attribute macro.
// Available tools:
// - list_skills: List all skills (CLI: `skill list`)
// - get_skill: Get a skill by ID (CLI: `skill show`)
// - get_skill_context: Get skill content with execution context (No CLI needed - AI only)
// - get_skill_reference: Load deep reference content (No CLI needed - use `skill show`)
// - create_skill: Create a new skill (CLI: `skill create`)
// - update_skill: Update an existing skill (CLI: `skill update`)
// - delete_skill: Delete a skill (CLI: `skill delete`)
