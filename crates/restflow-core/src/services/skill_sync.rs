use anyhow::Result;
use std::path::Path;

use crate::AppCore;
use crate::loader::skill_folder::SkillFolderLoader;
use crate::models::{Skill, SkillGating, SkillReference, SkillScript};

#[derive(Debug, Clone, Default)]
pub struct SkillSyncReport {
    pub scanned: usize,
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub async fn sync_all(core: &AppCore, base_dir: impl AsRef<Path>) -> Result<SkillSyncReport> {
    let loader = SkillFolderLoader::new(base_dir.as_ref());
    let (skills, failed) = loader.scan()?;

    let mut report = SkillSyncReport {
        scanned: skills.len(),
        failed,
        ..Default::default()
    };

    for mut skill in skills {
        let existing = core.storage.skills.get(&skill.id)?;

        if let Some(existing_skill) = existing {
            skill.created_at = existing_skill.created_at;
            if skills_equal(&existing_skill, &skill) {
                report.skipped += 1;
                continue;
            }
            skill.updated_at = chrono::Utc::now().timestamp_millis();
            core.storage.skills.update(&skill.id, &skill)?;
            report.updated += 1;
        } else {
            core.storage.skills.create(&skill)?;
            report.created += 1;
        }
    }

    Ok(report)
}

fn skills_equal(left: &Skill, right: &Skill) -> bool {
    left.id == right.id
        && left.name == right.name
        && left.description == right.description
        && left.tags == right.tags
        && left.content == right.content
        && left.folder_path == right.folder_path
        && left.suggested_tools == right.suggested_tools
        && normalize_scripts(&left.scripts) == normalize_scripts(&right.scripts)
        && normalize_references(&left.references) == normalize_references(&right.references)
        && normalize_gating(left.gating.as_ref()) == normalize_gating(right.gating.as_ref())
        && left.version == right.version
        && left.author == right.author
        && left.license == right.license
        && left.content_hash == right.content_hash
        && left.storage_mode == right.storage_mode
        && left.is_synced == right.is_synced
}

fn normalize_scripts(scripts: &[SkillScript]) -> Vec<(String, String, Option<String>)> {
    let mut data: Vec<(String, String, Option<String>)> = scripts
        .iter()
        .map(|script| (script.id.clone(), script.path.clone(), script.lang.clone()))
        .collect();
    data.sort();
    data
}

fn normalize_references(references: &[SkillReference]) -> Vec<(String, String)> {
    let mut data: Vec<(String, String)> = references
        .iter()
        .map(|reference| (reference.id.clone(), reference.path.clone()))
        .collect();
    data.sort();
    data
}

fn normalize_gating(
    gating: Option<&SkillGating>,
) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    gating.map(|g| {
        let mut bins = g.bins.clone().unwrap_or_default();
        let mut env = g.env.clone().unwrap_or_default();
        let mut os = g.os.clone().unwrap_or_default();
        bins.sort();
        env.sort();
        os.sort();
        (bins, env, os)
    })
}
