//! Gating requirements checker.
//!
//! Checks whether system requirements are met for a skill:
//! - Required binaries
//! - Environment variables
//! - OS compatibility
//! - RestFlow version

use std::process::Command;

use crate::models::{
    BinaryRequirement, GatingCheckResult, GatingRequirements, SkillVersion,
};

/// Gating requirements checker
pub struct GatingChecker {
    /// Current RestFlow version
    restflow_version: SkillVersion,
}

impl GatingChecker {
    /// Create a new gating checker
    pub fn new(restflow_version: SkillVersion) -> Self {
        Self { restflow_version }
    }

    /// Create a gating checker with default RestFlow version
    pub fn default_version() -> Self {
        Self::new(SkillVersion::new(0, 1, 0))
    }

    /// Check all gating requirements
    pub fn check(&self, requirements: &GatingRequirements) -> GatingCheckResult {
        let mut result = GatingCheckResult::pass();

        // Check OS compatibility
        if !requirements.supported_os.is_empty() {
            let os_supported = requirements
                .supported_os
                .iter()
                .any(|os| os.matches_current());

            if !os_supported {
                result.os_supported = false;
                result.passed = false;
            }
        }

        // Check RestFlow version
        if let Some(ref min_version) = requirements.min_restflow_version
            && self.restflow_version.compare(min_version) < 0
        {
            result.restflow_version_ok = false;
            result.passed = false;
        }

        // Check binaries
        for binary in &requirements.binaries {
            if !self.check_binary(binary) {
                result.missing_binaries.push(binary.name.clone());
                result.passed = false;
            }
        }

        // Check environment variables
        for env_var in &requirements.env_vars {
            if env_var.required && std::env::var(&env_var.name).is_err() {
                result.missing_env_vars.push(env_var.name.clone());
                result.passed = false;
            }
        }

        // Generate summary
        result.summary = self.generate_summary(&result, requirements);

        result
    }

    /// Check if a binary requirement is met
    fn check_binary(&self, requirement: &BinaryRequirement) -> bool {
        // Try to find the binary using 'which' on Unix or 'where' on Windows
        let which_cmd = if cfg!(windows) { "where" } else { "which" };
        
        let output = Command::new(which_cmd)
            .arg(&requirement.name)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                // Binary exists, check version if required
                if requirement.version.is_some() {
                    self.check_binary_version(requirement)
                } else {
                    true
                }
            }
            _ => false,
        }
    }

    /// Check if a binary meets the version requirement
    fn check_binary_version(&self, requirement: &BinaryRequirement) -> bool {
        let version_cmd = requirement
            .version_command
            .as_deref()
            .unwrap_or("--version");

        let output = Command::new(&requirement.name)
            .arg(version_cmd)
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let output_str = String::from_utf8_lossy(&output.stdout);
                
                // Extract version using pattern or simple heuristics
                if let Some(ref pattern) = requirement.version_pattern {
                    self.extract_version_with_pattern(&output_str, pattern, requirement)
                } else {
                    // Simple version extraction: look for semver-like pattern
                    self.extract_simple_version(&output_str, requirement)
                }
            }
            _ => false,
        }
    }

    /// Extract version using a regex pattern
    fn extract_version_with_pattern(
        &self,
        output: &str,
        pattern: &str,
        requirement: &BinaryRequirement,
    ) -> bool {
        // Try to compile the pattern as a regex
        if let Ok(re) = regex::Regex::new(pattern)
            && let Some(caps) = re.captures(output)
        {
            // Try to get named groups or positional groups
            let version_str = caps
                .name("version")
                .or_else(|| caps.get(1))
                .map(|m| m.as_str());

            if let Some(v) = version_str
                && let Some(version) = SkillVersion::parse(v)
                && let Some(ref req) = requirement.version
            {
                return version.satisfies(req);
            }
        }
        false
    }

    /// Extract version using simple heuristics
    fn extract_simple_version(&self, output: &str, requirement: &BinaryRequirement) -> bool {
        // Look for patterns like "1.2.3" or "v1.2.3"
        let re = regex::Regex::new(r"v?(\d+\.\d+\.\d+)").unwrap();
        
        if let Some(caps) = re.captures(output)
            && let Some(m) = caps.get(1)
            && let Some(version) = SkillVersion::parse(m.as_str())
            && let Some(ref req) = requirement.version
        {
            return version.satisfies(req);
        }
        
        // If no version requirement, just return true (binary exists)
        requirement.version.is_none()
    }

    /// Generate a human-readable summary
    fn generate_summary(
        &self,
        result: &GatingCheckResult,
        requirements: &GatingRequirements,
    ) -> String {
        if result.passed {
            return "All requirements met".to_string();
        }

        let mut issues = Vec::new();

        if !result.os_supported {
            let supported: Vec<_> = requirements
                .supported_os
                .iter()
                .map(|os| format!("{:?}", os))
                .collect();
            issues.push(format!(
                "OS not supported (requires: {})",
                supported.join(", ")
            ));
        }

        if !result.restflow_version_ok
            && let Some(ref min) = requirements.min_restflow_version
        {
            issues.push(format!(
                "RestFlow version {} required (have: {})",
                min, self.restflow_version
            ));
        }

        if !result.missing_binaries.is_empty() {
            issues.push(format!(
                "Missing binaries: {}",
                result.missing_binaries.join(", ")
            ));
        }

        if !result.missing_env_vars.is_empty() {
            issues.push(format!(
                "Missing environment variables: {}",
                result.missing_env_vars.join(", ")
            ));
        }

        issues.join("; ")
    }

    /// Check a single binary requirement (for UI display)
    pub fn check_single_binary(&self, name: &str) -> bool {
        let req = BinaryRequirement {
            name: name.to_string(),
            version: None,
            version_command: None,
            version_pattern: None,
        };
        self.check_binary(&req)
    }

    /// Check a single environment variable
    pub fn check_env_var(&self, name: &str) -> bool {
        std::env::var(name).is_ok()
    }
}

