//! Skill Marketplace models for versioning, dependencies, and permissions.
//!
//! This module extends the base Skill model with marketplace functionality:
//! - Semantic versioning for skills
//! - Dependency management between skills
//! - Permission control for skill capabilities
//! - Gating requirements (binary checks, env vars, OS compatibility)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Semantic version for skills
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct SkillVersion {
    /// Major version (breaking changes)
    pub major: u32,
    /// Minor version (new features, backward compatible)
    pub minor: u32,
    /// Patch version (bug fixes)
    pub patch: u32,
    /// Optional prerelease tag (e.g., "alpha", "beta.1")
    pub prerelease: Option<String>,
}

impl SkillVersion {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    /// Parse a version string (e.g., "1.2.3" or "1.2.3-beta.1")
    pub fn parse(s: &str) -> Option<Self> {
        let (version_part, prerelease) = if let Some(idx) = s.find('-') {
            (&s[..idx], Some(s[idx + 1..].to_string()))
        } else {
            (s, None)
        };

        let parts: Vec<&str> = version_part.split('.').collect();
        if parts.len() != 3 {
            return None;
        }

        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
            prerelease,
        })
    }

    /// Check if this version satisfies a version requirement
    pub fn satisfies(&self, requirement: &VersionRequirement) -> bool {
        match requirement {
            VersionRequirement::Exact(v) => self == v,
            VersionRequirement::Caret(v) => {
                // ^1.2.3 allows >=1.2.3 and <2.0.0
                if self.major != v.major {
                    return false;
                }
                if self.major == 0 {
                    // For 0.x versions, caret is more restrictive
                    self.minor == v.minor && self.patch >= v.patch
                } else {
                    (self.minor > v.minor)
                        || (self.minor == v.minor && self.patch >= v.patch)
                }
            }
            VersionRequirement::Tilde(v) => {
                // ~1.2.3 allows >=1.2.3 and <1.3.0
                self.major == v.major
                    && self.minor == v.minor
                    && self.patch >= v.patch
            }
            VersionRequirement::GreaterThan(v) => self.compare(v) > 0,
            VersionRequirement::GreaterOrEqual(v) => self.compare(v) >= 0,
            VersionRequirement::LessThan(v) => self.compare(v) < 0,
            VersionRequirement::LessOrEqual(v) => self.compare(v) <= 0,
            VersionRequirement::Any => true,
        }
    }

    /// Compare two versions (-1, 0, 1)
    pub fn compare(&self, other: &Self) -> i32 {
        if self.major != other.major {
            return if self.major > other.major { 1 } else { -1 };
        }
        if self.minor != other.minor {
            return if self.minor > other.minor { 1 } else { -1 };
        }
        if self.patch != other.patch {
            return if self.patch > other.patch { 1 } else { -1 };
        }
        // Prerelease versions are lower than release versions
        match (&self.prerelease, &other.prerelease) {
            (None, Some(_)) => 1,
            (Some(_), None) => -1,
            _ => 0,
        }
    }
}

impl std::fmt::Display for SkillVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref pre) = self.prerelease {
            write!(f, "{}.{}.{}-{}", self.major, self.minor, self.patch, pre)
        } else {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

impl Default for SkillVersion {
    fn default() -> Self {
        Self::new(1, 0, 0)
    }
}

/// Version requirement for dependencies
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "version")]
pub enum VersionRequirement {
    /// Exact version match
    Exact(SkillVersion),
    /// Caret requirement (^1.2.3)
    Caret(SkillVersion),
    /// Tilde requirement (~1.2.3)
    Tilde(SkillVersion),
    /// Greater than
    GreaterThan(SkillVersion),
    /// Greater than or equal
    GreaterOrEqual(SkillVersion),
    /// Less than
    LessThan(SkillVersion),
    /// Less than or equal
    LessOrEqual(SkillVersion),
    /// Any version
    Any,
}

impl VersionRequirement {
    /// Parse a version requirement string
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s == "*" || s.is_empty() {
            return Some(Self::Any);
        }
        if let Some(v) = s.strip_prefix("^") {
            return SkillVersion::parse(v).map(Self::Caret);
        }
        if let Some(v) = s.strip_prefix("~") {
            return SkillVersion::parse(v).map(Self::Tilde);
        }
        if let Some(v) = s.strip_prefix(">=") {
            return SkillVersion::parse(v).map(Self::GreaterOrEqual);
        }
        if let Some(v) = s.strip_prefix(">") {
            return SkillVersion::parse(v).map(Self::GreaterThan);
        }
        if let Some(v) = s.strip_prefix("<=") {
            return SkillVersion::parse(v).map(Self::LessOrEqual);
        }
        if let Some(v) = s.strip_prefix("<") {
            return SkillVersion::parse(v).map(Self::LessThan);
        }
        if let Some(v) = s.strip_prefix("=") {
            return SkillVersion::parse(v).map(Self::Exact);
        }
        // Default to exact match
        SkillVersion::parse(s).map(Self::Exact)
    }
}

