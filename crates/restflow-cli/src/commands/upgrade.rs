use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::{Client, RequestBuilder, header};
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tar::Archive;
use zip::ZipArchive;

use crate::cli::UpgradeArgs;
use crate::output::{OutputFormat, json::print_json};

const DEFAULT_REPO: &str = "lhwzds/restflow";
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, Copy)]
struct PlatformSpec {
    asset_name: &'static str,
    binary_name: &'static str,
    archive_kind: ArchiveKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VersionRelation {
    LatestNewer,
    CurrentNewer,
    Equal,
    Unknown,
}

pub async fn run(args: UpgradeArgs, format: OutputFormat) -> Result<()> {
    let repo = std::env::var("RESTFLOW_UPGRADE_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
    let current_version = env!("CARGO_PKG_VERSION");

    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("Failed to initialize HTTP client")?;

    let release = fetch_latest_release(&client, &repo).await?;
    let latest_version = normalize_release_version(&release.tag_name);
    let relation = compare_versions(current_version, latest_version);

    if !args.force && relation == VersionRelation::Equal {
        return print_skip_result(
            format,
            "up_to_date",
            current_version,
            &release.tag_name,
            None,
            "Current version is already the latest release",
        );
    }

    if !args.force && relation == VersionRelation::CurrentNewer {
        return print_skip_result(
            format,
            "current_newer",
            current_version,
            &release.tag_name,
            None,
            "Current version is newer than the latest published release",
        );
    }

    let platform = detect_platform_spec()?;
    let asset = select_asset(&release.assets, platform.asset_name).with_context(|| {
        let available: Vec<_> = release
            .assets
            .iter()
            .map(|item| item.name.clone())
            .collect();
        format!(
            "Release {} does not contain asset {}. Available assets: {}",
            release.tag_name,
            platform.asset_name,
            available.join(", ")
        )
    })?;

    if !format.is_json() {
        println!(
            "Latest release: {} ({})",
            release.tag_name, platform.asset_name
        );
        println!("Downloading asset...");
    }

    let archive_bytes = download_asset(&client, &asset.browser_download_url)
        .await
        .with_context(|| format!("Failed to download {}", asset.name))?;

    if !format.is_json() {
        println!("Extracting binary...");
    }

    let binary_bytes = extract_binary(&archive_bytes, platform.archive_kind, platform.binary_name)?;

    let install_path = install_path()?;
    install_binary(&binary_bytes, &install_path)?;
    let alias_updated = ensure_rf_alias(&install_path)?;
    let codesigned = try_codesign(&install_path);

    if format.is_json() {
        return print_json(&json!({
            "status": "upgraded",
            "current_version": current_version,
            "latest_tag": release.tag_name,
            "installed_path": install_path,
            "rf_alias_updated": alias_updated,
            "codesigned": codesigned,
            "forced": args.force,
        }));
    }

    println!("Upgrade complete.");
    println!("Version: {current_version} -> {latest_version}");
    println!("Installed: {}", install_path.display());
    if alias_updated {
        println!("Alias: rf -> restflow");
    }
    if cfg!(target_os = "macos") && !codesigned {
        println!("Warning: codesign step failed; binary may trigger macOS verification warnings.");
    }

    Ok(())
}

async fn fetch_latest_release(client: &Client, repo: &str) -> Result<GitHubRelease> {
    let release_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let response = with_github_headers(client.get(release_url))
        .send()
        .await
        .context("Failed to request latest release metadata")?
        .error_for_status()
        .context("GitHub API returned an error for latest release metadata")?;

    response
        .json::<GitHubRelease>()
        .await
        .context("Failed to decode latest release metadata")
}

async fn download_asset(client: &Client, url: &str) -> Result<Vec<u8>> {
    let response = with_github_headers(client.get(url))
        .send()
        .await
        .context("Failed to request release asset")?
        .error_for_status()
        .context("GitHub returned an error for release asset download")?;

    let bytes = response
        .bytes()
        .await
        .context("Failed to read release asset bytes")?;
    Ok(bytes.to_vec())
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

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok())
        .filter(|token| !token.trim().is_empty())
}

fn select_asset<'a>(assets: &'a [GitHubAsset], asset_name: &str) -> Option<&'a GitHubAsset> {
    assets.iter().find(|asset| asset.name == asset_name)
}

fn extract_binary(
    archive_bytes: &[u8],
    archive_kind: ArchiveKind,
    binary_name: &str,
) -> Result<Vec<u8>> {
    match archive_kind {
        ArchiveKind::TarGz => extract_from_tar_gz(archive_bytes, binary_name),
        ArchiveKind::Zip => extract_from_zip(archive_bytes, binary_name),
    }
}

