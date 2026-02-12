//! CLI commands for Telegram pairing and route binding management.

use anyhow::{Result, anyhow};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use restflow_core::AppCore;
use restflow_core::channel::pairing::PairingManager;
use restflow_core::channel::route_binding::{RouteBindingType, RouteResolver};
use restflow_storage::PairingStorage;

use crate::cli::{PairingCommands, RouteCommands};
use crate::commands::utils::format_timestamp;
use crate::output::OutputFormat;
use crate::output::json::print_json;
use serde_json::json;

/// Run pairing commands
pub async fn run(
    core: Arc<AppCore>,
    command: PairingCommands,
    format: OutputFormat,
) -> Result<()> {
    let pairing_storage = Arc::new(PairingStorage::new(core.storage.get_db())?);
    let manager = PairingManager::new(pairing_storage);

    match command {
        PairingCommands::List => list_pairing(&manager, format),
        PairingCommands::Approve { code } => approve_pairing(&manager, &code, format),
        PairingCommands::Deny { code } => deny_pairing(&manager, &code, format),
        PairingCommands::Revoke { peer_id } => revoke_peer(&manager, &peer_id, format),
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
                (RouteBindingType::Group, group_id)
            } else if default {
                (RouteBindingType::Default, "*".to_string())
            } else {
                return Err(anyhow!(
                    "Must specify --peer, --group, or --default"
                ));
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

fn approve_pairing(manager: &PairingManager, code: &str, format: OutputFormat) -> Result<()> {
    let peer = manager.approve(code, "cli")?;

    if format.is_json() {
        return print_json(&json!({
            "approved": true,
            "peer_id": peer.peer_id,
            "peer_name": peer.peer_name,
        }));
    }

    println!(
        "Approved peer {} ({})",
        peer.peer_id,
        peer.peer_name.as_deref().unwrap_or("unknown")
    );
    Ok(())
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
