use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use restflow_ai::agent::SubagentTracker;
use restflow_traits::store::KvStore;
use restflow_traits::{
    AssignTeamTaskRequest, ContractRunSpawnRequest, PendingTeamApproval, ResolveTeamApprovalRequest,
    SendTeamMessageRequest, StartTeamRequest, SubagentManager, TeamApprovalRequest,
    TeamApprovalStatus, TeamAssignment, TeamAssignmentStatus, TeamCoordinator, TeamMailbox,
    TeamMemberSpec, TeamMemberState, TeamMemberStatus, TeamMessage, TeamMessageKind, TeamRole,
    TeamState, TeamStatus, ToolError,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

const TEAM_STATE_PREFIX: &str = "team_runtime:state";
const TEAM_MESSAGE_PREFIX: &str = "team_runtime:message";
const TEAM_ASSIGNMENT_PREFIX: &str = "team_runtime:assignment";
const TEAM_APPROVAL_PREFIX: &str = "team_runtime:approval";
const TEAM_VISIBILITY: &str = "shared";
const TEAM_CONTENT_TYPE: &str = "application/json";

pub struct TeamRuntimeService {
    kv_store: Arc<dyn KvStore>,
    subagent_manager: Arc<dyn SubagentManager>,
    subagent_tracker: Arc<SubagentTracker>,
}

impl TeamRuntimeService {
    pub fn new(
        kv_store: Arc<dyn KvStore>,
        subagent_manager: Arc<dyn SubagentManager>,
        subagent_tracker: Arc<SubagentTracker>,
    ) -> Self {
        Self {
            kv_store,
            subagent_manager,
            subagent_tracker,
        }
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    fn state_key(team_run_id: &str) -> String {
        format!("{TEAM_STATE_PREFIX}:{team_run_id}")
    }

    fn message_key(team_run_id: &str, message_id: &str) -> String {
        format!("{TEAM_MESSAGE_PREFIX}:{team_run_id}:{message_id}")
    }

    fn assignment_key(team_run_id: &str, assignment_id: &str) -> String {
        format!("{TEAM_ASSIGNMENT_PREFIX}:{team_run_id}:{assignment_id}")
    }

    fn approval_key(team_run_id: &str, approval_id: &str) -> String {
        format!("{TEAM_APPROVAL_PREFIX}:{team_run_id}:{approval_id}")
    }

    fn write_json<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        type_hint: &'static str,
        tags: Vec<String>,
    ) -> Result<(), ToolError> {
        let content = serde_json::to_string(value)
            .map_err(|error| ToolError::Tool(format!("Failed to serialize {type_hint}: {error}")))?;
        self.kv_store
            .set_entry(
                key,
                &content,
                Some(TEAM_VISIBILITY),
                Some(TEAM_CONTENT_TYPE),
                Some(type_hint),
                Some(tags),
                None,
            )
            .map(|_| ())
    }

    fn read_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, ToolError> {
        let payload = self.kv_store.get_entry(key)?;
        if !payload
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(None);
        }
        let raw = payload
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Tool(format!("Stored payload is invalid for key '{key}'")))?;
        serde_json::from_str(raw)
            .map(Some)
            .map_err(|error| ToolError::Tool(format!("Failed to decode '{key}': {error}")))
    }

    fn list_by_prefix<T: DeserializeOwned>(&self, prefix: &str) -> Result<Vec<T>, ToolError> {
        let payload = self.kv_store.list_entries(Some(prefix))?;
        let entries = payload
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut results = Vec::new();
        for entry in entries {
            let Some(key) = entry.get("key").and_then(Value::as_str) else {
                continue;
            };
            if let Some(value) = self.read_json::<T>(key)? {
                results.push(value);
            }
        }
        Ok(results)
    }

    fn build_leader_state(leader_member_id: &str) -> TeamMemberState {
        TeamMemberState {
            member_id: leader_member_id.to_string(),
            agent_id: None,
            model: None,
            provider: None,
            max_iterations: None,
            inline_name: None,
            inline_system_prompt: None,
            inline_allowed_tools: None,
            role: TeamRole::Leader,
            status: TeamMemberStatus::Idle,
            task_id: None,
            current_assignment_id: None,
            last_read_message_id: None,
        }
    }

    fn build_member_state(spec: &TeamMemberSpec) -> TeamMemberState {
        TeamMemberState {
            member_id: spec.member_id.clone(),
            agent_id: spec.agent_id.clone(),
            model: spec.model.clone(),
            provider: spec.provider.clone(),
            max_iterations: spec.max_iterations,
            inline_name: spec.inline_name.clone(),
            inline_system_prompt: spec.inline_system_prompt.clone(),
            inline_allowed_tools: spec.inline_allowed_tools.clone(),
            role: TeamRole::Member,
            status: TeamMemberStatus::Idle,
            task_id: None,
            current_assignment_id: None,
            last_read_message_id: None,
        }
    }

    fn validate_start_request(request: &StartTeamRequest) -> Result<(), ToolError> {
        if request.members.is_empty() {
            return Err(ToolError::Tool(
                "start_team requires at least one member.".to_string(),
            ));
        }
        if request.leader_member_id.trim().is_empty() {
            return Err(ToolError::Tool(
                "leader_member_id must not be empty.".to_string(),
            ));
        }
        let mut seen = HashMap::new();
        for member in &request.members {
            if member.member_id.trim().is_empty() {
                return Err(ToolError::Tool(
                    "member_id must not be empty.".to_string(),
                ));
            }
            if member.role != TeamRole::Member {
                return Err(ToolError::Tool(
                    "start_team members must use role 'member'; leader is represented separately."
                        .to_string(),
                ));
            }
            let has_agent_id = member
                .agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
            let has_inline = member
                .inline_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
                || member
                    .inline_system_prompt
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
                || member
                    .inline_allowed_tools
                    .as_ref()
                    .map(|value| !value.is_empty())
                    .unwrap_or(false);
            if !has_agent_id && !has_inline {
                return Err(ToolError::Tool(format!(
                    "member '{}' requires agent_id or inline temporary-subagent fields.",
                    member.member_id
                )));
            }
            if seen.insert(member.member_id.clone(), true).is_some() {
                return Err(ToolError::Tool(format!(
                    "Duplicate member_id '{}'.",
                    member.member_id
                )));
            }
        }
        if seen.contains_key(&request.leader_member_id) {
            return Err(ToolError::Tool(
                "leader_member_id must not collide with worker member ids.".to_string(),
            ));
        }
        Ok(())
    }

    async fn refresh_state(&self, mut state: TeamState) -> Result<TeamState, ToolError> {
        let assignments = self.list_team_assignments(&state.team_run_id).await?;
        let approvals = self.pending_approvals(&state.team_run_id)?;
        let approval_members = approvals
            .iter()
            .map(|approval| approval.member_id.clone())
            .collect::<Vec<_>>();

        let mut assignment_map = assignments
            .iter()
            .map(|assignment| (assignment.assignment_id.clone(), assignment.clone()))
            .collect::<HashMap<_, _>>();

        for member in &mut state.members {
            if member.role == TeamRole::Leader {
                member.status = if approvals.is_empty() {
                    TeamMemberStatus::Idle
                } else {
                    TeamMemberStatus::WaitingApproval
                };
                continue;
            }

            if approval_members.iter().any(|value| value == &member.member_id) {
                member.status = TeamMemberStatus::WaitingApproval;
                continue;
            }

            let Some(task_id) = member.task_id.clone() else {
                if member.current_assignment_id.is_some() {
                    member.status = TeamMemberStatus::Pending;
                } else {
                    member.status = TeamMemberStatus::Idle;
                }
                continue;
            };

            let Some(subagent_state) = self.subagent_tracker.get(&task_id) else {
                continue;
            };

            if let Some(current_assignment_id) = member.current_assignment_id.clone()
                && let Some(assignment) = assignment_map.get_mut(&current_assignment_id)
            {
                match subagent_state.status {
                    restflow_traits::SubagentStatus::Running => {
                        assignment.status = TeamAssignmentStatus::InProgress;
                        member.status = TeamMemberStatus::Running;
                    }
                    restflow_traits::SubagentStatus::Pending => {
                        assignment.status = TeamAssignmentStatus::Pending;
                        member.status = TeamMemberStatus::Pending;
                    }
                    restflow_traits::SubagentStatus::Completed => {
                        assignment.status = TeamAssignmentStatus::Completed;
                        member.status = TeamMemberStatus::Completed;
                    }
                    restflow_traits::SubagentStatus::Failed
                    | restflow_traits::SubagentStatus::TimedOut => {
                        assignment.status = TeamAssignmentStatus::Failed;
                        member.status = TeamMemberStatus::Failed;
                    }
                    restflow_traits::SubagentStatus::Interrupted => {
                        assignment.status = TeamAssignmentStatus::Cancelled;
                        member.status = TeamMemberStatus::Cancelled;
                    }
                }
            }
        }

        for assignment in assignment_map.values() {
            self.write_json(
                &Self::assignment_key(&assignment.team_run_id, &assignment.assignment_id),
                assignment,
                "team_runtime_assignment",
                vec!["team_runtime".to_string(), "assignment".to_string()],
            )?;
        }

        state.pending_message_count = approvals.len();
        state.pending_assignment_count = assignment_map
            .values()
            .filter(|assignment| {
                matches!(
                    assignment.status,
                    TeamAssignmentStatus::Pending | TeamAssignmentStatus::InProgress
                )
            })
            .count();
        state.status = if !approvals.is_empty() {
            TeamStatus::WaitingApproval
        } else if state
            .members
            .iter()
            .any(|member| member.status == TeamMemberStatus::Failed)
        {
            TeamStatus::Failed
        } else if state.pending_assignment_count == 0 && !assignment_map.is_empty() {
            TeamStatus::Completed
        } else {
            TeamStatus::Running
        };
        state.updated_at = Self::now_ms();
        self.persist_state(&state)?;
        Ok(state)
    }

    fn persist_state(&self, state: &TeamState) -> Result<(), ToolError> {
        self.write_json(
            &Self::state_key(&state.team_run_id),
            state,
            "team_runtime_state",
            vec!["team_runtime".to_string(), "state".to_string()],
        )
    }

    fn pending_approvals(&self, team_run_id: &str) -> Result<Vec<PendingTeamApproval>, ToolError> {
        Ok(self
            .list_by_prefix::<PendingTeamApproval>(&format!("{TEAM_APPROVAL_PREFIX}:{team_run_id}"))?
            .into_iter()
            .filter(|approval| approval.status == TeamApprovalStatus::Pending)
            .collect())
    }

    async fn create_assignment_for_member(
        &self,
        state: &mut TeamState,
        assignee_member_id: String,
        content: String,
    ) -> Result<TeamAssignment, ToolError> {
        let assignment_id = uuid::Uuid::new_v4().to_string();
        let now = Self::now_ms();
        let member = state
            .members
            .iter_mut()
            .find(|member| member.member_id == assignee_member_id)
            .ok_or_else(|| ToolError::Tool(format!("Unknown team member '{assignee_member_id}'.")))?;

        if member.role != TeamRole::Member {
            return Err(ToolError::Tool(
                "Assignments can only target worker members.".to_string(),
            ));
        }

        let inline = if member.agent_id.is_none() {
            Some(restflow_contracts::request::InlineAgentRunConfig {
                name: member.inline_name.clone(),
                system_prompt: member.inline_system_prompt.clone(),
                allowed_tools: member.inline_allowed_tools.clone(),
                max_iterations: member.max_iterations,
            })
        } else {
            None
        };

        let handle = self
            .subagent_manager
            .spawn(ContractRunSpawnRequest {
                agent_id: member.agent_id.clone(),
                inline,
                task: content.clone(),
                timeout_secs: None,
                max_iterations: member.max_iterations,
                priority: None,
                model: member.model.clone(),
                model_provider: member.provider.clone(),
                parent_run_id: Some(state.team_run_id.clone()),
                trace_session_id: Some(state.team_run_id.clone()),
                trace_scope_id: Some(state.team_run_id.clone()),
                team_run_id: Some(state.team_run_id.clone()),
                team_member_id: Some(member.member_id.clone()),
                leader_member_id: Some(state.leader_member_id.clone()),
                team_role: Some("member".to_string()),
            })?;

        let assignment = TeamAssignment {
            team_run_id: state.team_run_id.clone(),
            assignment_id: assignment_id.clone(),
            assignee_member_id: assignee_member_id.clone(),
            content: content.clone(),
            status: TeamAssignmentStatus::InProgress,
            created_at: now,
            updated_at: now,
        };
        member.current_assignment_id = Some(assignment_id.clone());
        member.task_id = Some(handle.id.clone());
        member.status = TeamMemberStatus::Running;
        state.updated_at = now;

        self.write_json(
            &Self::assignment_key(&state.team_run_id, &assignment.assignment_id),
            &assignment,
            "team_runtime_assignment",
            vec!["team_runtime".to_string(), "assignment".to_string()],
        )?;
        self.persist_state(state)?;
        let _ = self
            .send_team_message(SendTeamMessageRequest {
                team_run_id: state.team_run_id.clone(),
                from_member_id: state.leader_member_id.clone(),
                to_member_id: Some(assignee_member_id),
                kind: Some(TeamMessageKind::Assignment),
                content,
            })
            .await?;
        Ok(assignment)
    }
}

