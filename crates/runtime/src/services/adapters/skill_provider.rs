//! SkillProvider implementation for the skrun-managed skill catalog.

use skrun::{ArtifactKind, SkillArtifact};
use std::path::PathBuf;
use types::Skill;
use types::skill::{SkillContent, SkillInfo, SkillProvider, SkillSource};

const SKRUN_TOOL_NAME: &str = "run_skill";

fn validate_skill_id(skill_id: &str) -> Result<(), String> {
    if skill_id.is_empty() {
        return Err("skill id cannot be empty".to_string());
    }
    if !skill_id
        .chars()
        .all(|item| item.is_ascii_alphanumeric() || item == '-' || item == '_')
    {
        return Err("skill id must contain only ASCII letters, numbers, '-' or '_'".to_string());
    }
    if !skill_id
        .chars()
        .next()
        .is_some_and(|item| item.is_ascii_alphanumeric())
    {
        return Err("skill id must start with an ASCII letter or number".to_string());
    }
    Ok(())
}

fn skill_info(skill: Skill) -> SkillInfo {
    SkillInfo {
        id: skill.id,
        name: skill.name,
        description: skill.description,
        tags: skill.tags,
        kind: skill.kind,
        executable: skill.executable,
        suggested_tools: skill.suggested_tools,
        source: skill.source,
        read_only: skill.read_only,
        source_ref: skill.source_ref,
    }
}

fn skill_content(skill: Skill) -> SkillContent {
    SkillContent {
        id: skill.id,
        name: skill.name,
        content: skill.content,
        kind: skill.kind,
        executable: skill.executable,
        suggested_tools: skill.suggested_tools,
        source: skill.source,
        read_only: skill.read_only,
        source_ref: skill.source_ref,
    }
}

fn artifact_kind_label(kind: &ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Markdown => "markdown",
        ArtifactKind::RustBinary => "rust_binary",
        ArtifactKind::PythonUv => "python_uv",
    }
}

fn skrun_artifact_to_model(record: SkillArtifact) -> Skill {
    let kind = artifact_kind_label(&record.kind);
    let executable = record.executable || record.kind != ArtifactKind::Markdown;
    let description = record.description.clone().or_else(|| {
        Some(if executable {
            format!("Executable skrun {kind} skill.")
        } else {
            format!("Guidance-only skrun {kind} skill.")
        })
    });
    let content = record.content.unwrap_or_else(|| {
        format!(
            "# {}\n\n{} skrun skill.\n\n- id: `{}`\n- kind: `{}`\n- version: `{}`\n",
            record.name,
            if executable {
                "Executable"
            } else {
                "Guidance-only"
            },
            record.id,
            kind,
            record.version
        )
    });
    let mut skill = Skill::new(
        record.id.clone(),
        record.name,
        description,
        record.tags,
        content,
    );
    skill.kind = Some(kind.to_string());
    skill.executable = executable;
    skill.source = SkillSource::External;
    skill.read_only = true;
    skill.version = Some(record.version.clone());
    skill.source_ref = record
        .source_ref
        .or_else(|| Some(format!("skrun:{}@{}", record.id, record.version)));
    skill.suggested_tools = record.suggested_tools;
    if executable
        && !skill
            .suggested_tools
            .iter()
            .any(|tool| tool == SKRUN_TOOL_NAME)
    {
        skill.suggested_tools.push(SKRUN_TOOL_NAME.to_string());
    }
    skill
}

/// Read-only provider for skills exposed by the skrun public CLI contract.
pub struct SkrunSkillProvider {
    root: Option<PathBuf>,
}

impl SkrunSkillProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
        }
    }

    pub fn from_default_root() -> Self {
        Self { root: None }
    }

    fn root(&self) -> Result<PathBuf, String> {
        match &self.root {
            Some(root) => Ok(root.clone()),
            None => {
                crate::services::skills::skill_catalog_root().map_err(|error| error.to_string())
            }
        }
    }

    pub fn try_list_skill_models(&self) -> Result<Vec<Skill>, String> {
        let root = self.root()?;
        let mut skills = skrun::list_installed_skills(root)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(skrun_artifact_to_model)
            .collect::<Vec<_>>();
        skills.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(skills)
    }

    pub fn list_skill_models(&self) -> Vec<Skill> {
        match self.try_list_skill_models() {
            Ok(skills) => skills,
            Err(error) => {
                tracing::debug!(error = %error, "skrun skill catalog is not available");
                Vec::new()
            }
        }
    }

    pub fn try_get_skill_model(&self, id: &str) -> Result<Option<Skill>, String> {
        validate_skill_id(id)?;
        let root = self.root()?;
        let skill_root = root.join(id);
        if !skill_root.exists() {
            return Ok(None);
        }

        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let skill_root = skill_root
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !skill_root.starts_with(&root) {
            return Err(format!(
                "skill id '{}' resolves outside the skill catalog",
                id
            ));
        }

        let artifact = skrun::load_artifact(skill_root).map_err(|error| error.to_string())?;
        if artifact.id != id {
            return Err(format!(
                "skill artifact id mismatch: requested '{}', found '{}'",
                id, artifact.id
            ));
        }

        Ok(Some(skrun_artifact_to_model(artifact)))
    }

    pub fn get_skill_model(&self, id: &str) -> Option<Skill> {
        match self.try_get_skill_model(id) {
            Ok(skill) => skill,
            Err(error) => {
                tracing::debug!(error = %error, skill_id = %id, "skrun skill catalog is not available");
                None
            }
        }
    }
}

