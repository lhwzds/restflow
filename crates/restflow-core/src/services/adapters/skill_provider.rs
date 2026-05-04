//! SkillProvider implementation for the skrun-managed skill catalog.

use crate::models::Skill;
use restflow_traits::skill::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillSource, SkillUpdate,
};
use skrun::{ArtifactKind, SkillArtifact};
use std::path::PathBuf;

const SKRUN_TOOL_NAME: &str = "run_skill";

fn skill_info(skill: Skill) -> SkillInfo {
    SkillInfo {
        id: skill.id,
        name: skill.name,
        description: skill.description,
        tags: skill.tags,
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
            None => skrun::default_skills_dir().map_err(|error| error.to_string()),
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
        let root = self.root()?;
        let skill_root = root.join(id);
        if !skill_root.exists() {
            return Ok(None);
        }

        skrun::load_artifact(skill_root)
            .map(skrun_artifact_to_model)
            .map(Some)
            .map_err(|error| error.to_string())
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

    fn create_skill(&self, _: SkillRecord) -> Result<SkillRecord, String> {
        Err("RestFlow does not persist skills; install or edit skills through skrun".to_string())
    }

    fn update_skill(&self, id: &str, _: SkillUpdate) -> Result<SkillRecord, String> {
        Err(format!(
            "RestFlow does not persist skill '{}'; update it through skrun",
            id
        ))
    }

    fn delete_skill(&self, id: &str) -> Result<bool, String> {
        Err(format!(
            "RestFlow does not persist skill '{}'; remove it through skrun",
            id
        ))
    }

    fn export_skill(&self, id: &str) -> Result<String, String> {
        self.get_skill_model(id)
            .map(|skill| skill.to_markdown())
            .ok_or_else(|| format!("Skill {} not found", id))
    }

    fn import_skill(&self, id: &str, _: &str, _: bool) -> Result<SkillRecord, String> {
        Err(format!(
            "RestFlow does not import skill '{}'; install it through skrun",
            id
        ))
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
}
