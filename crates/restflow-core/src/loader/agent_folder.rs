use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct AgentFolderLoader {
    base_dir: PathBuf,
}

impl AgentFolderLoader {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn scan(&self) -> Result<Vec<AgentFolder>> {
        let mut agents = Vec::new();
        if !self.base_dir.exists() {
            return Ok(agents);
        }

        for entry in WalkDir::new(&self.base_dir)
            .min_depth(1)
            .max_depth(2)
            .follow_links(false)
        {
            let entry = entry?;
            if !entry.file_type().is_dir() {
                continue;
            }

            let folder_path = entry.path();
            let agent_path = folder_path.join("AGENT.md");
            if !agent_path.exists() {
                continue;
            }

            let agent = self
                .load_agent_folder(folder_path)
                .with_context(|| format!("Failed to load agent folder at {:?}", folder_path))?;
            agents.push(agent);
        }

        Ok(agents)
    }

    pub fn load_agent_folder(&self, folder_path: &Path) -> Result<AgentFolder> {
        let agent_id = folder_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid agent folder name"))?
            .to_string();

        let agent_path = folder_path.join("AGENT.md");
        let content = std::fs::read_to_string(&agent_path)
            .with_context(|| format!("Failed to read agent file at {:?}", agent_path))?;

        let (meta, prompt) = Self::parse_agent_markdown(&content)?;

        Ok(AgentFolder {
            id: agent_id,
            folder_path: folder_path.to_string_lossy().to_string(),
            prompt,
            meta,
            content_hash: Self::hash_content(&content),
        })
    }

    fn parse_agent_markdown(markdown: &str) -> Result<(AgentFrontmatter, String)> {
        if !markdown.starts_with("---") {
            return Err(anyhow::anyhow!(
                "Invalid markdown format: missing frontmatter"
            ));
        }

        let rest = &markdown[3..];
        let end_index = rest
            .find("---")
            .ok_or_else(|| anyhow::anyhow!("Invalid markdown format: frontmatter not closed"))?;

        let frontmatter_str = &rest[..end_index].trim();
        let prompt = rest[end_index + 3..].trim().to_string();

        let frontmatter: AgentFrontmatter = serde_yaml::from_str(frontmatter_str)?;
        Ok((frontmatter, prompt))
    }

    fn hash_content(content: &str) -> String {
        hex::encode(Sha256::digest(content.as_bytes()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFrontmatter {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct AgentFolder {
    pub id: String,
    pub folder_path: String,
    pub prompt: String,
    pub meta: AgentFrontmatter,
    pub content_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_agent_markdown() {
        let markdown = r#"---
name: Main Assistant
description: Primary assistant
author: user
version: 1.0.0
tags:
  - assistant
  - general
---

# System Prompt

You are a helpful AI assistant."#;

        let (meta, prompt) = AgentFolderLoader::parse_agent_markdown(markdown).unwrap();
        assert_eq!(meta.name, "Main Assistant");
        assert_eq!(meta.description, Some("Primary assistant".to_string()));
        assert_eq!(meta.author, Some("user".to_string()));
        assert_eq!(meta.version, Some("1.0.0".to_string()));
        assert_eq!(meta.tags, Some(vec!["assistant".to_string(), "general".to_string()]));
        assert!(prompt.contains("You are a helpful AI assistant"));
    }

    #[test]
    fn test_load_agent_folder() {
        let temp_dir = tempdir().unwrap();
        let agent_dir = temp_dir.path().join("main-assistant");
        std::fs::create_dir_all(&agent_dir).unwrap();

        let content = r#"---
name: Main Assistant
---

# System Prompt

Hello world"#;

        std::fs::write(agent_dir.join("AGENT.md"), content).unwrap();

        let loader = AgentFolderLoader::new(temp_dir.path());
        let folder = loader.load_agent_folder(&agent_dir).unwrap();

        assert_eq!(folder.id, "main-assistant");
        assert_eq!(folder.meta.name, "Main Assistant");
        assert!(folder.prompt.contains("Hello world"));
    }
}
