//! Auth Profile Management Tauri Commands
//!
//! Provides IPC endpoints for frontend credential management.

use restflow_core::auth::{
    AuthManagerConfig, AuthProfile, AuthProfileManager, AuthProvider, Credential,
    CredentialSource, DiscoverySummary, ManagerSummary, ProfileUpdate,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use ts_rs::TS;

/// Auth manager state for Tauri app
pub struct AuthState {
    manager: Arc<AuthProfileManager>,
    initialized: Arc<RwLock<bool>>,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(AuthProfileManager::new()),
            initialized: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_config(config: AuthManagerConfig) -> Self {
        Self {
            manager: Arc::new(AuthProfileManager::with_config(config)),
            initialized: Arc::new(RwLock::new(false)),
        }
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Request to add a manual profile
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
pub struct AddProfileRequest {
    /// Display name for the profile
    pub name: String,
    /// API key value
    pub api_key: String,
    /// Provider (anthropic, openai, google, other)
    pub provider: AuthProvider,
    /// Associated email (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Priority (lower = higher priority)
    #[serde(default)]
    pub priority: i32,
}

/// Response for profile operations
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
pub struct ProfileResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<AuthProfile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ProfileResponse {
    pub fn success(profile: AuthProfile) -> Self {
        Self {
            success: true,
            profile: Some(profile),
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            profile: None,
            error: Some(msg.into()),
        }
    }
}

/// Initialize the auth manager (run discovery)
#[tauri::command]
pub async fn auth_initialize(state: State<'_, AuthState>) -> Result<DiscoverySummary, String> {
    let mut initialized = state.initialized.write().await;
    if *initialized {
        // Already initialized, just return current summary as discovery summary
        let summary = state.manager.get_summary().await;
        return Ok(DiscoverySummary {
            total: summary.total,
            by_source: summary.by_source,
            by_provider: summary.by_provider,
            available: summary.available,
            errors: Vec::new(),
        });
    }

    let result = state
        .manager
        .initialize()
        .await
        .map_err(|e| e.to_string())?;

    *initialized = true;
    Ok(result)
}

/// Run credential discovery
#[tauri::command]
pub async fn auth_discover(state: State<'_, AuthState>) -> Result<DiscoverySummary, String> {
    state.manager.discover().await.map_err(|e| e.to_string())
}

/// List all profiles
#[tauri::command]
pub async fn auth_list_profiles(state: State<'_, AuthState>) -> Result<Vec<AuthProfile>, String> {
    Ok(state.manager.list_profiles().await)
}

/// Get profiles for a specific provider
#[tauri::command]
pub async fn auth_get_profiles_for_provider(
    state: State<'_, AuthState>,
    provider: AuthProvider,
) -> Result<Vec<AuthProfile>, String> {
    Ok(state.manager.get_profiles_for_provider(provider).await)
}

/// Get available profiles (enabled, not expired, not in cooldown)
#[tauri::command]
pub async fn auth_get_available_profiles(
    state: State<'_, AuthState>,
) -> Result<Vec<AuthProfile>, String> {
    Ok(state.manager.get_available_profiles().await)
}

/// Get a specific profile by ID
#[tauri::command]
pub async fn auth_get_profile(
    state: State<'_, AuthState>,
    profile_id: String,
) -> Result<Option<AuthProfile>, String> {
    Ok(state.manager.get_profile(&profile_id).await)
}

/// Add a manual profile
#[tauri::command]
pub async fn auth_add_profile(
    state: State<'_, AuthState>,
    request: AddProfileRequest,
) -> Result<ProfileResponse, String> {
    let credential = Credential::ApiKey {
        key: request.api_key,
        email: request.email,
    };

    let mut profile = AuthProfile::new(
        request.name,
        credential,
        CredentialSource::Manual,
        request.provider,
    );
    profile.priority = request.priority;

    match state.manager.add_profile(profile).await {
        Ok(id) => {
            let profile = state.manager.get_profile(&id).await;
            match profile {
                Some(p) => Ok(ProfileResponse::success(p)),
                None => Ok(ProfileResponse::error("Profile created but not found")),
            }
        }
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Remove a profile
#[tauri::command]
pub async fn auth_remove_profile(
    state: State<'_, AuthState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    match state.manager.remove_profile(&profile_id).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Update a profile
#[tauri::command]
pub async fn auth_update_profile(
    state: State<'_, AuthState>,
    profile_id: String,
    update: ProfileUpdate,
) -> Result<ProfileResponse, String> {
    match state.manager.update_profile(&profile_id, update).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Enable a profile
#[tauri::command]
pub async fn auth_enable_profile(
    state: State<'_, AuthState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state.manager.enable_profile(&profile_id).await {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.manager.get_profile(&profile_id).await {
        Some(profile) => Ok(ProfileResponse::success(profile)),
        None => Ok(ProfileResponse::error("Profile not found after enable")),
    }
}

/// Disable a profile
#[tauri::command]
pub async fn auth_disable_profile(
    state: State<'_, AuthState>,
    profile_id: String,
    reason: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state.manager.disable_profile(&profile_id, &reason).await {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.manager.get_profile(&profile_id).await {
        Some(profile) => Ok(ProfileResponse::success(profile)),
        None => Ok(ProfileResponse::error("Profile not found after disable")),
    }
}

/// Mark a profile as successfully used
#[tauri::command]
pub async fn auth_mark_success(
    state: State<'_, AuthState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state.manager.mark_success(&profile_id).await {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.manager.get_profile(&profile_id).await {
        Some(profile) => Ok(ProfileResponse::success(profile)),
        None => Ok(ProfileResponse::error("Profile not found after mark_success")),
    }
}

/// Mark a profile as failed
#[tauri::command]
pub async fn auth_mark_failure(
    state: State<'_, AuthState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state.manager.mark_failure(&profile_id).await {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.manager.get_profile(&profile_id).await {
        Some(profile) => Ok(ProfileResponse::success(profile)),
        None => Ok(ProfileResponse::error("Profile not found after mark_failure")),
    }
}

/// Get API key for a provider (selects best available profile)
#[tauri::command]
pub async fn auth_get_api_key(
    state: State<'_, AuthState>,
    provider: AuthProvider,
) -> Result<Option<String>, String> {
    Ok(state.manager.get_api_key(provider).await)
}

/// Get manager summary
#[tauri::command]
pub async fn auth_get_summary(state: State<'_, AuthState>) -> Result<ManagerSummary, String> {
    Ok(state.manager.get_summary().await)
}

/// Clear all profiles
#[tauri::command]
pub async fn auth_clear(state: State<'_, AuthState>) -> Result<(), String> {
    state.manager.clear().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_profile_request_serialization() {
        let request = AddProfileRequest {
            name: "Test".to_string(),
            api_key: "key123".to_string(),
            provider: AuthProvider::Anthropic,
            email: Some("test@example.com".to_string()),
            priority: 0,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("key123"));
    }

    #[test]
    fn test_profile_response_success() {
        let profile = AuthProfile::new(
            "Test",
            Credential::ApiKey {
                key: "key".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::Anthropic,
        );

        let response = ProfileResponse::success(profile.clone());
        assert!(response.success);
        assert!(response.profile.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_profile_response_error() {
        let response = ProfileResponse::error("Test error");
        assert!(!response.success);
        assert!(response.profile.is_none());
        assert_eq!(response.error, Some("Test error".to_string()));
    }

    #[test]
    fn test_auth_state_default() {
        let state = AuthState::default();
        assert!(Arc::strong_count(&state.manager) == 1);
    }
}