impl Default for GatingChecker {
    fn default() -> Self {
        Self::default_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EnvVarRequirement;
    use crate::OsType;

    #[test]
    fn test_os_check() {
        let checker = GatingChecker::default_version();
        
        // Should pass with any OS
        let requirements = GatingRequirements {
            supported_os: vec![OsType::Any],
            ..Default::default()
        };
        let result = checker.check(&requirements);
        assert!(result.os_supported);
        
        // Should pass with current OS
        #[cfg(target_os = "macos")]
        {
            let requirements = GatingRequirements {
                supported_os: vec![OsType::MacOS],
                ..Default::default()
            };
            let result = checker.check(&requirements);
            assert!(result.os_supported);
        }
    }

    #[test]
    fn test_binary_check() {
        let checker = GatingChecker::default_version();
        
        // Check for a common binary
        assert!(checker.check_single_binary("sh") || checker.check_single_binary("cmd"));
        
        // Check for a non-existent binary
        assert!(!checker.check_single_binary("this-binary-does-not-exist-12345"));
    }

    #[test]
    fn test_env_var_check() {
        let checker = GatingChecker::default_version();
        
        // PATH should always exist
        assert!(checker.check_env_var("PATH"));
        
        // Random env var should not exist
        assert!(!checker.check_env_var("THIS_ENV_VAR_SHOULD_NOT_EXIST_12345"));
    }

    #[test]
    fn test_full_check() {
        let checker = GatingChecker::default_version();
        
        let requirements = GatingRequirements {
            binaries: vec![BinaryRequirement {
                name: "sh".to_string(),
                version: None,
                version_command: None,
                version_pattern: None,
            }],
            env_vars: vec![EnvVarRequirement {
                name: "PATH".to_string(),
                required: true,
                description: None,
            }],
            supported_os: vec![OsType::Any],
            min_restflow_version: None,
        };

        let result = checker.check(&requirements);
        
        #[cfg(unix)]
        assert!(result.passed, "Check failed: {}", result.summary);
    }
}
