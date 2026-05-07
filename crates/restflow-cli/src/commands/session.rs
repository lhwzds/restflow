use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde_json::json;
use std::sync::Arc;

use crate::cli::SessionCommands;
use crate::commands::utils::{format_timestamp, short_id};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::chat_session::ChatRole;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: SessionCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        SessionCommands::List => list_sessions(executor, format).await,
        SessionCommands::Show { id } => show_session(executor, &id, format).await,
        SessionCommands::Create { agent, model } => {
            create_session(executor, &agent, &model, format).await
        }
        SessionCommands::Delete { id } => delete_session(executor, &id, format).await,
        SessionCommands::Search {
            query,
            agent,
            limit,
        } => search_sessions(executor, &query, agent.as_deref(), limit, format).await,
    }
}

async fn list_sessions(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let sessions = executor.list_sessions().await?;

    if format.is_json() {
        return print_json(&sessions);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Agent", "Model", "Messages", "Updated"]);

    for session in sessions {
        table.add_row(vec![
            Cell::new(short_id(&session.id)),
            Cell::new(session.name),
            Cell::new(session.agent_id),
            Cell::new(session.model),
            Cell::new(session.message_count),
            Cell::new(format_timestamp(Some(session.updated_at))),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_session(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved_id = resolve_session_id(executor.clone(), id).await?;
    let session = executor.get_session(&resolved_id).await?;

    if format.is_json() {
        return print_json(&session);
    }

    println!("Session: {} ({})", session.name, session.id);
    println!("Agent: {}", session.agent_id);
    println!("Model: {}", session.model);
    println!("Messages: {}", session.messages.len());
    println!("Updated: {}", format_timestamp(Some(session.updated_at)));
    println!();

    for msg in &session.messages {
        let role = match msg.role {
            ChatRole::User => "User",
            ChatRole::Assistant => "Assistant",
            ChatRole::System => "System",
        };

        println!("{}", role);
        println!("{}", msg.content);
        println!();
    }

    Ok(())
}

async fn create_session(
    executor: Arc<dyn CommandExecutor>,
    agent: &str,
    model: &str,
    format: OutputFormat,
) -> Result<()> {
    let session = executor
        .create_session(
            Some(agent.to_string()),
            Some(model.to_string()),
            Some("New Chat".to_string()),
            None,
        )
        .await?;

    if format.is_json() {
        return print_json(&session);
    }

    println!("Created session: {}", session.id);
    Ok(())
}

async fn delete_session(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved = match resolve_session_id_optional(executor.clone(), id).await? {
        Some(id) => id,
        None => {
            if format.is_json() {
                return print_json(&json!({ "deleted": false, "id": id }));
            }
            println!("Session not found: {}", id);
            return Ok(());
        }
    };

    let deleted = executor.delete_session(&resolved).await?;

    if format.is_json() {
        return print_json(&json!({
            "deleted": deleted,
            "id": resolved,
        }));
    }

    if deleted {
        println!("Deleted session: {}", resolved);
    } else {
        println!("Session not found: {}", resolved);
    }

    Ok(())
}

async fn search_sessions(
    executor: Arc<dyn CommandExecutor>,
    query: &str,
    agent: Option<&str>,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        bail!("Search query cannot be empty");
    }

    let results = executor
        .search_sessions(&normalized, agent, limit.max(1))
        .await?;

    if format.is_json() {
        return print_json(&results);
    }

    if results.is_empty() {
        println!("No sessions matched: {}", query);
        return Ok(());
    }

    for (index, result) in results.iter().enumerate() {
        println!("{}. {} ({})", index + 1, result.name, result.id);
        println!("   Agent: {}", result.agent_id);
        println!("   Model: {}", result.model);
        println!("   Messages: {}", result.message_count);
        println!("   Updated: {}", format_timestamp(Some(result.updated_at)));
        if let Some(ref preview) = result.last_message_preview {
            println!("   Preview: {}", preview);
        }
        println!();
    }

    Ok(())
}

async fn resolve_session_id_optional(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
) -> Result<Option<String>> {
    let sessions = executor.list_sessions().await?;
    if sessions.iter().any(|session| session.id == id) {
        return Ok(Some(id.to_string()));
    }
    let mut matches: Vec<_> = sessions
        .iter()
        .filter(|session| session.id.starts_with(id))
        .collect();

    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0).id.clone())),
        _ => bail!("Session id is ambiguous: {}", id),
    }
}

async fn resolve_session_id(executor: Arc<dyn CommandExecutor>, id: &str) -> Result<String> {
    resolve_session_id_optional(executor, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
}
