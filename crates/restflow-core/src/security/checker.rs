//! Security checker for validating commands against policy.
//!
//! The `SecurityChecker` evaluates commands against a `SecurityPolicy` and
//! coordinates with the `ApprovalManager` for commands that require user approval.

use std::sync::Arc;
use tokio::sync::RwLock;

use super::ApprovalManager;
use crate::models::security::{SecurityAction, SecurityCheckResult, SecurityPolicy};

/// Security checker for validating commands.
///
/// Checks commands against a security policy and manages approval requests
/// for commands that require user approval.
pub struct SecurityChecker {
    /// The security policy to enforce
    policy: RwLock<SecurityPolicy>,

    /// Manager for approval requests
    approval_manager: Arc<ApprovalManager>,
}

impl SecurityChecker {
    /// Create a new security checker with the given policy and approval manager.
    pub fn new(policy: SecurityPolicy, approval_manager: Arc<ApprovalManager>) -> Self {
        Self {
            policy: RwLock::new(policy),
            approval_manager,
        }
    }

    /// Create a new security checker with default policy and a new approval manager.
    pub fn with_defaults() -> Self {
        Self {
            policy: RwLock::new(SecurityPolicy::default()),
            approval_manager: Arc::new(ApprovalManager::new()),
        }
    }

    /// Get a reference to the approval manager.
    pub fn approval_manager(&self) -> Arc<ApprovalManager> {
        self.approval_manager.clone()
    }

    /// Update the security policy.
    pub async fn set_policy(&self, policy: SecurityPolicy) {
        let mut current = self.policy.write().await;
        *current = policy;
    }

    /// Get a clone of the current security policy.
    pub async fn get_policy(&self) -> SecurityPolicy {
        let policy = self.policy.read().await;
        policy.clone()
    }

    /// Check if a command is allowed to execute.
    ///
    /// This checks the command against the security policy in the following order:
    /// 1. Check blocklist - if matched, block the command
    /// 2. Check allowlist - if matched, allow the command
    /// 3. Check approval_required - if matched, require approval
    /// 4. Apply default action
    ///
    /// For commands that require approval, this creates an approval request
    /// and returns a result with `requires_approval = true`.
    pub async fn check_command(
        &self,
        command: &str,
        task_id: &str,
        agent_id: &str,
    ) -> anyhow::Result<SecurityCheckResult> {
        self.check_command_with_workdir(command, task_id, agent_id, None)
            .await
    }

    /// Check if a command is allowed to execute, with an optional working directory.
    pub async fn check_command_with_workdir(
        &self,
        command: &str,
        task_id: &str,
        agent_id: &str,
        workdir: Option<String>,
    ) -> anyhow::Result<SecurityCheckResult> {
        let policy = self.policy.read().await;
        let command_trimmed = command.trim();

        // Check blocklist first - always block
        for pattern in &policy.blocklist {
            if pattern.matches(command_trimmed) {
                return Ok(SecurityCheckResult::blocked(
                    format!("Command blocked by policy: {}", pattern.pattern),
                    Some(pattern.pattern.clone()),
                ));
            }
        }

        // Check allowlist - always allow
        for pattern in &policy.allowlist {
            if pattern.matches(command_trimmed) {
                return Ok(SecurityCheckResult::allowed(Some(format!(
                    "Command allowed by policy: {}",
                    pattern.pattern
                ))));
            }
        }

        // Check approval_required
        for pattern in &policy.approval_required {
            if pattern.matches(command_trimmed) {
                // Create an approval request
                let approval_id = self
                    .approval_manager
                    .create_approval(command, task_id, agent_id, workdir)
                    .await?;

                return Ok(SecurityCheckResult::requires_approval(
                    approval_id,
                    Some(format!(
                        "Command requires approval (matched: {})",
                        pattern.pattern
                    )),
                ));
            }
        }

        // Apply default action
        match policy.default_action {
            SecurityAction::Allow => Ok(SecurityCheckResult::allowed(Some(
                "Command allowed by default policy".to_string(),
            ))),
            SecurityAction::Block => Ok(SecurityCheckResult::blocked(
                "Command blocked by default policy".to_string(),
                None,
            )),
            SecurityAction::RequireApproval => {
                let approval_id = self
                    .approval_manager
                    .create_approval(command, task_id, agent_id, workdir)
                    .await?;

                Ok(SecurityCheckResult::requires_approval(
                    approval_id,
                    Some("Command requires approval by default policy".to_string()),
                ))
            }
        }
    }

