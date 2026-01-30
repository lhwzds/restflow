//! Security models for command execution policy and approval workflow.
//!
//! This module provides types for configuring security policies that control
//! which commands can be executed automatically, which require user approval,
//! and which are always blocked.
//!
//! # Example
//!
//! ```rust
//! use restflow_core::models::security::{SecurityPolicy, CommandPattern, SecurityAction};
//!
//! let policy = SecurityPolicy::default();
//! assert!(policy.allowlist.iter().any(|p| p.pattern == "ls *"));
//! ```

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Security policy for command execution.
///
/// Defines which commands are allowed, blocked, or require approval.
/// Commands are checked in order: blocklist → allowlist → approval_required → default_action.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SecurityPolicy {
    /// Default action for commands not matching any pattern
    #[serde(default)]
    pub default_action: SecurityAction,

    /// Commands that are always allowed without approval
    #[serde(default)]
    pub allowlist: Vec<CommandPattern>,

    /// Commands that are always blocked
    #[serde(default)]
    pub blocklist: Vec<CommandPattern>,

    /// Commands that require explicit user approval
    #[serde(default)]
    pub approval_required: Vec<CommandPattern>,

    /// Approval timeout in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_secs: u64,
}

fn default_approval_timeout() -> u64 {
    300
}

/// Action to take for a command based on security policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SecurityAction {
    /// Command is allowed to execute
    Allow,
    /// Command is blocked and cannot execute
    Block,
    /// Command requires explicit user approval before execution
    #[default]
    RequireApproval,
}

/// Pattern for matching commands.
///
/// Supports glob-style patterns with `*` for wildcard matching.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CommandPattern {
    /// Pattern to match (supports glob-style wildcards)
    pub pattern: String,

    /// Optional description for this rule
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl CommandPattern {
    /// Create a new command pattern.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            description: None,
        }
    }

    /// Create a new command pattern with description.
    pub fn with_description(pattern: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            description: Some(description.into()),
        }
    }

    /// Check if a command matches this pattern.
    ///
    /// Uses glob-style matching where `*` matches any sequence of characters.
    pub fn matches(&self, command: &str) -> bool {
        glob_match(&self.pattern, command)
    }
}

/// Glob-style pattern matching for command security.
///
/// Supports `*` as a wildcard that matches any sequence of characters (including empty).
/// Special handling: trailing ` *` (space + wildcard) also matches when there are no arguments.
/// This means `ls *` matches both `ls` and `ls -la`.
fn glob_match(pattern: &str, text: &str) -> bool {
    // Special case: if pattern ends with " *", also try matching without it
    // This allows "ls *" to match both "ls" and "ls -la"
    if let Some(base_pattern) = pattern.strip_suffix(" *")
        && text == base_pattern
    {
        return true;
    }

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let p_len = pattern_chars.len();
    let t_len = text_chars.len();

    // dp[i][j] = true if pattern[0..i] matches text[0..j]
    let mut dp = vec![vec![false; t_len + 1]; p_len + 1];
    dp[0][0] = true;

    // Handle patterns that start with wildcards (they can match empty string)
    for i in 0..p_len {
        if pattern_chars[i] == '*' {
            dp[i + 1][0] = dp[i][0];
        } else {
            break;
        }
    }

    for i in 0..p_len {
        let p_ch = pattern_chars[i];
        for j in 0..t_len {
            let t_ch = text_chars[j];
            if p_ch == '*' {
                // * matches zero or more characters
                // dp[i][j+1] = match zero more (skip this char in text)
                // dp[i+1][j] = match one char and continue with same *
                dp[i + 1][j + 1] = dp[i][j + 1] || dp[i + 1][j];
            } else if p_ch == '?' || p_ch == t_ch {
                // ? matches exactly one character, or exact match
                dp[i + 1][j + 1] = dp[i][j];
            }
        }
        // Handle trailing * (can match empty remaining text)
        if p_ch == '*' {
            dp[i + 1][t_len] = dp[i][t_len] || dp[i + 1][t_len];
        }
    }

    dp[p_len][t_len]
}

