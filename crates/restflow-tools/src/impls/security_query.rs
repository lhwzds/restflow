//! Security query tool for inspecting security policy and checking permissions.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::SecurityQueryProvider;

pub struct SecurityQueryTool {
    provider: Arc<dyn SecurityQueryProvider>,
}

impl SecurityQueryTool {
    pub fn new(provider: Arc<dyn SecurityQueryProvider>) -> Self {
        Self { provider }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum SecurityQueryOperation {
    CheckPermission {
        tool_name: String,
        operation_name: String,
        #[serde(default)]
        target: Option<String>,
        #[serde(default)]
        summary: Option<String>,
    },
    ListPermissions,
    ShowPolicy,
    RequestElevation {
        reason: String,
    },
}

#[async_trait]
impl Tool for SecurityQueryTool {
    fn name(&self) -> &str {
        "security_query"
    }

    fn description(&self) -> &str {
        "Inspect default security policy and evaluate whether an action would require approval."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["check_permission", "list_permissions", "show_policy", "request_elevation"]
                },
                "tool_name": { "type": "string" },
                "operation_name": { "type": "string" },
                "target": { "type": "string" },
                "summary": { "type": "string" },
                "reason": { "type": "string" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let operation: SecurityQueryOperation = serde_json::from_value(input)?;
        match operation {
            SecurityQueryOperation::ShowPolicy => {
                let policy = self.provider.show_policy()?;
                Ok(ToolOutput::success(policy))
            }
            SecurityQueryOperation::ListPermissions => {
                let perms = self.provider.list_permissions()?;
                Ok(ToolOutput::success(perms))
            }
            SecurityQueryOperation::CheckPermission {
                tool_name,
                operation_name,
                target,
                summary,
            } => {
                let result = self
                    .provider
                    .check_permission(
                        &tool_name,
                        &operation_name,
                        target.as_deref(),
                        summary.as_deref(),
                    )
                    .await?;
                Ok(ToolOutput::success(result))
            }
            SecurityQueryOperation::RequestElevation { reason } => Ok(ToolOutput::error(format!(
                "Elevation requires human approval outside runtime tools: {}",
                reason
            ))),
        }
    }
}
