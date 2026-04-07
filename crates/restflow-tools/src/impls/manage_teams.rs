use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::impls::spawn_subagent_batch::types::StoredBatchSubagentSpec;
use crate::impls::team_template::load_scoped_team_document;
use crate::{Result, Tool, ToolError, ToolOutput};
use restflow_traits::{
    AssignTeamTaskRequest, MANAGE_TEAMS_TOOL_DESCRIPTION, MANAGE_TEAMS_TOOL_NAME,
    ResolveTeamApprovalRequest, SendTeamMessageRequest, StartTeamRequest, TeamCoordinator,
    TeamMemberSpec, TeamMessageKind, TeamRole, TeamTemplateDocument,
};
use restflow_traits::store::KvStore;

const SUBAGENT_TEAM_TEMPLATE_SCOPE: crate::impls::team_template::TeamTemplateScope =
    crate::impls::team_template::TeamTemplateScope::new("subagent_team", "subagent_team", 1);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManageTeamsOperation {
    StartTeam,
    GetTeamState,
    ListTeamMessages,
    SendTeamMessage,
    ListTeamAssignments,
    AssignTeamTask,
    ResolveTeamApproval,
}

#[derive(Debug, Clone, Deserialize)]
struct TeamMemberInput {
    #[serde(default)]
    member_id: Option<String>,
    agent_id: String,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    max_iterations: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManageTeamsParams {
    operation: ManageTeamsOperation,
    #[serde(default)]
    team_run_id: Option<String>,
    #[serde(default)]
    team: Option<String>,
    #[serde(default)]
    members: Option<Vec<TeamMemberInput>>,
    #[serde(default)]
    assignments: Option<Vec<String>>,
    #[serde(default)]
    task: Option<String>,
    #[serde(default)]
    from_member_id: Option<String>,
    #[serde(default)]
    to_member_id: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    assignee_member_id: Option<String>,
    #[serde(default)]
    approval_id: Option<String>,
    #[serde(default)]
    approved: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
}

pub struct ManageTeamsTool {
    coordinator: Arc<dyn TeamCoordinator>,
    kv_store: Arc<dyn KvStore>,
}

impl ManageTeamsTool {
    pub fn new(coordinator: Arc<dyn TeamCoordinator>, kv_store: Arc<dyn KvStore>) -> Self {
        Self {
            coordinator,
            kv_store,
        }
    }

    fn load_template_members(&self, team_name: &str) -> Result<Vec<TeamMemberSpec>> {
        let document: TeamTemplateDocument<StoredBatchSubagentSpec> = load_scoped_team_document(
            self.kv_store.as_ref(),
            SUBAGENT_TEAM_TEMPLATE_SCOPE,
            team_name,
        )?;

        let mut members = Vec::new();
        let mut index = 0usize;
        for spec in document.members {
            let count = spec.count.max(1);
            for _ in 0..count {
                index += 1;
                members.push(TeamMemberSpec {
                    member_id: format!("member-{index}"),
                    agent_id: spec.agent.clone(),
                    role: TeamRole::Member,
                    input: None,
                    model: spec.model.clone(),
                    provider: spec.provider.clone(),
                    max_iterations: spec.inline_max_iterations,
                    inline_name: spec.inline_name.clone(),
                    inline_system_prompt: spec.inline_system_prompt.clone(),
                    inline_allowed_tools: spec.inline_allowed_tools.clone(),
                });
            }
        }
        if members.is_empty() {
            return Err(ToolError::Tool(format!(
                "Team template '{team_name}' has no spawnable members."
            )));
        }
        Ok(members)
    }

    fn explicit_members(&self, members: Vec<TeamMemberInput>) -> Result<Vec<TeamMemberSpec>> {
        if members.is_empty() {
            return Err(ToolError::Tool(
                "start_team requires non-empty 'members'.".to_string(),
            ));
        }
        Ok(members
            .into_iter()
            .enumerate()
            .map(|(index, member)| TeamMemberSpec {
                member_id: member
                    .member_id
                    .unwrap_or_else(|| format!("member-{}", index + 1)),
                agent_id: Some(member.agent_id),
                role: TeamRole::Member,
                input: member.input,
                model: member.model,
                provider: member.provider,
                max_iterations: member.max_iterations,
                inline_name: None,
                inline_system_prompt: None,
                inline_allowed_tools: None,
            })
            .collect())
    }

