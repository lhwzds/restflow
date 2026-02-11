//! Skill tool for listing and reading skills

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::AiError;
use crate::error::Result;
use crate::tools::traits::{SkillProvider, SkillRecord, SkillUpdate, Tool, ToolOutput};

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SkillInput {
    List,
    Read {
        id: String,
    },
    Create {
        id: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        content: String,
    },
    Update {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        description: Option<Option<String>>,
        #[serde(default)]
        tags: Option<Option<Vec<String>>>,
        #[serde(default)]
        content: Option<String>,
    },
    Delete {
        id: String,
    },
    Export {
        id: String,
    },
    Import {
        id: String,
        markdown: String,
        #[serde(default)]
        overwrite: Option<bool>,
    },
}

/// Skill tool for managing skills
pub struct SkillTool {
    provider: Arc<dyn SkillProvider>,
    allow_write: bool,
}

impl SkillTool {
    /// Create a new skill tool with the given provider
    pub fn new(provider: Arc<dyn SkillProvider>) -> Self {
        Self {
            provider,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(AiError::Tool(
                "Write access to skills is disabled. Available read-only operations: list, get, search. To modify skills, the user must grant write permissions.".to_string(),
            ))
        }
    }

    fn to_record(
        id: String,
        name: String,
        description: Option<String>,
        tags: Option<Vec<String>>,
        content: String,
    ) -> SkillRecord {
        SkillRecord {
            id,
            name,
            description,
            tags,
            content,
        }
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Create, read, update, list, import, export, and delete reusable skill definitions."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read", "create", "update", "delete", "export", "import"],
                    "description": "Action to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Skill ID (required for read/update/delete/export/import/create)"
                },
                "name": {
                    "type": "string",
                    "description": "Skill name (required for create)"
                },
                "description": {
                    "type": ["string", "null"],
                    "description": "Skill description (optional, set to null to clear on update)"
                },
                "tags": {
                    "type": ["array", "null"],
                    "items": { "type": "string" },
                    "description": "Skill tags (optional, set to null to clear on update)"
                },
                "content": {
                    "type": "string",
                    "description": "Skill markdown content"
                },
                "markdown": {
                    "type": "string",
                    "description": "Skill markdown with YAML frontmatter (for import)"
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Whether to overwrite existing skill on import",
                    "default": false
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SkillInput = serde_json::from_value(input)?;

        match params {
            SkillInput::List => {
                let skills = self.provider.list_skills();
                Ok(ToolOutput::success(json!({
                    "skills": skills
                })))
            }
            SkillInput::Read { id } => match self.provider.get_skill(&id) {
                Some(skill) => Ok(ToolOutput::success(json!(skill))),
                None => Ok(ToolOutput::error(format!("Skill '{}' not found", id))),
            },
            SkillInput::Create {
                id,
                name,
                description,
                tags,
                content,
            } => {
                self.write_guard()?;
                let record = Self::to_record(id, name, description, tags, content);
                match self.provider.create_skill(record) {
                    Ok(created) => Ok(ToolOutput::success(json!(created))),
                    Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
                }
            }
            SkillInput::Update {
                id,
                name,
                description,
                tags,
                content,
            } => {
                self.write_guard()?;
                let update = SkillUpdate {
                    name,
                    description,
                    tags,
                    content,
                };
                match self.provider.update_skill(&id, update) {
                    Ok(updated) => Ok(ToolOutput::success(json!(updated))),
                    Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
                }
            }
            SkillInput::Delete { id } => {
                self.write_guard()?;
                match self.provider.delete_skill(&id) {
                    Ok(deleted) => Ok(ToolOutput::success(json!({
                        "id": id,
                        "deleted": deleted
                    }))),
                    Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
                }
            }
            SkillInput::Export { id } => match self.provider.export_skill(&id) {
                Ok(markdown) => Ok(ToolOutput::success(json!({
                    "id": id,
                    "markdown": markdown
                }))),
                Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
            },
            SkillInput::Import {
                id,
                markdown,
                overwrite,
            } => {
                self.write_guard()?;
                let overwrite = overwrite.unwrap_or(false);
                match self.provider.import_skill(&id, &markdown, overwrite) {
                    Ok(imported) => Ok(ToolOutput::success(json!(imported))),
                    Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::traits::{SkillContent, SkillInfo};

    struct MockSkillProvider {
        skills: Vec<SkillRecord>,
    }

    impl SkillProvider for MockSkillProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            self.skills
                .iter()
                .map(|skill| SkillInfo {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    tags: skill.tags.clone(),
                })
                .collect()
        }

        fn get_skill(&self, id: &str) -> Option<SkillContent> {
            self.skills
                .iter()
                .find(|skill| skill.id == id)
                .map(|skill| SkillContent {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    content: skill.content.clone(),
                })
        }

        fn create_skill(&self, skill: SkillRecord) -> std::result::Result<SkillRecord, String> {
            if self.skills.iter().any(|s| s.id == skill.id) {
                return Err(format!("Skill {} already exists", skill.id));
            }
            Ok(skill)
        }

        fn update_skill(
            &self,
            id: &str,
            update: SkillUpdate,
        ) -> std::result::Result<SkillRecord, String> {
            let Some(existing) = self.skills.iter().find(|s| s.id == id) else {
                return Err(format!("Skill {} not found", id));
            };
            let mut updated = existing.clone();
            if let Some(name) = update.name {
                updated.name = name;
            }
            if let Some(description) = update.description {
                updated.description = description;
            }
            if let Some(tags) = update.tags {
                updated.tags = tags;
            }
            if let Some(content) = update.content {
                updated.content = content;
            }
            Ok(updated)
        }

        fn delete_skill(&self, id: &str) -> std::result::Result<bool, String> {
            Ok(self.skills.iter().any(|s| s.id == id))
        }

        fn export_skill(&self, id: &str) -> std::result::Result<String, String> {
            let skill = self
                .skills
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| format!("Skill {} not found", id))?;
            Ok(format!(
                "---\nname: {}\n---\n\n{}",
                skill.name, skill.content
            ))
        }

        fn import_skill(
            &self,
            id: &str,
            markdown: &str,
            _overwrite: bool,
        ) -> std::result::Result<SkillRecord, String> {
            Ok(SkillRecord {
                id: id.to_string(),
                name: "Imported Skill".to_string(),
                description: None,
                tags: None,
                content: markdown.to_string(),
            })
        }
    }

    fn create_mock_provider() -> Arc<dyn SkillProvider> {
        Arc::new(MockSkillProvider {
            skills: vec![SkillRecord {
                id: "test-skill".to_string(),
                name: "Test Skill".to_string(),
                description: Some("A test skill".to_string()),
                tags: Some(vec!["test".to_string()]),
                content: "# Test Skill Content\n\nThis is a test.".to_string(),
            }],
        })
    }

    #[test]
    fn test_skill_tool_schema() {
        let tool = SkillTool::new(create_mock_provider());
        assert_eq!(tool.name(), "skill");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_list_skills() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool.execute(json!({ "action": "list" })).await.unwrap();

        assert!(result.success);
        let skills = result.result.get("skills").unwrap().as_array().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0]["id"], "test-skill");
    }

    #[tokio::test]
    async fn test_read_skill() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool
            .execute(json!({ "action": "read", "id": "test-skill" }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.result["id"], "test-skill");
        assert!(
            result.result["content"]
                .as_str()
                .unwrap()
                .contains("Test Skill Content")
        );
    }

    #[tokio::test]
    async fn test_read_skill_not_found() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool
            .execute(json!({ "action": "read", "id": "nonexistent" }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_write_guard_blocks_create() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool
            .execute(json!({
                "action": "create",
                "id": "new",
                "name": "New",
                "content": "# New"
            }))
            .await;

        let err = result.err().expect("expected write-guard error");
        assert!(
            err.to_string()
                .contains("Available read-only operations: list, get, search")
        );
    }
}