/// Record of a pending approval request.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PendingApproval {
    /// Unique identifier for this approval request
    pub id: String,

    /// The command awaiting approval
    pub command: String,

    /// Working directory for command execution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,

    /// ID of the task requesting execution
    pub task_id: String,

    /// ID of the agent requesting execution
    pub agent_id: String,

    /// Unix timestamp when the request was created
    pub created_at: i64,

    /// Unix timestamp when the request expires
    pub expires_at: i64,

    /// Current status of the approval request
    pub status: ApprovalStatus,

    /// Optional reason for rejection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

impl PendingApproval {
    /// Create a new pending approval request.
    pub fn new(
        command: impl Into<String>,
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        timeout_secs: u64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            command: command.into(),
            workdir: None,
            task_id: task_id.into(),
            agent_id: agent_id.into(),
            created_at: now,
            expires_at: now + timeout_secs as i64,
            status: ApprovalStatus::Pending,
            rejection_reason: None,
        }
    }

    /// Set the working directory for command execution.
    pub fn with_workdir(mut self, workdir: impl Into<String>) -> Self {
        self.workdir = Some(workdir.into());
        self
    }

    /// Check if the approval request has expired.
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.expires_at
    }

    /// Approve the request.
    pub fn approve(&mut self) {
        self.status = ApprovalStatus::Approved;
    }

    /// Reject the request with an optional reason.
    pub fn reject(&mut self, reason: Option<String>) {
        self.status = ApprovalStatus::Rejected;
        self.rejection_reason = reason;
    }

    /// Mark the request as expired.
    pub fn expire(&mut self) {
        self.status = ApprovalStatus::Expired;
    }
}

/// Status of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Awaiting user decision
    #[default]
    Pending,
    /// User approved the command
    Approved,
    /// User rejected the command
    Rejected,
    /// Request timed out without decision
    Expired,
}

/// Result of a security check for a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityCheckResult {
    /// Whether the command is allowed to execute
    pub allowed: bool,

    /// Whether the command requires user approval
    pub requires_approval: bool,

    /// Whether approval has been granted (only relevant if requires_approval is true)
    pub approved: bool,

    /// ID of the pending approval (if requires_approval is true)
    pub approval_id: Option<String>,

    /// Reason for the decision
    pub reason: Option<String>,

    /// The matched pattern (if any)
    pub matched_pattern: Option<String>,
}

impl SecurityCheckResult {
    /// Create an "allowed" result.
    pub fn allowed(reason: Option<String>) -> Self {
        Self {
            allowed: true,
            requires_approval: false,
            approved: false,
            approval_id: None,
            reason,
            matched_pattern: None,
        }
    }

    /// Create a "blocked" result.
    pub fn blocked(reason: String, matched_pattern: Option<String>) -> Self {
        Self {
            allowed: false,
            requires_approval: false,
            approved: false,
            approval_id: None,
            reason: Some(reason),
            matched_pattern,
        }
    }

    /// Create a "requires approval" result.
    pub fn requires_approval(approval_id: String, reason: Option<String>) -> Self {
        Self {
            allowed: false,
            requires_approval: true,
            approved: false,
            approval_id: Some(approval_id),
            reason,
            matched_pattern: None,
        }
    }

