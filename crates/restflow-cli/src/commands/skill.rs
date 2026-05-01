use anyhow::{Context, Result, bail};
use comfy_table::{Cell, Table};
use flate2::read::GzDecoder;
use futures::StreamExt as _;
use reqwest::{Client, RequestBuilder, header};
use restflow_build::{ArtifactKind, SkillArtifactMetadata, resolve_skill_binary_entry_path};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tar::Archive;
use tempfile::TempDir;
use zip::ZipArchive;

use crate::cli::SkillCommands;
use crate::commands::utils::{format_timestamp, preview_text, slugify};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::loader::git_source::GitSkillSource;
use restflow_core::loader::skill_folder::{
    INSTALL_SOURCE_METADATA_FILE, SkillFolderLoader, discover_skill_dirs,
};
use restflow_core::loader::skill_package::SkillPackageImporter;
use restflow_core::models::{Skill, SkillSource, StorageMode};
use restflow_core::paths;
use restflow_core::registry::{MarketplaceProvider, SkillRegistry, SkillSearchQuery};
use restflow_core::services::skills as skill_service;
use restflow_storage::RegistrySettings;
use serde_json::json;

const OFFICIAL_BINARY_SKILL_REPO: &str = "lhwzds/restflow-skills";
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const MAX_BINARY_SKILL_ARCHIVE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_BINARY_SKILL_CHECKSUM_BYTES: u64 = 16 * 1024;
const MAX_BINARY_SKILL_UNPACKED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BINARY_SKILL_ENTRIES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OfficialBinarySkillSource {
    skill_id: String,
    version: String,
    tag: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BinarySkillAssetSpec {
    asset_name: String,
    archive_kind: ReleaseArchiveKind,
    target: String,
}

#[derive(Debug, Serialize)]
struct InstallSourceMetadata<'a> {
    source: SkillSource,
    source_ref: &'a str,
}

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: SkillCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        SkillCommands::List => list_skills(executor, format).await,
        SkillCommands::Show { id } => show_skill(executor, &id, format).await,
        SkillCommands::Create { name } => create_skill(executor, &name, format).await,
        SkillCommands::Update {
            id,
            name,
            description,
            content,
            content_file,
            tags,
        } => {
            update_skill(
                executor,
                &id,
                name,
                description,
                content,
                content_file,
                tags,
                format,
            )
            .await
        }
        SkillCommands::Delete { id } => delete_skill(executor, &id, format).await,
        SkillCommands::Import { path } => import_skill(executor, &path, format).await,
        SkillCommands::Export { id, output } => export_skill(executor, &id, output, format).await,
        SkillCommands::Search { query } => search_skills(executor, &query, format).await,
        SkillCommands::Install {
            source,
            path,
            scope,
        } => install_skill(executor, &source, path.as_deref(), &scope, format).await,
    }
}

