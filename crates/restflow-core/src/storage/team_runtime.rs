//! Typed team runtime storage wrapper.

use anyhow::Result;
use redb::Database;
use restflow_traits::{PendingTeamApproval, TeamAssignment, TeamMessage, TeamState};
use std::sync::Arc;

#[derive(Clone)]
pub struct TeamRuntimeStorage {
    inner: restflow_storage::TeamRuntimeStorage,
}

impl TeamRuntimeStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::TeamRuntimeStorage::new(db)?,
        })
    }

    pub fn save_state(&self, state: &TeamState) -> Result<()> {
        self.inner
            .put_state_raw(&state.team_run_id, &serde_json::to_vec(state)?)
    }

    pub fn get_state(&self, team_run_id: &str) -> Result<Option<TeamState>> {
        self.decode_optional(self.inner.get_state_raw(team_run_id)?)
    }

    pub fn list_states(&self) -> Result<Vec<TeamState>> {
        let mut states = self.decode_vec::<TeamState>(self.inner.list_states_raw()?)?;
        states.sort_by_key(|state| std::cmp::Reverse(state.updated_at));
        Ok(states)
    }

    pub fn save_message(&self, message: &TeamMessage) -> Result<()> {
        self.inner.put_message_raw(
            &message.team_run_id,
            &message.message_id,
            &serde_json::to_vec(message)?,
        )
    }

    pub fn list_messages(&self, team_run_id: &str) -> Result<Vec<TeamMessage>> {
        let mut messages =
            self.decode_vec::<TeamMessage>(self.inner.list_messages_raw(team_run_id)?)?;
        messages.sort_by_key(|message| message.created_at);
        Ok(messages)
    }

    pub fn save_assignment(&self, assignment: &TeamAssignment) -> Result<()> {
        self.inner.put_assignment_raw(
            &assignment.team_run_id,
            &assignment.assignment_id,
            &serde_json::to_vec(assignment)?,
        )
    }

    pub fn list_assignments(&self, team_run_id: &str) -> Result<Vec<TeamAssignment>> {
        let mut assignments =
            self.decode_vec::<TeamAssignment>(self.inner.list_assignments_raw(team_run_id)?)?;
        assignments.sort_by_key(|assignment| assignment.created_at);
        Ok(assignments)
    }

    pub fn save_approval(&self, approval: &PendingTeamApproval) -> Result<()> {
        self.inner.put_approval_raw(
            &approval.team_run_id,
            &approval.approval_id,
            &serde_json::to_vec(approval)?,
        )
    }

    pub fn get_approval(
        &self,
        team_run_id: &str,
        approval_id: &str,
    ) -> Result<Option<PendingTeamApproval>> {
        self.decode_optional(self.inner.get_approval_raw(team_run_id, approval_id)?)
    }

    pub fn list_approvals(&self, team_run_id: &str) -> Result<Vec<PendingTeamApproval>> {
        let mut approvals =
            self.decode_vec::<PendingTeamApproval>(self.inner.list_approvals_raw(team_run_id)?)?;
        approvals.sort_by_key(|approval| approval.requested_at);
        Ok(approvals)
    }

    fn decode_optional<T: serde::de::DeserializeOwned>(
        &self,
        raw: Option<Vec<u8>>,
    ) -> Result<Option<T>> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&raw)?))
    }

    fn decode_vec<T: serde::de::DeserializeOwned>(
        &self,
        raw: Vec<(String, Vec<u8>)>,
    ) -> Result<Vec<T>> {
        raw.into_iter()
            .map(|(_, bytes)| serde_json::from_slice::<T>(&bytes).map_err(Into::into))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::{TeamMemberState, TeamMemberStatus, TeamRole, TeamStatus};
    use tempfile::tempdir;

    #[test]
    fn stores_and_lists_team_state() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("team-runtime.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = TeamRuntimeStorage::new(db).expect("storage should be created");
        let state = TeamState {
            team_run_id: "team-1".to_string(),
            leader_member_id: "leader".to_string(),
            members: vec![TeamMemberState {
                member_id: "leader".to_string(),
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
            }],
            status: TeamStatus::Running,
            pending_message_count: 0,
            pending_assignment_count: 0,
            updated_at: 1,
        };

        storage.save_state(&state).expect("save should succeed");
        assert_eq!(
            storage.get_state("team-1").unwrap().unwrap().team_run_id,
            "team-1"
        );
        assert_eq!(storage.list_states().unwrap().len(), 1);
    }
}
