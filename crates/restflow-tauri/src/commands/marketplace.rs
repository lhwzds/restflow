//! Skill Marketplace Tauri commands

use crate::state::AppState;
use restflow_core::registry::{
    SkillSearchQuery, SkillSearchResult, SkillSortOrder,
    MarketplaceProvider, GitHubProvider, GatingChecker,
};
use restflow_core::models::{GatingCheckResult, SkillManifest, SkillVersion};
use restflow_core::models::storage_mode::StorageMode;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Search request from frontend
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// Search text
    pub query: Option<String>,
    /// Filter by category
    pub category: Option<String>,
    /// Filter by tags
    pub tags: Option<Vec<String>>,
    /// Filter by author
    pub author: Option<String>,
    /// Maximum results
    pub limit: Option<usize>,
    /// Offset for pagination
    pub offset: Option<usize>,
    /// Sort order
    pub sort: Option<String>,
    /// Include GitHub results
    pub include_github: Option<bool>,
}

/// Search result for frontend
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultResponse {
    pub manifest: SkillManifest,
    pub score: u32,
    pub downloads: Option<u64>,
    pub rating: Option<f32>,
    pub source: String,
}

fn resolve_content_version(
    requested: Option<String>,
    fallback: Option<SkillVersion>,
) -> Result<SkillVersion, String> {
    if let Some(version) = requested {
        SkillVersion::parse(&version).ok_or_else(|| format!("Invalid version: {}", version))
    } else {
        fallback.ok_or_else(|| "Version is required".to_string())
    }
}

impl From<SkillSearchResult> for SearchResultResponse {
    fn from(result: SkillSearchResult) -> Self {
        let source = match &result.manifest.source {
            restflow_core::models::SkillSource::Marketplace { .. } => "marketplace",
            restflow_core::models::SkillSource::GitHub { .. } => "github",
            restflow_core::models::SkillSource::Local => "local",
            restflow_core::models::SkillSource::Builtin => "builtin",
            restflow_core::models::SkillSource::Git { .. } => "git",
        };
        
        Self {
            manifest: result.manifest,
            score: result.score,
            downloads: result.downloads,
            rating: result.rating,
            source: source.to_string(),
        }
    }
}

