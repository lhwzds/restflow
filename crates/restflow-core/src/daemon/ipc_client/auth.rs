#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
#[cfg(unix)]
use restflow_contracts::{ApiKeyResponse, OkResponse};

#[cfg(unix)]
impl IpcClient {
    pub async fn list_auth_profiles(&mut self) -> Result<Vec<AuthProfile>> {
        self.request_typed(IpcRequest::ListAuthProfiles).await
    }

    pub async fn get_auth_profile(&mut self, id: String) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::GetAuthProfile { id }).await
    }

    pub async fn add_auth_profile(
        &mut self,
        name: String,
        credential: Credential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Result<AuthProfile> {
        let credential = to_contract(credential)?;
        let source = to_contract(source)?;
        let provider = to_contract(provider)?;
        self.request_typed(IpcRequest::AddAuthProfile {
            name,
            credential,
            source,
            provider,
        })
        .await
    }

    pub async fn remove_auth_profile(&mut self, id: String) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::RemoveAuthProfile { id })
            .await
    }

    pub async fn update_auth_profile(
        &mut self,
        id: String,
        updates: ProfileUpdate,
    ) -> Result<AuthProfile> {
        let updates = to_contract(updates)?;
        self.request_typed(IpcRequest::UpdateAuthProfile { id, updates })
            .await
    }

    pub async fn discover_auth(&mut self) -> Result<crate::auth::DiscoverySummary> {
        self.request_typed(IpcRequest::DiscoverAuth).await
    }

    pub async fn enable_auth_profile(&mut self, id: String) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::EnableAuthProfile { id })
            .await?;
        Ok(())
    }

    pub async fn disable_auth_profile(&mut self, id: String, reason: String) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DisableAuthProfile { id, reason })
            .await?;
        Ok(())
    }

    pub async fn get_api_key(&mut self, provider: AuthProvider) -> Result<String> {
        let provider = to_contract(provider)?;
        let resp: ApiKeyResponse = self
            .request_typed(IpcRequest::GetApiKey { provider })
            .await?;
        Ok(resp.api_key)
    }

    pub async fn get_api_key_for_profile(&mut self, id: String) -> Result<String> {
        let resp: ApiKeyResponse = self
            .request_typed(IpcRequest::GetApiKeyForProfile { id })
            .await?;
        Ok(resp.api_key)
    }

    pub async fn test_auth_profile(&mut self, id: String) -> Result<bool> {
        let resp: OkResponse = self
            .request_typed(IpcRequest::TestAuthProfile { id })
            .await?;
        Ok(resp.ok)
    }

    pub async fn mark_auth_success(&mut self, id: String) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::MarkAuthSuccess { id })
            .await?;
        Ok(())
    }

    pub async fn mark_auth_failure(&mut self, id: String) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::MarkAuthFailure { id })
            .await?;
        Ok(())
    }

    pub async fn clear_auth_profiles(&mut self) -> Result<()> {
        let _: OkResponse = self.request_typed(IpcRequest::ClearAuthProfiles).await?;
        Ok(())
    }
}
