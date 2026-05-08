#[cfg(unix)]
use super::*;

#[cfg(unix)]
impl IpcClient {
    pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
        self.request_typed(IpcRequest::ListSkills).await
    }

    pub async fn get_skill(&mut self, id: String) -> Result<Option<Skill>> {
        self.request_optional(IpcRequest::GetSkill { id }).await
    }

    pub async fn get_skill_reference(
        &mut self,
        skill_id: String,
        ref_id: String,
    ) -> Result<Option<String>> {
        self.request_optional(IpcRequest::GetSkillReference { skill_id, ref_id })
            .await
    }

    pub async fn list_agents(&mut self) -> Result<Vec<StoredAgent>> {
        self.request_typed(IpcRequest::ListAgents).await
    }

    pub async fn get_agent(&mut self, id: String) -> Result<StoredAgent> {
        self.request_typed(IpcRequest::GetAgent { id }).await
    }
}