async fn list_skills(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let skills = executor.list_skills().await?;

    if format.is_json() {
        return print_json(&skills);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Updated", "Tags"]);

    for skill in skills {
        let tags = skill
            .tags
            .as_ref()
            .map(|values| values.join(", "))
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(skill.id),
            Cell::new(skill.name),
            Cell::new(format_timestamp(Some(skill.updated_at))),
            Cell::new(tags),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let skill = executor
        .get_skill(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("ID:          {}", skill.id);
    println!("Name:        {}", skill.name);
    println!(
        "Description: {}",
        skill.description.clone().unwrap_or_else(|| "-".to_string())
    );
    println!(
        "Tags:        {}",
        skill.tags.clone().unwrap_or_default().join(", ")
    );
    println!("Updated:     {}", format_timestamp(Some(skill.updated_at)));
    println!("\nContent:\n{}", skill.content);

    Ok(())
}

async fn create_skill(
    executor: Arc<dyn CommandExecutor>,
    name: &str,
    format: OutputFormat,
) -> Result<()> {
    let id = slugify(name);
    let content = format!("# {}\n", name);
    let skill = Skill::new(id.clone(), name.to_string(), None, None, content);

    executor.create_skill(skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill created: {} ({})", skill.name, skill.id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: Option<String>,
    description: Option<String>,
    content: Option<String>,
    content_file: Option<String>,
    tags: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut skill = executor
        .get_skill(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

    if let Some(n) = name {
        skill.name = n;
    }
    if let Some(d) = description {
        skill.description = Some(d);
    }
    if let Some(c) = content {
        skill.content = c;
    }
    if let Some(path) = content_file {
        let file_content = std::fs::read_to_string(&path)?;
        skill.content = file_content;
    }
    if let Some(t) = tags {
        skill.tags = Some(t.split(',').map(|s| s.trim().to_string()).collect());
    }

    skill.updated_at = chrono::Utc::now().timestamp_millis();
    executor.update_skill(id, skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill updated: {} ({})", skill.name, skill.id);
    Ok(())
}

async fn delete_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.delete_skill(id).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "id": id }));
    }

    println!("Skill deleted: {id}");
    Ok(())
}

async fn import_skill(
    executor: Arc<dyn CommandExecutor>,
    path: &str,
    format: OutputFormat,
) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let filename = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("imported-skill");
    let id = slugify(filename);

    let skill = skill_service::import_skill_from_markdown(&id, &content)?;
    executor.create_skill(skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill imported: {} ({})", skill.name, skill.id);
    Ok(())
}

async fn export_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    output: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let skill = executor
        .get_skill(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

    let markdown = skill_service::export_skill_to_markdown(&skill);
    let path = output.unwrap_or_else(|| format!("{}.md", id));
    std::fs::write(&path, markdown)?;

    if format.is_json() {
        return print_json(&json!({ "id": id, "output": path }));
    }

    println!("Exported to: {}", path);
    Ok(())
}

async fn search_skills(
    executor: Arc<dyn CommandExecutor>,
    query: &str,
    format: OutputFormat,
) -> Result<()> {
    let mut registry = SkillRegistry::with_defaults();
    let registry_defaults = executor
        .get_config()
        .await
        .map(|config| config.registry_defaults)
        .unwrap_or_else(|_| RegistrySettings::default());
    registry.add_provider(Arc::new(
        MarketplaceProvider::new()
            .with_cache_ttl_secs(registry_defaults.marketplace_cache_ttl_secs),
    ));

    let query = SkillSearchQuery {
        query: Some(query.to_string()),
        limit: Some(20),
        ..SkillSearchQuery::default()
    };

    let results = registry.search(&query).await;

    if format.is_json() {
        return print_json(&results);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Version", "Description"]);

    for result in results {
        let description = result
            .manifest
            .description
            .clone()
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(result.manifest.id),
            Cell::new(result.manifest.name),
            Cell::new(result.manifest.version.to_string()),
            Cell::new(preview_text(&description, 60)),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn install_skill(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    subpath: Option<&str>,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    if let Some(binary_source) = OfficialBinarySkillSource::parse(source) {
        if subpath.is_some() {
            bail!("--path is not supported for official binary skill releases");
        }
        return install_from_official_binary_release(executor, &binary_source, scope, format).await;
    }

    if is_git_source(source) {
        return install_from_git(executor, source, subpath, scope, format).await;
    }

    if is_skill_package(source) {
        return install_from_package(executor, source, scope, format).await;
    }

    let path = Path::new(source);
    if path.exists() {
        return install_from_local_path(executor, path, scope, format).await;
    }

    install_from_marketplace(executor, source, format).await
}

async fn install_from_official_binary_release(
    executor: Arc<dyn CommandExecutor>,
    source: &OfficialBinarySkillSource,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("Failed to initialize HTTP client")?;

    let release = fetch_github_release(&client, OFFICIAL_BINARY_SKILL_REPO, &source.tag).await?;
    let asset_spec = binary_skill_asset_spec_for_current_platform(&source.skill_id)?;
    let asset =
        select_github_asset(&release.assets, &asset_spec.asset_name).with_context(|| {
            let available = release
                .assets
                .iter()
                .map(|asset| asset.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Release {} does not contain asset {}. Available assets: {}",
                release.tag_name, asset_spec.asset_name, available
            )
        })?;
    ensure_github_asset_size(asset, MAX_BINARY_SKILL_ARCHIVE_BYTES)?;
    ensure_github_download_url(&asset.browser_download_url)?;

    let checksum_asset_name = format!("{}.sha256", asset_spec.asset_name);
    let checksum_asset = select_github_asset(&release.assets, &checksum_asset_name);
    if let Some(checksum_asset) = checksum_asset {
        ensure_github_asset_size(checksum_asset, MAX_BINARY_SKILL_CHECKSUM_BYTES)?;
        ensure_github_download_url(&checksum_asset.browser_download_url)?;
    }

    if !format.is_json() {
        println!(
            "Downloading binary skill {} from {} ({})",
            source.skill_id, release.tag_name, asset.name
        );
    }

    let archive_bytes = download_github_asset(
        &client,
        &asset.browser_download_url,
        MAX_BINARY_SKILL_ARCHIVE_BYTES,
    )
    .await
    .with_context(|| format!("Failed to download {}", asset.name))?;
    if let Some(checksum_asset) = checksum_asset {
        let checksum_bytes = download_github_asset(
            &client,
            &checksum_asset.browser_download_url,
            MAX_BINARY_SKILL_CHECKSUM_BYTES,
        )
        .await
        .with_context(|| format!("Failed to download {}", checksum_asset.name))?;
        verify_github_asset_sha256(&archive_bytes, &asset.name, &checksum_bytes)?;
    } else if !format.is_json() {
        println!(
            "Checksum asset {} not found; continuing with artifact validation",
            checksum_asset_name
        );
    }

    let (temp_dir, skill_dirs) =
        extract_binary_skill_archive(&archive_bytes, asset_spec.archive_kind, &source.skill_id)?;
    if skill_dirs.len() != 1 {
        bail!(
            "Official binary skill release must contain exactly one skill folder, found {}",
            skill_dirs.len()
        );
    }
    validate_official_binary_skill_dir(&skill_dirs[0], source, &asset_spec)?;
    let _guard = temp_dir;
    let source_ref = format!(
        "github_release:{repo}:{tag}:{asset}",
        repo = OFFICIAL_BINARY_SKILL_REPO,
        tag = source.tag,
        asset = asset.name
    );
    install_from_dirs_with_source_ref(
        executor,
        &source_ref,
        Some(&source_ref),
        &skill_dirs,
        scope,
        format,
    )
    .await
}

async fn install_from_marketplace(
    executor: Arc<dyn CommandExecutor>,
    name: &str,
    format: OutputFormat,
) -> Result<()> {
    if restflow_core::skill_files::systemskill_ids().any(|id| id == name) {
        bail!("Cannot install marketplace skill over systemskill: {name}");
    }

    let mut registry = SkillRegistry::with_defaults();
    let registry_defaults = executor
        .get_config()
        .await
        .map(|config| config.registry_defaults)
        .unwrap_or_else(|_| RegistrySettings::default());
    registry.add_provider(Arc::new(
        MarketplaceProvider::new()
            .with_cache_ttl_secs(registry_defaults.marketplace_cache_ttl_secs),
    ));

    let installed = registry.install(name).await?;

    let tags = if installed.manifest.keywords.is_empty() {
        None
    } else {
        Some(installed.manifest.keywords.clone())
    };

    let mut skill = Skill::new(
        installed.manifest.id.clone(),
        installed.manifest.name.clone(),
        installed.manifest.description.clone(),
        tags,
        installed.content.clone(),
    );
    skill.source = SkillSource::External;
    skill.source_ref = Some(format!(
        "marketplace:{}@{}",
        installed.manifest.id, installed.manifest.version
    ));

    let existing = executor.get_skill(&installed.manifest.id).await?;
    if let Some(mut existing_skill) = existing {
        existing_skill.source = skill.source;
        existing_skill.source_ref = skill.source_ref.clone();
        existing_skill.read_only = false;
        existing_skill.update(
            Some(skill.name),
            Some(skill.description),
            Some(skill.tags),
            Some(skill.content),
        );
        executor
            .update_skill(&installed.manifest.id, existing_skill)
            .await?;
    } else {
        executor.create_skill(skill.clone()).await?;
    }

    if format.is_json() {
        return print_json(&installed);
    }

    println!(
        "Skill installed from marketplace: {} ({})",
        installed.manifest.name, installed.manifest.id
    );
    Ok(())
}

async fn install_from_git(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    subpath: Option<&str>,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let (temp_dir, skill_dirs) = GitSkillSource::clone_and_discover(source, subpath).await?;
    let _guard = temp_dir;
    install_from_dirs(executor, source, &skill_dirs, scope, format).await
}

async fn install_from_package(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let path = Path::new(source);
    let (temp_dir, skill_dirs) = SkillPackageImporter::import(path)?;
    let _guard = temp_dir;
    install_from_dirs(executor, source, &skill_dirs, scope, format).await
}

async fn install_from_local_path(
    executor: Arc<dyn CommandExecutor>,
    path: &Path,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let skill_dirs = discover_skill_dirs(path)?;
    install_from_dirs(
        executor,
        path.to_string_lossy().as_ref(),
        &skill_dirs,
        scope,
        format,
    )
    .await
}

async fn install_from_dirs(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    skill_dirs: &[PathBuf],
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    install_from_dirs_with_source_ref(executor, source, None, skill_dirs, scope, format).await
}

async fn install_from_dirs_with_source_ref(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    source_ref_override: Option<&str>,
    skill_dirs: &[PathBuf],
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    if skill_dirs.is_empty() {
        return Err(anyhow::anyhow!("No skills found in source: {}", source));
    }

    let target_base = resolve_scope_dir(scope)?;
    let loader = SkillFolderLoader::new(PathBuf::new());

    let mut installed_ids = Vec::new();
    for skill_dir in skill_dirs {
        let mut skill = loader.load_skill_folder(skill_dir)?;
        if restflow_core::skill_files::systemskill_ids().any(|id| id == skill.id.as_str()) {
            bail!("Cannot install skill over systemskill: {}", skill.id);
        }
        let target_dir = target_base.join(&skill.id);
        copy_skill_dir(skill_dir, &target_dir)?;

        let skill_id = skill.id.clone();
        skill.folder_path = Some(target_dir.to_string_lossy().to_string());
        skill.storage_mode = StorageMode::FileSystemOnly;
        if source_is_external_install(source) {
            skill.source = SkillSource::External;
            skill.source_ref = Some(
                source_ref_override
                    .map(str::to_string)
                    .unwrap_or_else(|| source.to_string()),
            );
            if let Some(source_ref) = skill.source_ref.as_deref() {
                write_install_source_metadata(&target_dir, source_ref)?;
            }
        } else {
            skill.source = SkillSource::User;
            skill.source_ref = None;
            remove_install_source_metadata(&target_dir)?;
        }
        ensure_binary_skill_executable(&target_dir)?;
        upsert_skill(&executor, skill).await?;
        installed_ids.push(skill_id);
    }

    if format.is_json() {
        return print_json(&json!({
            "source": source,
            "scope": scope,
            "installed": installed_ids,
        }));
    }

    println!(
        "Installed {} skill(s) from {} into {} scope",
        installed_ids.len(),
        source,
        scope
    );
    Ok(())
}

fn source_is_external_install(source: &str) -> bool {
    is_git_source(source) || is_skill_package(source) || source.starts_with("github_release:")
}

impl OfficialBinarySkillSource {
    fn parse(source: &str) -> Option<Self> {
        let rest = source.strip_prefix("github:")?;
        let rest = rest.strip_prefix(OFFICIAL_BINARY_SKILL_REPO)?;
        let skill_and_version = rest.strip_prefix('/')?;
        let (skill_id, version) = skill_and_version.rsplit_once('@')?;
        if skill_id.is_empty() || version.is_empty() || skill_id.contains('/') {
            return None;
        }
        Some(Self {
            skill_id: skill_id.to_string(),
            version: version.to_string(),
            tag: format!("{skill_id}@{version}"),
        })
    }
}

async fn fetch_github_release(client: &Client, repo: &str, tag: &str) -> Result<GitHubRelease> {
    let release_url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let response = with_github_headers(client.get(release_url))
        .send()
        .await
        .context("Failed to request GitHub release metadata")?
        .error_for_status()
        .context("GitHub API returned an error for release metadata")?;

    response
        .json::<GitHubRelease>()
        .await
        .context("Failed to decode GitHub release metadata")
}

async fn download_github_asset(client: &Client, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
    ensure_github_download_url(url)?;
    let response = with_github_download_headers(client.get(url))
        .send()
        .await
        .context("Failed to request GitHub release asset")?
        .error_for_status()
        .context("GitHub returned an error for release asset download")?;

    if let Some(content_length) = response.content_length()
        && content_length > max_bytes
    {
        bail!(
            "GitHub release asset is too large: {} bytes (limit: {} bytes)",
            content_length,
            max_bytes
        );
    }

    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read GitHub release asset bytes")?;
        let next_len = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| anyhow::anyhow!("GitHub release asset size overflow"))?;
        if next_len as u64 > max_bytes {
            bail!(
                "GitHub release asset is too large: {} bytes (limit: {} bytes)",
                next_len,
                max_bytes
            );
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn with_github_headers(request: RequestBuilder) -> RequestBuilder {
    let request = request
        .header(
            header::USER_AGENT,
            format!("restflow-cli/{}", env!("CARGO_PKG_VERSION")),
        )
        .header(header::ACCEPT, GITHUB_API_ACCEPT);

    match github_token() {
        Some(token) => request.bearer_auth(token),
        None => request,
    }
}

fn with_github_download_headers(request: RequestBuilder) -> RequestBuilder {
    request
        .header(
            header::USER_AGENT,
            format!("restflow-cli/{}", env!("CARGO_PKG_VERSION")),
        )
        .header(header::ACCEPT, "application/octet-stream")
}

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok())
        .filter(|token| !token.trim().is_empty())
}

fn select_github_asset<'a>(assets: &'a [GitHubAsset], asset_name: &str) -> Option<&'a GitHubAsset> {
    assets.iter().find(|asset| asset.name == asset_name)
}

fn ensure_github_asset_size(asset: &GitHubAsset, max_bytes: u64) -> Result<()> {
    if let Some(size) = asset.size
        && size > max_bytes
    {
        bail!(
            "GitHub release asset {} is too large: {} bytes (limit: {} bytes)",
            asset.name,
            size,
            max_bytes
        );
    }
    Ok(())
}

fn ensure_github_download_url(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url)
        .with_context(|| format!("Invalid GitHub release asset URL: {url}"))?;
    if parsed.scheme() != "https" {
        bail!("GitHub release asset URL must use HTTPS: {url}");
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("GitHub release asset URL is missing a host: {url}"))?;
    let allowed = host == "github.com"
        || host == "objects.githubusercontent.com"
        || host.ends_with(".githubusercontent.com");
    if !allowed {
        bail!("Refusing to download release asset from non-GitHub host: {host}");
    }
    Ok(())
}