    fn resolve_start_members(&self, params: &ManageTeamsParams) -> Result<Vec<TeamMemberSpec>> {
        match (params.team.as_deref(), params.members.clone()) {
            (Some(_), Some(_)) => Err(ToolError::Tool(
                "start_team accepts either 'team' or 'members', not both.".to_string(),
            )),
            (Some(team_name), None) => self.load_template_members(team_name),
            (None, Some(members)) => self.explicit_members(members),
            (None, None) => Err(ToolError::Tool(
                "start_team requires 'team' or 'members'.".to_string(),
            )),
        }
    }

    fn schema() -> Value {
        json!({
            "type": "object",
            "required": ["operation"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": [
                        "start_team",
                        "get_team_state",
                        "list_team_messages",
                        "send_team_message",
                        "list_team_assignments",
                        "assign_team_task",
                        "resolve_team_approval"
                    ]
                },
                "team_run_id": { "type": "string" },
                "team": { "type": "string" },
                "members": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["agent_id"],
                        "properties": {
                            "member_id": { "type": "string" },
                            "agent_id": { "type": "string" },
                            "input": { "type": "string" },
                            "model": { "type": "string" },
                            "provider": { "type": "string" },
                            "max_iterations": { "type": "integer" }
                        }
                    }
                },
                "assignments": { "type": "array", "items": { "type": "string" } },
                "task": { "type": "string" },
                "from_member_id": { "type": "string" },
                "to_member_id": { "type": "string" },
                "content": { "type": "string" },
                "assignee_member_id": { "type": "string" },
                "approval_id": { "type": "string" },
                "approved": { "type": "boolean" },
                "reason": { "type": "string" }
            }
        })
    }
}

#[async_trait]
impl Tool for ManageTeamsTool {
    fn name(&self) -> &str {
        MANAGE_TEAMS_TOOL_NAME
    }

    fn description(&self) -> &str {
        MANAGE_TEAMS_TOOL_DESCRIPTION
    }