/// Skill dependency
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillDependency {
    /// ID of the required skill
    pub skill_id: String,
    /// Version requirement
    pub version: VersionRequirement,
    /// Whether this dependency is optional
    pub optional: bool,
}

/// Permission types that skills can request
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SkillPermission {
    /// Read files from the filesystem
    FileRead,
    /// Write files to the filesystem
    FileWrite,
    /// Execute shell commands
    ShellExec,
    /// Make network requests
    Network,
    /// Access environment variables
    Environment,
    /// Access clipboard
    Clipboard,
    /// Access system notifications
    Notifications,
    /// Access keychain/secrets
    Keychain,
    /// Access camera
    Camera,
    /// Access microphone
    Microphone,
    /// Access location
    Location,
    /// Custom permission with a name
    Custom(String),
}

/// Skill permissions configuration
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct SkillPermissions {
    /// Required permissions (skill won't work without these)
    pub required: Vec<SkillPermission>,
    /// Optional permissions (enhances functionality if granted)
    pub optional: Vec<SkillPermission>,
}

/// Operating system type
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum OsType {
    Windows,
    MacOS,
    Linux,
    Any,
}

impl OsType {
    /// Check if the current OS matches
    pub fn matches_current(&self) -> bool {
        match self {
            OsType::Any => true,
            OsType::Windows => cfg!(target_os = "windows"),
            OsType::MacOS => cfg!(target_os = "macos"),
            OsType::Linux => cfg!(target_os = "linux"),
        }
    }
}

/// Binary requirement for gating
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BinaryRequirement {
    /// Name of the binary (e.g., "git", "docker")
    pub name: String,
    /// Optional minimum version requirement
    pub version: Option<VersionRequirement>,
    /// Version check command (e.g., "--version")
    pub version_command: Option<String>,
    /// Regex pattern to extract version from command output
    pub version_pattern: Option<String>,
}

/// Environment variable requirement for gating
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EnvVarRequirement {
    /// Name of the environment variable
    pub name: String,
    /// Whether the variable must be set
    pub required: bool,
    /// Optional description of what this variable is for
    pub description: Option<String>,
}

/// Gating requirements for a skill
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct GatingRequirements {
    /// Required binaries
    pub binaries: Vec<BinaryRequirement>,
    /// Required environment variables
    pub env_vars: Vec<EnvVarRequirement>,
    /// Supported operating systems
    pub supported_os: Vec<OsType>,
    /// Minimum RestFlow version required
    pub min_restflow_version: Option<SkillVersion>,
}

/// Author information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillAuthor {
    /// Author name
    pub name: String,
    /// Optional email
    pub email: Option<String>,
    /// Optional URL (website, GitHub profile)
    pub url: Option<String>,
}

/// Skill source information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillSource {
    /// Local skill (user-created)
    Local,
    /// Built-in skill (bundled with RestFlow)
    Builtin,
    /// From RestFlow marketplace
    Marketplace {
        /// Marketplace URL
        url: String,
    },
    /// From GitHub repository
    GitHub {
        /// Repository owner
        owner: String,
        /// Repository name
        repo: String,
        /// Optional branch/tag/commit
        #[serde(rename = "ref")]
        git_ref: Option<String>,
        /// Path within the repository
        path: Option<String>,
    },
    /// From a Git URL
    Git {
        /// Git URL
        url: String,
        /// Optional branch/tag/commit
        #[serde(rename = "ref")]
        git_ref: Option<String>,
    },
}

impl Default for SkillSource {
    fn default() -> Self {
        SkillSource::Local
    }
}

/// Extended skill metadata for marketplace
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SkillManifest {
    /// Skill ID (unique identifier)
    pub id: String,
    /// Display name
    pub name: String,
    /// Semantic version
    pub version: SkillVersion,
    /// Description
    pub description: Option<String>,
    /// Author information
    pub author: Option<SkillAuthor>,
    /// License (SPDX identifier)
    pub license: Option<String>,
    /// Homepage URL
    pub homepage: Option<String>,
    /// Repository URL
    pub repository: Option<String>,
    /// Keywords for search
    pub keywords: Vec<String>,
    /// Categories
    pub categories: Vec<String>,
    /// Dependencies on other skills
    pub dependencies: Vec<SkillDependency>,
    /// Permissions required
    pub permissions: SkillPermissions,
    /// Gating requirements
    pub gating: GatingRequirements,
    /// Source information
    #[serde(default)]
    pub source: SkillSource,
    /// Icon URL or data URI
    pub icon: Option<String>,
    /// Readme content (markdown)
    pub readme: Option<String>,
    /// Changelog content (markdown)
    pub changelog: Option<String>,
    /// Additional metadata
    #[ts(type = "Record<string, unknown>")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for SkillManifest {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version: SkillVersion::default(),
            description: None,
            author: None,
            license: None,
            homepage: None,
            repository: None,
            keywords: Vec::new(),
            categories: Vec::new(),
            dependencies: Vec::new(),
            permissions: SkillPermissions::default(),
            gating: GatingRequirements::default(),
            source: SkillSource::Local,
            icon: None,
            readme: None,
            changelog: None,
            metadata: HashMap::new(),
        }
    }
}