    /// Check if a previously created approval has been granted.
    ///
    /// Returns an updated `SecurityCheckResult` based on the approval status.
    pub async fn check_approval(&self, approval_id: &str) -> anyhow::Result<SecurityCheckResult> {
        let status = self.approval_manager.check_status(approval_id).await;

        match status {
            Some(crate::models::security::ApprovalStatus::Approved) => {
                Ok(SecurityCheckResult::approved_result(approval_id.to_string()))
            }
            Some(crate::models::security::ApprovalStatus::Rejected) => {
                let approval = self.approval_manager.get(approval_id).await;
                let reason = approval
                    .and_then(|a| a.rejection_reason)
                    .unwrap_or_else(|| "User rejected".to_string());
                Ok(SecurityCheckResult::blocked(reason, None))
            }
            Some(crate::models::security::ApprovalStatus::Expired) => {
                Ok(SecurityCheckResult::blocked("Approval request expired".to_string(), None))
            }
            Some(crate::models::security::ApprovalStatus::Pending) => {
                Ok(SecurityCheckResult::requires_approval(
                    approval_id.to_string(),
                    Some("Waiting for user approval".to_string()),
                ))
            }
            None => Ok(SecurityCheckResult::blocked(
                "Approval request not found".to_string(),
                None,
            )),
        }
    }

    /// Quick check if a command would be allowed without creating an approval request.
    ///
    /// This is useful for UI previews or validation without side effects.
    pub async fn would_allow(&self, command: &str) -> SecurityAction {
        let policy = self.policy.read().await;
        let command_trimmed = command.trim();

        // Check blocklist first
        for pattern in &policy.blocklist {
            if pattern.matches(command_trimmed) {
                return SecurityAction::Block;
            }
        }

        // Check allowlist
        for pattern in &policy.allowlist {
            if pattern.matches(command_trimmed) {
                return SecurityAction::Allow;
            }
        }

        // Check approval_required
        for pattern in &policy.approval_required {
            if pattern.matches(command_trimmed) {
                return SecurityAction::RequireApproval;
            }
        }

        // Return default action
        policy.default_action
    }

    /// Add a pattern to the allowlist.
    pub async fn allow_pattern(&self, pattern: &str, description: Option<String>) {
        let mut policy = self.policy.write().await;
        let cmd_pattern = if let Some(desc) = description {
            crate::models::security::CommandPattern::with_description(pattern, desc)
        } else {
            crate::models::security::CommandPattern::new(pattern)
        };
        policy.allowlist.push(cmd_pattern);
    }

    /// Add a pattern to the blocklist.
    pub async fn block_pattern(&self, pattern: &str, description: Option<String>) {
        let mut policy = self.policy.write().await;
        let cmd_pattern = if let Some(desc) = description {
            crate::models::security::CommandPattern::with_description(pattern, desc)
        } else {
            crate::models::security::CommandPattern::new(pattern)
        };
        policy.blocklist.push(cmd_pattern);
    }

    /// Add a pattern to the approval_required list.
    pub async fn require_approval_pattern(&self, pattern: &str, description: Option<String>) {
        let mut policy = self.policy.write().await;
        let cmd_pattern = if let Some(desc) = description {
            crate::models::security::CommandPattern::with_description(pattern, desc)
        } else {
            crate::models::security::CommandPattern::new(pattern)
        };
        policy.approval_required.push(cmd_pattern);
    }

