use super::super::*;
use restflow_contracts::{
    AllowedPeerResponse, DeleteResponse, OkResponse, PairingApprovalResponse, PairingOwnerResponse,
    PairingRequestResponse, PairingStateResponse, RouteBindingResponse,
};

const TELEGRAM_CHAT_ID_SECRET: &str = "TELEGRAM_CHAT_ID";
const TELEGRAM_DEFAULT_CHAT_ID_SECRET: &str = "TELEGRAM_DEFAULT_CHAT_ID";

impl IpcServer {
    pub(super) async fn handle_list_pairing_state(core: &Arc<AppCore>) -> IpcResponse {
        let manager = match pairing_manager(core) {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };

        let allowed_peers = match manager.list_allowed() {
            Ok(peers) => peers
                .into_iter()
                .map(|peer| AllowedPeerResponse {
                    peer_id: peer.peer_id,
                    peer_name: peer.peer_name,
                    approved_at: peer.approved_at,
                    approved_by: peer.approved_by,
                })
                .collect(),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };

        let pending_requests = match manager.list_pending() {
            Ok(requests) => requests
                .into_iter()
                .map(|request| PairingRequestResponse {
                    code: request.code,
                    peer_id: request.peer_id,
                    peer_name: request.peer_name,
                    chat_id: request.chat_id,
                    created_at: request.created_at,
                    expires_at: request.expires_at,
                })
                .collect(),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };

        IpcResponse::success(PairingStateResponse {
            allowed_peers,
            pending_requests,
        })
    }

    pub(super) async fn handle_approve_pairing(core: &Arc<AppCore>, code: String) -> IpcResponse {
        let manager = match pairing_manager(core) {
            Ok(manager) => manager,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };

        let (peer, request) = match manager.approve_with_request(&code, "cli") {
            Ok(result) => result,
            Err(err) => return pairing_error_response(err),
        };

        let owner_auto_bound =
            match auto_bind_owner_chat_id_if_missing(&core.storage.secrets, &request.chat_id) {
                Ok(bound) => bound,
                Err(err) => return IpcResponse::error(500, err.to_string()),
            };
        let owner = match resolve_owner_chat_id(&core.storage.secrets) {
            Ok(owner) => owner,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };

        IpcResponse::success(PairingApprovalResponse {
            approved: true,
            peer_id: peer.peer_id,
            peer_name: peer.peer_name,
            owner_chat_id: owner.as_ref().map(|value| value.0.clone()),
            owner_auto_bound,
        })
    }

    pub(super) async fn handle_deny_pairing(core: &Arc<AppCore>, code: String) -> IpcResponse {
        match pairing_manager(core).and_then(|manager| manager.deny(&code)) {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => pairing_error_response(err),
        }
    }