#[async_trait]
impl TeamMailbox for TeamRuntimeService {
    async fn list_team_messages(&self, team_run_id: &str) -> Result<Vec<TeamMessage>, ToolError> {
        let mut messages =
            self.list_by_prefix::<TeamMessage>(&format!("{TEAM_MESSAGE_PREFIX}:{team_run_id}"))?;
        messages.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(messages)
    }

    async fn send_team_message(
        &self,
        request: SendTeamMessageRequest,
    ) -> Result<TeamMessage, ToolError> {
        let mut state = self
            .read_json::<TeamState>(&Self::state_key(&request.team_run_id))?
            .ok_or_else(|| ToolError::Tool(format!("Unknown team run '{}'.", request.team_run_id)))?;
        let message = TeamMessage {
            team_run_id: request.team_run_id.clone(),
            message_id: uuid::Uuid::new_v4().to_string(),
            from_member_id: request.from_member_id,
            to_member_id: request.to_member_id.clone(),
            kind: request.kind.unwrap_or(TeamMessageKind::Note),
            content: request.content.clone(),
            created_at: Self::now_ms(),
        };
        self.write_json(
            &Self::message_key(&message.team_run_id, &message.message_id),
            &message,
            "team_runtime_message",
            vec!["team_runtime".to_string(), "message".to_string()],
        )?;

        if let Some(target_member_id) = request.to_member_id
            && let Some(target) = state
                .members
                .iter()
                .find(|member| member.member_id == target_member_id)
            && let Some(task_id) = target.task_id.as_deref()
        {
            let _ = self
                .subagent_tracker
                .steer(
                    task_id,
                    restflow_traits::SteerMessage::message(
                        request.content,
                        restflow_traits::SteerSource::Api,
                    ),
                )
                .await;
        }

        state.updated_at = Self::now_ms();
        self.persist_state(&state)?;
        Ok(message)
    }
}