    /// Set the default action for commands that don't match any pattern.
    pub async fn set_default_action(&self, action: SecurityAction) {
        let mut policy = self.policy.write().await;
        policy.default_action = action;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_checker() -> SecurityChecker {
        SecurityChecker::with_defaults()
    }

    #[tokio::test]
    async fn test_checker_with_defaults() {
        let checker = create_test_checker();
        let policy = checker.get_policy().await;
        assert!(!policy.allowlist.is_empty());
        assert!(!policy.blocklist.is_empty());
    }

    #[tokio::test]
    async fn test_check_allowed_command() {
        let checker = create_test_checker();
        let result = checker
            .check_command("ls -la", "task-1", "agent-1")
            .await
            .unwrap();

        assert!(result.allowed);
        assert!(!result.requires_approval);
    }

    #[tokio::test]
    async fn test_check_blocked_command() {
        let checker = create_test_checker();
        let result = checker
            .check_command("rm -rf /", "task-1", "agent-1")
            .await
            .unwrap();

        assert!(!result.allowed);
        assert!(!result.requires_approval);
        assert!(result.reason.is_some());
    }

    #[tokio::test]
    async fn test_check_approval_required_command() {
        let checker = create_test_checker();
        let result = checker
            .check_command("rm file.txt", "task-1", "agent-1")
            .await
            .unwrap();

        assert!(!result.allowed);
        assert!(result.requires_approval);
        assert!(result.approval_id.is_some());
    }

    #[tokio::test]
    async fn test_check_unknown_command_default_action() {
        let checker = create_test_checker();
        // A command that doesn't match any pattern
        let result = checker
            .check_command("some-custom-command --flag", "task-1", "agent-1")
            .await
            .unwrap();

        // Default action is RequireApproval
        assert!(!result.allowed);
        assert!(result.requires_approval);
    }

    #[tokio::test]
    async fn test_check_approval_after_approve() {
        let checker = create_test_checker();
        let result = checker
            .check_command("rm file.txt", "task-1", "agent-1")
            .await
            .unwrap();

        let approval_id = result.approval_id.unwrap();

        // Approve the request
        checker
            .approval_manager()
            .approve(&approval_id)
            .await
            .unwrap();

        // Check the approval status
        let check_result = checker.check_approval(&approval_id).await.unwrap();
        assert!(check_result.allowed);
        assert!(check_result.approved);
    }

    #[tokio::test]
    async fn test_check_approval_after_reject() {
        let checker = create_test_checker();
        let result = checker
            .check_command("rm file.txt", "task-1", "agent-1")
            .await
            .unwrap();

        let approval_id = result.approval_id.unwrap();

        // Reject the request
        checker
            .approval_manager()
            .reject(&approval_id, Some("Not allowed".to_string()))
            .await
            .unwrap();

        // Check the approval status
        let check_result = checker.check_approval(&approval_id).await.unwrap();
        assert!(!check_result.allowed);
        assert!(check_result.reason.unwrap().contains("Not allowed"));
    }

    #[tokio::test]
    async fn test_check_approval_nonexistent() {
        let checker = create_test_checker();
        let result = checker.check_approval("nonexistent").await.unwrap();

        assert!(!result.allowed);
        assert!(result.reason.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_would_allow_allowed() {
        let checker = create_test_checker();
        let action = checker.would_allow("ls -la").await;
        assert_eq!(action, SecurityAction::Allow);
    }

    #[tokio::test]
    async fn test_would_allow_blocked() {
        let checker = create_test_checker();
        let action = checker.would_allow("rm -rf /").await;
        assert_eq!(action, SecurityAction::Block);
    }

    #[tokio::test]
    async fn test_would_allow_requires_approval() {
        let checker = create_test_checker();
        let action = checker.would_allow("rm file.txt").await;
        assert_eq!(action, SecurityAction::RequireApproval);
    }

    #[tokio::test]
    async fn test_add_allow_pattern() {
        let checker = create_test_checker();
        checker
            .allow_pattern("my-safe-command *", Some("Safe command".to_string()))
            .await;

        let action = checker.would_allow("my-safe-command --flag").await;
        assert_eq!(action, SecurityAction::Allow);
    }

    #[tokio::test]
    async fn test_add_block_pattern() {
        let checker = create_test_checker();
        checker
            .block_pattern("danger-command *", Some("Dangerous".to_string()))
            .await;

        let action = checker.would_allow("danger-command --flag").await;
        assert_eq!(action, SecurityAction::Block);
    }

    #[tokio::test]
    async fn test_add_require_approval_pattern() {
        let checker = create_test_checker();
        checker
            .require_approval_pattern("risky-command *", None)
            .await;

        let action = checker.would_allow("risky-command --flag").await;
        assert_eq!(action, SecurityAction::RequireApproval);
    }

    #[tokio::test]
    async fn test_set_default_action() {
        let checker = create_test_checker();

        // Set default to Allow
        checker.set_default_action(SecurityAction::Allow).await;

        // A command that doesn't match any pattern should now be allowed
        let action = checker.would_allow("random-unknown-command").await;
        assert_eq!(action, SecurityAction::Allow);
    }

    #[tokio::test]
    async fn test_set_policy() {
        let checker = create_test_checker();

        let new_policy = SecurityPolicy {
            default_action: SecurityAction::Block,
            ..Default::default()
        };

        checker.set_policy(new_policy).await;

        let policy = checker.get_policy().await;
        assert_eq!(policy.default_action, SecurityAction::Block);
    }

    #[tokio::test]
    async fn test_check_command_with_workdir() {
        let checker = create_test_checker();
        let result = checker
            .check_command_with_workdir(
                "rm file.txt",
                "task-1",
                "agent-1",
                Some("/tmp".to_string()),
            )
            .await
            .unwrap();

        assert!(result.requires_approval);

        // The approval should have the workdir
        let approval_id = result.approval_id.unwrap();
        let approval = checker.approval_manager().get(&approval_id).await.unwrap();
        assert_eq!(approval.workdir, Some("/tmp".to_string()));
    }

    #[tokio::test]
    async fn test_blocklist_takes_priority() {
        let checker = create_test_checker();

        // Add "rm *" to allowlist
        checker.allow_pattern("rm *", None).await;

        // But rm -rf /* is still blocked because blocklist is checked first
        let action = checker.would_allow("rm -rf /").await;
        assert_eq!(action, SecurityAction::Block);
    }

    #[tokio::test]
    async fn test_allowlist_takes_priority_over_approval() {
        let checker = create_test_checker();

        // Even though "rm *" is in approval_required by default,
        // if we add it to allowlist, it should be allowed
        checker.allow_pattern("rm test.txt", None).await;

        let action = checker.would_allow("rm test.txt").await;
        assert_eq!(action, SecurityAction::Allow);
    }

    #[tokio::test]
    async fn test_git_read_commands_allowed() {
        let checker = create_test_checker();

        assert_eq!(checker.would_allow("git status").await, SecurityAction::Allow);
        assert_eq!(checker.would_allow("git log --oneline").await, SecurityAction::Allow);
        assert_eq!(checker.would_allow("git diff HEAD").await, SecurityAction::Allow);
        assert_eq!(checker.would_allow("git branch -a").await, SecurityAction::Allow);
    }

    #[tokio::test]
    async fn test_git_write_commands_require_approval() {
        let checker = create_test_checker();

        assert_eq!(
            checker.would_allow("git push origin main").await,
            SecurityAction::RequireApproval
        );
        assert_eq!(
            checker.would_allow("git reset --hard HEAD~1").await,
            SecurityAction::RequireApproval
        );
    }

    #[tokio::test]
    async fn test_cargo_commands_allowed() {
        let checker = create_test_checker();

        assert_eq!(checker.would_allow("cargo test").await, SecurityAction::Allow);
        assert_eq!(checker.would_allow("cargo build --release").await, SecurityAction::Allow);
        assert_eq!(checker.would_allow("cargo check").await, SecurityAction::Allow);
        assert_eq!(checker.would_allow("cargo clippy").await, SecurityAction::Allow);
    }

    #[tokio::test]
    async fn test_cargo_publish_requires_approval() {
        let checker = create_test_checker();

        assert_eq!(
            checker.would_allow("cargo publish").await,
            SecurityAction::RequireApproval
        );
    }
}