fn verify_github_asset_sha256(
    archive_bytes: &[u8],
    asset_name: &str,
    checksum_bytes: &[u8],
) -> Result<()> {
    let checksum_text = std::str::from_utf8(checksum_bytes)
        .context("GitHub release checksum asset is not valid UTF-8")?;
    let expected = checksum_text
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("GitHub release checksum asset is empty"))?;
    if expected.len() != 64 || !expected.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("GitHub release checksum asset does not contain a valid SHA-256 digest");
    }

    let actual = format!("{:x}", Sha256::digest(archive_bytes));
    if actual != expected.to_ascii_lowercase() {
        bail!(
            "SHA-256 mismatch for {}: expected {}, got {}",
            asset_name,
            expected,
            actual
        );
    }
    Ok(())
}

fn binary_skill_asset_spec_for_current_platform(skill_id: &str) -> Result<BinarySkillAssetSpec> {
    binary_skill_asset_spec_for(std::env::consts::OS, std::env::consts::ARCH, skill_id).ok_or_else(
        || {
            anyhow::anyhow!(
                "Unsupported binary skill platform: {}-{}",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        },
    )
}

fn binary_skill_asset_spec_for(
    os: &str,
    arch: &str,
    skill_id: &str,
) -> Option<BinarySkillAssetSpec> {
    match (os, arch) {
        ("macos", "aarch64") => Some(BinarySkillAssetSpec {
            asset_name: format!("{skill_id}-aarch64-macos.tar.gz"),
            archive_kind: ReleaseArchiveKind::TarGz,
            target: "aarch64-macos".to_string(),
        }),
        ("linux", "x86_64") => Some(BinarySkillAssetSpec {
            asset_name: format!("{skill_id}-x86_64-linux.tar.gz"),
            archive_kind: ReleaseArchiveKind::TarGz,
            target: "x86_64-linux".to_string(),
        }),
        ("windows", "x86_64") => Some(BinarySkillAssetSpec {
            asset_name: format!("{skill_id}-x86_64-windows.zip"),
            archive_kind: ReleaseArchiveKind::Zip,
            target: "x86_64-windows".to_string(),
        }),
        _ => None,
    }
}

fn extract_binary_skill_archive(
    archive_bytes: &[u8],
    archive_kind: ReleaseArchiveKind,
    skill_id: &str,
) -> Result<(TempDir, Vec<PathBuf>)> {
    let temp_dir = TempDir::new().context("Failed to create binary skill temp directory")?;
    let target_dir = temp_dir.path().join(skill_id);
    std::fs::create_dir_all(&target_dir).with_context(|| {
        format!(
            "Failed to create binary skill extraction directory {}",
            target_dir.display()
        )
    })?;
    match archive_kind {
        ReleaseArchiveKind::TarGz => {
            let gz = GzDecoder::new(std::io::Cursor::new(archive_bytes));
            let mut archive = Archive::new(gz);
            unpack_tar_gz_archive(&mut archive, &target_dir)?;
        }
        ReleaseArchiveKind::Zip => {
            let cursor = std::io::Cursor::new(archive_bytes);
            let mut archive =
                ZipArchive::new(cursor).context("Failed to open binary skill zip archive")?;
            unpack_zip_archive(&mut archive, &target_dir)?;
        }
    }

    let skill_dirs = discover_skill_dirs(temp_dir.path())?;
    Ok((temp_dir, skill_dirs))
}

fn unpack_tar_gz_archive<R: std::io::Read>(
    archive: &mut Archive<R>,
    target_dir: &Path,
) -> Result<()> {
    let mut entry_count = 0usize;
    let mut unpacked_bytes = 0u64;

    for entry in archive
        .entries()
        .context("Failed to read binary skill tar.gz archive entries")?
    {
        let mut entry = entry.context("Failed to read binary skill tar.gz archive entry")?;
        entry_count += 1;
        ensure_binary_skill_entry_count(entry_count)?;

        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            bail!("Binary skill tar.gz archive contains a non-file entry");
        }

        let entry_path = entry
            .path()
            .context("Failed to read binary skill tar.gz archive entry path")?
            .into_owned();
        ensure_relative_archive_path(&entry_path)?;

        let entry_size = if entry_type.is_file() {
            entry
                .header()
                .size()
                .context("Failed to read binary skill tar.gz entry size")?
        } else {
            0
        };
        unpacked_bytes = unpacked_bytes
            .checked_add(entry_size)
            .ok_or_else(|| anyhow::anyhow!("Binary skill archive size overflow"))?;
        ensure_binary_skill_unpacked_size(unpacked_bytes)?;

        let unpacked = entry
            .unpack_in(target_dir)
            .context("Failed to extract binary skill tar.gz archive entry")?;
        if !unpacked {
            bail!(
                "Binary skill tar.gz archive entry refused extraction outside target: {}",
                entry_path.display()
            );
        }
    }

    Ok(())
}

