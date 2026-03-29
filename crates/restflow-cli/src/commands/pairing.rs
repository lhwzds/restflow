//! CLI commands for Telegram pairing and route binding management.

use anyhow::{Result, anyhow};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::{PairingCommands, PairingOwnerCommands, RouteCommands};
use crate::commands::utils::format_timestamp;
use crate::executor::CommandExecutor;
use crate::output::OutputFormat;
use crate::output::json::print_json;
use serde_json::json;

/// Run pairing commands.
pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: PairingCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        PairingCommands::List => list_pairing(executor, format).await,
        PairingCommands::Approve { code } => approve_pairing(executor, &code, format).await,
        PairingCommands::Deny { code } => deny_pairing(executor, &code, format).await,
        PairingCommands::Revoke { peer_id } => revoke_peer(executor, &peer_id, format).await,
        PairingCommands::Owner { command } => run_owner_command(executor, command, format).await,
    }
}

/// Run route commands.
pub async fn run_route(
    executor: Arc<dyn CommandExecutor>,
    command: RouteCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        RouteCommands::List => list_routes(executor, format).await,
        RouteCommands::Bind {
            peer,
            group,
            default,
            agent,
        } => {
            let (binding_type, target_id) = route_binding_input(peer, group, default)?;
            bind_route(executor, binding_type, &target_id, &agent, format).await
        }
        RouteCommands::Unbind { id } => unbind_route(executor, &id, format).await,
    }
}

async fn list_pairing(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let state = executor.list_pairing_state().await?;

    if format.is_json() {
        return print_json(&state);
    }

    println!("Allowed Peers:");
    if state.allowed_peers.is_empty() {
        println!("  (none)");
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Peer ID", "Name", "Approved At", "Approved By"]);
        for peer in &state.allowed_peers {
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
    println!("Pending Pairing Requests:");
    if state.pending_requests.is_empty() {
        println!("  (none)");
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Code", "Peer ID", "Name", "Chat ID", "Expires At"]);
        for request in &state.pending_requests {
            table.add_row(vec![
                Cell::new(&request.code),
                Cell::new(&request.peer_id),
                Cell::new(request.peer_name.as_deref().unwrap_or("-")),
                Cell::new(&request.chat_id),
                Cell::new(format_timestamp(Some(request.expires_at))),
            ]);
        }
        crate::output::table::print_table(table)?;
    }

    Ok(())
}

async fn approve_pairing(
    executor: Arc<dyn CommandExecutor>,
    code: &str,
    format: OutputFormat,
) -> Result<()> {
    let response = executor.approve_pairing(code).await?;

    if format.is_json() {
        return print_json(&json!({
            "approved": response.approved,
            "peer_id": response.peer_id,
            "peer_name": response.peer_name,
            "owner_chat_id": response.owner_chat_id,
            "owner_auto_bound": response.owner_auto_bound,
        }));
    }

    println!(
        "Approved peer {} ({})",
        response.peer_id,
        response.peer_name.as_deref().unwrap_or("unknown")
    );
    if response.owner_auto_bound
        && let Some(owner_chat_id) = response.owner_chat_id.as_deref()
    {
        println!("Owner chat bound to approved request chat ID: {owner_chat_id}");
    }
    Ok(())
}

async fn run_owner_command(
    executor: Arc<dyn CommandExecutor>,
    command: PairingOwnerCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        PairingOwnerCommands::Show => show_owner(executor, format).await,
        PairingOwnerCommands::Set { chat_id } => set_owner(executor, &chat_id, format).await,
    }
}

async fn show_owner(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let owner = executor.get_pairing_owner().await?;

    if format.is_json() {
        return print_json(&json!({
            "owner_chat_id": owner.owner_chat_id,
            "source": owner.source,
        }));
    }

    match owner.owner_chat_id {
        Some(owner_chat_id) => println!(
            "Owner chat ID: {} (source: {})",
            owner_chat_id,
            owner.source.as_deref().unwrap_or("unknown")
        ),
        None => println!("Owner chat ID: (not set)"),
    }
    Ok(())
}

async fn set_owner(
    executor: Arc<dyn CommandExecutor>,
    chat_id: &str,
    format: OutputFormat,
) -> Result<()> {
    let owner = executor.set_pairing_owner(chat_id).await?;

    if format.is_json() {
        return print_json(&json!({
            "updated": true,
            "owner_chat_id": owner.owner_chat_id,
            "source": owner.source,
        }));
    }

    println!(
        "Owner chat ID set to {} (source: {})",
        owner.owner_chat_id.as_deref().unwrap_or(""),
        owner.source.as_deref().unwrap_or("unknown")
    );
    Ok(())
}

async fn deny_pairing(
    executor: Arc<dyn CommandExecutor>,
    code: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.deny_pairing(code).await?;

    if format.is_json() {
        return print_json(&json!({ "denied": true, "code": code }));
    }

    println!("Denied pairing request: {code}");
    Ok(())
}

async fn revoke_peer(
    executor: Arc<dyn CommandExecutor>,
    peer_id: &str,
    format: OutputFormat,
) -> Result<()> {
    let revoked = executor.revoke_paired_peer(peer_id).await?;

    if format.is_json() {
        return print_json(&json!({ "revoked": revoked, "peer_id": peer_id }));
    }

    if revoked {
        println!("Revoked peer: {peer_id}");
    } else {
        println!("Peer not found: {peer_id}");
    }
    Ok(())
}

async fn list_routes(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let bindings = executor.list_route_bindings().await?;

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
            Cell::new(short_id(&binding.id)),
            Cell::new(&binding.binding_type),
            Cell::new(&binding.target_id),
            Cell::new(&binding.agent_id),
            Cell::new(binding.priority.to_string()),
        ]);
    }
    crate::output::table::print_table(table)
}