fn extract_from_tar_gz(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    let gz = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = Archive::new(gz);

    for entry in archive.entries().context("Failed to list tar entries")? {
        let mut entry = entry.context("Failed to read tar entry")?;
        let path = entry.path().context("Failed to read tar entry path")?;
        let file_name = path.file_name().and_then(|name| name.to_str());
        if file_name == Some(binary_name) {
            let mut binary = Vec::new();
            entry
                .read_to_end(&mut binary)
                .context("Failed to extract binary from tar archive")?;
            return Ok(binary);
        }
    }

    bail!("Binary {} not found in tar.gz archive", binary_name);
}

fn extract_from_zip(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    let cursor = Cursor::new(archive_bytes);
    let mut archive = ZipArchive::new(cursor).context("Failed to open zip archive")?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("Failed to read zip entry {index}"))?;
        if entry.name().ends_with(binary_name) {
            let mut binary = Vec::new();
            entry
                .read_to_end(&mut binary)
                .context("Failed to extract binary from zip archive")?;
            return Ok(binary);
        }
    }

    bail!("Binary {} not found in zip archive", binary_name);
}

fn detect_platform_spec() -> Result<PlatformSpec> {
    platform_spec_for(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
        anyhow::anyhow!(
            "Unsupported platform: {}-{}",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    })
}

fn platform_spec_for(os: &str, arch: &str) -> Option<PlatformSpec> {
    match (os, arch) {
        ("macos", "aarch64") => Some(PlatformSpec {
            asset_name: "restflow-aarch64-apple-darwin.tar.gz",
            binary_name: "restflow",
            archive_kind: ArchiveKind::TarGz,
        }),
        ("macos", "x86_64") => Some(PlatformSpec {
            asset_name: "restflow-x86_64-apple-darwin.tar.gz",
            binary_name: "restflow",
            archive_kind: ArchiveKind::TarGz,
        }),
        ("linux", "aarch64") => Some(PlatformSpec {
            asset_name: "restflow-aarch64-unknown-linux-gnu.tar.gz",
            binary_name: "restflow",
            archive_kind: ArchiveKind::TarGz,
        }),
        ("linux", "x86_64") => Some(PlatformSpec {
            asset_name: "restflow-x86_64-unknown-linux-gnu.tar.gz",
            binary_name: "restflow",
            archive_kind: ArchiveKind::TarGz,
        }),
        ("windows", "x86_64") => Some(PlatformSpec {
            asset_name: "restflow-x86_64-pc-windows-msvc.zip",
            binary_name: "restflow.exe",
            archive_kind: ArchiveKind::Zip,
        }),
        _ => None,
    }
}

fn normalize_release_version(tag: &str) -> &str {
    tag.strip_prefix("cli-v")
        .or_else(|| tag.strip_prefix("v"))
        .unwrap_or(tag)
}

fn compare_versions(current: &str, latest: &str) -> VersionRelation {
    if current == latest {
        return VersionRelation::Equal;
    }

    let current_triplet = parse_semver_triplet(current);
    let latest_triplet = parse_semver_triplet(latest);

    match (current_triplet, latest_triplet) {
        (Some(current_triplet), Some(latest_triplet)) => match current_triplet.cmp(&latest_triplet)
        {
            std::cmp::Ordering::Less => VersionRelation::LatestNewer,
            std::cmp::Ordering::Greater => VersionRelation::CurrentNewer,
            std::cmp::Ordering::Equal => VersionRelation::Equal,
        },
        _ => VersionRelation::Unknown,
    }
}

/// Parsed semver with optional prerelease tag.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<String>,
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let triplet_cmp = (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch));
        if triplet_cmp != std::cmp::Ordering::Equal {
            return triplet_cmp;
        }
        // Per semver spec: prerelease < release for same triplet
        match (&self.prerelease, &other.prerelease) {
            (None, None) => std::cmp::Ordering::Equal,
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_semver_triplet(version: &str) -> Option<SemVer> {
    // Split off build metadata first
    let without_build = version
        .split_once('+')
        .map(|(left, _)| left)
        .unwrap_or(version);

    // Split prerelease from core
    let (core, prerelease) = match without_build.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (without_build, None),
    };

    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;

    Some(SemVer {
        major,
        minor,
        patch,
        prerelease,
    })
}

fn install_path() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let base = dirs::data_local_dir().context("Failed to resolve local data directory")?;
        return Ok(base.join("restflow").join("bin").join("restflow.exe"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = dirs::home_dir().context("Failed to resolve home directory")?;
        Ok(home.join(".local").join("bin").join("restflow"))
    }
}