/// Search the marketplace for skills
#[tauri::command]
pub async fn marketplace_search(
    _state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<Vec<SearchResultResponse>, String> {
    use restflow_core::registry::SkillProvider;
    
    let query = SkillSearchQuery {
        query: request.query,
        category: request.category,
        tags: request.tags.unwrap_or_default(),
        author: request.author,
        limit: request.limit,
        offset: request.offset,
        sort: request.sort.and_then(|s| match s.as_str() {
            "relevance" => Some(SkillSortOrder::Relevance),
            "updated" | "recently_updated" => Some(SkillSortOrder::RecentlyUpdated),
            "popular" | "downloads" => Some(SkillSortOrder::Popular),
            "name" => Some(SkillSortOrder::Name),
            _ => None,
        }),
    };

    let mut results = Vec::new();

    // Search marketplace
    let marketplace = MarketplaceProvider::new();
    match marketplace.search(&query).await {
        Ok(marketplace_results) => {
            results.extend(marketplace_results.into_iter().map(SearchResultResponse::from));
        }
        Err(e) => {
            tracing::warn!("Marketplace search failed: {}", e);
        }
    }

    // Optionally search GitHub
    if request.include_github.unwrap_or(false) {
        let github = GitHubProvider::new();
        match github.search(&query).await {
            Ok(github_results) => {
                results.extend(github_results.into_iter().map(SearchResultResponse::from));
            }
            Err(e) => {
                tracing::warn!("GitHub search failed: {}", e);
            }
        }
    }

    // Sort combined results by score
    results.sort_by(|a, b| b.score.cmp(&a.score));

    // Apply limit if provided
    if let Some(limit) = request.limit {
        results.truncate(limit);
    }

    Ok(results)
}

/// Get skill details from marketplace
#[tauri::command]
pub async fn marketplace_get_skill(
    _state: State<'_, AppState>,
    id: String,
    source: Option<String>,
) -> Result<SkillManifest, String> {
    use restflow_core::registry::SkillProvider;
    
    let source = source.unwrap_or_else(|| "marketplace".to_string());
    
    match source.as_str() {
        "marketplace" => {
            let provider = MarketplaceProvider::new();
            provider.get_manifest(&id).await.map_err(|e| e.to_string())
        }
        "github" => {
            let provider = GitHubProvider::new();
            provider.get_manifest(&id).await.map_err(|e| e.to_string())
        }
        _ => Err(format!("Unknown source: {}", source)),
    }
}

/// Get skill versions
#[tauri::command]
pub async fn marketplace_get_versions(
    _state: State<'_, AppState>,
    id: String,
    source: Option<String>,
) -> Result<Vec<SkillVersion>, String> {
    use restflow_core::registry::SkillProvider;
    
    let source = source.unwrap_or_else(|| "marketplace".to_string());
    
    match source.as_str() {
        "marketplace" => {
            let provider = MarketplaceProvider::new();
            provider.list_versions(&id).await.map_err(|e| e.to_string())
        }
        "github" => {
            let provider = GitHubProvider::new();
            provider.list_versions(&id).await.map_err(|e| e.to_string())
        }
        _ => Err(format!("Unknown source: {}", source)),
    }
}

/// Get skill content (readme/documentation)
#[tauri::command]
pub async fn marketplace_get_content(
    _state: State<'_, AppState>,
    id: String,
    version: Option<String>,
    source: Option<String>,
) -> Result<String, String> {
    use restflow_core::registry::SkillProvider;
    
    let source = source.unwrap_or_else(|| "marketplace".to_string());
    
    match source.as_str() {
        "marketplace" => {
            let provider = MarketplaceProvider::new();
            let fallback_version = if version.is_none() {
                Some(provider.get_manifest(&id).await.map_err(|e| e.to_string())?.version)
            } else {
                None
            };
            let resolved_version = resolve_content_version(version, fallback_version)?;
            provider.get_content(&id, &resolved_version).await.map_err(|e| e.to_string())
        }
        "github" => {
            let provider = GitHubProvider::new();
            let fallback_version = if version.is_none() {
                Some(provider.get_manifest(&id).await.map_err(|e| e.to_string())?.version)
            } else {
                None
            };
            let resolved_version = resolve_content_version(version, fallback_version)?;
            provider.get_content(&id, &resolved_version).await.map_err(|e| e.to_string())
        }
        _ => Err(format!("Unknown source: {}", source)),
    }
}

/// Check gating requirements for a skill
#[tauri::command]
pub async fn marketplace_check_gating(
    _state: State<'_, AppState>,
    id: String,
    source: Option<String>,
) -> Result<GatingCheckResult, String> {
    use restflow_core::registry::SkillProvider;
    
    let source = source.unwrap_or_else(|| "marketplace".to_string());
    
    // Get the manifest first
    let manifest = match source.as_str() {
        "marketplace" => {
            let provider = MarketplaceProvider::new();
            provider.get_manifest(&id).await.map_err(|e| e.to_string())?
        }
        "github" => {
            let provider = GitHubProvider::new();
            provider.get_manifest(&id).await.map_err(|e| e.to_string())?
        }
        _ => return Err(format!("Unknown source: {}", source)),
    };

    // Check gating requirements
    let checker = GatingChecker::default();
    Ok(checker.check(&manifest.gating))
}

/// Install a skill from marketplace
#[tauri::command]
pub async fn marketplace_install_skill(
    state: State<'_, AppState>,
    id: String,
    version: Option<String>,
    source: Option<String>,
) -> Result<(), String> {
    use restflow_core::registry::SkillProvider;
    use restflow_core::Skill;
    use restflow_core::models::{OsType, SkillGating};
    
    let version = version.and_then(|v| SkillVersion::parse(&v));
    let source = source.unwrap_or_else(|| "marketplace".to_string());
    
    // Get the manifest
    let manifest = match source.as_str() {
        "marketplace" => {
            let provider = MarketplaceProvider::new();
            provider.get_manifest(&id).await.map_err(|e| e.to_string())?
        }
        "github" => {
            let provider = GitHubProvider::new();
            provider.get_manifest(&id).await.map_err(|e| e.to_string())?
        }
        _ => return Err(format!("Unknown source: {}", source)),
    };

    // Check gating requirements
    let checker = GatingChecker::default();
    let gating_result = checker.check(&manifest.gating);
    if !gating_result.passed {
        return Err(format!("Gating requirements not met: {}", gating_result.summary));
    }

    // Get the content
    let content_version = version.unwrap_or_else(|| manifest.version.clone());
    let content = match source.as_str() {
        "marketplace" => {
            let provider = MarketplaceProvider::new();
            provider.get_content(&id, &content_version).await.ok()
        }
        "github" => {
            let provider = GitHubProvider::new();
            provider.get_content(&id, &content_version).await.ok()
        }
        _ => None,
    };

    let gating = if manifest.gating.binaries.is_empty()
        && manifest.gating.env_vars.is_empty()
        && manifest.gating.supported_os.is_empty()
    {
        None
    } else {
        Some(SkillGating {
            bins: if manifest.gating.binaries.is_empty() {
                None
            } else {
                Some(
                    manifest
                        .gating
                        .binaries
                        .iter()
                        .map(|binary| binary.name.clone())
                        .collect(),
                )
            },
            env: if manifest.gating.env_vars.is_empty() {
                None
            } else {
                Some(
                    manifest
                        .gating
                        .env_vars
                        .iter()
                        .map(|env_var| env_var.name.clone())
                        .collect(),
                )
            },
            os: if manifest.gating.supported_os.is_empty() {
                None
            } else {
                Some(
                    manifest
                        .gating
                        .supported_os
                        .iter()
                        .map(|os| match os {
                            OsType::Windows => "windows".to_string(),
                            OsType::MacOS => "macos".to_string(),
                            OsType::Linux => "linux".to_string(),
                            OsType::Any => "any".to_string(),
                        })
                        .collect(),
                )
            },
        })
    };

    // Create a Skill from the manifest
    let skill = Skill {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        tags: Some(manifest.keywords.clone()),
        content: content.unwrap_or_default(),
        folder_path: None,
        suggested_tools: Vec::new(),
        scripts: Vec::new(),
        references: Vec::new(),
        gating,
        version: Some(manifest.version.to_string()),
        author: manifest.author.as_ref().map(|author| author.name.clone()),
        license: manifest.license.clone(),
        content_hash: None,
        storage_mode: StorageMode::DatabaseOnly,
        is_synced: false,
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };

    // Save to storage
    state
        .executor()
        .create_skill(skill)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Uninstall a skill
#[tauri::command]
pub async fn marketplace_uninstall_skill(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .executor()
        .delete_skill(id)
        .await
        .map_err(|e| e.to_string())
}

/// Get installed skills with marketplace info
#[tauri::command]
pub async fn marketplace_list_installed(
    state: State<'_, AppState>,
) -> Result<Vec<restflow_core::Skill>, String> {
    state
        .executor()
        .list_skills()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_content_version_uses_fallback_when_missing() {
        let fallback = SkillVersion::new(1, 2, 3);
        let resolved = resolve_content_version(None, Some(fallback.clone())).unwrap();
        assert_eq!(resolved, fallback);
    }

    #[test]
    fn resolve_content_version_parses_requested() {
        let resolved = resolve_content_version(
            Some("1.2.3-beta.1".to_string()),
            None,
        )
        .unwrap();
        assert_eq!(resolved.prerelease.as_deref(), Some("beta.1"));
    }

    #[test]
    fn resolve_content_version_errors_on_invalid_requested() {
        let error = resolve_content_version(Some("not-a-version".to_string()), None).unwrap_err();
        assert_eq!(error, "Invalid version: not-a-version");
    }

    #[test]
    fn resolve_content_version_errors_without_requested_or_fallback() {
        let error = resolve_content_version(None, None).unwrap_err();
        assert_eq!(error, "Version is required");
    }
}