/// Gating check result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GatingCheckResult {
    /// Whether all requirements are met
    pub passed: bool,
    /// List of missing binaries
    pub missing_binaries: Vec<String>,
    /// List of missing environment variables
    pub missing_env_vars: Vec<String>,
    /// Whether OS is supported
    pub os_supported: bool,
    /// Whether RestFlow version is sufficient
    pub restflow_version_ok: bool,
    /// Human-readable summary
    pub summary: String,
}

impl GatingCheckResult {
    /// Create a passing result
    pub fn pass() -> Self {
        Self {
            passed: true,
            missing_binaries: Vec::new(),
            missing_env_vars: Vec::new(),
            os_supported: true,
            restflow_version_ok: true,
            summary: "All requirements met".to_string(),
        }
    }

    /// Create a failing result
    pub fn fail(summary: String) -> Self {
        Self {
            passed: false,
            missing_binaries: Vec::new(),
            missing_env_vars: Vec::new(),
            os_supported: true,
            restflow_version_ok: true,
            summary,
        }
    }
}

/// Installed skill status
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    /// Skill is installed and ready to use
    Installed,
    /// Skill is installed but has unmet gating requirements
    RequirementsNotMet,
    /// Skill is installed but has missing dependencies
    MissingDependencies,
    /// Skill is being installed
    Installing,
    /// Skill installation failed
    Failed,
    /// Skill is not installed
    NotInstalled,
}

/// Installed skill information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InstalledSkill {
    /// Skill manifest
    pub manifest: SkillManifest,
    /// The actual skill content
    pub content: String,
    /// Installation status
    pub status: InstallStatus,
    /// When the skill was installed (Unix timestamp ms)
    #[ts(type = "number")]
    pub installed_at: i64,
    /// When the skill was last updated (Unix timestamp ms)
    #[ts(type = "number")]
    pub updated_at: i64,
    /// Whether updates are available
    pub update_available: Option<SkillVersion>,
    /// Gating check result
    pub gating_result: Option<GatingCheckResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse() {
        let v = SkillVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert_eq!(v.prerelease, None);

        let v = SkillVersion::parse("1.2.3-beta.1").unwrap();
        assert_eq!(v.prerelease, Some("beta.1".to_string()));
    }

    #[test]
    fn test_version_satisfies() {
        let v123 = SkillVersion::new(1, 2, 3);
        let v124 = SkillVersion::new(1, 2, 4);
        let v130 = SkillVersion::new(1, 3, 0);
        let v200 = SkillVersion::new(2, 0, 0);

        // Caret
        let req = VersionRequirement::Caret(v123.clone());
        assert!(v123.satisfies(&req));
        assert!(v124.satisfies(&req));
        assert!(v130.satisfies(&req));
        assert!(!v200.satisfies(&req));

        // Tilde
        let req = VersionRequirement::Tilde(v123.clone());
        assert!(v123.satisfies(&req));
        assert!(v124.satisfies(&req));
        assert!(!v130.satisfies(&req));
    }

    #[test]
    fn test_version_requirement_parse() {
        assert!(matches!(
            VersionRequirement::parse("^1.2.3"),
            Some(VersionRequirement::Caret(_))
        ));
        assert!(matches!(
            VersionRequirement::parse("~1.2.3"),
            Some(VersionRequirement::Tilde(_))
        ));
        assert!(matches!(
            VersionRequirement::parse(">=1.0.0"),
            Some(VersionRequirement::GreaterOrEqual(_))
        ));
        assert!(matches!(
            VersionRequirement::parse("*"),
            Some(VersionRequirement::Any)
        ));
    }

    #[test]
    fn test_manifest_deserialize_without_source() {
        let json = r#"
        {
          "id": "test-skill",
          "name": "Test Skill",
          "version": { "major": 1, "minor": 2, "patch": 3, "prerelease": null },
          "description": null,
          "author": null,
          "license": null,
          "homepage": null,
          "repository": null,
          "keywords": [],
          "categories": [],
          "dependencies": [],
          "permissions": { "required": [], "optional": [] },
          "gating": {
            "binaries": [],
            "env_vars": [],
            "supported_os": [],
            "min_restflow_version": null
          },
          "icon": null,
          "readme": null,
          "changelog": null,
          "metadata": {}
        }
        "#;

        let manifest: SkillManifest = serde_json::from_str(json).unwrap();
        assert!(matches!(manifest.source, SkillSource::Local));
    }

    #[test]
    fn test_os_matches() {
        assert!(OsType::Any.matches_current());

        #[cfg(target_os = "macos")]
        assert!(OsType::MacOS.matches_current());

        #[cfg(target_os = "windows")]
        assert!(OsType::Windows.matches_current());

        #[cfg(target_os = "linux")]
        assert!(OsType::Linux.matches_current());
    }
}