fn install_binary(binary: &[u8], install_path: &Path) -> Result<()> {
    let parent = install_path
        .parent()
        .context("Install path must have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "Failed to create install directory {}",
            parent.to_string_lossy()
        )
    })?;

    let temp_path = parent.join(format!(
        ".{}.tmp-{}",
        install_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("restflow"),
        std::process::id()
    ));

    fs::write(&temp_path, binary).with_context(|| {
        format!(
            "Failed to write temporary binary {}",
            temp_path.to_string_lossy()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&temp_path)
            .with_context(|| {
                format!(
                    "Failed to read metadata for {}",
                    temp_path.to_string_lossy()
                )
            })?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&temp_path, permissions).with_context(|| {
            format!(
                "Failed to set executable permission on {}",
                temp_path.to_string_lossy()
            )
        })?;
    }

    if install_path.exists() {
        fs::remove_file(install_path).with_context(|| {
            format!(
                "Failed to replace existing binary {}",
                install_path.to_string_lossy()
            )
        })?;
    }

    fs::rename(&temp_path, install_path).with_context(|| {
        format!(
            "Failed to install binary at {}",
            install_path.to_string_lossy()
        )
    })?;

    Ok(())
}

#[cfg(unix)]
fn ensure_rf_alias(install_path: &Path) -> Result<bool> {
    use std::os::unix::fs::symlink;

    let alias_path = install_path
        .parent()
        .context("Install path must have a parent directory")?
        .join("rf");

    if fs::symlink_metadata(&alias_path).is_ok() {
        fs::remove_file(&alias_path).with_context(|| {
            format!(
                "Failed to remove existing rf alias {}",
                alias_path.to_string_lossy()
            )
        })?;
    }

    symlink(install_path, &alias_path).with_context(|| {
        format!(
            "Failed to create rf alias at {}",
            alias_path.to_string_lossy()
        )
    })?;

    Ok(true)
}

#[cfg(not(unix))]
fn ensure_rf_alias(_: &Path) -> Result<bool> {
    Ok(false)
}

#[cfg(target_os = "macos")]
fn try_codesign(install_path: &Path) -> bool {
    match std::process::Command::new("codesign")
        .arg("--force")
        .arg("--sign")
        .arg("-")
        .arg(install_path)
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

#[cfg(not(target_os = "macos"))]
fn try_codesign(_: &Path) -> bool {
    false
}

fn print_skip_result(
    format: OutputFormat,
    status: &str,
    current_version: &str,
    latest_tag: &str,
    installed_path: Option<&Path>,
    reason: &str,
) -> Result<()> {
    if format.is_json() {
        return print_json(&json!({
            "status": status,
            "current_version": current_version,
            "latest_tag": latest_tag,
            "installed_path": installed_path,
            "reason": reason,
        }));
    }

    println!("{reason}.");
    println!("Current version: {current_version}");
    println!("Latest tag: {latest_tag}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ArchiveKind, VersionRelation, compare_versions, normalize_release_version,
        parse_semver_triplet, platform_spec_for,
    };

    #[test]
    fn normalizes_release_tags() {
        assert_eq!(normalize_release_version("cli-v0.2.0"), "0.2.0");
        assert_eq!(normalize_release_version("v1.0.0"), "1.0.0");
        assert_eq!(normalize_release_version("0.3.0"), "0.3.0");
    }

    #[test]
    fn compares_versions() {
        assert_eq!(
            compare_versions("0.2.0", "0.2.1"),
            VersionRelation::LatestNewer
        );
        assert_eq!(
            compare_versions("0.2.1", "0.2.0"),
            VersionRelation::CurrentNewer
        );
        assert_eq!(compare_versions("0.2.1", "0.2.1"), VersionRelation::Equal);
        assert_eq!(
            compare_versions("main", "cli-v0.2.1"),
            VersionRelation::Unknown
        );
    }

    #[test]
    fn parses_semver_triplets() {
        let v = parse_semver_triplet("1.2.3").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
        assert_eq!(v.prerelease, None);

        let v = parse_semver_triplet("1.2.3-beta.1").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
        assert_eq!(v.prerelease, Some("beta.1".to_string()));

        assert!(parse_semver_triplet("1.2").is_none());
    }

    #[test]
    fn prerelease_compares_correctly() {
        // prerelease < release for same triplet
        assert_eq!(
            compare_versions("1.0.0-rc.1", "1.0.0"),
            VersionRelation::LatestNewer
        );
        // alpha < beta (lexicographic)
        assert_eq!(
            compare_versions("1.0.0-alpha", "1.0.0-beta"),
            VersionRelation::LatestNewer
        );
        // same release version
        assert_eq!(
            compare_versions("1.0.0", "1.0.0"),
            VersionRelation::Equal
        );
    }

    #[test]
    fn resolves_platform_spec() {
        let mac = platform_spec_for("macos", "aarch64").expect("macOS aarch64 spec");
        assert_eq!(mac.asset_name, "restflow-aarch64-apple-darwin.tar.gz");
        assert_eq!(mac.binary_name, "restflow");
        assert!(matches!(mac.archive_kind, ArchiveKind::TarGz));

        let windows = platform_spec_for("windows", "x86_64").expect("windows spec");
        assert_eq!(windows.asset_name, "restflow-x86_64-pc-windows-msvc.zip");
        assert_eq!(windows.binary_name, "restflow.exe");
        assert!(matches!(windows.archive_kind, ArchiveKind::Zip));

        assert!(platform_spec_for("linux", "armv7").is_none());
    }
}
