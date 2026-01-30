//! RestFlow Marketplace provider - fetches skills from the RestFlow marketplace API.

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
    GatingRequirements, BinaryRequirement, EnvVarRequirement, OsType,
};

/// Default marketplace API URL
pub const DEFAULT_MARKETPLACE_URL: &str = "https://api.restflow.dev/v1";

/// Cache TTL for marketplace data
const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

/// RestFlow Marketplace API response for skill list
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MarketplaceSkillListResponse {
    skills: Vec<MarketplaceSkill>,
    total: u64,
    page: u32,
    per_page: u32,
}

/// Skill data from the marketplace API
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MarketplaceSkill {
    id: String,
    name: String,
    description: Option<String>,
    version: String,
    author: Option<String>,
    license: Option<String>,
    keywords: Vec<String>,
    categories: Vec<String>,
    downloads: u64,
    rating: Option<f32>,
    repository: Option<String>,
    homepage: Option<String>,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    dependencies: HashMap<String, String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    gating: MarketplaceGating,
}

/// Gating requirements from marketplace
#[derive(Debug, Default, Deserialize)]
struct MarketplaceGating {
    #[serde(default)]
    required_binaries: Vec<String>,
    #[serde(default)]
    required_env: Vec<String>,
    #[serde(default)]
    os: Vec<String>,
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

/// RestFlow Marketplace skill provider
pub struct MarketplaceProvider {
    /// HTTP client
    client: Client,
    /// Base URL for the marketplace API
    base_url: String,
    /// API key (optional, for rate limiting bypass)
    api_key: Option<String>,
    /// Cache for manifests
    manifest_cache: Arc<RwLock<HashMap<String, CacheEntry<SkillManifest>>>>,
    /// Cache for search results
    search_cache: Arc<RwLock<HashMap<String, CacheEntry<Vec<SkillSearchResult>>>>>,
}

impl MarketplaceProvider {
    /// Create a new marketplace provider with default URL
    pub fn new() -> Self {
        Self::with_url(DEFAULT_MARKETPLACE_URL)
    }

    /// Create a new marketplace provider with custom URL
    pub fn with_url(url: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("RestFlow/1.0")
                .build()
                .unwrap_or_default(),
            base_url: url.trim_end_matches('/').to_string(),
            api_key: None,
            manifest_cache: Arc::new(RwLock::new(HashMap::new())),
            search_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the API key for authenticated requests
    pub fn with_api_key(mut self, key: String) -> Self {
        self.api_key = Some(key);
        self
    }

    /// Build the search cache key
    fn search_cache_key(query: &SkillSearchQuery) -> String {
        format!(
            "q={:?}&cat={:?}&tags={:?}&author={:?}&limit={:?}&offset={:?}&sort={:?}",
            query.query,
            query.category,
            query.tags,
            query.author,
            query.limit,
            query.offset,
            query.sort.map(|s| format!("{:?}", s))
        )
    }

    /// Convert marketplace skill to internal manifest
    fn to_manifest(skill: MarketplaceSkill) -> Result<SkillManifest, SkillProviderError> {
        use crate::models::{SkillDependency, VersionRequirement};
        
        let version = SkillVersion::parse(&skill.version)
            .ok_or_else(|| SkillProviderError::Parse(format!("Invalid version: {}", skill.version)))?;

        // Parse author
        let author = skill.author.map(|name| SkillAuthor {
            name,
            email: None,
            url: None,
        });

        // Parse dependencies
        let dependencies: Vec<SkillDependency> = skill.dependencies.into_iter().map(|(k, v)| {
            SkillDependency {
                skill_id: k,
                version: VersionRequirement::parse(&v).unwrap_or(VersionRequirement::Any),
                optional: false,
            }
        }).collect();

        // Parse gating requirements
        let gating = GatingRequirements {
            binaries: skill.gating.required_binaries.into_iter().map(|name| {
                BinaryRequirement {
                    name,
                    version: None,
                    version_command: None,
                    version_pattern: None,
                }
            }).collect(),
            env_vars: skill.gating.required_env.into_iter().map(|name| {
                EnvVarRequirement {
                    name,
                    required: true,
                    description: None,
                }
            }).collect(),
            supported_os: skill.gating.os.into_iter().filter_map(|os| {
                match os.to_lowercase().as_str() {
                    "windows" => Some(OsType::Windows),
                    "macos" | "darwin" => Some(OsType::MacOS),
                    "linux" => Some(OsType::Linux),
                    _ => None,
                }
            }).collect(),
            min_restflow_version: None,
        };

        Ok(SkillManifest {
            id: skill.id,
            name: skill.name,
            description: skill.description,
            version,
            author,
            license: skill.license,
            keywords: skill.keywords,
            categories: skill.categories,
            repository: skill.repository,
            homepage: skill.homepage,
            source: SkillSource::Marketplace {
                url: DEFAULT_MARKETPLACE_URL.to_string(),
            },
            readme: None,
            changelog: None,
            icon: None,
            metadata: HashMap::new(),
            dependencies,
            permissions: SkillPermissions::default(),
            gating,
        })
    }

    /// Make an authenticated request
    async fn request(&self, url: &str) -> Result<reqwest::Response, SkillProviderError> {
        let mut request = self.client.get(url);
        
        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        request
            .send()
            .await
            .map_err(|e| SkillProviderError::Network(e.to_string()))
    }
}

impl Default for MarketplaceProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SkillProvider for MarketplaceProvider {
    fn name(&self) -> &str {
        "marketplace"
    }

    fn priority(&self) -> u32 {
        50 // Medium priority
    }