fn unpack_zip_archive<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    target_dir: &Path,
) -> Result<()> {
    let mut entry_count = 0usize;
    let mut unpacked_bytes = 0u64;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .context("Failed to read binary skill zip archive entry")?;
        entry_count += 1;
        ensure_binary_skill_entry_count(entry_count)?;

        let entry_path = entry
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("Binary skill zip archive contains an unsafe path"))?
            .to_path_buf();
        ensure_relative_archive_path(&entry_path)?;

        #[cfg(unix)]
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            bail!("Binary skill zip archive contains a symlink entry");
        }

        if entry.is_dir() {
            std::fs::create_dir_all(target_dir.join(&entry_path)).with_context(|| {
                format!(
                    "Failed to create binary skill zip directory {}",
                    entry_path.display()
                )
            })?;
            continue;
        }

        unpacked_bytes = unpacked_bytes
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("Binary skill archive size overflow"))?;
        ensure_binary_skill_unpacked_size(unpacked_bytes)?;

        let output_path = target_dir.join(&entry_path);
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create binary skill zip parent directory {}",
                    parent.display()
                )
            })?;
        }
        let mut output = File::create(&output_path).with_context(|| {
            format!(
                "Failed to create binary skill zip output {}",
                output_path.display()
            )
        })?;
        std::io::copy(&mut entry, &mut output).with_context(|| {
            format!(
                "Failed to extract binary skill zip entry {}",
                entry_path.display()
            )
        })?;
    }

    Ok(())
}