#[async_trait]
impl TeamCoordinator for TeamRuntimeService {
    async fn start_team(&self, request: StartTeamRequest) -> Result<TeamState, ToolError> {
        Self::validate_start_request(&request)?;
        let team_run_id = request
            .team_run_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let mut state = TeamState {
            team_run_id: team_run_id.clone(),
            leader_member_id: request.leader_member_id.clone(),
            members: std::iter::once(Self::build_leader_state(&request.leader_member_id))
                .chain(request.members.iter().map(Self::build_member_state))
                .collect(),
            status: TeamStatus::Starting,
            pending_message_count: 0,
            pending_assignment_count: 0,
            updated_at: Self::now_ms(),
        };
        self.persist_state(&state)?;

        let workers = request
            .members
            .iter()
            .map(|member| member.member_id.clone())
            .collect::<Vec<_>>();
        for (index, content) in request.assignments.into_iter().enumerate() {
            let assignee_member_id = workers
                .get(index % workers.len())
                .cloned()
                .ok_or_else(|| ToolError::Tool("No worker members available for assignment.".to_string()))?;
            let _ = self
                .create_assignment_for_member(&mut state, assignee_member_id, content)
                .await?;
        }

        self.refresh_state(state).await
    }

    async fn get_team_state(&self, team_run_id: &str) -> Result<TeamState, ToolError> {
        let state = self
            .read_json::<TeamState>(&Self::state_key(team_run_id))?
            .ok_or_else(|| ToolError::Tool(format!("Unknown team run '{team_run_id}'.")))?;
        self.refresh_state(state).await
    }

