//! Security management Tauri commands
//!
//! This module provides IPC commands for:
//! - Viewing and updating the security policy
//! - Managing pending approval requests
//! - Approving/rejecting commands that require approval
//! - Previewing command security status

use crate::state::AppState;
use restflow_core::models::security::{
    ApprovalStatus, PendingApproval, SecurityAction, SecurityPolicy, ToolRule,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

// ============================================================================
// Response Types
// ============================================================================

/// Result of a security check preview
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SecurityCheckPreview {
    /// The action that would be taken
    pub action: SecurityAction,
    /// Human-readable description
    pub description: String,
}

/// Summary of security status
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SecuritySummary {
    /// Number of patterns in the allowlist
    pub allowlist_count: usize,
    /// Number of patterns in the blocklist
    pub blocklist_count: usize,
    /// Number of patterns requiring approval
    pub approval_required_count: usize,
    /// Number of tool-specific rules
    pub tool_rule_count: usize,
    /// Number of pending approvals
    pub pending_approvals_count: usize,
    /// The default action for unmatched commands
    pub default_action: SecurityAction,
}

// ============================================================================
// Request Types
// ============================================================================

/// Request to add a command pattern to a list
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct AddPatternRequest {
    /// The pattern to add (glob-style)
    pub pattern: String,
    /// Optional description
    pub description: Option<String>,
}

/// Request to reject a command
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RejectRequest {
    /// The approval ID
    pub approval_id: String,
    /// Optional rejection reason
    pub reason: Option<String>,
}

// ============================================================================
// Policy Commands
// ============================================================================

/// Get the current security policy
#[tauri::command]
pub async fn get_security_policy(state: State<'_, AppState>) -> Result<SecurityPolicy, String> {
    Ok(state.security_checker().get_policy().await)
}

/// Update the security policy
#[tauri::command]
pub async fn update_security_policy(
    state: State<'_, AppState>,
    policy: SecurityPolicy,
) -> Result<SecurityPolicy, String> {
    state.security_checker().set_policy(policy).await;
    Ok(state.security_checker().get_policy().await)
}

/// Get a summary of the security configuration
#[tauri::command]
pub async fn get_security_summary(state: State<'_, AppState>) -> Result<SecuritySummary, String> {
    let policy = state.security_checker().get_policy().await;
    let pending = state
        .security_checker()
        .approval_manager()
        .get_all_pending()
        .await;

    Ok(SecuritySummary {
        allowlist_count: policy.allowlist.len(),
        blocklist_count: policy.blocklist.len(),
        approval_required_count: policy.approval_required.len(),
        tool_rule_count: policy.tool_rules.len(),
        pending_approvals_count: pending.len(),
        default_action: policy.default_action,
    })
}

/// Set the default action for commands that don't match any pattern
#[tauri::command]
pub async fn set_default_security_action(
    state: State<'_, AppState>,
    action: SecurityAction,
) -> Result<(), String> {
    state.security_checker().set_default_action(action).await;
    Ok(())
}

// ============================================================================
// Pattern Management Commands
// ============================================================================

/// Add a pattern to the allowlist
#[tauri::command]
pub async fn add_allowlist_pattern(
    state: State<'_, AppState>,
    request: AddPatternRequest,
) -> Result<SecurityPolicy, String> {
    state
        .security_checker()
        .allow_pattern(&request.pattern, request.description)
        .await;
    Ok(state.security_checker().get_policy().await)
}

/// Add a pattern to the blocklist
#[tauri::command]
pub async fn add_blocklist_pattern(
    state: State<'_, AppState>,
    request: AddPatternRequest,
) -> Result<SecurityPolicy, String> {
    state
        .security_checker()
        .block_pattern(&request.pattern, request.description)
        .await;
    Ok(state.security_checker().get_policy().await)
}

/// Add a pattern to the approval-required list
#[tauri::command]
pub async fn add_approval_required_pattern(
    state: State<'_, AppState>,
    request: AddPatternRequest,
) -> Result<SecurityPolicy, String> {
    state
        .security_checker()
        .require_approval_pattern(&request.pattern, request.description)
        .await;
    Ok(state.security_checker().get_policy().await)
}

/// Remove a pattern from the allowlist by index
#[tauri::command]
pub async fn remove_allowlist_pattern(
    state: State<'_, AppState>,
    index: usize,
) -> Result<SecurityPolicy, String> {
    let mut policy = state.security_checker().get_policy().await;
    if index < policy.allowlist.len() {
        policy.allowlist.remove(index);
        state.security_checker().set_policy(policy).await;
    }
    Ok(state.security_checker().get_policy().await)
}

/// Remove a pattern from the blocklist by index
#[tauri::command]
pub async fn remove_blocklist_pattern(
    state: State<'_, AppState>,
    index: usize,
) -> Result<SecurityPolicy, String> {
    let mut policy = state.security_checker().get_policy().await;
    if index < policy.blocklist.len() {
        policy.blocklist.remove(index);
        state.security_checker().set_policy(policy).await;
    }
    Ok(state.security_checker().get_policy().await)
}

/// Remove a pattern from the approval-required list by index
#[tauri::command]
pub async fn remove_approval_required_pattern(
    state: State<'_, AppState>,
    index: usize,
) -> Result<SecurityPolicy, String> {
    let mut policy = state.security_checker().get_policy().await;
    if index < policy.approval_required.len() {
        policy.approval_required.remove(index);
        state.security_checker().set_policy(policy).await;
    }
    Ok(state.security_checker().get_policy().await)
}

// ============================================================================
// Tool Rule Commands
// ============================================================================

/// Add a tool rule to the policy
#[tauri::command]
pub async fn add_tool_rule(
    state: State<'_, AppState>,
    rule: ToolRule,
) -> Result<SecurityPolicy, String> {
    let mut policy = state.security_checker().get_policy().await;
    policy.tool_rules.push(rule);
    state.security_checker().set_policy(policy).await;
    Ok(state.security_checker().get_policy().await)
}

/// Remove a tool rule by ID
#[tauri::command]
pub async fn remove_tool_rule(
    state: State<'_, AppState>,
    rule_id: String,
) -> Result<SecurityPolicy, String> {
    let mut policy = state.security_checker().get_policy().await;
    policy.tool_rules.retain(|rule| rule.id != rule_id);
    state.security_checker().set_policy(policy).await;
    Ok(state.security_checker().get_policy().await)
}

/// List all tool rules
#[tauri::command]
pub async fn list_tool_rules(state: State<'_, AppState>) -> Result<Vec<ToolRule>, String> {
    let policy = state.security_checker().get_policy().await;
    Ok(policy.tool_rules)
}

// ============================================================================
// Approval Management Commands
// ============================================================================

/// List all pending approval requests
#[tauri::command]
pub async fn list_pending_approvals(
    state: State<'_, AppState>,
) -> Result<Vec<PendingApproval>, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .get_all_pending()
        .await)
}