impl Default for SkrunSkillProvider {
    fn default() -> Self {
        Self::from_default_root()
    }
}

impl SkillProvider for SkrunSkillProvider {
    fn list_skills(&self) -> Vec<SkillInfo> {
        self.list_skill_models()
            .into_iter()
            .map(skill_info)
            .collect()
    }

    fn get_skill(&self, id: &str) -> Option<SkillContent> {
        self.get_skill_model(id).map(skill_content)
    }

    fn export_skill(&self, id: &str) -> Result<String, String> {
        self.get_skill_model(id)
            .map(|skill| skill.to_markdown())
            .ok_or_else(|| format!("Skill {} not found", id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn save_test_artifact(root: &std::path::Path, artifact: &SkillArtifact) {
        let skill_root = root.join(&artifact.id);
        skrun::save_artifact(skill_root, artifact).unwrap();
    }

    #[test]
    fn test_skrun_provider_lists_installed_artifacts() {
        let dir = tempdir().unwrap();
        let mut artifact = SkillArtifact::rust_binary("regex-finder", "Regex Finder", "0.1.0");
        artifact.description = Some("Find text with regex.".to_string());
        artifact.content = Some("# Regex Finder\n\nFind text with regex.".to_string());
        save_test_artifact(dir.path(), &artifact);

        let provider = SkrunSkillProvider::new(dir.path());
        let skills = provider.list_skill_models();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "regex-finder");
        assert_eq!(skills[0].source, SkillSource::External);
        assert!(skills[0].read_only);
        assert_eq!(skills[0].suggested_tools, vec![SKRUN_TOOL_NAME]);
    }

    #[test]
    fn test_skrun_provider_lists_markdown_skills_without_run_tool() {
        let dir = tempdir().unwrap();
        let mut artifact =
            SkillArtifact::markdown("team", "Team", "0.1.0", "# Team\n\nUse workers.");
        artifact.description = Some("Coordinate workers.".to_string());
        artifact.tags = Some(vec!["team".to_string()]);
        artifact.suggested_tools = vec!["spawn_subagent_batch".to_string()];
        save_test_artifact(dir.path(), &artifact);

        let provider = SkrunSkillProvider::new(dir.path());
        let skills = provider.list_skill_models();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "team");
        assert_eq!(skills[0].source, SkillSource::External);
        assert_eq!(skills[0].tags, Some(vec!["team".to_string()]));
        assert_eq!(skills[0].suggested_tools, vec!["spawn_subagent_batch"]);
    }

    #[test]
    fn test_default_test_catalog_is_empty_without_skrun_override() {
        let dir = tempdir().unwrap();
        let provider = SkrunSkillProvider::new(dir.path().join("missing"));
        assert!(provider.try_list_skill_models().unwrap().is_empty());
        assert!(provider.try_get_skill_model("team").unwrap().is_none());
    }

    #[test]
    fn test_get_rejects_path_like_skill_id() {
        let dir = tempdir().unwrap();
        let provider = SkrunSkillProvider::new(dir.path());

        let error = provider
            .try_get_skill_model("../outside")
            .expect_err("path-like skill id should be rejected");

        assert!(error.contains("must contain only ASCII letters"));
    }

    #[test]
    fn test_get_rejects_artifact_id_mismatch() {
        let dir = tempdir().unwrap();
        let artifact =
            SkillArtifact::markdown("actual", "Actual", "0.1.0", "# Actual\n\nUse this.");
        skrun::save_artifact(dir.path().join("alias"), &artifact).unwrap();
        let provider = SkrunSkillProvider::new(dir.path());

        let error = provider
            .try_get_skill_model("alias")
            .expect_err("artifact id mismatch should be rejected");

        assert!(error.contains("artifact id mismatch"));
    }
}
