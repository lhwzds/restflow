//! Shared Team V1 runtime contracts.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

pub const MANAGE_TEAMS_TOOL_NAME: &str = "manage_teams";
pub const MANAGE_TEAMS_TOOL_DESCRIPTION: &str = "Manage runtime teams. Operations: start_team, get_team_state, list_team_messages, send_team_message, list_team_assignments, assign_team_task, resolve_team_approval.";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
    Leader,
    Member,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    Starting,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamMemberStatus {
    Idle,
    Pending,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamMessageKind {
    Note,
    ApprovalRequest,
    ApprovalResolution,
    Assignment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamAssignmentStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamMemberSpec {
    pub member_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    pub role: TeamRole,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub inline_name: Option<String>,
    #[serde(default)]
    pub inline_system_prompt: Option<String>,
    #[serde(default)]
    pub inline_allowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamMemberState {
    pub member_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub inline_name: Option<String>,
    #[serde(default)]
    pub inline_system_prompt: Option<String>,
    #[serde(default)]
    pub inline_allowed_tools: Option<Vec<String>>,
    pub role: TeamRole,
    pub status: TeamMemberStatus,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub current_assignment_id: Option<String>,
    #[serde(default)]
    pub last_read_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamState {
    pub team_run_id: String,
    pub leader_member_id: String,
    pub members: Vec<TeamMemberState>,
    pub status: TeamStatus,
    pub pending_message_count: usize,
    pub pending_assignment_count: usize,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamMessage {
    pub team_run_id: String,
    pub message_id: String,
    pub from_member_id: String,
    #[serde(default)]
    pub to_member_id: Option<String>,
    pub kind: TeamMessageKind,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamAssignment {
    pub team_run_id: String,
    pub assignment_id: String,
    pub assignee_member_id: String,
    pub content: String,
    pub status: TeamAssignmentStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingTeamApproval {
    pub team_run_id: String,
    pub approval_id: String,
    pub member_id: String,
    pub tool_name: String,
    pub content: String,
    pub status: TeamApprovalStatus,
    pub requested_at: i64,
    #[serde(default)]
    pub resolved_at: Option<i64>,
    #[serde(default)]
    pub resolution_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartTeamRequest {
    #[serde(default)]
    pub team_run_id: Option<String>,
    pub leader_member_id: String,
    pub members: Vec<TeamMemberSpec>,
    #[serde(default)]
    pub assignments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendTeamMessageRequest {
    pub team_run_id: String,
    pub from_member_id: String,
    #[serde(default)]
    pub to_member_id: Option<String>,
    #[serde(default)]
    pub kind: Option<TeamMessageKind>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssignTeamTaskRequest {
    pub team_run_id: String,
    pub assignee_member_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolveTeamApprovalRequest {
    pub team_run_id: String,
    pub approval_id: String,
    pub approved: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamApprovalRequest {
    pub team_run_id: String,
    pub member_id: String,
    pub approval_id: String,
    pub tool_name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamExecutionContext {
    pub team_run_id: String,
    pub team_member_id: String,
    pub team_role: TeamRole,
    pub leader_member_id: String,
}

#[async_trait]
pub trait TeamMailbox: Send + Sync {
    async fn list_team_messages(&self, team_run_id: &str) -> Result<Vec<TeamMessage>>;
    async fn send_team_message(&self, request: SendTeamMessageRequest) -> Result<TeamMessage>;
}

#[async_trait]
pub trait TeamCoordinator: TeamMailbox + Send + Sync {
    async fn start_team(&self, request: StartTeamRequest) -> Result<TeamState>;
    async fn get_team_state(&self, team_run_id: &str) -> Result<TeamState>;
    async fn list_team_assignments(&self, team_run_id: &str) -> Result<Vec<TeamAssignment>>;
    async fn assign_team_task(&self, request: AssignTeamTaskRequest) -> Result<TeamAssignment>;
    async fn record_pending_approval(
        &self,
        request: TeamApprovalRequest,
    ) -> Result<PendingTeamApproval>;
    async fn resolve_team_approval(
        &self,
        request: ResolveTeamApprovalRequest,
    ) -> Result<PendingTeamApproval>;
}
