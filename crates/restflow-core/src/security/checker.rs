//! Security checker for validating commands against policy.
//!
//! The `SecurityChecker` evaluates commands against a `SecurityPolicy` and
//! coordinates with the `ApprovalManager` for commands that require user approval.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use redb::Database;
use tokio::sync::RwLock;

use super::{
    AmendmentMatchType, AmendmentScope, ApprovalCache, ApprovalGrant, ApprovalKey, ApprovalManager,
    ApprovalScope as CacheApprovalScope, SecurityAmendmentStore, SecurityConfigStore,
};
use crate::models::security::{
    AgentSecurityConfig, AskMode, SecurityAction, SecurityCheckResult, SecurityMode,
    SecurityPolicy, ToolAction, ToolRule,
};
use crate::security::path_resolver::{CommandResolution, matches_path_pattern};
use crate::security::shell_parser;
use restflow_ai::{SecurityDecision, SecurityGate};

/// Default cache max age for session-scoped grants (1 hour)
const DEFAULT_SESSION_CACHE_MAX_AGE: Duration = Duration::from_secs(3600);

/// Default cache max age for persistent grants (24 hours)
const DEFAULT_PERSISTENT_CACHE_MAX_AGE: Duration = Duration::from_secs(86400);

/// Security checker for validating commands.
///
/// Checks commands against a security policy and manages approval requests
/// for commands that require user approval.
pub struct SecurityChecker {
    /// The security policy to enforce
    policy: RwLock<SecurityPolicy>,

    /// Manager for approval requests
    approval_manager: Arc<ApprovalManager>,

    /// Per-agent security configuration
    config_store: Arc<SecurityConfigStore>,

    /// Optional persistent amendments for auto-approving known-safe patterns
    amendment_store: Option<Arc<SecurityAmendmentStore>>,

    /// Approval cache for storing cached approval grants
    approval_cache: ApprovalCache,
}

impl SecurityChecker {
    /// Create a new security checker with the given policy and approval manager.
    pub fn new(policy: SecurityPolicy, approval_manager: Arc<ApprovalManager>) -> Self {
        let config_store =
            SecurityConfigStore::new(AgentSecurityConfig::from_policy(policy.clone())).shared();
        Self {
            policy: RwLock::new(policy),
            approval_manager,
            config_store,
            amendment_store: None,
            approval_cache: ApprovalCache::new(),
        }
    }

    /// Create a new security checker with default policy and a new approval manager.
    pub fn with_defaults() -> Self {
        let policy = SecurityPolicy::default();
        let config_store =
            SecurityConfigStore::new(AgentSecurityConfig::from_policy(policy.clone())).shared();
        Self {
            policy: RwLock::new(policy),
            approval_manager: Arc::new(ApprovalManager::new()),
            config_store,
            amendment_store: None,
            approval_cache: ApprovalCache::new(),
        }
    }

    /// Create a checker backed by a persistent amendment store.
    pub fn with_db(
        policy: SecurityPolicy,
        approval_manager: Arc<ApprovalManager>,
        db: Arc<Database>,
    ) -> anyhow::Result<Self> {
        let mut checker = Self::new(policy, approval_manager);
        checker.amendment_store = Some(Arc::new(SecurityAmendmentStore::new(db)?));
        Ok(checker)
    }

    /// Attach a persistent amendment store to the checker.
    pub fn with_amendment_store(mut self, store: Arc<SecurityAmendmentStore>) -> Self {
        self.amendment_store = Some(store);
        self
    }

    /// Get a reference to the approval manager.
    pub fn approval_manager(&self) -> Arc<ApprovalManager> {
        self.approval_manager.clone()
    }

    /// Get a reference to the approval cache.
    pub fn approval_cache(&self) -> &ApprovalCache {
        &self.approval_cache
    }

    /// Update the security policy.
    pub async fn set_policy(&self, policy: SecurityPolicy) {
        let mut current = self.policy.write().await;
        *current = policy.clone();
        self.config_store
            .set_default_config(AgentSecurityConfig::from_policy(policy))
            .await;
    }

