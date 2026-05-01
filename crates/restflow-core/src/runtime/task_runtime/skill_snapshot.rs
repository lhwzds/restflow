use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};

use crate::models::Skill;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillSnapshotKey {
    pub agent_id: Option<String>,
    pub skill_filter_signature: String,
    pub trigger_context_signature: String,
}

impl SkillSnapshotKey {
    pub fn new(
        agent_id: Option<String>,
        skill_filter_signature: String,
        trigger_context_signature: String,
    ) -> Self {
        Self {
            agent_id,
            skill_filter_signature,
            trigger_context_signature,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkillSnapshotPayload {
    pub resolved_skills: Vec<Skill>,
    pub triggered_skill_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct CachedSkillSnapshot {
    version_hash: String,
    payload: SkillSnapshotPayload,
}

#[derive(Debug, Clone)]
pub struct SkillSnapshotLookup {
    pub payload: SkillSnapshotPayload,
    pub hit: bool,
}

#[derive(Debug, Default)]
pub struct SkillSnapshotCache {
    entries: RwLock<HashMap<SkillSnapshotKey, CachedSkillSnapshot>>,
}

impl SkillSnapshotCache {
    pub fn resolve_with<F>(
        &self,
        key: SkillSnapshotKey,
        version_hash: String,
        refresh: F,
    ) -> Result<SkillSnapshotLookup>
    where
        F: FnOnce() -> Result<SkillSnapshotPayload>,
    {
        {
            let entries = self
                .entries
                .read()
                .map_err(|error| anyhow!("Skill snapshot cache lock poisoned: {error}"))?;
            if let Some(cached) = entries.get(&key)
                && cached.version_hash == version_hash
            {
                return Ok(SkillSnapshotLookup {
                    payload: cached.payload.clone(),
                    hit: true,
                });
            }
        }

        let refreshed = refresh()?;
        let cached = CachedSkillSnapshot {
            version_hash,
            payload: refreshed.clone(),
        };

        let mut entries = self
            .entries
            .write()
            .map_err(|error| anyhow!("Skill snapshot cache lock poisoned: {error}"))?;
        entries.insert(key, cached);

        Ok(SkillSnapshotLookup {
            payload: refreshed,
            hit: false,
        })
    }
}

pub fn build_skill_filter_signature(skill_filter: Option<&[String]>) -> String {
    let mut ids: Vec<&str> = skill_filter
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect();
    ids.sort_unstable();
    hash_text(&ids.join("|"))
}

pub fn build_trigger_context_signature(trigger_context: Option<&str>) -> String {
    hash_text(trigger_context.map(str::trim).unwrap_or(""))
}

pub fn build_skill_version_hash(skills: &[Skill]) -> String {
    let mut versions: Vec<String> = skills
        .iter()
        .map(|skill| {
            let fallback_content_hash = hex::encode(Sha256::digest(skill.content.as_bytes()));
            let content_version_hash = skill
                .content_hash
                .as_ref()
                .cloned()
                .unwrap_or(fallback_content_hash);
            format!("{}:{}:{}", skill.id, skill.updated_at, content_version_hash)
        })
        .collect();
    versions.sort_unstable();
    hash_text(&versions.join("\n"))
}

fn hash_text(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn test_key(trigger_sig: &str) -> SkillSnapshotKey {
        SkillSnapshotKey::new(
            Some("agent-1".to_string()),
            "filter-signature".to_string(),
            trigger_sig.to_string(),
        )
    }

    fn test_skill(id: &str, updated_at: i64, content: &str, content_hash: Option<&str>) -> Skill {
        let mut skill = Skill::new(
            id.to_string(),
            id.to_string(),
            None,
            None,
            content.to_string(),
        );
        skill.updated_at = updated_at;
        skill.content_hash = content_hash.map(|value| value.to_string());
        skill
    }

    #[test]
    fn test_cache_hit_reuses_resolved_snapshot() {
        let cache = SkillSnapshotCache::default();
        let refresh_count = Arc::new(AtomicUsize::new(0));

        let first = cache
            .resolve_with(test_key("ctx-1"), "version-a".to_string(), {
                let refresh_count = Arc::clone(&refresh_count);
                move || {
                    refresh_count.fetch_add(1, Ordering::SeqCst);
                    Ok(SkillSnapshotPayload {
                        resolved_skills: vec![test_skill("skill-a", 1, "A", Some("hash-a"))],
                        triggered_skill_ids: vec!["skill-a".to_string()],
                    })
                }
            })
            .expect("first resolution should succeed");

        let second = cache
            .resolve_with(test_key("ctx-1"), "version-a".to_string(), {
                let refresh_count = Arc::clone(&refresh_count);
                move || {
                    refresh_count.fetch_add(1, Ordering::SeqCst);
                    Ok(SkillSnapshotPayload::default())
                }
            })
            .expect("second resolution should succeed");

        assert!(!first.hit);
        assert!(second.hit);
        assert_eq!(refresh_count.load(Ordering::SeqCst), 1);
        assert_eq!(second.payload.resolved_skills.len(), 1);
    }

    #[test]
    fn test_cache_miss_on_different_snapshot_key() {
        let cache = SkillSnapshotCache::default();
        let refresh_count = Arc::new(AtomicUsize::new(0));

        let first = cache
            .resolve_with(test_key("ctx-1"), "version-a".to_string(), {
                let refresh_count = Arc::clone(&refresh_count);
                move || {
                    refresh_count.fetch_add(1, Ordering::SeqCst);
                    Ok(SkillSnapshotPayload::default())
                }
            })
            .expect("first resolution should succeed");

        let second = cache
            .resolve_with(test_key("ctx-2"), "version-a".to_string(), {
                let refresh_count = Arc::clone(&refresh_count);
                move || {
                    refresh_count.fetch_add(1, Ordering::SeqCst);
                    Ok(SkillSnapshotPayload::default())
                }
            })
            .expect("second resolution should succeed");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(refresh_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_cache_invalidates_when_version_hash_changes() {
        let cache = SkillSnapshotCache::default();
        let refresh_count = Arc::new(AtomicUsize::new(0));

        cache
            .resolve_with(test_key("ctx-1"), "version-a".to_string(), {
                let refresh_count = Arc::clone(&refresh_count);
                move || {
                    refresh_count.fetch_add(1, Ordering::SeqCst);
                    Ok(SkillSnapshotPayload {
                        resolved_skills: vec![test_skill("skill-a", 1, "A", Some("hash-a"))],
                        triggered_skill_ids: vec!["skill-a".to_string()],
                    })
                }
            })
            .expect("first resolution should succeed");

        let second = cache
            .resolve_with(test_key("ctx-1"), "version-b".to_string(), {
                let refresh_count = Arc::clone(&refresh_count);
                move || {
                    refresh_count.fetch_add(1, Ordering::SeqCst);
                    Ok(SkillSnapshotPayload {
                        resolved_skills: vec![test_skill("skill-b", 2, "B", Some("hash-b"))],
                        triggered_skill_ids: vec!["skill-b".to_string()],
                    })
                }
            })
            .expect("second resolution should succeed");

        assert!(!second.hit);
        assert_eq!(refresh_count.load(Ordering::SeqCst), 2);
        assert_eq!(second.payload.resolved_skills[0].id, "skill-b");
    }

    #[test]
    fn test_skill_version_hash_changes_with_updated_at_or_content_hash() {
        let mut baseline = test_skill("skill-a", 100, "content", Some("hash-a"));
        let baseline_hash = build_skill_version_hash(&[baseline.clone()]);

        baseline.updated_at = 101;
        let updated_at_hash = build_skill_version_hash(&[baseline.clone()]);
        assert_ne!(baseline_hash, updated_at_hash);

        baseline.updated_at = 100;
        baseline.content_hash = Some("hash-b".to_string());
        let content_hash = build_skill_version_hash(&[baseline]);
        assert_ne!(baseline_hash, content_hash);
    }
}