    pub(super) async fn handle_revoke_paired_peer(
        core: &Arc<AppCore>,
        peer_id: String,
    ) -> IpcResponse {
        match pairing_manager(core).and_then(|manager| manager.revoke(&peer_id)) {
            Ok(deleted) => IpcResponse::success(DeleteResponse { deleted }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_pairing_owner(core: &Arc<AppCore>) -> IpcResponse {
        match resolve_owner_chat_id(&core.storage.secrets) {
            Ok(owner) => IpcResponse::success(PairingOwnerResponse {
                owner_chat_id: owner.as_ref().map(|value| value.0.clone()),
                source: owner.map(|value| value.1),
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_set_pairing_owner(
        core: &Arc<AppCore>,
        chat_id: String,
    ) -> IpcResponse {
        let normalized_chat_id = chat_id.trim();
        if normalized_chat_id.is_empty() {
            return IpcResponse::error(400, "chat_id cannot be empty");
        }

        match core
            .storage
            .secrets
            .set_secret(TELEGRAM_CHAT_ID_SECRET, normalized_chat_id, None)
        {
            Ok(()) => IpcResponse::success(PairingOwnerResponse {
                owner_chat_id: Some(normalized_chat_id.to_string()),
                source: Some(TELEGRAM_CHAT_ID_SECRET.to_string()),
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_list_route_bindings(core: &Arc<AppCore>) -> IpcResponse {
        match route_resolver(core).and_then(|resolver| resolver.list()) {
            Ok(bindings) => {
                let response: Result<Vec<_>, _> =
                    bindings.into_iter().map(route_binding_response).collect();
                match response {
                    Ok(bindings) => IpcResponse::success(bindings),
                    Err(err) => IpcResponse::error(500, err.to_string()),
                }
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_bind_route(
        core: &Arc<AppCore>,
        binding_type: String,
        target_id: String,
        agent_id: String,
    ) -> IpcResponse {
        let binding_type = match parse_route_binding_type(&binding_type, &target_id) {
            Ok(binding_type) => binding_type,
            Err(err) => return IpcResponse::error(400, err),
        };

        match route_resolver(core)
            .and_then(|resolver| resolver.bind(binding_type, &target_id, &agent_id))
        {
            Ok(binding) => match route_binding_response(binding) {
                Ok(binding) => IpcResponse::success(binding),
                Err(err) => IpcResponse::error(500, err.to_string()),
            },
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_unbind_route(core: &Arc<AppCore>, id: String) -> IpcResponse {
        match route_resolver(core).and_then(|resolver| resolver.unbind(&id)) {
            Ok(deleted) => IpcResponse::success(DeleteResponse { deleted }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}

fn pairing_manager(core: &Arc<AppCore>) -> anyhow::Result<crate::channel::PairingManager> {
    let storage = Arc::new(crate::storage::PairingStorage::new(core.storage.get_db())?);
    Ok(crate::channel::PairingManager::new(storage))
}

fn route_resolver(core: &Arc<AppCore>) -> anyhow::Result<crate::channel::RouteResolver> {
    let storage = Arc::new(crate::storage::PairingStorage::new(core.storage.get_db())?);
    Ok(crate::channel::RouteResolver::new(storage))
}

fn resolve_owner_chat_id(
    secrets: &crate::storage::SecretStorage,
) -> anyhow::Result<Option<(String, String)>> {
    if let Some(value) = secrets.get_non_empty(TELEGRAM_CHAT_ID_SECRET)? {
        return Ok(Some((value, TELEGRAM_CHAT_ID_SECRET.to_string())));
    }

    if let Some(value) = secrets.get_non_empty(TELEGRAM_DEFAULT_CHAT_ID_SECRET)? {
        return Ok(Some((value, TELEGRAM_DEFAULT_CHAT_ID_SECRET.to_string())));
    }

    Ok(None)
}

fn auto_bind_owner_chat_id_if_missing(
    secrets: &crate::storage::SecretStorage,
    chat_id: &str,
) -> anyhow::Result<bool> {
    if resolve_owner_chat_id(secrets)?.is_some() {
        return Ok(false);
    }
    secrets.set_secret(TELEGRAM_CHAT_ID_SECRET, chat_id, None)?;
    Ok(true)
}

fn parse_route_binding_type(
    value: &str,
    target_id: &str,
) -> Result<crate::channel::RouteBindingType, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "peer" => Ok(crate::channel::RouteBindingType::Peer),
        "account" => Ok(crate::channel::RouteBindingType::Account),
        "channel" => Ok(crate::channel::RouteBindingType::Channel),
        "default" => Ok(crate::channel::RouteBindingType::Default),
        "group" => {
            tracing::warn!(
                target_id = %target_id,
                "Using deprecated --group flag, consider using --channel instead"
            );
            Ok(crate::channel::RouteBindingType::Group)
        }
        other => Err(format!("Unsupported route binding type: {other}")),
    }
}

fn route_binding_response(
    binding: crate::channel::RouteBinding,
) -> anyhow::Result<RouteBindingResponse> {
    Ok(RouteBindingResponse {
        id: binding.id,
        binding_type: binding.binding_type.to_string(),
        target_id: binding.target_id,
        agent_id: binding.agent_id,
        created_at: binding.created_at,
        priority: binding.priority,
    })
}

fn pairing_error_response(err: anyhow::Error) -> IpcResponse {
    let message = err.to_string();
    if message.contains("not found") {
        return IpcResponse::not_found("Pairing request");
    }
    if message.contains("expired") {
        return IpcResponse::error(400, message);
    }
    IpcResponse::error(500, message)
}