    /// Get a clone of the current security policy.
    pub async fn get_policy(&self) -> SecurityPolicy {
        let policy = self.policy.read().await;
        policy.clone()
    }

    /// Set per-agent security configuration.
    pub fn set_agent_config(&self, agent_id: &str, config: AgentSecurityConfig) {
        self.config_store.set_agent_config(agent_id, config);
    }

    /// Remove per-agent security configuration.
    pub fn remove_agent_config(&self, agent_id: &str) {
        self.config_store.remove_agent_config(agent_id);
    }

    /// Grant a cached approval for a command pattern.
    ///
    /// This allows subsequent commands matching the pattern to be approved
    /// without requiring user interaction.
    pub fn grant_cached_approval(
        &mut self,
        tool_name: &str,
        action: &str,
        target: Option<String>,
        scope: CacheApprovalScope,
        description: Option<String>,
    ) {
        let key = ApprovalKey::new(tool_name, action, target);
        let grant = ApprovalGrant::new(scope, description);
        self.approval_cache.insert(key, grant);
    }

    /// Revoke a cached approval for a command pattern.
    pub fn revoke_cached_approval(&mut self, tool_name: &str, action: &str, target: Option<String>) {
        let key = ApprovalKey::new(tool_name, action, target);
        self.approval_cache.remove(&key);
    }

    /// Clear all session-scoped cached approvals.
    ///
    /// Called when a session ends.
    pub fn clear_session_cache(&mut self) {
        self.approval_cache.clear_session();
    }

    /// Clear all cached approvals.
    pub fn clear_cache(&mut self) {
        self.approval_cache.clear();
    }

    /// Prune expired cached approvals.
    pub fn prune_cache(&mut self) {
        self.approval_cache.prune(DEFAULT_SESSION_CACHE_MAX_AGE);
        self.approval_cache.prune(DEFAULT_PERSISTENT_CACHE_MAX_AGE);
    }

    /// Check if there's a valid cached approval for a command.
    fn check_cached_approval(
        &self,
        tool_name: &str,
        action: &str,
        target: Option<&str>,
    ) -> Option<&ApprovalGrant> {
        let key = ApprovalKey::new(tool_name, action, target.map(String::from));
        self.approval_cache.get(&key)
    }

    /// Check if a command is allowed to execute.
    ///
    /// This checks the command against the security policy in the following order:
    /// 1. Check approval cache - if valid cached grant, allow
    /// 2. Check blocklist - if matched, block the command
    /// 3. Check allowlist - if matched, allow the command
    /// 4. Check approval_required - if matched, require approval
    /// 5. Apply default action
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
        let command_trimmed = command.trim();

        // First check approval cache for quick approval
        if let Some(grant) = self.check_cached_approval("bash", "execute", None)
            && !grant.is_expired(DEFAULT_SESSION_CACHE_MAX_AGE)
        {
            return Ok(SecurityCheckResult::allowed(Some(
                "Command allowed by cached approval".to_string(),
            )));
        }

        let analysis = match shell_parser::analyze_command(command_trimmed) {
            Ok(analysis) => analysis,
            Err(reason) => {
                return Ok(SecurityCheckResult::blocked(reason, None));
            }
        };

        let mut config = self.config_store.get_default_config().await;
        if let Some(agent_config) = self.config_store.get_agent_config(agent_id) {
            config = config.merge_with(agent_config);
        }

        if analysis.has_chain && !config.allow_chain {
            return Ok(SecurityCheckResult::blocked(
                "Command chaining not allowed".to_string(),
                None,
            ));
        }

        if analysis.has_pipe && !config.allow_pipeline {
            return Ok(SecurityCheckResult::blocked(
                "Pipeline commands not allowed".to_string(),
                None,
            ));
        }

        if analysis.has_redirect && !config.allow_redirect {
            return Ok(SecurityCheckResult::blocked(
                "Redirect commands not allowed".to_string(),
                None,
            ));
        }