fn ensure_binary_skill_entry_count(entry_count: usize) -> Result<()> {
    if entry_count > MAX_BINARY_SKILL_ENTRIES {
        bail!(
            "Binary skill archive has too many entries: {} (limit: {})",
            entry_count,
            MAX_BINARY_SKILL_ENTRIES
        );
    }
    Ok(())
}

fn ensure_binary_skill_unpacked_size(unpacked_bytes: u64) -> Result<()> {
    if unpacked_bytes > MAX_BINARY_SKILL_UNPACKED_BYTES {
        bail!(
            "Binary skill archive unpacked size is too large: {} bytes (limit: {} bytes)",
            unpacked_bytes,
            MAX_BINARY_SKILL_UNPACKED_BYTES
        );
    }
    Ok(())
}

fn ensure_relative_archive_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        bail!("Binary skill archive entry path must be relative");
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        bail!(
            "Binary skill archive entry path must not contain traversal or root components: {}",
            path.display()
        );
    }
    Ok(())
}

fn validate_official_binary_skill_dir(
    skill_dir: &Path,
    source: &OfficialBinarySkillSource,
    asset_spec: &BinarySkillAssetSpec,
) -> Result<()> {
    let artifact = read_binary_skill_artifact(skill_dir)?;
    if !matches!(&artifact.kind, ArtifactKind::SkillBinary) {
        bail!("Official binary skill artifact kind must be skill_binary");
    }
    if artifact.id != source.skill_id {
        bail!(
            "Official binary skill artifact id mismatch: expected {}, got {}",
            source.skill_id,
            artifact.id
        );
    }
    if artifact.version != source.version {
        bail!(
            "Official binary skill artifact version mismatch: expected {}, got {}",
            source.version,
            artifact.version
        );
    }
    let download = artifact.download.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Official binary skill artifact is missing download metadata")
    })?;
    if download.repo != OFFICIAL_BINARY_SKILL_REPO {
        bail!(
            "Official binary skill artifact repo mismatch: expected {}, got {}",
            OFFICIAL_BINARY_SKILL_REPO,
            download.repo
        );
    }
    if download.tag != source.tag {
        bail!(
            "Official binary skill artifact tag mismatch: expected {}, got {}",
            source.tag,
            download.tag
        );
    }
    if download.asset != asset_spec.asset_name {
        bail!(
            "Official binary skill artifact asset mismatch: expected {}, got {}",
            asset_spec.asset_name,
            download.asset
        );
    }
    if artifact.target != asset_spec.target {
        bail!(
            "Official binary skill artifact target mismatch: expected {}, got {}",
            asset_spec.target,
            artifact.target
        );
    }

    let entry_binary = resolve_skill_binary_entry_path(skill_dir, &artifact)
        .with_context(|| "Invalid official binary skill entry_binary")?;
    ensure_executable_bit(&entry_binary)?;
    write_binary_skill_artifact(skill_dir, &artifact)
}

