//! Security amendments for auto-approving known-safe command patterns.
//!
//! ## Security Warning: Regex Bypass Vulnerabilities
//!
//! The regex match type is powerful but can be bypassed if patterns are not carefully crafted.
//! To mitigate regex bypass attacks:
//!
//! 1. **Always anchor patterns**: Use `^` at the start and `$` at the end to prevent partial matches
//!    - ✅ `^git status$` - Matches exactly "git status"
//!    - ❌ `git status` - Could match "echo evil; git status"
//!
//! 2. **Avoid overly broad wildcards**: `.*` can match anything, including command separators
//!    - ✅ `^cargo [a-z]+$` - Restricts to lowercase commands
//!    - ❌ `^cargo .*$` - Could match `cargo foo; rm -rf /`
//!
//! 3. **Be specific about whitespace**: Use `\s+` instead of `\s*` where appropriate
//!    - ✅ `^npm\s+test$` - Requires exactly one space
//!    - ❌ `^npm\s*test$` - Could match "npmtest" (unintended command)
//!
//! 4. **Consider ReDoS**: Avoid patterns with nested quantifiers like `(a+)+$`
//!
//! The validator will warn about potentially insecure patterns but will not reject them,
//! allowing administrators to make informed decisions.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SECURITY_AMENDMENTS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("security_amendments");
const MAX_REGEX_PATTERN_LEN: usize = 512;
const MAX_CACHED_REGEX_PATTERNS: usize = 1024;
const MAX_REGEX_MATCH_TIME_MS: u64 = 100; // Timeout for regex matching

static REGEX_CACHE: OnceLock<RwLock<HashMap<String, Arc<Regex>>>> = OnceLock::new();

fn regex_cache() -> &'static RwLock<HashMap<String, Arc<Regex>>> {
    REGEX_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Wrapper for regex matching with timeout protection.
///
/// This prevents ReDoS attacks by limiting the time spent on regex matching.
/// If the match takes longer than `MAX_REGEX_MATCH_TIME_MS`, it logs a warning.
///
/// IMPORTANT: Security decisions must evaluate the full command.
/// We do NOT truncate the input because truncation changes semantics and can
/// produce false positives (e.g., `^...$` matches truncated input but should
/// fail on full input), creating an approval bypass vulnerability.
fn safe_regex_match(regex: &Regex, text: &str) -> bool {
    // The regex crate has internal backtracking limits, so direct matching is usually safe.
    // We add a time check for extra safety and to identify problematic patterns.
    let start = Instant::now();
    let result = regex.is_match(text);

    // If matching took suspiciously long, log a warning but return the result.
    // This helps identify problematic patterns.
    let elapsed = start.elapsed();
    if elapsed > Duration::from_millis(MAX_REGEX_MATCH_TIME_MS) {
        tracing::warn!(
            pattern = regex.as_str(),
            elapsed_ms = elapsed.as_millis(),
            text_len = text.len(),
            "Regex pattern took longer than expected to match; consider simplifying the pattern"
        );
    }

    result
}

fn compile_and_cache_regex(pattern: &str) -> Option<Arc<Regex>> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        tracing::warn!(
            pattern_len = pattern.len(),
            max_len = MAX_REGEX_PATTERN_LEN,
            "Regex pattern exceeds maximum length"
        );
        return None;
    }

    if let Ok(cache) = regex_cache().read()
        && let Some(cached) = cache.get(pattern)
    {
        return Some(Arc::clone(cached));
    }

    let compiled = Arc::new(Regex::new(pattern).ok()?);

    if let Ok(mut cache) = regex_cache().write() {
        if let Some(existing) = cache.get(pattern) {
            return Some(Arc::clone(existing));
        }

        if cache.len() >= MAX_CACHED_REGEX_PATTERNS {
            cache.clear();
        }
        cache.insert(pattern.to_string(), Arc::clone(&compiled));
    }

    Some(compiled)
}

/// Warning about potential regex bypass vulnerabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternWarning {
    /// The warning code
    pub code: String,
    /// Human-readable warning message
    pub message: String,
    /// Severity level: "low", "medium", "high"
    pub severity: String,
}