    /// Create an "approved" result.
    pub fn approved_result(approval_id: String) -> Self {
        Self {
            allowed: true,
            requires_approval: true,
            approved: true,
            approval_id: Some(approval_id),
            reason: Some("User approved".to_string()),
            matched_pattern: None,
        }
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            default_action: SecurityAction::RequireApproval,
            allowlist: vec![
                // Safe read-only commands
                CommandPattern::with_description("ls *", "List directory contents"),
                CommandPattern::with_description("cat *", "Display file contents"),
                CommandPattern::with_description("head *", "Display first lines of file"),
                CommandPattern::with_description("tail *", "Display last lines of file"),
                CommandPattern::with_description("pwd", "Print working directory"),
                CommandPattern::with_description("echo *", "Print text"),
                CommandPattern::with_description("which *", "Locate a command"),
                CommandPattern::with_description("env", "Display environment variables"),
                CommandPattern::with_description("whoami", "Display current user"),
                CommandPattern::with_description("date", "Display current date/time"),
                CommandPattern::with_description("wc *", "Word/line/character count"),
                CommandPattern::with_description("grep *", "Search text patterns"),
                CommandPattern::with_description("find *", "Find files"),
                CommandPattern::with_description("tree *", "Display directory tree"),
                // Git read commands
                CommandPattern::with_description("git status*", "Show git status"),
                CommandPattern::with_description("git log*", "Show git log"),
                CommandPattern::with_description("git diff*", "Show git diff"),
                CommandPattern::with_description("git branch*", "List git branches"),
                CommandPattern::with_description("git show*", "Show git objects"),
                CommandPattern::with_description("git remote*", "Manage remotes"),
                // Cargo/npm read commands
                CommandPattern::with_description("cargo check*", "Check Rust code"),
                CommandPattern::with_description("cargo test*", "Run Rust tests"),
                CommandPattern::with_description("cargo build*", "Build Rust project"),
                CommandPattern::with_description("cargo fmt*", "Format Rust code"),
                CommandPattern::with_description("cargo clippy*", "Lint Rust code"),
                CommandPattern::with_description("npm test*", "Run npm tests"),
                CommandPattern::with_description("npm run *", "Run npm scripts"),
                CommandPattern::with_description("pnpm test*", "Run pnpm tests"),
                CommandPattern::with_description("pnpm run *", "Run pnpm scripts"),
            ],
            blocklist: vec![
                // Extremely dangerous commands
                CommandPattern::with_description("rm -rf /*", "Delete entire filesystem"),
                CommandPattern::with_description("rm -rf ~/*", "Delete home directory"),
                CommandPattern::with_description("rm -rf $HOME/*", "Delete home directory"),
                CommandPattern::with_description("sudo rm -rf *", "Privileged recursive delete"),
                CommandPattern::with_description(
                    ":(){ :|:& };:",
                    "Fork bomb - will crash system",
                ),
                CommandPattern::with_description("mkfs*", "Format filesystem"),
                CommandPattern::with_description("dd if=* of=/dev/*", "Write to raw device"),
                CommandPattern::with_description("> /dev/sda*", "Overwrite disk"),
                CommandPattern::with_description("chmod -R 777 /*", "Make everything world-writable"),
                CommandPattern::with_description("curl * | bash", "Execute remote script"),
                CommandPattern::with_description("wget * | bash", "Execute remote script"),
                CommandPattern::with_description("curl * | sh", "Execute remote script"),
                CommandPattern::with_description("wget * | sh", "Execute remote script"),
            ],
            approval_required: vec![
                // Potentially dangerous but useful
                CommandPattern::with_description("rm *", "Delete files"),
                CommandPattern::with_description("sudo *", "Privileged command"),
                CommandPattern::with_description("chmod *", "Change permissions"),
                CommandPattern::with_description("chown *", "Change ownership"),
                CommandPattern::with_description("git push*", "Push to remote"),
                CommandPattern::with_description("git reset*", "Reset git state"),
                CommandPattern::with_description("git checkout*", "Switch branches"),
                CommandPattern::with_description("git merge*", "Merge branches"),
                CommandPattern::with_description("git rebase*", "Rebase commits"),
                CommandPattern::with_description("npm publish*", "Publish npm package"),
                CommandPattern::with_description("cargo publish*", "Publish Rust crate"),
                CommandPattern::with_description("mv *", "Move/rename files"),
                CommandPattern::with_description("cp -r *", "Copy recursively"),
            ],
            approval_timeout_secs: default_approval_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("ls", "ls"));
        assert!(!glob_match("ls", "cat"));
    }

    #[test]
    fn test_glob_match_wildcard_suffix() {
        assert!(glob_match("ls *", "ls -la"));
        assert!(glob_match("ls *", "ls"));
        assert!(glob_match("git status*", "git status"));
        assert!(glob_match("git status*", "git status --short"));
    }

    #[test]
    fn test_glob_match_wildcard_prefix() {
        assert!(glob_match("*foo", "foo"));
        assert!(glob_match("*foo", "barfoo"));
        assert!(!glob_match("*foo", "foobar"));
    }

    #[test]
    fn test_glob_match_wildcard_middle() {
        assert!(glob_match("a*b", "ab"));
        assert!(glob_match("a*b", "aXb"));
        assert!(glob_match("a*b", "aXYZb"));
        assert!(!glob_match("a*b", "aXYZ"));
    }

    #[test]
    fn test_glob_match_multiple_wildcards() {
        assert!(glob_match("*a*b*", "ab"));
        assert!(glob_match("*a*b*", "XaYbZ"));
        assert!(glob_match("rm -rf */*", "rm -rf /home/user"));
    }

    #[test]
    fn test_command_pattern_matches() {
        let pattern = CommandPattern::new("ls *");
        assert!(pattern.matches("ls -la"));
        assert!(pattern.matches("ls"));
        assert!(!pattern.matches("cat file"));
    }

    #[test]
    fn test_security_policy_default() {
        let policy = SecurityPolicy::default();
        assert!(!policy.allowlist.is_empty());
        assert!(!policy.blocklist.is_empty());
        assert!(!policy.approval_required.is_empty());
        assert_eq!(policy.default_action, SecurityAction::RequireApproval);
    }

    #[test]
    fn test_security_policy_allowlist_contains_ls() {
        let policy = SecurityPolicy::default();
        assert!(policy.allowlist.iter().any(|p| p.pattern == "ls *"));
    }

    #[test]
    fn test_security_policy_blocklist_contains_rm_rf() {
        let policy = SecurityPolicy::default();
        assert!(policy.blocklist.iter().any(|p| p.pattern == "rm -rf /*"));
    }

    #[test]
    fn test_pending_approval_new() {
        let approval = PendingApproval::new("rm -rf temp", "task-1", "agent-1", 300);
        assert_eq!(approval.command, "rm -rf temp");
        assert_eq!(approval.task_id, "task-1");
        assert_eq!(approval.agent_id, "agent-1");
        assert_eq!(approval.status, ApprovalStatus::Pending);
        assert!(approval.expires_at > approval.created_at);
    }

    #[test]
    fn test_pending_approval_with_workdir() {
        let approval = PendingApproval::new("ls", "task-1", "agent-1", 300).with_workdir("/tmp");
        assert_eq!(approval.workdir, Some("/tmp".to_string()));
    }

    #[test]
    fn test_pending_approval_approve() {
        let mut approval = PendingApproval::new("rm file", "task-1", "agent-1", 300);
        approval.approve();
        assert_eq!(approval.status, ApprovalStatus::Approved);
    }

    #[test]
    fn test_pending_approval_reject() {
        let mut approval = PendingApproval::new("rm file", "task-1", "agent-1", 300);
        approval.reject(Some("Too dangerous".to_string()));
        assert_eq!(approval.status, ApprovalStatus::Rejected);
        assert_eq!(approval.rejection_reason, Some("Too dangerous".to_string()));
    }

    #[test]
    fn test_pending_approval_expire() {
        let mut approval = PendingApproval::new("rm file", "task-1", "agent-1", 300);
        approval.expire();
        assert_eq!(approval.status, ApprovalStatus::Expired);
    }

    #[test]
    fn test_security_check_result_allowed() {
        let result = SecurityCheckResult::allowed(Some("Allowlisted".to_string()));
        assert!(result.allowed);
        assert!(!result.requires_approval);
    }

    #[test]
    fn test_security_check_result_blocked() {
        let result =
            SecurityCheckResult::blocked("Blocked by policy".to_string(), Some("rm -rf *".to_string()));
        assert!(!result.allowed);
        assert!(!result.requires_approval);
        assert_eq!(result.matched_pattern, Some("rm -rf *".to_string()));
    }

    #[test]
    fn test_security_check_result_requires_approval() {
        let result =
            SecurityCheckResult::requires_approval("approval-123".to_string(), Some("Needs user OK".to_string()));
        assert!(!result.allowed);
        assert!(result.requires_approval);
        assert_eq!(result.approval_id, Some("approval-123".to_string()));
    }

    #[test]
    fn test_security_action_default() {
        assert_eq!(SecurityAction::default(), SecurityAction::RequireApproval);
    }

    #[test]
    fn test_approval_status_default() {
        assert_eq!(ApprovalStatus::default(), ApprovalStatus::Pending);
    }

    #[test]
    fn test_security_policy_serialization() {
        let policy = SecurityPolicy::default();
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: SecurityPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.allowlist.len(), policy.allowlist.len());
    }

    #[test]
    fn test_pending_approval_serialization() {
        let approval = PendingApproval::new("ls -la", "task-1", "agent-1", 300);
        let json = serde_json::to_string(&approval).unwrap();
        let parsed: PendingApproval = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.command, approval.command);
        assert_eq!(parsed.id, approval.id);
    }
}
