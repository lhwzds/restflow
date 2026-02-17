//! CLI commands for Telegram pairing and route binding management.

use anyhow::{Result, anyhow};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use restflow_core::AppCore;
use restflow_core::channel::pairing::PairingManager;
use restflow_core::channel::route_binding::{RouteBindingType, RouteResolver};
use restflow_core::storage::SecretStorage;
use restflow_storage::PairingStorage;

use crate::cli::{PairingCommands, PairingOwnerCommands, RouteCommands};
use crate::commands::utils::format_timestamp;
use crate::output::OutputFormat;
use crate::output::json::print_json;
use serde_json::json;

const TELEGRAM_CHAT_ID_SECRET: &str = "TELEGRAM_CHAT_ID";
const TELEGRAM_DEFAULT_CHAT_ID_SECRET: &str = "TELEGRAM_DEFAULT_CHAT_ID";

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnerChatId {
    value: String,
    source: &'static str,
}

/// Run pairing commands
pub async fn run(core: Arc<AppCore>, command: PairingCommands, format: OutputFormat) -> Result<()> {
    let pairing_storage = Arc::new(PairingStorage::new(core.storage.get_db())?);
    let manager = PairingManager::new(pairing_storage);
    let secrets = &core.storage.secrets;

    match command {
        PairingCommands::List => list_pairing(&manager, format),
        PairingCommands::Approve { code } => approve_pairing(&manager, secrets, &code, format),
        PairingCommands::Deny { code } => deny_pairing(&manager, &code, format),
        PairingCommands::Revoke { peer_id } => revoke_peer(&manager, &peer_id, format),
        PairingCommands::Owner { command } => run_owner_command(secrets, command, format),
    }
}

/// Run route commands
pub async fn run_route(
    core: Arc<AppCore>,
    command: RouteCommands,
    format: OutputFormat,
) -> Result<()> {
    let pairing_storage = Arc::new(PairingStorage::new(core.storage.get_db())?);
    let resolver = RouteResolver::new(pairing_storage);

    match command {
        RouteCommands::List => list_routes(&resolver, format),
        RouteCommands::Bind {
            peer,
            group,
            default,
            agent,
        } => {
            let (binding_type, target_id) = if let Some(peer_id) = peer {
                (RouteBindingType::Peer, peer_id)
            } else if let Some(group_id) = group {
                // Map --group to Channel binding for backward compatibility
                // Group binding is deprecated; use --channel instead
                // This allows existing CLI scripts using --group to continue working
                tracing::warn!(
                    target_id = %group_id,
                    "Using deprecated --group flag, consider using --channel instead"
                );
                (RouteBindingType::Channel, group_id)
            } else if default {
                (RouteBindingType::Default, "*".to_string())
            } else {
                return Err(anyhow!("Must specify --peer, --group, or --default"));
            };
            bind_route(&resolver, binding_type, &target_id, &agent, format)
        }
        RouteCommands::Unbind { id } => unbind_route(&resolver, &id, format),
    }
}

fn list_pairing(manager: &PairingManager, format: OutputFormat) -> Result<()> {
    let peers = manager.list_allowed()?;
    let requests = manager.list_pending()?;

    if format.is_json() {
        return print_json(&json!({
            "allowed_peers": peers,
            "pending_requests": requests,
        }));
    }

    // Allowed peers
    println!("Allowed Peers:");
    if peers.is_empty() {
        println!("  (none)");
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Peer ID", "Name", "Approved At", "Approved By"]);
        for peer in &peers {
            table.add_row(vec![
                Cell::new(&peer.peer_id),
                Cell::new(peer.peer_name.as_deref().unwrap_or("-")),
                Cell::new(format_timestamp(Some(peer.approved_at))),
                Cell::new(&peer.approved_by),
            ]);
        }
        crate::output::table::print_table(table)?;
    }

    println!();

    // Pending requests
    println!("Pending Pairing Requests:");
    if requests.is_empty() {
        println!("  (none)");
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Code", "Peer ID", "Name", "Chat ID", "Expires At"]);
        for req in &requests {
            table.add_row(vec![
                Cell::new(&req.code),
                Cell::new(&req.peer_id),
                Cell::new(req.peer_name.as_deref().unwrap_or("-")),
                Cell::new(&req.chat_id),
                Cell::new(format_timestamp(Some(req.expires_at))),
            ]);
        }
        crate::output::table::print_table(table)?;
    }

    Ok(())
}