fn ensure_binary_skill_executable(skill_dir: &Path) -> Result<()> {
    let artifact_path = skill_dir.join("artifact.json");
    if !artifact_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&artifact_path).with_context(|| {
        format!(
            "Failed to read artifact file at {}",
            artifact_path.display()
        )
    })?;
    let raw: serde_json::Value = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse artifact file at {}",
            artifact_path.display()
        )
    })?;
    if raw.get("kind").and_then(|value| value.as_str()) != Some("skill_binary") {
        return Ok(());
    };
    let metadata = read_binary_skill_artifact(skill_dir)?;
    let entry_binary = resolve_skill_binary_entry_path(skill_dir, &metadata)?;
    ensure_executable_bit(&entry_binary)
}

fn read_binary_skill_artifact(skill_dir: &Path) -> Result<SkillArtifactMetadata> {
    let artifact_path = skill_dir.join("artifact.json");
    if !artifact_path.exists() {
        bail!("Binary skill artifact is missing artifact.json");
    }
    let content = std::fs::read_to_string(&artifact_path).with_context(|| {
        format!(
            "Failed to read artifact file at {}",
            artifact_path.display()
        )
    })?;
    let mut value: serde_json::Value = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse artifact file at {}",
            artifact_path.display()
        )
    })?;
    if let Some(object) = value.as_object_mut()
        && !object.contains_key("name")
        && let Some(id) = object.get("id").and_then(|value| value.as_str())
    {
        object.insert(
            "name".to_string(),
            serde_json::Value::String(id.to_string()),
        );
    }
    serde_json::from_value(value).with_context(|| {
        format!(
            "Failed to decode artifact metadata at {}",
            artifact_path.display()
        )
    })
}

fn write_binary_skill_artifact(skill_dir: &Path, artifact: &SkillArtifactMetadata) -> Result<()> {
    let artifact_path = skill_dir.join("artifact.json");
    std::fs::write(&artifact_path, serde_json::to_string_pretty(artifact)?).with_context(|| {
        format!(
            "Failed to write normalized artifact file at {}",
            artifact_path.display()
        )
    })
}

fn write_install_source_metadata(target_dir: &Path, source_ref: &str) -> Result<()> {
    let metadata = InstallSourceMetadata {
        source: SkillSource::External,
        source_ref,
    };
    let path = target_dir.join(INSTALL_SOURCE_METADATA_FILE);
    std::fs::write(&path, serde_json::to_string_pretty(&metadata)?).with_context(|| {
        format!(
            "Failed to write install source metadata at {}",
            path.display()
        )
    })
}

fn remove_install_source_metadata(target_dir: &Path) -> Result<()> {
    let path = target_dir.join(INSTALL_SOURCE_METADATA_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| {
            format!(
                "Failed to remove install source metadata at {}",
                path.display()
            )
        }),
    }
}

#[cfg(unix)]
fn ensure_executable_bit(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if !path.exists() {
        return Ok(());
    }
    let metadata =
        std::fs::metadata(path).with_context(|| format!("Failed to stat {}", path.display()))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("Failed to mark {} executable", path.display()))
}

#[cfg(not(unix))]
fn ensure_executable_bit(_path: &Path) -> Result<()> {
    Ok(())
}

async fn upsert_skill(executor: &Arc<dyn CommandExecutor>, mut skill: Skill) -> Result<()> {
    let existing = executor.get_skill(&skill.id).await?;
    if let Some(existing_skill) = existing {
        let skill_id = skill.id.clone();
        skill.created_at = existing_skill.created_at;
        skill.updated_at = chrono::Utc::now().timestamp_millis();
        executor.update_skill(&skill_id, skill).await?;
    } else {
        executor.create_skill(skill).await?;
    }
    Ok(())
}

fn resolve_scope_dir(scope: &str) -> Result<PathBuf> {
    match scope {
        "user" => paths::user_skills_dir(),
        _ => Err(anyhow::anyhow!("Invalid scope: {}", scope)),
    }
}

fn is_git_source(source: &str) -> bool {
    source.starts_with("https://")
        || source.starts_with("http://")
        || source.ends_with(".git")
        || source.starts_with("git@")
}

fn is_skill_package(source: &str) -> bool {
    source.ends_with(".skill") || source.ends_with(".zip")
}

