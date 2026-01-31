use anyhow::{bail, Result};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::MemoryCommands;
use crate::commands::utils::{format_timestamp, preview_text};
use crate::output::{json::print_json, OutputFormat};
use restflow_core::memory::MemoryExporter;
use restflow_core::models::memory::{MemoryChunk, MemorySearchQuery};
use restflow_core::services::agent as agent_service;
use restflow_core::AppCore;
use serde_json::json;

pub async fn run(core: Arc<AppCore>, command: MemoryCommands, format: OutputFormat) -> Result<()> {
    match command {
        MemoryCommands::Search { query } => search_memory(&core, &query, format).await,
        MemoryCommands::List { agent, tag } => list_memory(&core, agent, tag, format).await,
        MemoryCommands::Export { agent, output } => export_memory(&core, agent, output, format).await,
        MemoryCommands::Stats => memory_stats(&core, format).await,
        MemoryCommands::Clear { agent } => clear_memory(&core, agent, format).await,
    }
}

async fn search_memory(core: &Arc<AppCore>, query: &str, format: OutputFormat) -> Result<()> {
    let agent_id = resolve_agent_id(core, None).await?;
    let search = MemorySearchQuery::new(agent_id).with_query(query.to_string());
    let results = core.storage.memory.search(&search)?;

    if format.is_json() {
        return print_json(&results);
    }

    for (index, chunk) in results.chunks.iter().enumerate() {
        print_chunk_summary(index + 1, chunk);
    }

    Ok(())
}

async fn list_memory(
    core: &Arc<AppCore>,
    agent: Option<String>,
    tag: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let chunks = match (agent, tag) {
        (Some(agent_id), Some(tag)) => core
            .storage
            .memory
            .list_chunks(&agent_id)?
            .into_iter()
            .filter(|chunk| chunk.tags.iter().any(|value| value == &tag))
            .collect(),
        (Some(agent_id), None) => core.storage.memory.list_chunks(&agent_id)?,
        (None, Some(tag)) => core.storage.memory.list_chunks_by_tag(&tag)?,
        (None, None) => {
            let agent_id = resolve_agent_id(core, None).await?;
            core.storage.memory.list_chunks(&agent_id)?
        }
    };

    if format.is_json() {
        return print_json(&chunks);
    }

    render_chunks_table(&chunks)
}

async fn export_memory(
    core: &Arc<AppCore>,
    agent: Option<String>,
    output: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let agent_id = resolve_agent_id(core, agent).await?;
    let exporter = MemoryExporter::new(core.storage.memory.clone());
    let result = exporter.export_agent(&agent_id)?;

    let output_path = output.unwrap_or_else(|| result.suggested_filename.clone());
    std::fs::write(&output_path, &result.markdown)?;

    if format.is_json() {
        return print_json(&json!({
            "agent_id": agent_id,
            "output": output_path,
            "chunk_count": result.chunk_count,
            "session_count": result.session_count
        }));
    }

    println!("Exported to: {}", output_path);
    Ok(())
}

async fn memory_stats(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let agents = agent_service::list_agents(core).await?;
    if agents.is_empty() {
        bail!("No agents available");
    }

    let mut stats = Vec::new();
    for agent in agents {
        let stat = core.storage.memory.get_stats(&agent.id)?;
        stats.push(stat);
    }

    if format.is_json() {
        return print_json(&stats);
    }

    for stat in stats {
        println!("Agent: {}", stat.agent_id);
        println!("  Sessions: {}", stat.session_count);
        println!("  Chunks:   {}", stat.chunk_count);
        println!("  Tokens:   {}", stat.total_tokens);
        println!("  Oldest:   {}", format_timestamp(stat.oldest_memory));
        println!("  Newest:   {}", format_timestamp(stat.newest_memory));
        println!();
    }

    Ok(())
}

async fn clear_memory(core: &Arc<AppCore>, agent: Option<String>, format: OutputFormat) -> Result<()> {
    let agent_ids = if let Some(agent_id) = agent {
        vec![agent_id]
    } else {
        agent_service::list_agents(core)
            .await?
            .into_iter()
            .map(|agent| agent.id)
            .collect()
    };

    if agent_ids.is_empty() {
        bail!("No agents available");
    }

    let mut results = Vec::new();
    for agent_id in agent_ids {
        let deleted = core.storage.memory.delete_chunks_for_agent(&agent_id)?;
        results.push((agent_id, deleted));
    }

    if format.is_json() {
        let payload: Vec<_> = results
            .iter()
            .map(|(agent_id, deleted)| json!({ "agent_id": agent_id, "deleted": deleted }))
            .collect();
        return print_json(&payload);
    }

    for (agent_id, deleted) in results {
        println!("Cleared {} chunks for {}", deleted, agent_id);
    }

    Ok(())
}

async fn resolve_agent_id(core: &Arc<AppCore>, agent: Option<String>) -> Result<String> {
    if let Some(id) = agent {
        return Ok(id);
    }

    let agents = agent_service::list_agents(core).await?;
    if agents.is_empty() {
        bail!("No agents available");
    }

    Ok(agents[0].id.clone())
}

fn render_chunks_table(chunks: &[MemoryChunk]) -> Result<()> {
    let mut table = Table::new();
    table.set_header(vec!["ID", "Agent", "Created", "Tags", "Preview"]);

    for chunk in chunks {
        table.add_row(vec![
            Cell::new(chunk.id.clone()),
            Cell::new(chunk.agent_id.clone()),
            Cell::new(format_timestamp(Some(chunk.created_at))),
            Cell::new(chunk.tags.join(", ")),
            Cell::new(preview_text(&chunk.content, 60)),
        ]);
    }

    crate::output::table::print_table(table)
}

fn print_chunk_summary(index: usize, chunk: &MemoryChunk) {
    println!("{}. {}", index, chunk.id);
    println!("   Agent:   {}", chunk.agent_id);
    println!("   Created: {}", format_timestamp(Some(chunk.created_at)));
    if !chunk.tags.is_empty() {
        println!("   Tags:    {}", chunk.tags.join(", "));
    }
    println!("   {}", preview_text(&chunk.content, 120));
    println!();
}