    async fn list_team_assignments(&self, team_run_id: &str) -> Result<Vec<TeamAssignment>, ToolError> {
        let mut assignments =
            self.list_by_prefix::<TeamAssignment>(&format!("{TEAM_ASSIGNMENT_PREFIX}:{team_run_id}"))?;
        assignments.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(assignments)
    }

    async fn assign_team_task(
        &self,
        request: AssignTeamTaskRequest,
    ) -> Result<TeamAssignment, ToolError> {
        let mut state = self
            .read_json::<TeamState>(&Self::state_key(&request.team_run_id))?
            .ok_or_else(|| ToolError::Tool(format!("Unknown team run '{}'.", request.team_run_id)))?;
        let assignment = self
            .create_assignment_for_member(
                &mut state,
                request.assignee_member_id,
                request.content,
            )
            .await?;
        let _ = self.refresh_state(state).await?;
        Ok(assignment)
    }

    async fn record_pending_approval(
        &self,
        request: TeamApprovalRequest,
    ) -> Result<PendingTeamApproval, ToolError> {
        let mut state = self
            .read_json::<TeamState>(&Self::state_key(&request.team_run_id))?
            .ok_or_else(|| ToolError::Tool(format!("Unknown team run '{}'.", request.team_run_id)))?;
        let approval = PendingTeamApproval {
            team_run_id: request.team_run_id.clone(),
            approval_id: request.approval_id.clone(),
            member_id: request.member_id.clone(),
            tool_name: request.tool_name.clone(),
            content: request.content.clone(),
            status: TeamApprovalStatus::Pending,
            requested_at: Self::now_ms(),
            resolved_at: None,
            resolution_reason: None,
        };
        self.write_json(
            &Self::approval_key(&request.team_run_id, &request.approval_id),
            &approval,
            "team_runtime_approval",
            vec!["team_runtime".to_string(), "approval".to_string()],
        )?;
        let _ = self
            .send_team_message(SendTeamMessageRequest {
                team_run_id: request.team_run_id.clone(),
                from_member_id: request.member_id,
                to_member_id: Some(state.leader_member_id.clone()),
                kind: Some(TeamMessageKind::ApprovalRequest),
                content: format!(
                    "Approval requested for tool '{}' ({})",
                    request.tool_name, request.approval_id
                ),
            })
            .await?;
        state.updated_at = Self::now_ms();
        self.persist_state(&state)?;
        Ok(approval)
    }

