//! SkillProvider implementation for the skrun-managed skill catalog.

use crate::models::Skill;
use restflow_traits::skill::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillSource, SkillUpdate,
};
use std::path::PathBuf;
use std::process::Command;

const RESTFLOW_SKRUN_BIN_ENV: &str = "RESTFLOW_SKRUN_BIN";
const SKRUN_TOOL_NAME: &str = "run_skill";

#[derive(Debug, Clone, serde::Deserialize)]
struct SkrunCliSkill {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    suggested_tools: Vec<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    executable: bool,
    #[serde(default, rename = "source_ref", alias = "ref")]
    source_ref: Option<String>,
}

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

fn default_skrun_bin() -> PathBuf {
    std::env::var_os(RESTFLOW_SKRUN_BIN_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("skrun"))
}

fn parse_skrun_json_list(stdout: &str) -> Result<Vec<SkrunCliSkill>, serde_json::Error> {
    serde_json::from_str::<Vec<SkrunCliSkill>>(stdout)
}

fn parse_skrun_tsv_list(stdout: &str) -> Vec<SkrunCliSkill> {
    stdout
        .lines()
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() < 4 {
                return None;
            }
            Some(SkrunCliSkill {
                id: fields[0].to_string(),
                kind: fields[1].to_string(),
                version: fields[2].to_string(),
                name: fields[3].to_string(),
                description: None,
                tags: None,
                suggested_tools: Vec::new(),
                content: None,
                executable: fields[1] != "markdown",
                source_ref: None,
            })
        })
        .collect()
}

fn skrun_cli_skill_to_model(record: SkrunCliSkill) -> Skill {
    let executable = record.executable || record.kind != "markdown";
    let description = record.description.clone().or_else(|| {
        Some(if executable {
            format!("Executable skrun {} skill.", record.kind)
        } else {
            format!("Guidance-only skrun {} skill.", record.kind)
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
            record.kind,
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
    bin: PathBuf,
}

impl SkrunSkillProvider {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    pub fn from_default_bin() -> Self {
        Self::new(default_skrun_bin())
    }

    #[cfg(test)]
    fn uses_test_empty_default_catalog(&self) -> bool {
        self.bin.as_path() == std::path::Path::new("skrun")
            && std::env::var_os(RESTFLOW_SKRUN_BIN_ENV).is_none()
    }

    #[cfg(not(test))]
    fn uses_test_empty_default_catalog(&self) -> bool {
        false
    }

    pub fn try_list_skill_models(&self) -> Result<Vec<Skill>, String> {
        if self.uses_test_empty_default_catalog() {
            return Ok(Vec::new());
        }

        let output = match Command::new(&self.bin)
            .arg("skill")
            .arg("list")
            .arg("--format")
            .arg("json")
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => match Command::new(&self.bin).arg("skill").arg("list").output() {
                Ok(output) if output.status.success() => output,
                Ok(output) => {
                    return Err(format!(
                        "skrun skill list failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                Err(error) => {
                    return Err(format!("skrun executable is not available: {error}"));
                }
            },
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let records =
            parse_skrun_json_list(&stdout).unwrap_or_else(|_| parse_skrun_tsv_list(&stdout));
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let mut skills = records
            .into_iter()
            .map(skrun_cli_skill_to_model)
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
        if self.uses_test_empty_default_catalog() {
            return Ok(None);
        }

        let show_output = Command::new(&self.bin)
            .arg("skill")
            .arg("show")
            .arg(id)
            .arg("--format")
            .arg("json")
            .output();
        if let Ok(output) = show_output
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(record) = serde_json::from_str::<SkrunCliSkill>(&stdout) {
                return Ok(Some(skrun_cli_skill_to_model(record)));
            }
        }
        Ok(self
            .try_list_skill_models()?
            .into_iter()
            .find(|skill| skill.id == id))
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
        Self::from_default_bin()
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

    #[cfg(unix)]
    fn fake_skrun_bin(dir: &tempfile::TempDir, response: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let bin = dir.path().join("skrun");
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\nprintf '%s' '{}'\n",
                response.replace('\'', "'\\''")
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&bin, permissions).unwrap();
        bin
    }

    #[test]
    fn test_skrun_json_list_parser() {
        let records = parse_skrun_json_list(
            r##"[{
              "id": "regex-finder",
              "name": "Regex Finder",
              "version": "0.1.0",
              "kind": "rust_binary",
              "description": "Find text with regex.",
              "tags": ["search"],
              "suggested_tools": ["custom_tool"],
              "content": "# Regex Finder\n\nFind text with regex.",
              "executable": true,
              "source_ref": "crate:regex-finder@0.1.0"
            }]"##,
        )
        .unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "regex-finder");
        assert_eq!(
            records[0].source_ref.as_deref(),
            Some("crate:regex-finder@0.1.0")
        );
        assert_eq!(records[0].tags, Some(vec!["search".to_string()]));
        assert_eq!(records[0].suggested_tools, vec!["custom_tool"]);
    }

    #[test]
    fn test_skrun_tsv_list_parser() {
        let records = parse_skrun_tsv_list("regex-finder\trust_binary\t0.1.0\tRegex Finder\n");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "regex-finder");
        assert_eq!(records[0].name, "Regex Finder");
    }

    #[cfg(unix)]
    #[test]
    fn test_skrun_provider_lists_installed_artifacts() {
        let dir = tempdir().unwrap();
        let bin = fake_skrun_bin(
            &dir,
            r##"[{
              "id": "regex-finder",
              "name": "Regex Finder",
              "version": "0.1.0",
              "kind": "rust_binary",
              "description": "Find text with regex.",
              "content": "# Regex Finder\n\nFind text with regex."
            }]"##,
        );

        let provider = SkrunSkillProvider::new(bin);
        let skills = provider.list_skill_models();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "regex-finder");
        assert_eq!(skills[0].source, SkillSource::External);
        assert!(skills[0].read_only);
        assert_eq!(skills[0].suggested_tools, vec![SKRUN_TOOL_NAME]);
    }

    #[cfg(unix)]
    #[test]
    fn test_skrun_provider_lists_markdown_skills_without_run_tool() {
        let dir = tempdir().unwrap();
        let bin = fake_skrun_bin(
            &dir,
            r##"[{
              "id": "team",
              "name": "Team",
              "version": "0.1.0",
              "kind": "markdown",
              "description": "Coordinate workers.",
              "tags": ["team"],
              "suggested_tools": ["spawn_subagent_batch"],
              "content": "# Team\n\nUse workers.",
              "executable": false
            }]"##,
        );

        let provider = SkrunSkillProvider::new(bin);
        let skills = provider.list_skill_models();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "team");
        assert_eq!(skills[0].source, SkillSource::External);
        assert_eq!(skills[0].tags, Some(vec!["team".to_string()]));
        assert_eq!(skills[0].suggested_tools, vec!["spawn_subagent_batch"]);
    }

    #[test]
    fn test_default_test_catalog_is_empty_without_skrun_override() {
        let provider = SkrunSkillProvider::default();
        assert!(provider.try_list_skill_models().unwrap().is_empty());
        assert!(provider.try_get_skill_model("team").unwrap().is_none());
    }
}
