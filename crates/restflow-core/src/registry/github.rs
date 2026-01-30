//! GitHub skill provider - fetches skills from GitHub repositories.
//!
//! Supports fetching skills from:
//! - Individual repositories with skill manifests
//! - Topic-based discovery (repositories with `restflow-skill` topic)
//! - GitHub releases for versioned skills

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::{SkillProvider, SkillProviderError, SkillSearchQuery, SkillSearchResult, SkillSortOrder};
use crate::models::{
    SkillManifest, SkillSource, SkillVersion, SkillAuthor, SkillPermissions,
    GatingRequirements,
};

/// GitHub API base URL
const GITHUB_API_URL: &str = "https://api.github.com";

/// Cache TTL for GitHub data
const CACHE_TTL: Duration = Duration::from_secs(600); // 10 minutes

/// GitHub repository search result
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubSearchResponse {
    total_count: u64,
    items: Vec<GitHubRepo>,
}

/// GitHub repository
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubRepo {
    id: u64,
    name: String,
    full_name: String,
    description: Option<String>,
    owner: GitHubOwner,
    html_url: String,
    clone_url: String,
    default_branch: String,
    stargazers_count: u64,
    topics: Vec<String>,
    license: Option<GitHubLicense>,
    updated_at: String,
}

/// GitHub repository owner
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubOwner {
    login: String,
    avatar_url: String,
}

/// GitHub license
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubLicense {
    key: String,
    name: String,
    spdx_id: Option<String>,
}

/// GitHub release
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    id: u64,
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    prerelease: bool,
    draft: bool,
    published_at: Option<String>,
    assets: Vec<GitHubAsset>,
}

/// GitHub release asset
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubAsset {
    id: u64,
    name: String,
    content_type: String,
    size: u64,
    browser_download_url: String,
}

/// Cache entry with expiration
struct CacheEntry<T> {
    data: T,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Instant::now() + ttl,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// GitHub skill provider
pub struct GitHubProvider {
    /// HTTP client
    client: Client,
    /// GitHub personal access token (optional, for higher rate limits)
    token: Option<String>,
    /// Cache for manifests
    manifest_cache: Arc<RwLock<HashMap<String, CacheEntry<SkillManifest>>>>,
    /// Cache for repo to manifest mapping (reserved for future use)
    #[allow(dead_code)]
    repo_cache: Arc<RwLock<HashMap<String, CacheEntry<Vec<SkillSearchResult>>>>>,
}

impl GitHubProvider {
    /// Create a new GitHub provider
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("RestFlow/1.0")
                .build()
                .unwrap_or_default(),
            token: None,
            manifest_cache: Arc::new(RwLock::new(HashMap::new())),
            repo_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the GitHub personal access token
    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }

    /// Make an authenticated request to GitHub API
    async fn request(&self, url: &str) -> Result<reqwest::Response, SkillProviderError> {
        let mut request = self.client.get(url)
            .header("Accept", "application/vnd.github.v3+json");
        
        if let Some(ref token) = self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        request
            .send()
            .await
            .map_err(|e| SkillProviderError::Network(e.to_string()))
    }

    /// Parse a GitHub repository URL to get owner and repo
    fn parse_repo_url(url: &str) -> Option<(String, String)> {
        // Handle formats:
        // - https://github.com/owner/repo
        // - github.com/owner/repo
        // - owner/repo
        let url = url.trim_start_matches("https://");
        let url = url.trim_start_matches("http://");
        let url = url.trim_start_matches("github.com/");
        
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 2 {
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            None
        }
    }