async fn bind_route(
    executor: Arc<dyn CommandExecutor>,
    binding_type: &'static str,
    target_id: &str,
    agent_id: &str,
    format: OutputFormat,
) -> Result<()> {
    let binding = executor
        .bind_route(binding_type, target_id, agent_id)
        .await?;

    if format.is_json() {
        return print_json(&binding);
    }

    println!(
        "Route bound: {} {} -> agent {}",
        binding.binding_type, binding.target_id, binding.agent_id
    );
    Ok(())
}

async fn unbind_route(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let removed = executor.unbind_route(id).await?;

    if format.is_json() {
        return print_json(&json!({ "removed": removed, "id": id }));
    }

    if removed {
        println!("Route binding removed: {id}");
    } else {
        println!("Route binding not found: {id}");
    }
    Ok(())
}

fn route_binding_input(
    peer: Option<String>,
    group: Option<String>,
    default: bool,
) -> Result<(&'static str, String)> {
    if let Some(peer_id) = peer {
        return Ok(("peer", peer_id));
    }

    if let Some(group_id) = group {
        return Ok(("group", group_id));
    }

    if default {
        return Ok(("default", "*".to_string()));
    }

    Err(anyhow!("Must specify --peer, --group, or --default"))
}

fn short_id(value: &str) -> &str {
    if value.len() > 8 { &value[..8] } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_binding_input_prefers_peer() {
        let (binding_type, target_id) =
            route_binding_input(Some("peer-1".to_string()), None, false).expect("peer input");
        assert_eq!(binding_type, "peer");
        assert_eq!(target_id, "peer-1");
    }

    #[test]
    fn route_binding_input_preserves_group_legacy_type() {
        let (binding_type, target_id) =
            route_binding_input(None, Some("chat-1".to_string()), false).expect("group input");
        assert_eq!(binding_type, "group");
        assert_eq!(target_id, "chat-1");
    }

    #[test]
    fn route_binding_input_supports_default() {
        let (binding_type, target_id) =
            route_binding_input(None, None, true).expect("default input");
        assert_eq!(binding_type, "default");
        assert_eq!(target_id, "*");
    }
}