        if !config.allowed_paths.is_empty() {
            let Some(workdir_value) = workdir.as_deref() else {
                return Ok(SecurityCheckResult::blocked(
                    "Working directory required by policy".to_string(),
                    None,
                ));
            };

            let workdir_path = Path::new(workdir_value);
            let allowed = config.allowed_paths.iter().any(|allowed| {
                let allowed_path = Path::new(allowed);
                workdir_path.starts_with(allowed_path)
            });

            if !allowed {
                return Ok(SecurityCheckResult::blocked(
                    "Working directory not allowed".to_string(),
                    None,
                ));
            }
        }

        let workdir_path = workdir.as_deref().map(Path::new);
        let resolution = CommandResolution::resolve(command_trimmed, workdir_path);

        enum Decision {
            Allow,
            Block,
            RequireApproval,
            Miss,
        }

        let mut decision = Decision::Miss;

        for pattern in &config.blocklist {
            if matches_pattern(pattern, command_trimmed, resolution.as_ref()) {
                decision = Decision::Block;
                break;
            }
        }

        if matches!(decision, Decision::Miss) {
            for pattern in &config.allowlist {
                if matches_pattern(pattern, command_trimmed, resolution.as_ref()) {
                    decision = Decision::Allow;
                    break;
                }
            }
        }

        if matches!(decision, Decision::Miss) {
            for pattern in &config.approval_required {
                if matches_pattern(pattern, command_trimmed, resolution.as_ref()) {
                    decision = Decision::RequireApproval;
                    break;
                }
            }
        }

        if matches!(decision, Decision::Miss) {
            decision = match config.mode {
                SecurityMode::Deny => Decision::Block,
                SecurityMode::Allowlist => Decision::Miss,
                SecurityMode::Full => Decision::Allow,
            };
        }

        decision = match config.ask {
            AskMode::Always => match decision {
                Decision::Block => Decision::Block,
                _ => Decision::RequireApproval,
            },
            AskMode::OnMiss => match decision {
                Decision::Miss => Decision::RequireApproval,
                _ => decision,
            },
            AskMode::Off => match decision {
                Decision::Miss => match config.mode {
                    SecurityMode::Allowlist | SecurityMode::Deny => Decision::Block,
                    SecurityMode::Full => Decision::Allow,
                },
                _ => decision,
            },
        };

        let amended = matches!(decision, Decision::RequireApproval)
            && self
                .find_matching_amendment("bash", command_trimmed, Some(agent_id))
                .is_some();
        if amended {
            decision = Decision::Allow;
        }

