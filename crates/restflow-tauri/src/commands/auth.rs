//! Auth Profile Management Tauri Commands
//!
//! Provides IPC endpoints for frontend credential management.

use crate::state::AppState;
use restflow_core::auth::{
    AuthProfile, AuthProvider, Credential, CredentialSource, DiscoverySummary, ManagerSummary,
    ProfileHealth, ProfileUpdate,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

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
pub async fn auth_initialize(state: State<'_, AppState>) -> Result<DiscoverySummary, String> {
    state
        .executor()
        .discover_auth()
        .await
        .map_err(|e| e.to_string())
}

/// Run credential discovery
#[tauri::command]
pub async fn auth_discover(state: State<'_, AppState>) -> Result<DiscoverySummary, String> {
    state
        .executor()
        .discover_auth()
        .await
        .map_err(|e| e.to_string())
}

/// List all profiles
#[tauri::command]
pub async fn auth_list_profiles(state: State<'_, AppState>) -> Result<Vec<AuthProfile>, String> {
    state
        .executor()
        .list_auth_profiles()
        .await
        .map_err(|e| e.to_string())
}

/// Get profiles for a specific provider
#[tauri::command]
pub async fn auth_get_profiles_for_provider(
    state: State<'_, AppState>,
    provider: AuthProvider,
) -> Result<Vec<AuthProfile>, String> {
    let profiles = state
        .executor()
        .list_auth_profiles()
        .await
        .map_err(|e| e.to_string())?;

    Ok(profiles
        .into_iter()
        .filter(|profile| profile.provider == provider)
        .collect())
}

/// Get available profiles (enabled, not expired, not in cooldown)
#[tauri::command]
pub async fn auth_get_available_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<AuthProfile>, String> {
    let profiles = state
        .executor()
        .list_auth_profiles()
        .await
        .map_err(|e| e.to_string())?;

    Ok(profiles
        .into_iter()
        .filter(|profile| profile.is_available())
        .collect())
}

/// Get a specific profile by ID
#[tauri::command]
pub async fn auth_get_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Option<AuthProfile>, String> {
    let profiles = state
        .executor()
        .list_auth_profiles()
        .await
        .map_err(|e| e.to_string())?;

    Ok(profiles
        .into_iter()
        .find(|profile| profile.id == profile_id))
}

/// Add a manual profile
#[tauri::command]
pub async fn auth_add_profile(
    state: State<'_, AppState>,
    request: AddProfileRequest,
) -> Result<ProfileResponse, String> {
    let credential = Credential::ApiKey {
        key: request.api_key,
        email: request.email,
    };

    match state
        .executor()
        .add_auth_profile(
            request.name,
            credential,
            CredentialSource::Manual,
            request.provider,
        )
        .await
    {
        Ok(mut profile) => {
            if request.priority != 0 {
                let update = ProfileUpdate {
                    name: None,
                    enabled: None,
                    priority: Some(request.priority),
                };
                profile = state
                    .executor()
                    .update_auth_profile(profile.id.clone(), update)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(ProfileResponse::success(profile))
        }
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Remove a profile
#[tauri::command]
pub async fn auth_remove_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    match state.executor().remove_auth_profile(profile_id).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Update a profile
#[tauri::command]
pub async fn auth_update_profile(
    state: State<'_, AppState>,
    profile_id: String,
    update: ProfileUpdate,
) -> Result<ProfileResponse, String> {
    match state
        .executor()
        .update_auth_profile(profile_id, update)
        .await
    {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Enable a profile
#[tauri::command]
pub async fn auth_enable_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state
        .executor()
        .enable_auth_profile(profile_id.clone())
        .await
    {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.executor().get_auth_profile(profile_id).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Disable a profile
#[tauri::command]
pub async fn auth_disable_profile(
    state: State<'_, AppState>,
    profile_id: String,
    reason: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state
        .executor()
        .disable_auth_profile(profile_id.clone(), reason)
        .await
    {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.executor().get_auth_profile(profile_id).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Mark a profile as successfully used
#[tauri::command]
pub async fn auth_mark_success(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state.executor().mark_auth_success(profile_id.clone()).await {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.executor().get_auth_profile(profile_id).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Mark a profile as failed
#[tauri::command]
pub async fn auth_mark_failure(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ProfileResponse, String> {
    if let Err(e) = state.executor().mark_auth_failure(profile_id.clone()).await {
        return Ok(ProfileResponse::error(e.to_string()));
    }

    match state.executor().get_auth_profile(profile_id).await {
        Ok(profile) => Ok(ProfileResponse::success(profile)),
        Err(e) => Ok(ProfileResponse::error(e.to_string())),
    }
}

/// Check if an API key exists for a provider (selects best available profile)
#[tauri::command]
pub async fn auth_get_api_key(
    state: State<'_, AppState>,
    provider: AuthProvider,
) -> Result<Option<bool>, String> {
    match state.executor().get_api_key(provider).await {
        Ok(_) => Ok(Some(true)),
        Err(_) => Ok(None),
    }
}

/// Get manager summary
#[tauri::command]
pub async fn auth_get_summary(state: State<'_, AppState>) -> Result<ManagerSummary, String> {
    let profiles = state
        .executor()
        .list_auth_profiles()
        .await
        .map_err(|e| e.to_string())?;
    Ok(summary_from_profiles(&profiles))
}

/// Clear all profiles
#[tauri::command]
pub async fn auth_clear(state: State<'_, AppState>) -> Result<(), String> {
    state
        .executor()
        .clear_auth_profiles()
        .await
        .map_err(|e| e.to_string())
}

fn summary_from_profiles(profiles: &[AuthProfile]) -> ManagerSummary {
    let total = profiles.len();
    let enabled = profiles.iter().filter(|p| p.enabled).count();
    let available = profiles.iter().filter(|p| p.is_available()).count();
    let in_cooldown = profiles
        .iter()
        .filter(|p| p.health == ProfileHealth::Cooldown)
        .count();
    let disabled = profiles
        .iter()
        .filter(|p| p.health == ProfileHealth::Disabled)
        .count();

    let mut by_provider = std::collections::HashMap::new();
    let mut by_source = std::collections::HashMap::new();

    for profile in profiles {
        *by_provider.entry(profile.provider.to_string()).or_insert(0) += 1;
        *by_source.entry(profile.source.to_string()).or_insert(0) += 1;
    }

    ManagerSummary {
        total,
        enabled,
        available,
        in_cooldown,
        disabled,
        by_provider,
        by_source,
    }
}
