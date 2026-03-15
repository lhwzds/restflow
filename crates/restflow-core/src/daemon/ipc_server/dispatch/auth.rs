use super::super::runtime::build_auth_manager;
use super::super::*;
use restflow_contracts::{ApiKeyResponse, OkResponse};

impl IpcServer {
    pub(super) async fn handle_list_auth_profiles(core: &Arc<AppCore>) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        IpcResponse::success(manager.list_profiles().await)
    }

    pub(super) async fn handle_get_auth_profile(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.get_profile(&id).await {
            Some(profile) => IpcResponse::success(profile),
            None => IpcResponse::not_found("Auth profile"),
        }
    }

    pub(super) async fn handle_add_auth_profile(
        core: &Arc<AppCore>,
        name: String,
        credential: crate::auth::Credential,
        source: crate::auth::CredentialSource,
        provider: crate::auth::AuthProvider,
    ) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager
            .add_profile_from_credential(name, credential, source, provider)
            .await
        {
            Ok(id) => match manager.get_profile(&id).await {
                Some(profile) => IpcResponse::success(profile),
                None => IpcResponse::error(500, "Profile created but not found"),
            },
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_remove_auth_profile(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.remove_profile(&id).await {
            Ok(profile) => IpcResponse::success(profile),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_auth_profile(
        core: &Arc<AppCore>,
        id: String,
        updates: crate::auth::ProfileUpdate,
    ) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.update_profile(&id, updates).await {
            Ok(profile) => IpcResponse::success(profile),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_discover_auth(core: &Arc<AppCore>) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.discover().await {
            Ok(summary) => IpcResponse::success(summary),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_enable_auth_profile(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.enable_profile(&id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_disable_auth_profile(
        core: &Arc<AppCore>,
        id: String,
        reason: String,
    ) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.disable_profile(&id, &reason).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_api_key(
        core: &Arc<AppCore>,
        provider: crate::auth::AuthProvider,
    ) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.get_available_profile(provider).await {
            Some(profile) => match profile.get_api_key(manager.resolver()) {
                Ok(key) => IpcResponse::success(ApiKeyResponse {
                    api_key: key,
                    profile_id: Some(profile.id),
                }),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            None => IpcResponse::not_found("Auth profile"),
        }
    }

    pub(super) async fn handle_get_api_key_for_profile(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.get_profile(&id).await {
            Some(profile) => match profile.get_api_key(manager.resolver()) {
                Ok(key) => IpcResponse::success(ApiKeyResponse {
                    api_key: key,
                    profile_id: Some(profile.id),
                }),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            None => IpcResponse::not_found("Auth profile"),
        }
    }

    pub(super) async fn handle_test_auth_profile(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.get_profile(&id).await {
            Some(profile) => match profile.get_api_key(manager.resolver()) {
                Ok(_) => IpcResponse::success(OkResponse { ok: true }),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            None => IpcResponse::not_found("Auth profile"),
        }
    }

    pub(super) async fn handle_mark_auth_success(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.mark_success(&id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_mark_auth_failure(core: &Arc<AppCore>, id: String) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        match manager.mark_failure(&id).await {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_clear_auth_profiles(core: &Arc<AppCore>) -> IpcResponse {
        let manager = match build_auth_manager(core).await {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        manager.clear().await;
        IpcResponse::success(OkResponse { ok: true })
    }
}
