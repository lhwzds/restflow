//! SecurityQueryProvider adapter.

use crate::security::SecurityChecker;
use restflow_ai::tools::SecurityQueryProvider;
use restflow_tools::ToolError;
use serde_json::{Value, json};

pub struct SecurityQueryProviderAdapter;

impl Default for SecurityQueryProviderAdapter {
    fn default() -> Self {
        Self
    }
}

impl SecurityQueryProviderAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl SecurityQueryProvider for SecurityQueryProviderAdapter {
    fn show_policy(&self) -> restflow_tools::Result<Value> {
        let policy = crate::models::SecurityPolicy::default();
        Ok(serde_json::to_value(policy)?)
    }

    fn list_permissions(&self) -> restflow_tools::Result<Value> {
        let policy = crate::models::SecurityPolicy::default();
        Ok(json!({
            "default_action": policy.default_action,
            "allowlist_count": policy.allowlist.len(),
            "blocklist_count": policy.blocklist.len(),
            "approval_required_count": policy.approval_required.len(),
            "tool_rule_count": policy.tool_rules.len()
        }))
    }

    fn check_permission(
        &self,
        tool_name: &str,
        operation_name: &str,
        target: Option<&str>,
        summary: Option<&str>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = restflow_tools::Result<Value>> + Send + '_>> {
        let tool_name = tool_name.to_string();
        let operation_name = operation_name.to_string();
        let target = target.map(|s| s.to_string());
        let summary = summary.map(|s| s.to_string());
        Box::pin(async move {
            let checker = SecurityChecker::with_defaults();
            let target_str = target.unwrap_or_else(|| "*".to_string());
            let summary_str = summary
                .unwrap_or_else(|| format!("{}:{}", tool_name, operation_name));
            let ai_action = restflow_ai::ToolAction {
                tool_name: tool_name.clone(),
                operation: operation_name.clone(),
                target: target_str,
                summary: summary_str,
            };
            let decision = checker
                .check_tool_action(&ai_action, Some("runtime"), Some("runtime"))
                .await
                .map_err(|e| ToolError::Tool(e.to_string()))?;
            Ok(json!({
                "allowed": decision.allowed,
                "requires_approval": decision.requires_approval,
                "approval_id": decision.approval_id,
                "reason": decision.reason
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_policy_returns_valid_json() {
        let adapter = SecurityQueryProviderAdapter;
        let result = adapter.show_policy().unwrap();
        assert!(result.is_object(), "show_policy should return a JSON object");
        assert!(result.get("default_action").is_some());
    }

    #[test]
    fn test_list_permissions_contains_expected_fields() {
        let adapter = SecurityQueryProviderAdapter;
        let result = adapter.list_permissions().unwrap();
        assert!(result.get("default_action").is_some());
        assert!(result.get("allowlist_count").is_some());
        assert!(result.get("blocklist_count").is_some());
        assert!(result.get("approval_required_count").is_some());
        assert!(result.get("tool_rule_count").is_some());
    }

    #[tokio::test]
    async fn test_check_permission_returns_decision() {
        let adapter = SecurityQueryProviderAdapter;
        let result = adapter
            .check_permission("bash", "execute", Some("/bin/ls"), Some("list files"))
            .await
            .unwrap();
        assert!(result.get("allowed").is_some());
        assert!(result.get("requires_approval").is_some());
    }

    #[tokio::test]
    async fn test_check_permission_without_optionals() {
        let adapter = SecurityQueryProviderAdapter;
        let result = adapter
            .check_permission("http", "get", None, None)
            .await
            .unwrap();
        assert!(result.get("allowed").is_some());
    }
}
