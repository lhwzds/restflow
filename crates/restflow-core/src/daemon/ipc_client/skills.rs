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

    pub async fn create_skill(&mut self, skill: Skill) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::CreateSkill { skill })
            .await?;
        Ok(())
    }

    pub async fn update_skill(&mut self, id: String, skill: Skill) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::UpdateSkill { id, skill })
            .await?;
        Ok(())
    }

    pub async fn delete_skill(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::DeleteSkill { id }).await?;
        Ok(())
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