    async fn search(&self, query: &SkillSearchQuery) -> Result<Vec<SkillSearchResult>, SkillProviderError> {
        let cache_key = Self::search_cache_key(query);
        
        // Check cache
        {
            let cache = self.search_cache.read().await;
            if let Some(entry) = cache.get(&cache_key)
                && !entry.is_expired()
            {
                return Ok(entry.data.clone());
            }
        }

        // Build query params
        let mut params = vec![];
        if let Some(ref q) = query.query {
            params.push(format!("q={}", urlencoding::encode(q)));
        }
        if let Some(ref cat) = query.category {
            params.push(format!("category={}", urlencoding::encode(cat)));
        }
        for tag in &query.tags {
            params.push(format!("tag={}", urlencoding::encode(tag)));
        }
        if let Some(ref author) = query.author {
            params.push(format!("author={}", urlencoding::encode(author)));
        }
        if let Some(limit) = query.limit {
            params.push(format!("per_page={}", limit));
        }
        if let Some(offset) = query.offset {
            let page = offset / query.limit.unwrap_or(20) + 1;
            params.push(format!("page={}", page));
        }
        if let Some(sort) = query.sort {
            let sort_str = match sort {
                SkillSortOrder::Relevance => "relevance",
                SkillSortOrder::RecentlyUpdated => "updated",
                SkillSortOrder::Popular => "downloads",
                SkillSortOrder::Name => "name",
            };
            params.push(format!("sort={}", sort_str));
        }

        let url = if params.is_empty() {
            format!("{}/skills", self.base_url)
        } else {
            format!("{}/skills?{}", self.base_url, params.join("&"))
        };

        let response = self.request(&url).await?;
        
        if !response.status().is_success() {
            return Err(SkillProviderError::Network(format!(
                "Marketplace API returned status {}",
                response.status()
            )));
        }

        let list_response: MarketplaceSkillListResponse = response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

        let results: Vec<SkillSearchResult> = list_response
            .skills
            .into_iter()
            .filter_map(|s| {
                let downloads = s.downloads;
                let rating = s.rating;
                Self::to_manifest(s).ok().map(|manifest| SkillSearchResult {
                    manifest,
                    score: 50,
                    downloads: Some(downloads),
                    rating,
                })
            })
            .collect();

        // Update cache
        {
            let mut cache = self.search_cache.write().await;
            cache.insert(cache_key, CacheEntry::new(results.clone(), CACHE_TTL));
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

        let url = format!("{}/skills/{}", self.base_url, urlencoding::encode(id));
        let response = self.request(&url).await?;

        if response.status().as_u16() == 404 {
            return Err(SkillProviderError::NotFound(id.to_string()));
        }

        if !response.status().is_success() {
            return Err(SkillProviderError::Network(format!(
                "Marketplace API returned status {}",
                response.status()
            )));
        }

        let skill: MarketplaceSkill = response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

        let manifest = Self::to_manifest(skill)?;

        // Update cache
        {
            let mut cache = self.manifest_cache.write().await;
            cache.insert(id.to_string(), CacheEntry::new(manifest.clone(), CACHE_TTL));
        }

        Ok(manifest)
    }

    async fn get_content(&self, id: &str, version: &SkillVersion) -> Result<String, SkillProviderError> {
        let url = format!(
            "{}/skills/{}/content?version={}",
            self.base_url,
            urlencoding::encode(id),
            version
        );
        
        let response = self.request(&url).await?;

        if response.status().as_u16() == 404 {
            return Err(SkillProviderError::NotFound(format!("{}@{}", id, version)));
        }

        if !response.status().is_success() {
            return Err(SkillProviderError::Network(format!(
                "Marketplace API returned status {}",
                response.status()
            )));
        }

        response
            .text()
            .await
            .map_err(|e| SkillProviderError::Network(e.to_string()))
    }

    async fn list_versions(&self, id: &str) -> Result<Vec<SkillVersion>, SkillProviderError> {
        let url = format!("{}/skills/{}/versions", self.base_url, urlencoding::encode(id));
        let response = self.request(&url).await?;

        if response.status().as_u16() == 404 {
            return Err(SkillProviderError::NotFound(id.to_string()));
        }

        if !response.status().is_success() {
            return Err(SkillProviderError::Network(format!(
                "Marketplace API returned status {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct VersionsResponse {
            versions: Vec<String>,
        }

        let versions_resp: VersionsResponse = response
            .json()
            .await
            .map_err(|e| SkillProviderError::Parse(e.to_string()))?;

        versions_resp
            .versions
            .into_iter()
            .map(|v| {
                SkillVersion::parse(&v)
                    .ok_or_else(|| SkillProviderError::Parse(format!("Invalid version: {}", v)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_cache_key() {
        let query = SkillSearchQuery {
            query: Some("test".to_string()),
            category: Some("ai".to_string()),
            tags: vec!["llm".to_string()],
            ..Default::default()
        };
        
        let key = MarketplaceProvider::search_cache_key(&query);
        assert!(key.contains("test"));
        assert!(key.contains("ai"));
        assert!(key.contains("llm"));
    }

    #[test]
    fn test_default_provider() {
        let provider = MarketplaceProvider::new();
        assert_eq!(provider.base_url, DEFAULT_MARKETPLACE_URL);
        assert!(provider.api_key.is_none());
    }

    #[test]
    fn test_provider_with_api_key() {
        let provider = MarketplaceProvider::new().with_api_key("test-key".to_string());
        assert_eq!(provider.api_key, Some("test-key".to_string()));
    }
}