    fn parameters_schema(&self) -> Value {
        Self::schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ManageTeamsParams = serde_json::from_value(input)
            .map_err(|error| ToolError::Tool(format!("Invalid parameters: {error}")))?;

        match params.operation {
            ManageTeamsOperation::StartTeam => {
                let mut assignments = params.assignments.clone().unwrap_or_default();
                if let Some(task) = params.task.clone() {
                    assignments.push(task);
                }
                let state = self
                    .coordinator
                    .start_team(StartTeamRequest {
                        team_run_id: params.team_run_id.clone(),
                        leader_member_id: "leader".to_string(),
                        members: self.resolve_start_members(&params)?,
                        assignments,
                    })
                    .await?;
                Ok(ToolOutput::success(json!({
                    "operation": "start_team",
                    "team": state
                })))
            }
            ManageTeamsOperation::GetTeamState => {
                let team_run_id = params.team_run_id.ok_or_else(|| {
                    ToolError::Tool("get_team_state requires 'team_run_id'.".to_string())
                })?;
                let state = self.coordinator.get_team_state(&team_run_id).await?;
                Ok(ToolOutput::success(json!({
                    "operation": "get_team_state",
                    "team": state
                })))
            }
            ManageTeamsOperation::ListTeamMessages => {
                let team_run_id = params.team_run_id.ok_or_else(|| {
                    ToolError::Tool("list_team_messages requires 'team_run_id'.".to_string())
                })?;
                let messages = self.coordinator.list_team_messages(&team_run_id).await?;
                Ok(ToolOutput::success(json!({
                    "operation": "list_team_messages",
                    "messages": messages
                })))
            }
            ManageTeamsOperation::SendTeamMessage => {
                let team_run_id = params.team_run_id.ok_or_else(|| {
                    ToolError::Tool("send_team_message requires 'team_run_id'.".to_string())
                })?;
                let from_member_id = params.from_member_id.unwrap_or_else(|| "leader".to_string());
                let content = params.content.ok_or_else(|| {
                    ToolError::Tool("send_team_message requires 'content'.".to_string())
                })?;
                let message = self
                    .coordinator
                    .send_team_message(SendTeamMessageRequest {
                        team_run_id,
                        from_member_id,
                        to_member_id: params.to_member_id,
                        kind: Some(TeamMessageKind::Note),
                        content,
                    })
                    .await?;
                Ok(ToolOutput::success(json!({
                    "operation": "send_team_message",
                    "message": message
                })))
            }
            ManageTeamsOperation::ListTeamAssignments => {
                let team_run_id = params.team_run_id.ok_or_else(|| {
                    ToolError::Tool("list_team_assignments requires 'team_run_id'.".to_string())
                })?;
                let assignments = self.coordinator.list_team_assignments(&team_run_id).await?;
                Ok(ToolOutput::success(json!({
                    "operation": "list_team_assignments",
                    "assignments": assignments
                })))
            }
            ManageTeamsOperation::AssignTeamTask => {
                let team_run_id = params.team_run_id.ok_or_else(|| {
                    ToolError::Tool("assign_team_task requires 'team_run_id'.".to_string())
                })?;
                let assignee_member_id = params.assignee_member_id.ok_or_else(|| {
                    ToolError::Tool("assign_team_task requires 'assignee_member_id'.".to_string())
                })?;
                let content = params.content.or(params.task).ok_or_else(|| {
                    ToolError::Tool("assign_team_task requires 'content' or 'task'.".to_string())
                })?;
                let assignment = self
                    .coordinator
                    .assign_team_task(AssignTeamTaskRequest {
                        team_run_id,
                        assignee_member_id,
                        content,
                    })
                    .await?;
                Ok(ToolOutput::success(json!({
                    "operation": "assign_team_task",
                    "assignment": assignment
                })))
            }
            ManageTeamsOperation::ResolveTeamApproval => {
                let team_run_id = params.team_run_id.ok_or_else(|| {
                    ToolError::Tool("resolve_team_approval requires 'team_run_id'.".to_string())
                })?;
                let approval_id = params.approval_id.ok_or_else(|| {
                    ToolError::Tool("resolve_team_approval requires 'approval_id'.".to_string())
                })?;
                let approved = params.approved.ok_or_else(|| {
                    ToolError::Tool("resolve_team_approval requires 'approved'.".to_string())
                })?;
                let approval = self
                    .coordinator
                    .resolve_team_approval(ResolveTeamApprovalRequest {
                        team_run_id,
                        approval_id,
                        approved,
                        reason: params.reason,
                    })
                    .await?;
                Ok(ToolOutput::success(json!({
                    "operation": "resolve_team_approval",
                    "approval": approval
                })))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockCoordinator;

    #[async_trait]
    impl restflow_traits::TeamMailbox for MockCoordinator {
        async fn list_team_messages(&self, team_run_id: &str) -> Result<Vec<restflow_traits::TeamMessage>> {
            Ok(vec![restflow_traits::TeamMessage {
                team_run_id: team_run_id.to_string(),
                message_id: "msg-1".to_string(),
                from_member_id: "leader".to_string(),
                to_member_id: Some("member-1".to_string()),
                kind: TeamMessageKind::Note,
                content: "hello".to_string(),
                created_at: 1,
            }])
        }

        async fn send_team_message(
            &self,
            request: SendTeamMessageRequest,
        ) -> Result<restflow_traits::TeamMessage> {
            Ok(restflow_traits::TeamMessage {
                team_run_id: request.team_run_id,
                message_id: "msg-2".to_string(),
                from_member_id: request.from_member_id,
                to_member_id: request.to_member_id,
                kind: request.kind.unwrap_or(TeamMessageKind::Note),
                content: request.content,
                created_at: 2,
            })
        }
    }

    #[async_trait]
    impl TeamCoordinator for MockCoordinator {
        async fn start_team(&self, request: StartTeamRequest) -> Result<restflow_traits::TeamState> {
            Ok(restflow_traits::TeamState {
                team_run_id: request.team_run_id.unwrap_or_else(|| "team-1".to_string()),
                leader_member_id: request.leader_member_id,
                members: vec![],
                status: restflow_traits::TeamStatus::Running,
                pending_message_count: 0,
                pending_assignment_count: request.assignments.len(),
                updated_at: 1,
            })
        }

        async fn get_team_state(&self, team_run_id: &str) -> Result<restflow_traits::TeamState> {
            Ok(restflow_traits::TeamState {
                team_run_id: team_run_id.to_string(),
                leader_member_id: "leader".to_string(),
                members: vec![],
                status: restflow_traits::TeamStatus::Running,
                pending_message_count: 0,
                pending_assignment_count: 0,
                updated_at: 1,
            })
        }

        async fn list_team_assignments(&self, team_run_id: &str) -> Result<Vec<restflow_traits::TeamAssignment>> {
            Ok(vec![restflow_traits::TeamAssignment {
                team_run_id: team_run_id.to_string(),
                assignment_id: "assign-1".to_string(),
                assignee_member_id: "member-1".to_string(),
                content: "task".to_string(),
                status: restflow_traits::TeamAssignmentStatus::InProgress,
                created_at: 1,
                updated_at: 1,
            }])
        }

        async fn assign_team_task(
            &self,
            request: AssignTeamTaskRequest,
        ) -> Result<restflow_traits::TeamAssignment> {
            Ok(restflow_traits::TeamAssignment {
                team_run_id: request.team_run_id,
                assignment_id: "assign-2".to_string(),
                assignee_member_id: request.assignee_member_id,
                content: request.content,
                status: restflow_traits::TeamAssignmentStatus::InProgress,
                created_at: 1,
                updated_at: 1,
            })
        }

        async fn record_pending_approval(
            &self,
            request: restflow_traits::TeamApprovalRequest,
        ) -> Result<restflow_traits::PendingTeamApproval> {
            Ok(restflow_traits::PendingTeamApproval {
                team_run_id: request.team_run_id,
                approval_id: request.approval_id,
                member_id: request.member_id,
                tool_name: request.tool_name,
                content: request.content,
                status: restflow_traits::TeamApprovalStatus::Pending,
                requested_at: 1,
                resolved_at: None,
                resolution_reason: None,
            })
        }

        async fn resolve_team_approval(
            &self,
            request: ResolveTeamApprovalRequest,
        ) -> Result<restflow_traits::PendingTeamApproval> {
            Ok(restflow_traits::PendingTeamApproval {
                team_run_id: request.team_run_id,
                approval_id: request.approval_id,
                member_id: "member-1".to_string(),
                tool_name: "manage_tasks".to_string(),
                content: "{}".to_string(),
                status: if request.approved {
                    restflow_traits::TeamApprovalStatus::Approved
                } else {
                    restflow_traits::TeamApprovalStatus::Rejected
                },
                requested_at: 1,
                resolved_at: Some(2),
                resolution_reason: request.reason,
            })
        }
    }

    #[derive(Default)]
    struct MockKvStore {
        entries: Mutex<HashMap<String, String>>,
    }

    impl KvStore for MockKvStore {
        fn get_entry(&self, key: &str) -> Result<Value> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(value) = entries.get(key) {
                Ok(json!({ "found": true, "key": key, "value": value }))
            } else {
                Ok(json!({ "found": false, "key": key }))
            }
        }

        fn set_entry(
            &self,
            key: &str,
            content: &str,
            _visibility: Option<&str>,
            _content_type: Option<&str>,
            _type_hint: Option<&str>,
            _tags: Option<Vec<String>>,
            _accessor_id: Option<&str>,
        ) -> Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.insert(key.to_string(), content.to_string());
            Ok(json!({"success": true, "key": key}))
        }

        fn delete_entry(&self, _key: &str, _accessor_id: Option<&str>) -> Result<Value> {
            Ok(json!({"deleted": true}))
        }

        fn list_entries(&self, namespace: Option<&str>) -> Result<Value> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefix = namespace.unwrap_or_default().to_string();
            let list = entries
                .keys()
                .filter(|key| key.starts_with(&prefix))
                .map(|key| json!({ "key": key }))
                .collect::<Vec<_>>();
            Ok(json!({ "entries": list }))
        }
    }

    #[tokio::test]
    async fn start_team_from_saved_template_expands_members() {
        let store = Arc::new(MockKvStore::default());
        let document = TeamTemplateDocument {
            version: 1,
            name: "TeamOne".to_string(),
            members: vec![StoredBatchSubagentSpec {
                agent: Some("coder".to_string()),
                count: 2,
                timeout_secs: None,
                model: None,
                provider: None,
                inline_name: None,
                inline_system_prompt: None,
                inline_allowed_tools: None,
                inline_max_iterations: None,
            }],
            created_at: 1,
            updated_at: 1,
        };
        store
            .set_entry(
                "subagent_team:TeamOne",
                &serde_json::to_string(&document).unwrap(),
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let tool = ManageTeamsTool::new(Arc::new(MockCoordinator), store);
        let output = tool
            .execute(json!({
                "operation": "start_team",
                "team": "TeamOne"
            }))
            .await
            .expect("start team from template");

        assert_eq!(output.result["operation"], "start_team");
        assert_eq!(output.result["team"]["pending_assignment_count"], 0);
    }
}