fn copy_skill_dir(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;

    for entry in walkdir::WalkDir::new(source).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(source).unwrap_or(path);
        let dest = target.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &dest)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    #[test]
    fn test_git_source_detection() {
        assert!(is_git_source("https://github.com/org/repo.git"));
        assert!(is_git_source("git@github.com:org/repo.git"));
        assert!(!is_git_source("skills/my-local-skill"));
    }

    #[test]
    fn test_skill_package_detection() {
        assert!(is_skill_package("/tmp/example.skill"));
        assert!(is_skill_package("/tmp/example.zip"));
        assert!(!is_skill_package("/tmp/example"));
    }

    #[test]
    fn test_official_binary_skill_source_parse() {
        let source =
            OfficialBinarySkillSource::parse("github:lhwzds/restflow-skills/cdp-browser@0.1.2")
                .expect("official binary skill source");

        assert_eq!(source.skill_id, "cdp-browser");
        assert_eq!(source.version, "0.1.2");
        assert_eq!(source.tag, "cdp-browser@0.1.2");
    }

    #[test]
    fn test_official_binary_skill_source_rejects_other_repos() {
        assert!(
            OfficialBinarySkillSource::parse("github:someone/restflow-skills/cdp-browser@0.1.2")
                .is_none()
        );
        assert!(
            OfficialBinarySkillSource::parse("github:lhwzds/other/cdp-browser@0.1.2").is_none()
        );
        assert!(
            OfficialBinarySkillSource::parse("https://github.com/lhwzds/restflow-skills.git")
                .is_none()
        );
    }

    #[test]
    fn test_official_binary_skill_source_requires_version() {
        assert!(
            OfficialBinarySkillSource::parse("github:lhwzds/restflow-skills/cdp-browser").is_none()
        );
    }

    #[test]
    fn test_binary_skill_asset_spec_for_supported_platforms() {
        let mac = binary_skill_asset_spec_for("macos", "aarch64", "cdp-browser").unwrap();
        assert_eq!(mac.asset_name, "cdp-browser-aarch64-macos.tar.gz");
        assert_eq!(mac.archive_kind, ReleaseArchiveKind::TarGz);

        let linux = binary_skill_asset_spec_for("linux", "x86_64", "regex-finder").unwrap();
        assert_eq!(linux.asset_name, "regex-finder-x86_64-linux.tar.gz");
        assert_eq!(linux.archive_kind, ReleaseArchiveKind::TarGz);

        let windows = binary_skill_asset_spec_for("windows", "x86_64", "cdp-browser").unwrap();
        assert_eq!(windows.asset_name, "cdp-browser-x86_64-windows.zip");
        assert_eq!(windows.archive_kind, ReleaseArchiveKind::Zip);
    }

    #[test]
    fn test_binary_skill_asset_spec_rejects_unsupported_platforms() {
        assert!(binary_skill_asset_spec_for("macos", "x86_64", "cdp-browser").is_none());
        assert!(binary_skill_asset_spec_for("linux", "aarch64", "cdp-browser").is_none());
    }

    #[test]
    fn test_external_source_detection_includes_github_release() {
        assert!(source_is_external_install(
            "github_release:lhwzds/restflow-skills:cdp-browser@0.1.2:cdp-browser-aarch64-macos.tar.gz"
        ));
    }

    #[test]
    fn test_extract_binary_skill_tarball_discovers_skill_folder() -> Result<()> {
        let skill_md = br#"---
name: Test Binary Skill
---
Use the bundled binary.
"#;
        let mut archive_bytes = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut archive_bytes, flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(skill_md.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive.append_data(&mut header, "SKILL.md", &skill_md[..])?;
            archive.finish()?;
        }

        let (_temp_dir, skill_dirs) =
            extract_binary_skill_archive(&archive_bytes, ReleaseArchiveKind::TarGz, "test-skill")?;

        assert_eq!(skill_dirs.len(), 1);
        assert!(skill_dirs[0].join("SKILL.md").exists());
        assert_eq!(skill_dirs[0].file_name().unwrap(), "test-skill");
        Ok(())
    }

    #[test]
    fn test_verify_github_asset_sha256_accepts_matching_digest() -> Result<()> {
        let archive_bytes = b"archive bytes";
        let digest = format!("{:x}  test.tar.gz\n", Sha256::digest(archive_bytes));

        verify_github_asset_sha256(archive_bytes, "test.tar.gz", digest.as_bytes())?;
        Ok(())
    }

    #[test]
    fn test_verify_github_asset_sha256_rejects_mismatch() {
        let checksum = format!("{}  test.tar.gz", "0".repeat(64));
        let err = verify_github_asset_sha256(b"archive bytes", "test.tar.gz", checksum.as_bytes())
            .unwrap_err();
        assert!(err.to_string().contains("SHA-256 mismatch"));
    }

    #[test]
    fn test_resolve_skill_binary_entry_path_rejects_traversal() -> Result<()> {
        let temp = tempdir()?;
        let skill_dir = temp.path().join("tool-skill");
        std::fs::create_dir_all(&skill_dir)?;
        let artifact: SkillArtifactMetadata = serde_json::from_value(serde_json::json!({
            "kind": "skill_binary",
            "id": "tool-skill",
            "name": "Tool Skill",
            "version": "0.1.0",
            "entry_binary": "../tool"
        }))?;

        let err = resolve_skill_binary_entry_path(&skill_dir, &artifact).unwrap_err();
        assert!(err.to_string().contains("traversal"));
        Ok(())
    }

    #[test]
    fn test_validate_official_binary_skill_dir_accepts_matching_artifact() -> Result<()> {
        let temp = tempdir()?;
        let skill_dir = temp.path().join("cdp-browser");
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), "# CDP Browser\n")?;
        std::fs::write(skill_dir.join("cdp-browser"), "#!/bin/sh\n")?;
        std::fs::write(
            skill_dir.join("artifact.json"),
            serde_json::json!({
                "schema_version": 1,
                "kind": "skill_binary",
                "id": "cdp-browser",
                "name": "CDP Browser",
                "version": "0.1.2",
                "target": "aarch64-macos",
                "entry_binary": "cdp-browser",
                "download": {
                    "repo": "lhwzds/restflow-skills",
                    "tag": "cdp-browser@0.1.2",
                    "asset": "cdp-browser-aarch64-macos.tar.gz",
                    "sha256": null
                }
            })
            .to_string(),
        )?;
        let source =
            OfficialBinarySkillSource::parse("github:lhwzds/restflow-skills/cdp-browser@0.1.2")
                .unwrap();

        validate_official_binary_skill_dir(
            &skill_dir,
            &source,
            &BinarySkillAssetSpec {
                asset_name: "cdp-browser-aarch64-macos.tar.gz".to_string(),
                archive_kind: ReleaseArchiveKind::TarGz,
                target: "aarch64-macos".to_string(),
            },
        )?;
        Ok(())
    }

    #[test]
    fn test_validate_official_binary_skill_dir_rejects_bad_entry_binary() -> Result<()> {
        let temp = tempdir()?;
        let skill_dir = temp.path().join("cdp-browser");
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(skill_dir.join("SKILL.md"), "# CDP Browser\n")?;
        std::fs::write(
            skill_dir.join("artifact.json"),
            serde_json::json!({
                "schema_version": 1,
                "kind": "skill_binary",
                "id": "cdp-browser",
                "name": "CDP Browser",
                "version": "0.1.2",
                "target": "aarch64-macos",
                "entry_binary": "../cdp-browser",
                "download": {
                    "repo": "lhwzds/restflow-skills",
                    "tag": "cdp-browser@0.1.2",
                    "asset": "cdp-browser-aarch64-macos.tar.gz",
                    "sha256": null
                }
            })
            .to_string(),
        )?;
        let source =
            OfficialBinarySkillSource::parse("github:lhwzds/restflow-skills/cdp-browser@0.1.2")
                .unwrap();

        let err = validate_official_binary_skill_dir(
            &skill_dir,
            &source,
            &BinarySkillAssetSpec {
                asset_name: "cdp-browser-aarch64-macos.tar.gz".to_string(),
                archive_kind: ReleaseArchiveKind::TarGz,
                target: "aarch64-macos".to_string(),
            },
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("Invalid official binary skill entry_binary")
        );
        Ok(())
    }

    #[test]
    fn test_read_binary_skill_artifact_defaults_legacy_missing_name() -> Result<()> {
        let temp = tempdir()?;
        std::fs::write(
            temp.path().join("artifact.json"),
            serde_json::json!({
                "kind": "skill_binary",
                "id": "cdp-browser",
                "version": "0.1.2",
                "entry_binary": "cdp-browser",
                "target": "aarch64-macos",
                "download": {
                    "repo": "lhwzds/restflow-skills",
                    "tag": "cdp-browser@0.1.2",
                    "asset": "cdp-browser-aarch64-macos.tar.gz"
                }
            })
            .to_string(),
        )?;

        let artifact = read_binary_skill_artifact(temp.path())?;

        assert_eq!(artifact.name, "cdp-browser");
        assert_eq!(artifact.id, "cdp-browser");
        Ok(())
    }

    #[test]
    fn test_remove_install_source_metadata_ignores_missing_file() -> Result<()> {
        let temp = tempdir()?;

        remove_install_source_metadata(temp.path())?;

        Ok(())
    }

    #[test]
    fn test_remove_install_source_metadata_deletes_file() -> Result<()> {
        let temp = tempdir()?;
        let metadata_path = temp.path().join(INSTALL_SOURCE_METADATA_FILE);
        std::fs::write(&metadata_path, "{}")?;

        remove_install_source_metadata(temp.path())?;

        assert!(!metadata_path.exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_executable_bit_only_adds_execute_bits() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir()?;
        let binary = temp.path().join("tool");
        std::fs::write(&binary, "#!/bin/sh\n")?;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o600))?;

        ensure_executable_bit(&binary)?;

        let mode = std::fs::metadata(binary)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o711);
        Ok(())
    }

    #[test]
    fn test_copy_skill_dir_recursive() -> Result<()> {
        let source_root = tempdir()?;
        let target_root = tempdir()?;

        let source = source_root.path().join("source");
        let target = target_root.path().join("target");

        std::fs::create_dir_all(source.join("nested"))?;
        std::fs::write(source.join("SKILL.md"), "# Demo Skill\n")?;
        std::fs::write(source.join("nested").join("config.json"), "{\"k\":1}")?;

        copy_skill_dir(&source, &target)?;

        assert!(target.join("SKILL.md").exists());
        assert!(target.join("nested").join("config.json").exists());
        assert_eq!(
            std::fs::read_to_string(target.join("SKILL.md"))?,
            "# Demo Skill\n"
        );

        Ok(())
    }

    #[test]
    fn test_resolve_scope_dir_rejects_workspace_scope() {
        let _lock = env_lock();
        let temp = tempdir().unwrap();
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let user_dir = resolve_scope_dir("user").unwrap();
        assert!(user_dir.ends_with("skills"));

        let workspace_err = resolve_scope_dir("workspace").unwrap_err();
        assert!(workspace_err.to_string().contains("Invalid scope"));

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }
}