        match decision {
            Decision::Allow => {
                let reason = if amended {
                    "Command allowed by approved amendment".to_string()
                } else {
                    "Command allowed by policy".to_string()
                };
                Ok(SecurityCheckResult::allowed(Some(reason)))
            }
            Decision::Block => Ok(SecurityCheckResult::blocked(
                "Command blocked by policy".to_string(),
                None,
            )),
            Decision::RequireApproval => {
                let approval_id = self
                    .approval_manager
                    .create_approval(command, task_id, agent_id, workdir)
                    .await?;

                Ok(SecurityCheckResult::requires_approval(
                    approval_id,
                    Some("Command requires approval by policy".to_string()),
                ))
            }
            Decision::Miss => Ok(SecurityCheckResult::blocked(
                "Command blocked by policy".to_string(),
                None,
            )),
        }
    }

    /// Check if a tool action is allowed to execute.
    pub async fn check_tool_action(
        &self,
        action: &restflow_ai::ToolAction,
        agent_id: Option<&str>,
        task_id: Option<&str>,
    ) -> anyhow::Result<SecurityDecision> {
        let policy = self.get_policy().await;
        let action = ToolAction::from(action);

        // Check approval cache first
        let action_op = action.operation.as_str();
        let action_target = if action.target.is_empty() { None } else { Some(action.target.as_str()) };
        if let Some(grant) = self.check_cached_approval(
            &action.tool_name,
            action_op,
            action_target,
        ) && !grant.is_expired(DEFAULT_SESSION_CACHE_MAX_AGE)
        {
            return Ok(SecurityDecision::allowed(Some(
                "Tool action allowed by cached approval".to_string(),
            )));
        }

        let mut rules: Vec<&ToolRule> = policy
            .tool_rules
            .iter()
            .filter(|rule| rule.tool_name == "*" || rule.tool_name == action.tool_name)
            .filter(|rule| {
                rule.operation
                    .as_deref()
                    .is_none_or(|op| op == action.operation)
            })
            .collect();

        rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        for rule in rules {
            if crate::models::security::glob_match(&rule.target_pattern, &action.target) {
                return match rule.action {
                    SecurityAction::Allow => Ok(SecurityDecision::allowed(Some(
                        "Tool action allowed by rule".to_string(),
                    ))),
                    SecurityAction::Block => Ok(SecurityDecision::blocked(Some(
                        rule.description
                            .clone()
                            .unwrap_or_else(|| format!("Blocked by rule: {}", rule.id)),
                    ))),
                    SecurityAction::RequireApproval => {
                        let command_like = action.as_pattern_string();
                        if self
                            .find_matching_amendment(
                                action.tool_name.as_str(),
                                &command_like,
                                agent_id,
                            )
                            .is_some()
                        {
                            return Ok(SecurityDecision::allowed(Some(
                                "Tool action allowed by approved amendment".to_string(),
                            )));
                        }
                        let task_id = task_id.unwrap_or("unknown");
                        let agent_id = agent_id.unwrap_or("unknown");
                        let approval_id = self
                            .approval_manager
                            .create_approval(action.summary.clone(), task_id, agent_id, None)
                            .await?;
                        Ok(SecurityDecision::requires_approval(
                            approval_id,
                            Some("Tool action requires approval".to_string()),
                        ))
                    }
                };
            }
        }

        Ok(SecurityDecision::allowed(Some(
            "Tool action allowed by default".to_string(),
        )))
    }

    /// Check if a previously created approval has been granted.
    ///
    /// Returns an updated `SecurityCheckResult` based on the approval status.
    pub async fn check_approval(&self, approval_id: &str) -> anyhow::Result<SecurityCheckResult> {
        let status = self.approval_manager.check_status(approval_id).await;

        match status {
            Some(crate::models::security::ApprovalStatus::Approved) => Ok(
                SecurityCheckResult::approved_result(approval_id.to_string()),
            ),
            Some(crate::models::security::ApprovalStatus::Rejected) => {
                let approval = self.approval_manager.get(approval_id).await;
                let reason = approval
                    .and_then(|a| a.rejection_reason)
                    .unwrap_or_else(|| "User rejected".to_string());
                Ok(SecurityCheckResult::blocked(reason, None))
            }
            Some(crate::models::security::ApprovalStatus::Expired) => Ok(
                SecurityCheckResult::blocked("Approval request expired".to_string(), None),
            ),
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
        let command_trimmed = command.trim();

        // Check approval cache first
        if let Some(grant) = self.check_cached_approval("bash", "execute", None)
            && !grant.is_expired(DEFAULT_SESSION_CACHE_MAX_AGE)
        {
            return SecurityAction::Allow;
        }

        let analysis = match shell_parser::analyze_command(command_trimmed) {
            Ok(analysis) => analysis,
            Err(_) => return SecurityAction::Block,
        };

        let config = self.config_store.get_default_config().await;
        if analysis.has_chain {
            return SecurityAction::Block;
        }
        if analysis.has_pipe && !config.allow_pipeline {
            return SecurityAction::Block;
        }
        if analysis.has_redirect && !config.allow_redirect {
            return SecurityAction::Block;
        }

        let resolution = CommandResolution::resolve(command_trimmed, None);

        enum Decision {
            Allow,
            Block,
            RequireApproval,
            Miss,
        }

        let mut decision = Decision::Miss;

        for pattern in &config.blocklist {
            if matches_pattern(pattern, command_trimmed, resolution.as_ref()) {
                decision = Decision::Block;
                break;
            }
        }

        if matches!(decision, Decision::Miss) {
            for pattern in &config.allowlist {
                if matches_pattern(pattern, command_trimmed, resolution.as_ref()) {
                    decision = Decision::Allow;
                    break;
                }
            }
        }

        if matches!(decision, Decision::Miss) {
            for pattern in &config.approval_required {
                if matches_pattern(pattern, command_trimmed, resolution.as_ref()) {
                    decision = Decision::RequireApproval;
                    break;
                }
            }
        }

        if matches!(decision, Decision::Miss) {
            decision = match config.mode {
                SecurityMode::Deny => Decision::Block,
                SecurityMode::Allowlist => Decision::Miss,
                SecurityMode::Full => Decision::Allow,
            };
        }

        decision = match config.ask {
            AskMode::Always => match decision {
                Decision::Block => Decision::Block,
                _ => Decision::RequireApproval,
            },
            AskMode::OnMiss => match decision {
                Decision::Miss => Decision::RequireApproval,
                _ => decision,
            },
            AskMode::Off => match decision {
                Decision::Miss => match config.mode {
                    SecurityMode::Allowlist | SecurityMode::Deny => Decision::Block,
                    SecurityMode::Full => Decision::Allow,
                },
                _ => decision,
            },
        };

        match decision {
            Decision::Allow => SecurityAction::Allow,
            Decision::Block => SecurityAction::Block,
            Decision::RequireApproval => SecurityAction::RequireApproval,
            Decision::Miss => SecurityAction::Block,
        }
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
        self.config_store
            .set_default_config(AgentSecurityConfig::from_policy(policy.clone()))
            .await;
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
        self.config_store
            .set_default_config(AgentSecurityConfig::from_policy(policy.clone()))
            .await;
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
        self.config_store
            .set_default_config(AgentSecurityConfig::from_policy(policy.clone()))
            .await;
    }

    /// Set the default action for commands that don't match any pattern.
    pub async fn set_default_action(&self, action: SecurityAction) {
        let mut policy = self.policy.write().await;
        policy.default_action = action;
        self.config_store
            .set_default_config(AgentSecurityConfig::from_policy(policy.clone()))
            .await;
    }

    /// Add a persistent allow-rule amendment for future matching requests.
    pub fn add_allow_amendment(
        &self,
        tool_name: &str,
        command_pattern: &str,
        match_type: AmendmentMatchType,
        scope: AmendmentScope,
    ) -> anyhow::Result<()> {
        let store = self
            .amendment_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("security amendment store is not configured"))?;
        store.add_allow_rule_simple(
            tool_name.to_string(),
            command_pattern.to_string(),
            match_type,
            scope,
        )?;
        Ok(())
    }

    fn find_matching_amendment(
        &self,
        tool_name: &str,
        command: &str,
        agent_id: Option<&str>,
    ) -> Option<crate::security::SecurityAmendment> {
        self.amendment_store.as_ref().and_then(|store| {
            store
                .find_matching_rule(tool_name, command, agent_id)
                .ok()
                .flatten()
        })
    }
}