    async fn resolve_team_approval(
        &self,
        request: ResolveTeamApprovalRequest,
    ) -> Result<PendingTeamApproval, ToolError> {
        let mut approval = self
            .read_json::<PendingTeamApproval>(&Self::approval_key(
                &request.team_run_id,
                &request.approval_id,
            ))?
            .ok_or_else(|| ToolError::Tool(format!("Unknown approval '{}'.", request.approval_id)))?;
        let mut state = self
            .read_json::<TeamState>(&Self::state_key(&request.team_run_id))?
            .ok_or_else(|| ToolError::Tool(format!("Unknown team run '{}'.", request.team_run_id)))?;

        approval.status = if request.approved {
            TeamApprovalStatus::Approved
        } else {
            TeamApprovalStatus::Rejected
        };
        approval.resolved_at = Some(Self::now_ms());
        approval.resolution_reason = request.reason.clone();
        self.write_json(
            &Self::approval_key(&request.team_run_id, &request.approval_id),
            &approval,
            "team_runtime_approval",
            vec!["team_runtime".to_string(), "approval".to_string()],
        )?;

        if let Some(member) = state
            .members
            .iter()
            .find(|member| member.member_id == approval.member_id)
            && let Some(task_id) = member.task_id.as_deref()
        {
            let instruction = if request.approved {
                format!("approval {} approved", request.approval_id)
            } else {
                format!(
                    "approval {} denied {}",
                    request.approval_id,
                    request.reason.unwrap_or_else(|| "No reason provided.".to_string())
                )
            };
            let _ = self
                .subagent_tracker
                .steer(
                    task_id,
                    restflow_traits::SteerMessage::message(
                        instruction,
                        restflow_traits::SteerSource::Api,
                    ),
                )
                .await;
        }

        let _ = self
            .send_team_message(SendTeamMessageRequest {
                team_run_id: request.team_run_id.clone(),
                from_member_id: state.leader_member_id.clone(),
                to_member_id: Some(approval.member_id.clone()),
                kind: Some(TeamMessageKind::ApprovalResolution),
                content: format!(
                    "Approval {} {}",
                    approval.approval_id,
                    if request.approved { "approved" } else { "rejected" }
                ),
            })
            .await?;

        state.updated_at = Self::now_ms();
        self.persist_state(&state)?;
        Ok(approval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use serde_json::json;
    use tokio::sync::mpsc;
    use restflow_traits::SpawnHandle;

    #[derive(Default)]
    struct MockKvStore {
        entries: Mutex<HashMap<String, String>>,
    }

    impl KvStore for MockKvStore {
        fn get_entry(&self, key: &str) -> Result<Value, ToolError> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(value) = entries.get(key) {
                Ok(json!({"found": true, "key": key, "value": value }))
            } else {
                Ok(json!({"found": false, "key": key }))
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
        ) -> Result<Value, ToolError> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.insert(key.to_string(), content.to_string());
            Ok(json!({"success": true, "key": key }))
        }

        fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> Result<Value, ToolError> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(json!({"deleted": entries.remove(key).is_some()}))
        }

        fn list_entries(&self, namespace: Option<&str>) -> Result<Value, ToolError> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefix = namespace.unwrap_or_default().to_string();
            let rows = entries
                .keys()
                .filter(|key| key.starts_with(&prefix))
                .map(|key| json!({"key": key}))
                .collect::<Vec<_>>();
            Ok(json!({"entries": rows}))
        }
    }

    struct MockSubagentManager;

    #[async_trait]
    impl SubagentManager for MockSubagentManager {
        fn spawn(
            &self,
            request: ContractRunSpawnRequest,
        ) -> std::result::Result<SpawnHandle, ToolError> {
            Ok(SpawnHandle {
                id: format!("task-{}", request.team_member_id.unwrap_or_else(|| "member".to_string())),
                agent_name: request.agent_id.unwrap_or_else(|| "worker".to_string()),
                effective_limits: restflow_traits::SubagentEffectiveLimits {
                    timeout_secs: 300,
                    timeout_source: restflow_traits::SubagentLimitSource::ConfigDefault,
                    max_iterations: 8,
                    max_iterations_source: restflow_traits::SubagentLimitSource::ConfigDefault,
                },
            })
        }

        fn list_callable(&self) -> Vec<restflow_traits::SubagentDefSummary> {
            Vec::new()
        }

        fn list_running(&self) -> Vec<restflow_traits::SubagentState> {
            Vec::new()
        }

        fn running_count(&self) -> usize {
            0
        }

        async fn wait(&self, _task_id: &str) -> Option<restflow_traits::SubagentCompletion> {
            None
        }

        async fn wait_for_parent_owned_task(
            &self,
            _task_id: &str,
            _parent_run_id: &str,
        ) -> Option<restflow_traits::SubagentCompletion> {
            None
        }

        fn config(&self) -> &restflow_traits::SubagentConfig {
            static CONFIG: std::sync::OnceLock<restflow_traits::SubagentConfig> =
                std::sync::OnceLock::new();
            CONFIG.get_or_init(restflow_traits::SubagentConfig::default)
        }
    }

    fn make_service() -> TeamRuntimeService {
        let (tx, rx) = mpsc::channel(8);
        TeamRuntimeService::new(
            Arc::new(MockKvStore::default()),
            Arc::new(MockSubagentManager),
            Arc::new(SubagentTracker::new(tx, rx)),
        )
    }

    #[tokio::test]
    async fn start_team_creates_state_and_initial_assignment() {
        let service = make_service();
        let state = service
            .start_team(StartTeamRequest {
                team_run_id: Some("team-1".to_string()),
                leader_member_id: "leader".to_string(),
                members: vec![TeamMemberSpec {
                    member_id: "worker-1".to_string(),
                    agent_id: Some("coder".to_string()),
                    role: TeamRole::Member,
                    input: None,
                    model: None,
                    provider: None,
                    max_iterations: None,
                    inline_name: None,
                    inline_system_prompt: None,
                    inline_allowed_tools: None,
                }],
                assignments: vec!["Investigate the bug".to_string()],
            })
            .await
            .expect("start team should succeed");

        assert_eq!(state.team_run_id, "team-1");
        assert_eq!(state.leader_member_id, "leader");
        assert_eq!(state.pending_assignment_count, 1);
        assert_eq!(state.members.len(), 2);
    }

    #[tokio::test]
    async fn record_and_resolve_pending_approval_updates_mailbox() {
        let service = make_service();
        service
            .start_team(StartTeamRequest {
                team_run_id: Some("team-2".to_string()),
                leader_member_id: "leader".to_string(),
                members: vec![TeamMemberSpec {
                    member_id: "worker-1".to_string(),
                    agent_id: Some("coder".to_string()),
                    role: TeamRole::Member,
                    input: None,
                    model: None,
                    provider: None,
                    max_iterations: None,
                    inline_name: None,
                    inline_system_prompt: None,
                    inline_allowed_tools: None,
                }],
                assignments: vec!["Review logs".to_string()],
            })
            .await
            .expect("start team");

        let approval = service
            .record_pending_approval(TeamApprovalRequest {
                team_run_id: "team-2".to_string(),
                member_id: "worker-1".to_string(),
                approval_id: "approval-1".to_string(),
                tool_name: "manage_tasks".to_string(),
                content: "{\"operation\":\"delete\"}".to_string(),
            })
            .await
            .expect("record approval");
        assert_eq!(approval.status, TeamApprovalStatus::Pending);

        let resolved = service
            .resolve_team_approval(ResolveTeamApprovalRequest {
                team_run_id: "team-2".to_string(),
                approval_id: "approval-1".to_string(),
                approved: true,
                reason: None,
            })
            .await
            .expect("resolve approval");
        assert_eq!(resolved.status, TeamApprovalStatus::Approved);

        let messages = service
            .list_team_messages("team-2")
            .await
            .expect("list team messages");
        assert!(messages.iter().any(|message| message.kind == TeamMessageKind::ApprovalRequest));
        assert!(messages.iter().any(|message| message.kind == TeamMessageKind::ApprovalResolution));
    }
}