/// Get a specific pending approval by ID
#[tauri::command]
pub async fn get_pending_approval(
    state: State<'_, AppState>,
    approval_id: String,
) -> Result<Option<PendingApproval>, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .get(&approval_id)
        .await)
}

/// Approve a pending command
#[tauri::command]
pub async fn approve_command(
    state: State<'_, AppState>,
    approval_id: String,
) -> Result<bool, String> {
    let result = state
        .security_checker()
        .approval_manager()
        .approve(&approval_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.is_some())
}

/// Reject a pending command
#[tauri::command]
pub async fn reject_command(
    state: State<'_, AppState>,
    request: RejectRequest,
) -> Result<bool, String> {
    let result = state
        .security_checker()
        .approval_manager()
        .reject(&request.approval_id, request.reason)
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.is_some())
}

/// Get pending approvals for a specific task
#[tauri::command]
pub async fn get_task_pending_approvals(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<PendingApproval>, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .get_for_task(&task_id)
        .await)
}

/// Get pending approvals for a specific agent
#[tauri::command]
pub async fn get_agent_pending_approvals(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<PendingApproval>, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .get_for_agent(&agent_id)
        .await)
}

/// Check the status of an approval request
#[tauri::command]
pub async fn check_approval_status(
    state: State<'_, AppState>,
    approval_id: String,
) -> Result<Option<ApprovalStatus>, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .check_status(&approval_id)
        .await)
}

/// Remove a resolved approval from the manager
#[tauri::command]
pub async fn remove_approval(
    state: State<'_, AppState>,
    approval_id: String,
) -> Result<Option<PendingApproval>, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .remove(&approval_id)
        .await)
}

/// Clean up expired approvals
#[tauri::command]
pub async fn cleanup_expired_approvals(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state
        .security_checker()
        .approval_manager()
        .cleanup_expired()
        .await)
}

// ============================================================================
// Preview Commands
// ============================================================================

/// Preview what action would be taken for a command (without creating an approval request)
#[tauri::command]
pub async fn preview_command_security(
    state: State<'_, AppState>,
    command: String,
) -> Result<SecurityCheckPreview, String> {
    let action = state.security_checker().would_allow(&command).await;

    let description = match action {
        SecurityAction::Allow => "Command would be allowed immediately".to_string(),
        SecurityAction::Block => "Command would be blocked".to_string(),
        SecurityAction::RequireApproval => "Command would require user approval".to_string(),
    };

    Ok(SecurityCheckPreview {
        action,
        description,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_pattern_request_deserialization() {
        let json = r#"{"pattern": "rm *", "description": "Remove files"}"#;
        let request: AddPatternRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.pattern, "rm *");
        assert_eq!(request.description, Some("Remove files".to_string()));
    }

    #[test]
    fn test_add_pattern_request_minimal() {
        let json = r#"{"pattern": "ls *"}"#;
        let request: AddPatternRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.pattern, "ls *");
        assert!(request.description.is_none());
    }

    #[test]
    fn test_reject_request_deserialization() {
        let json = r#"{"approval_id": "abc123", "reason": "Not allowed"}"#;
        let request: RejectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.approval_id, "abc123");
        assert_eq!(request.reason, Some("Not allowed".to_string()));
    }

    #[test]
    fn test_reject_request_minimal() {
        let json = r#"{"approval_id": "abc123"}"#;
        let request: RejectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.approval_id, "abc123");
        assert!(request.reason.is_none());
    }

    #[test]
    fn test_security_check_preview_serialization() {
        let preview = SecurityCheckPreview {
            action: SecurityAction::Allow,
            description: "Command allowed".to_string(),
        };
        let json = serde_json::to_string(&preview).unwrap();
        assert!(json.contains("allow"));
        assert!(json.contains("Command allowed"));
    }

    #[test]
    fn test_security_summary_serialization() {
        let summary = SecuritySummary {
            allowlist_count: 5,
            blocklist_count: 3,
            approval_required_count: 2,
            tool_rule_count: 1,
            pending_approvals_count: 1,
            default_action: SecurityAction::RequireApproval,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"allowlist_count\":5"));
        assert!(json.contains("\"blocklist_count\":3"));
        assert!(json.contains("require_approval"));
        assert!(json.contains("\"tool_rule_count\":1"));
    }
}