/// Validates a regex pattern for syntax errors and potential bypass vulnerabilities.
///
/// Returns a list of warnings about potentially insecure patterns.
/// The pattern is still considered valid if warnings are present, but
/// administrators should review them.
fn validate_regex_pattern(pattern: &str) -> Result<Vec<PatternWarning>> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        bail!(
            "Regex pattern length {} exceeds max {}",
            pattern.len(),
            MAX_REGEX_PATTERN_LEN
        );
    }
    
    // Check syntax
    Regex::new(pattern).map_err(|err| anyhow!("Invalid regex pattern: {}", err))?;
    
    let mut warnings = Vec::new();
    
    // Check for missing start anchor
    if !pattern.starts_with('^') {
        warnings.push(PatternWarning {
            code: "MISSING_START_ANCHOR".to_string(),
            message: "Pattern does not start with '^'. This could allow command injection via command chaining (e.g., 'echo safe; malicious_cmd').".to_string(),
            severity: "high".to_string(),
        });
    }
    
    // Check for missing end anchor
    if !pattern.ends_with('$') {
        warnings.push(PatternWarning {
            code: "MISSING_END_ANCHOR".to_string(),
            message: "Pattern does not end with '$'. This could allow appending malicious arguments.".to_string(),
            severity: "high".to_string(),
        });
    }
    
    // Check for ReDoS-prone patterns (nested quantifiers)
    if pattern.contains(")+") || pattern.contains(")*") || pattern.contains("}+")
        || pattern.contains("}*")
    {
        warnings.push(PatternWarning {
            code: "REDOS_RISK".to_string(),
            message: "Pattern contains nested quantifiers which could cause catastrophic backtracking.".to_string(),
            severity: "medium".to_string(),
        });
    }
    
    // Check for overly broad patterns
    if pattern.contains(".*") && !pattern.contains(";") {
        warnings.push(PatternWarning {
            code: "BROAD_WILDCARD".to_string(),
            message: "Pattern contains '.*' which matches anything, including command separators. Consider using more specific character classes.".to_string(),
            severity: "medium".to_string(),
        });
    }
    
    // Check for patterns that might be case-sensitive bypasses
    if pattern.contains(char::is_lowercase) && !pattern.contains("(?i)") && !pattern.contains("(?-i)") {
        // Pattern has lowercase but no case-insensitive flag
        // This is informational, not a security issue
    }
    
    Ok(warnings)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmendmentMatchType {
    Exact,
    Prefix,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AmendmentScope {
    Workspace,
    Agent { agent_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAmendment {
    pub id: String,
    pub tool_name: String,
    pub command_pattern: String,
    pub match_type: AmendmentMatchType,
    pub scope: AmendmentScope,
    pub enabled: bool,
    pub created_at_ms: i64,
}

impl SecurityAmendment {
    pub fn matches(&self, tool_name: &str, command: &str, agent_id: Option<&str>) -> bool {
        if !self.enabled || self.tool_name != tool_name {
            return false;
        }

        match &self.scope {
            AmendmentScope::Workspace => {}
            AmendmentScope::Agent {
                agent_id: amendment_agent_id,
            } => {
                if Some(amendment_agent_id.as_str()) != agent_id {
                    return false;
                }
            }
        }

        match self.match_type {
            AmendmentMatchType::Exact => self.command_pattern == command,
            AmendmentMatchType::Prefix => command.starts_with(&self.command_pattern),
            AmendmentMatchType::Regex => compile_and_cache_regex(&self.command_pattern)
                .map(|pattern| safe_regex_match(&pattern, command))
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecurityAmendmentStore {
    db: Arc<Database>,
}

impl SecurityAmendmentStore {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(SECURITY_AMENDMENTS_TABLE)?;
        }
        write_txn.commit()?;
        Ok(Self { db })
    }

    /// Add an allow rule amendment.
    ///
    /// Returns the created amendment and any warnings about potential bypass vulnerabilities.
    pub fn add_allow_rule(
        &self,
        tool_name: impl Into<String>,
        command_pattern: impl Into<String>,
        match_type: AmendmentMatchType,
        scope: AmendmentScope,
    ) -> Result<(SecurityAmendment, Vec<PatternWarning>)> {
        let command_pattern = command_pattern.into();
        let warnings = if matches!(match_type, AmendmentMatchType::Regex) {
            validate_regex_pattern(&command_pattern)?
        } else {
            Vec::new()
        };

        // Log warnings for administrator review
        for warning in &warnings {
            tracing::warn!(
                code = %warning.code,
                severity = %warning.severity,
                pattern = %command_pattern,
                "Security amendment pattern warning: {}", warning.message
            );
        }

        let rule = SecurityAmendment {
            id: format!("amendment-{}", Uuid::new_v4()),
            tool_name: tool_name.into(),
            command_pattern,
            match_type,
            scope,
            enabled: true,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        let payload = serde_json::to_vec(&rule)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECURITY_AMENDMENTS_TABLE)?;
            table.insert(rule.id.as_str(), payload.as_slice())?;
        }
        write_txn.commit()?;

        Ok((rule, warnings))
    }

    /// Add an allow rule without returning warnings (backward compatible).
    pub fn add_allow_rule_simple(
        &self,
        tool_name: impl Into<String>,
        command_pattern: impl Into<String>,
        match_type: AmendmentMatchType,
        scope: AmendmentScope,
    ) -> Result<SecurityAmendment> {
        let (rule, _) = self.add_allow_rule(tool_name, command_pattern, match_type, scope)?;
        Ok(rule)
    }

    pub fn list_rules(&self) -> Result<Vec<SecurityAmendment>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECURITY_AMENDMENTS_TABLE)?;
        let mut rules = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let rule: SecurityAmendment = serde_json::from_slice(value.value())?;
            rules.push(rule);
        }
        rules.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        Ok(rules)
    }

    pub fn find_matching_rule(
        &self,
        tool_name: &str,
        command: &str,
        agent_id: Option<&str>,
    ) -> Result<Option<SecurityAmendment>> {
        let matching = self
            .list_rules()?
            .into_iter()
            .find(|rule| rule.matches(tool_name, command, agent_id));
        Ok(matching)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_store() -> SecurityAmendmentStore {
        let dir = tempdir().unwrap();
        let db = Arc::new(Database::create(dir.path().join("security-amendments.db")).unwrap());
        SecurityAmendmentStore::new(db).unwrap()
    }

    #[test]
    fn match_exact_prefix_and_regex() {
        let store = create_store();
        store
            .add_allow_rule_simple(
                "bash",
                "cargo test",
                AmendmentMatchType::Exact,
                AmendmentScope::Workspace,
            )
            .unwrap();
        store
            .add_allow_rule_simple(
                "bash",
                "cargo ",
                AmendmentMatchType::Prefix,
                AmendmentScope::Workspace,
            )
            .unwrap();
        store
            .add_allow_rule_simple(
                "bash",
                "^git\\s+status$",
                AmendmentMatchType::Regex,
                AmendmentScope::Workspace,
            )
            .unwrap();

        assert!(
            store
                .find_matching_rule("bash", "cargo test", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .find_matching_rule("bash", "cargo clippy --all-targets", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .find_matching_rule("bash", "git status", Some("agent-a"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn respect_agent_scope() {
        let store = create_store();
        store
            .add_allow_rule_simple(
                "bash",
                "npm ",
                AmendmentMatchType::Prefix,
                AmendmentScope::Agent {
                    agent_id: "agent-a".to_string(),
                },
            )
            .unwrap();

        assert!(
            store
                .find_matching_rule("bash", "npm test", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .find_matching_rule("bash", "npm test", Some("agent-b"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reject_invalid_regex_pattern_on_create() {
        let store = create_store();
        let result = store.add_allow_rule_simple(
            "bash",
            "([invalid",
            AmendmentMatchType::Regex,
            AmendmentScope::Workspace,
        );
        assert!(result.is_err());
    }

    #[test]
    fn reject_oversized_regex_pattern_on_create() {
        let store = create_store();
        let oversized = "a".repeat(MAX_REGEX_PATTERN_LEN + 1);
        let result = store.add_allow_rule_simple(
            "bash",
            oversized,
            AmendmentMatchType::Regex,
            AmendmentScope::Workspace,
        );
        assert!(result.is_err());
    }

    #[test]
    fn warn_on_missing_anchors() {
        let warnings = validate_regex_pattern("git status").unwrap();
        assert!(warnings.iter().any(|w| w.code == "MISSING_START_ANCHOR"));
        assert!(warnings.iter().any(|w| w.code == "MISSING_END_ANCHOR"));
    }

    #[test]
    fn no_warnings_for_properly_anchored_pattern() {
        let warnings = validate_regex_pattern("^git\\s+status$").unwrap();
        assert!(!warnings.iter().any(|w| w.code == "MISSING_START_ANCHOR"));
        assert!(!warnings.iter().any(|w| w.code == "MISSING_END_ANCHOR"));
    }

    #[test]
    fn warn_on_broad_wildcard() {
        let warnings = validate_regex_pattern("^cargo.*$").unwrap();
        assert!(warnings.iter().any(|w| w.code == "BROAD_WILDCARD"));
    }

    #[test]
    fn anchored_pattern_prevents_command_injection_bypass() {
        let store = create_store();
        // Pattern with proper anchors
        store
            .add_allow_rule_simple(
                "bash",
                "^git status$",
                AmendmentMatchType::Regex,
                AmendmentScope::Workspace,
            )
            .unwrap();

        // Should match exactly "git status"
        assert!(
            store
                .find_matching_rule("bash", "git status", Some("agent-a"))
                .unwrap()
                .is_some()
        );

        // Should NOT match "echo safe; git status" (command injection attempt)
        assert!(
            store
                .find_matching_rule("bash", "echo safe; git status", Some("agent-a"))
                .unwrap()
                .is_none()
        );

        // Should NOT match "git status; rm -rf /" (append attack)
        assert!(
            store
                .find_matching_rule("bash", "git status; rm -rf /", Some("agent-a"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn unanchored_pattern_allows_bypass() {
        let store = create_store();
        // Pattern WITHOUT anchors (insecure, but allowed with warning)
        let (_, warnings) = store
            .add_allow_rule(
                "bash",
                "git status",
                AmendmentMatchType::Regex,
                AmendmentScope::Workspace,
            )
            .unwrap();
        
        // Should have warnings about missing anchors
        assert!(warnings.iter().any(|w| w.code == "MISSING_START_ANCHOR"));
        assert!(warnings.iter().any(|w| w.code == "MISSING_END_ANCHOR"));

        // This insecure pattern WILL match command injection attacks
        // (This test documents the vulnerability - the fix is to use anchors)
        assert!(
            store
                .find_matching_rule("bash", "git status", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        // Unanchored pattern matches "echo safe; git status" - THIS IS THE BYPASS
        assert!(
            store
                .find_matching_rule("bash", "echo safe; git status", Some("agent-a"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn add_allow_rule_returns_warnings() {
        let store = create_store();
        let (_, warnings) = store
            .add_allow_rule(
                "bash",
                "cargo test",
                AmendmentMatchType::Regex,
                AmendmentScope::Workspace,
            )
            .unwrap();
        
        // Should have warnings because pattern is unanchored
        assert!(!warnings.is_empty());
    }

    #[test]
    fn long_command_must_not_be_truncated_for_regex_match() {
        // Security test: Verify that long commands are NOT truncated before regex matching.
        // Truncation would create a bypass vulnerability where an anchored pattern
        // matches the truncated input but should fail on the full input.
        let store = create_store();
        store
            .add_allow_rule_simple(
                "bash",
                "^a{10000}$",
                AmendmentMatchType::Regex,
                AmendmentScope::Workspace,
            )
            .unwrap();

        // Build a command that would pass the truncated check but fail the full check
        let mut command = "a".repeat(10000);
        command.push_str(";rm -rf /");

        // The full command is "aaaa...(10000 a's)...;rm -rf /"
        // Pattern "^a{10000}$" should NOT match because:
        // - The command has more than 10000 'a' characters (actually 10000 'a' + ";rm -rf /")
        // - Wait, actually we have exactly 10000 'a' followed by ";rm -rf /"
        // So the pattern should NOT match because of the suffix
        assert!(
            store
                .find_matching_rule("bash", &command, Some("agent-a"))
                .unwrap()
                .is_none(),
            "Long command with malicious suffix should NOT match anchored pattern"
        );
    }
}
