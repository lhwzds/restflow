use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

use crate::cli::SessionCommands;
use crate::commands::utils::{format_timestamp, preview_text};
use crate::output::{OutputFormat, json::print_json};
use restflow_core::AppCore;
use restflow_core::models::chat_session::{ChatRole, ChatSession};

#[derive(Debug, Serialize)]
struct SessionSearchResult {
    id: String,
    name: String,
    agent_id: String,
    model: String,
    updated_at: i64,
    match_count: usize,
    preview: Option<String>,
}

pub async fn run(core: Arc<AppCore>, command: SessionCommands, format: OutputFormat) -> Result<()> {
    match command {
        SessionCommands::List => list_sessions(&core, format).await,
        SessionCommands::Show { id } => show_session(&core, &id, format).await,
        SessionCommands::Create { agent, model } => {
            create_session(&core, &agent, &model, format).await
        }
        SessionCommands::Delete { id } => delete_session(&core, &id, format).await,
        SessionCommands::Search { query, agent } => {
            search_sessions(&core, &query, agent.as_deref(), format).await
        }
    }
}

async fn list_sessions(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let sessions = core.storage.chat_sessions.list_summaries()?;

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

async fn show_session(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let session = resolve_session(core, id)?;

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
    core: &Arc<AppCore>,
    agent: &str,
    model: &str,
    format: OutputFormat,
) -> Result<()> {
    let session = ChatSession::new(agent.to_string(), model.to_string());
    core.storage.chat_sessions.create(&session)?;

    if format.is_json() {
        return print_json(&session);
    }

    println!("Created session: {}", session.id);
    Ok(())
}

async fn delete_session(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let resolved = match resolve_session_id_optional(core, id)? {
        Some(id) => id,
        None => {
            if format.is_json() {
                return print_json(&json!({ "deleted": false, "id": id }));
            }
            println!("Session not found: {}", id);
            return Ok(());
        }
    };

    let deleted = core.storage.chat_sessions.delete(&resolved)?;

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
    core: &Arc<AppCore>,
    query: &str,
    agent: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        bail!("Search query cannot be empty");
    }

    let sessions = if let Some(agent_id) = agent {
        core.storage.chat_sessions.list_by_agent(agent_id)?
    } else {
        core.storage.chat_sessions.list()?
    };

    let mut results = Vec::new();

    for session in sessions {
        let (match_count, preview) = count_matches(&session, &normalized);
        if match_count > 0 {
            results.push(SessionSearchResult {
                id: session.id,
                name: session.name,
                agent_id: session.agent_id,
                model: session.model,
                updated_at: session.updated_at,
                match_count,
                preview,
            });
        }
    }

    results.sort_by(|a, b| b.match_count.cmp(&a.match_count));

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
        println!("   Matches: {}", result.match_count);
        println!("   Updated: {}", format_timestamp(Some(result.updated_at)));
        if let Some(ref preview) = result.preview {
            println!("   Preview: {}", preview);
        }
        println!();
    }

    Ok(())
}

fn count_matches(session: &ChatSession, query: &str) -> (usize, Option<String>) {
    let mut count = 0;
    let mut preview = None;

    if session.name.to_lowercase().contains(query) {
        count += 1;
        preview = Some(preview_text(&session.name, 80));
    }

    for message in &session.messages {
        let content = message.content.to_lowercase();
        if content.contains(query) {
            count += 1;
            if preview.is_none() {
                preview = Some(preview_text(&message.content, 120));
            }
        }
    }

    (count, preview)
}

fn resolve_session(core: &Arc<AppCore>, id: &str) -> Result<ChatSession> {
    let resolved = resolve_session_id(core, id)?;
    core.storage
        .chat_sessions
        .get(&resolved)?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
}

fn resolve_session_id_optional(core: &Arc<AppCore>, id: &str) -> Result<Option<String>> {
    if let Some(session) = core.storage.chat_sessions.get(id)? {
        return Ok(Some(session.id));
    }

    let sessions = core.storage.chat_sessions.list()?;
    let mut matches = sessions
        .iter()
        .filter(|session| session.id.starts_with(id))
        .collect::<Vec<_>>();

    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0).id.clone())),
        _ => bail!("Session id is ambiguous: {}", id),
    }
}

fn resolve_session_id(core: &Arc<AppCore>, id: &str) -> Result<String> {
    resolve_session_id_optional(core, id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}
