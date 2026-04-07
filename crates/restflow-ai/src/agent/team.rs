use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::agent::executor::AgentConfig;
use restflow_traits::{TeamApprovalRequest, TeamCoordinator, TeamExecutionContext, ToolError};

pub fn inject_team_execution_context(
    mut config: AgentConfig,
    team_context: &TeamExecutionContext,
) -> AgentConfig {
    config = config.with_context(
        "team_context",
        serde_json::to_value(team_context).unwrap_or(Value::Null),
    );
    config = config.with_context("team_run_id", json!(team_context.team_run_id));
    config = config.with_context("team_member_id", json!(team_context.team_member_id));
    config = config.with_context("leader_member_id", json!(team_context.leader_member_id));
    config = config.with_context(
        "team_role",
        json!(match team_context.team_role {
            restflow_traits::TeamRole::Leader => "leader",
            restflow_traits::TeamRole::Member => "member",
        }),
    );
    config
}

pub fn extract_team_execution_context(
    context: &HashMap<String, Value>,
) -> Option<TeamExecutionContext> {
    context
        .get("team_context")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

pub async fn record_pending_team_approval(
    coordinator: &Arc<dyn TeamCoordinator>,
    team_context: &TeamExecutionContext,
    approval_id: &str,
    tool_name: &str,
    content: String,
) -> Result<(), ToolError> {
    coordinator
        .record_pending_approval(TeamApprovalRequest {
            team_run_id: team_context.team_run_id.clone(),
            member_id: team_context.team_member_id.clone(),
            approval_id: approval_id.to_string(),
            tool_name: tool_name.to_string(),
            content,
        })
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::TeamRole;

    #[test]
    fn inject_and_extract_team_context_round_trip() {
        let team_context = TeamExecutionContext {
            team_run_id: "team-1".to_string(),
            team_member_id: "member-1".to_string(),
            leader_member_id: "leader".to_string(),
            team_role: TeamRole::Member,
        };

        let config =
            inject_team_execution_context(AgentConfig::new("test"), &team_context);
        let extracted =
            extract_team_execution_context(&config.context).expect("team context should exist");

        assert_eq!(extracted, team_context);
        assert_eq!(config.context["team_run_id"], "team-1");
        assert_eq!(config.context["team_member_id"], "member-1");
        assert_eq!(config.context["leader_member_id"], "leader");
        assert_eq!(config.context["team_role"], "member");
    }
}