    /// Convert a GitHub repo to a skill manifest
    async fn repo_to_manifest(&self, repo: &GitHubRepo) -> Result<SkillManifest, SkillProviderError> {
        // Try to fetch the skill manifest from the repo
        let manifest_url = format!(
            "https://raw.githubusercontent.com/{}/{}/skill.toml",
            repo.full_name, repo.default_branch
        );
        
        // Try TOML first, then JSON
        let response = self.client.get(&manifest_url).send().await;
        
        if let Ok(resp) = response
            && resp.status().is_success()
        {
            let content = resp.text().await
                .map_err(|e| SkillProviderError::Network(e.to_string()))?;
            
            let mut manifest: SkillManifest = toml::from_str(&content)
                .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

            manifest.source = SkillSource::GitHub {
                owner: repo.owner.login.clone(),
                repo: repo.name.clone(),
                git_ref: Some(repo.default_branch.clone()),
                path: None,
            };
            
            return Ok(manifest);
        }

        // Try JSON manifest
        let manifest_url = format!(
            "https://raw.githubusercontent.com/{}/{}/skill.json",
            repo.full_name, repo.default_branch
        );
        
        let response = self.client.get(&manifest_url).send().await;
        
        if let Ok(resp) = response
            && resp.status().is_success()
        {
            let content = resp.text().await
                .map_err(|e| SkillProviderError::Network(e.to_string()))?;
            
            let mut manifest: SkillManifest = serde_json::from_str(&content)
                .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

            manifest.source = SkillSource::GitHub {
                owner: repo.owner.login.clone(),
                repo: repo.name.clone(),
                git_ref: Some(repo.default_branch.clone()),
                path: None,
            };
            
            return Ok(manifest);
        }

        // No manifest found, create one from repo metadata
        Ok(SkillManifest {
            id: repo.full_name.replace('/', "-"),
            name: repo.name.clone(),
            description: repo.description.clone(),
            version: SkillVersion::new(0, 0, 1),
            author: Some(SkillAuthor {
                name: repo.owner.login.clone(),
                email: None,
                url: Some(format!("https://github.com/{}", repo.owner.login)),
            }),
            license: repo.license.as_ref().and_then(|l| l.spdx_id.clone()),
            keywords: repo.topics.clone(),
            categories: vec![],
            repository: Some(repo.html_url.clone()),
            homepage: None,
            source: SkillSource::GitHub { 
                owner: repo.owner.login.clone(),
                repo: repo.name.clone(),
                git_ref: Some(repo.default_branch.clone()),
                path: None,
            },
            readme: None,
            changelog: None,
            icon: None,
            metadata: std::collections::HashMap::new(),
            dependencies: vec![],
            permissions: SkillPermissions::default(),
            gating: GatingRequirements::default(),
        })
    }

    /// Fetch releases for a repository
    async fn fetch_releases(&self, owner: &str, repo: &str) -> Result<Vec<GitHubRelease>, SkillProviderError> {
        let url = format!("{}/repos/{}/{}/releases", GITHUB_API_URL, owner, repo);
        let response = self.request(&url).await?;

        if !response.status().is_success() {
            return Ok(vec![]); // No releases or no access
        }

        response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))
    }
}

impl Default for GitHubProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillProvider for GitHubProvider {
    fn name(&self) -> &str {
        "github"
    }

    fn priority(&self) -> u32 {
        30 // Lower priority than marketplace
    }

    async fn search(&self, query: &SkillSearchQuery) -> Result<Vec<SkillSearchResult>, SkillProviderError> {
        // Build GitHub search query
        let mut search_parts = vec!["topic:restflow-skill".to_string()];
        
        if let Some(ref q) = query.query {
            search_parts.push(q.clone());
        }

        for tag in &query.tags {
            search_parts.push(format!("topic:{}", tag));
        }

        let search_query = search_parts.join(" ");
        
        // Sort parameter
        let sort = match query.sort {
            Some(SkillSortOrder::RecentlyUpdated) => "updated",
            Some(SkillSortOrder::Popular) => "stars",
            _ => "best-match",
        };

        let per_page = query.limit.unwrap_or(20).min(100);
        let page = query.offset.map(|o| o / per_page + 1).unwrap_or(1);

        let url = format!(
            "{}/search/repositories?q={}&sort={}&per_page={}&page={}",
            GITHUB_API_URL,
            urlencoding::encode(&search_query),
            sort,
            per_page,
            page
        );

        let response = self.request(&url).await?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 403 {
                return Err(SkillProviderError::Network(
                    "GitHub API rate limit exceeded. Consider using a personal access token.".to_string()
                ));
            }
            return Err(SkillProviderError::Network(format!(
                "GitHub API returned status {}",
                status
            )));
        }

        let search_response: GitHubSearchResponse = response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

        let mut results = Vec::new();
        