fn approve_pairing(
    manager: &PairingManager,
    secrets: &SecretStorage,
    code: &str,
    format: OutputFormat,
) -> Result<()> {
    let (peer, request) = manager.approve_with_request(code, "cli")?;
    let owner_auto_bound = auto_bind_owner_chat_id_if_missing(secrets, &request.chat_id)?;
    let owner_chat_id = resolve_owner_chat_id(secrets)?.map(|owner| owner.value);

    if format.is_json() {
        return print_json(&json!({
            "approved": true,
            "peer_id": peer.peer_id,
            "peer_name": peer.peer_name,
            "owner_chat_id": owner_chat_id,
            "owner_auto_bound": owner_auto_bound,
        }));
    }

    println!(
        "Approved peer {} ({})",
        peer.peer_id,
        peer.peer_name.as_deref().unwrap_or("unknown")
    );
    if owner_auto_bound {
        println!(
            "Owner chat bound to approved request chat ID: {}",
            request.chat_id
        );
    }
    Ok(())
}

fn run_owner_command(
    secrets: &SecretStorage,
    command: PairingOwnerCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        PairingOwnerCommands::Show => show_owner(secrets, format),
        PairingOwnerCommands::Set { chat_id } => set_owner(secrets, &chat_id, format),
    }
}

fn show_owner(secrets: &SecretStorage, format: OutputFormat) -> Result<()> {
    let owner = resolve_owner_chat_id(secrets)?;

    if format.is_json() {
        return print_json(&json!({
            "owner_chat_id": owner.as_ref().map(|owner| owner.value.clone()),
            "source": owner.as_ref().map(|owner| owner.source),
        }));
    }

    match owner {
        Some(owner) => println!("Owner chat ID: {} (source: {})", owner.value, owner.source),
        None => println!("Owner chat ID: (not set)"),
    }
    Ok(())
}

fn set_owner(secrets: &SecretStorage, chat_id: &str, format: OutputFormat) -> Result<()> {
    let normalized_chat_id = chat_id.trim();
    if normalized_chat_id.is_empty() {
        return Err(anyhow!("chat_id cannot be empty"));
    }

    secrets.set_secret(TELEGRAM_CHAT_ID_SECRET, normalized_chat_id, None)?;

    if format.is_json() {
        return print_json(&json!({
            "updated": true,
            "owner_chat_id": normalized_chat_id,
            "source": TELEGRAM_CHAT_ID_SECRET,
        }));
    }

    println!(
        "Owner chat ID set to {} (source: {})",
        normalized_chat_id, TELEGRAM_CHAT_ID_SECRET
    );
    Ok(())
}

fn resolve_owner_chat_id(secrets: &SecretStorage) -> Result<Option<OwnerChatId>> {
    if let Some(value) = non_empty_secret(secrets, TELEGRAM_CHAT_ID_SECRET)? {
        return Ok(Some(OwnerChatId {
            value,
            source: TELEGRAM_CHAT_ID_SECRET,
        }));
    }

    if let Some(value) = non_empty_secret(secrets, TELEGRAM_DEFAULT_CHAT_ID_SECRET)? {
        return Ok(Some(OwnerChatId {
            value,
            source: TELEGRAM_DEFAULT_CHAT_ID_SECRET,
        }));
    }

    Ok(None)
}

fn auto_bind_owner_chat_id_if_missing(secrets: &SecretStorage, chat_id: &str) -> Result<bool> {
    if resolve_owner_chat_id(secrets)?.is_some() {
        return Ok(false);
    }
    secrets.set_secret(TELEGRAM_CHAT_ID_SECRET, chat_id, None)?;
    Ok(true)
}