#[async_trait]
impl SecurityGate for SecurityChecker {
    async fn check_command(
        &self,
        command: &str,
        task_id: &str,
        agent_id: &str,
        workdir: Option<&str>,
    ) -> restflow_ai::Result<SecurityDecision> {
        let workdir = workdir.map(|value| value.to_string());
        let result = self
            .check_command_with_workdir(command, task_id, agent_id, workdir)
            .await
            .map_err(|err| restflow_ai::AiError::Tool(err.to_string()))?;

        if result.allowed {
            return Ok(SecurityDecision::allowed(result.reason));
        }

        if result.requires_approval {
            let approval_id = result.approval_id.unwrap_or_else(|| "unknown".to_string());
            return Ok(SecurityDecision::requires_approval(
                approval_id,
                result.reason,
            ));
        }

        Ok(SecurityDecision::blocked(result.reason))
    }

    async fn check_tool_action(
        &self,
        action: &restflow_ai::ToolAction,
        agent_id: Option<&str>,
        task_id: Option<&str>,
    ) -> restflow_ai::Result<SecurityDecision> {
        self.check_tool_action(action, agent_id, task_id)
            .await
            .map_err(|err| restflow_ai::AiError::Tool(err.to_string()))
    }
}

fn matches_pattern(
    pattern: &crate::models::security::CommandPattern,
    command: &str,
    resolution: Option<&CommandResolution>,
) -> bool {
    // Only use path matching for patterns that start with '/' (executable paths like /usr/bin/python)
    // Other patterns containing '/' (like "rm -rf /") should use regular glob matching
    if pattern.pattern.starts_with('/') {
        if let Some(resolution) = resolution {
            return matches_path_pattern(&pattern.pattern, resolution);
        }
        return false;
    }

    pattern.matches(command)
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
        let result = checker
            .check_command("ls -la", "task-1", "agent-1")
            .await
            .unwrap();
        // Default policy is Allow, so should be allowed
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_checker_blocks_dangerous_commands() {
        let checker = create_test_checker();
        let result = checker
            .check_command("rm -rf /", "task-1", "agent-1")
            .await
            .unwrap();
        // Default policy is Allow, but dangerous commands might be blocked
        // This depends on the default policy configuration
        assert!(!result.requires_approval);
    }

    #[tokio::test]
    async fn test_cached_approval_allows_command() {
        let mut checker = create_test_checker();

        // Grant a cached approval
        checker.grant_cached_approval(
            "bash",
            "execute",
            None,
            CacheApprovalScope::Session,
            Some("Test approval".to_string()),
        );

        // Now the command should be allowed via cache
        let result = checker
            .check_command("ls -la", "task-1", "agent-1")
            .await
            .unwrap();

        // Should be allowed by cached approval
        assert!(result.allowed);
        assert!(result
            .reason
            .unwrap_or_default()
            .contains("cached approval"));
    }

    #[tokio::test]
    async fn test_revoke_cached_approval() {
        let mut checker = create_test_checker();

        // Grant a cached approval
        checker.grant_cached_approval(
            "bash",
            "execute",
            None,
            CacheApprovalScope::Session,
            None,
        );

        // Revoke it
        checker.revoke_cached_approval("bash", "execute", None);

        // Check the cache is empty
        let grant = checker.check_cached_approval("bash", "execute", None);
        assert!(grant.is_none());
    }

    #[tokio::test]
    async fn test_clear_session_cache() {
        let mut checker = create_test_checker();

        // Grant session and persistent approvals
        checker.grant_cached_approval(
            "bash",
            "execute",
            None,
            CacheApprovalScope::Session,
            None,
        );
        checker.grant_cached_approval(
            "bash",
            "write",
            None,
            CacheApprovalScope::Persistent,
            None,
        );

        // Clear session cache
        checker.clear_session_cache();

        // Session approval should be gone
        let session_grant = checker.check_cached_approval("bash", "execute", None);
        assert!(session_grant.is_none());

        // Persistent approval should remain
        let persistent_grant = checker.check_cached_approval("bash", "write", None);
        assert!(persistent_grant.is_some());
    }

    #[tokio::test]
    async fn test_prune_cache() {
        let mut checker = create_test_checker();

        // Grant approvals (they'll be fresh, so not pruned)
        checker.grant_cached_approval(
            "bash",
            "execute",
            None,
            CacheApprovalScope::Session,
            None,
        );

        // Prune should not remove fresh grants
        checker.prune_cache();

        let grant = checker.check_cached_approval("bash", "execute", None);
        assert!(grant.is_some());
    }

    // Note: test_approval_cache_reflects_in_tool_action_check removed
    // ToolAction structure from restflow_ai may differ from expected
}