        for repo in search_response.items {
            match self.repo_to_manifest(&repo).await {
                Ok(manifest) => {
                    results.push(SkillSearchResult {
                        manifest,
                        score: 30,
                        downloads: Some(repo.stargazers_count),
                        rating: None,
                    });
                }
                Err(e) => {
                    tracing::debug!("Failed to parse manifest for {}: {}", repo.full_name, e);
                }
            }
        }

        Ok(results)
    }

    async fn get_manifest(&self, id: &str) -> Result<SkillManifest, SkillProviderError> {
        // Check cache
        {
            let cache = self.manifest_cache.read().await;
            if let Some(entry) = cache.get(id)
                && !entry.is_expired()
            {
                return Ok(entry.data.clone());
            }
        }

        // Parse the ID as owner/repo
        let (owner, repo) = Self::parse_repo_url(id)
            .ok_or_else(|| SkillProviderError::NotFound(format!("Invalid GitHub skill ID: {}", id)))?;

        let url = format!("{}/repos/{}/{}", GITHUB_API_URL, owner, repo);
        let response = self.request(&url).await?;

        if response.status().as_u16() == 404 {
            return Err(SkillProviderError::NotFound(id.to_string()));
        }

        if !response.status().is_success() {
            return Err(SkillProviderError::Network(format!(
                "GitHub API returned status {}",
                response.status()
            )));
        }

        let repo_data: GitHubRepo = response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

        let manifest = self.repo_to_manifest(&repo_data).await?;

        // Update cache
        {
            let mut cache = self.manifest_cache.write().await;
            cache.insert(id.to_string(), CacheEntry::new(manifest.clone(), CACHE_TTL));
        }

        Ok(manifest)
    }

    async fn get_content(&self, id: &str, _version: &SkillVersion) -> Result<String, SkillProviderError> {
        let (owner, repo) = Self::parse_repo_url(id)
            .ok_or_else(|| SkillProviderError::NotFound(format!("Invalid GitHub skill ID: {}", id)))?;

        // Get default branch
        let repo_url = format!("{}/repos/{}/{}", GITHUB_API_URL, owner, repo);
        let response = self.request(&repo_url).await?;
        
        let repo_data: GitHubRepo = response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

        // Try to fetch SKILL.md or README.md
        for filename in &["SKILL.md", "skill.md", "README.md", "readme.md"] {
            let content_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}",
                repo_data.full_name, repo_data.default_branch, filename
            );
            
            let response = self.client.get(&content_url).send().await;
            
            if let Ok(resp) = response
                && resp.status().is_success()
            {
                return resp.text().await
                    .map_err(|e| SkillProviderError::Network(e.to_string()));
            }
        }

        Err(SkillProviderError::NotFound(format!(
            "No skill content found for {}",
            id
        )))
    }

    async fn list_versions(&self, id: &str) -> Result<Vec<SkillVersion>, SkillProviderError> {
        let (owner, repo) = Self::parse_repo_url(id)
            .ok_or_else(|| SkillProviderError::NotFound(format!("Invalid GitHub skill ID: {}", id)))?;

        let releases = self.fetch_releases(&owner, &repo).await?;

        let versions: Vec<SkillVersion> = releases
            .into_iter()
            .filter(|r| !r.draft)
            .filter_map(|r| {
                // Remove 'v' prefix if present
                let tag = r.tag_name.trim_start_matches('v');
                SkillVersion::parse(tag)
            })
            .collect();

        if versions.is_empty() {
            // Return a default version if no releases
            Ok(vec![SkillVersion::new(0, 0, 1)])
        } else {
            Ok(versions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repo_url() {
        assert_eq!(
            GitHubProvider::parse_repo_url("https://github.com/owner/repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            GitHubProvider::parse_repo_url("github.com/owner/repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            GitHubProvider::parse_repo_url("owner/repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            GitHubProvider::parse_repo_url("owner"),
            None
        );
    }

    #[test]
    fn test_default_provider() {
        let provider = GitHubProvider::new();
        assert!(provider.token.is_none());
    }

    #[test]
    fn test_provider_with_token() {
        let provider = GitHubProvider::new().with_token("test-token".to_string());
        assert_eq!(provider.token, Some("test-token".to_string()));
    }
}