fn non_empty_secret(secrets: &SecretStorage, key: &str) -> Result<Option<String>> {
    Ok(secrets
        .get_secret(key)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn deny_pairing(manager: &PairingManager, code: &str, format: OutputFormat) -> Result<()> {
    manager.deny(code)?;

    if format.is_json() {
        return print_json(&json!({ "denied": true, "code": code }));
    }

    println!("Denied pairing request: {}", code);
    Ok(())
}

fn revoke_peer(manager: &PairingManager, peer_id: &str, format: OutputFormat) -> Result<()> {
    let removed = manager.revoke(peer_id)?;

    if format.is_json() {
        return print_json(&json!({ "revoked": removed, "peer_id": peer_id }));
    }

    if removed {
        println!("Revoked peer: {}", peer_id);
    } else {
        println!("Peer not found: {}", peer_id);
    }
    Ok(())
}

fn list_routes(resolver: &RouteResolver, format: OutputFormat) -> Result<()> {
    let bindings = resolver.list()?;

    if format.is_json() {
        return print_json(&bindings);
    }

    if bindings.is_empty() {
        println!("No route bindings configured.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Type", "Target", "Agent", "Priority"]);
    for binding in &bindings {
        table.add_row(vec![
            Cell::new(&binding.id[..8]),
            Cell::new(binding.binding_type.to_string()),
            Cell::new(&binding.target_id),
            Cell::new(&binding.agent_id),
            Cell::new(binding.priority.to_string()),
        ]);
    }
    crate::output::table::print_table(table)
}

fn bind_route(
    resolver: &RouteResolver,
    binding_type: RouteBindingType,
    target_id: &str,
    agent_id: &str,
    format: OutputFormat,
) -> Result<()> {
    let binding = resolver.bind(binding_type, target_id, agent_id)?;

    if format.is_json() {
        return print_json(&binding);
    }

    println!(
        "Route bound: {} {} -> agent {}",
        binding.binding_type, binding.target_id, binding.agent_id
    );
    Ok(())
}

fn unbind_route(resolver: &RouteResolver, id: &str, format: OutputFormat) -> Result<()> {
    let removed = resolver.unbind(id)?;

    if format.is_json() {
        return print_json(&json!({ "removed": removed, "id": id }));
    }

    if removed {
        println!("Route binding removed: {}", id);
    } else {
        println!("Route binding not found: {}", id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    fn create_test_secrets() -> (SecretStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("secrets.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        (SecretStorage::new(db).unwrap(), temp_dir)
    }

    #[test]
    fn resolve_owner_prefers_primary_secret() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_DEFAULT_CHAT_ID_SECRET, "legacy", None)
            .unwrap();
        secrets
            .set_secret(TELEGRAM_CHAT_ID_SECRET, "primary", None)
            .unwrap();

        let owner = resolve_owner_chat_id(&secrets).unwrap().unwrap();
        assert_eq!(owner.value, "primary");
        assert_eq!(owner.source, TELEGRAM_CHAT_ID_SECRET);
    }

    #[test]
    fn auto_bind_owner_sets_primary_secret_once() {
        let (secrets, _temp_dir) = create_test_secrets();
        let bound = auto_bind_owner_chat_id_if_missing(&secrets, "123456").unwrap();
        assert!(bound);
        let owner = resolve_owner_chat_id(&secrets).unwrap().unwrap();
        assert_eq!(owner.value, "123456");
        assert_eq!(owner.source, TELEGRAM_CHAT_ID_SECRET);

        let second = auto_bind_owner_chat_id_if_missing(&secrets, "999999").unwrap();
        assert!(!second);
        let owner_after = resolve_owner_chat_id(&secrets).unwrap().unwrap();
        assert_eq!(owner_after.value, "123456");
    }

    #[test]
    fn approve_pairing_auto_binds_owner_only_when_missing() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("pairing.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let secrets = SecretStorage::new(db.clone()).unwrap();
        let manager = PairingManager::new(Arc::new(PairingStorage::new(db).unwrap()));

        let code = manager
            .create_request("peer-1", Some("Peer 1"), "chat-100")
            .unwrap();
        let (_peer, request) = manager.approve_with_request(&code, "cli").unwrap();
        let first_bound = auto_bind_owner_chat_id_if_missing(&secrets, &request.chat_id).unwrap();
        assert!(first_bound);
        assert_eq!(
            resolve_owner_chat_id(&secrets).unwrap().unwrap().value,
            "chat-100"
        );

        let code_second = manager
            .create_request("peer-2", Some("Peer 2"), "chat-200")
            .unwrap();
        let (_peer, request_second) = manager.approve_with_request(&code_second, "cli").unwrap();
        let second_bound =
            auto_bind_owner_chat_id_if_missing(&secrets, &request_second.chat_id).unwrap();
        assert!(!second_bound);
        assert_eq!(
            resolve_owner_chat_id(&secrets).unwrap().unwrap().value,
            "chat-100"
        );
    }
}
