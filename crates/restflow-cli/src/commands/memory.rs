use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::MemoryCommands;
use crate::commands::utils::{format_timestamp, preview_text};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::memory::MemoryChunk;
use serde_json::json;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: MemoryCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        MemoryCommands::Search { query } => search_memory(executor, &query, format).await,
        MemoryCommands::List { agent, tag } => list_memory(executor, agent, tag, format).await,
        MemoryCommands::Export { agent, output } => {
            export_memory(executor, agent, output, format).await
        }
        MemoryCommands::Stats => memory_stats(executor, format).await,
        MemoryCommands::Clear { agent } => clear_memory(executor, agent, format).await,
    }
}

async fn search_memory(
    executor: Arc<dyn CommandExecutor>,
    query: &str,
    format: OutputFormat,
) -> Result<()> {
    let results = executor
        .search_memory(query.to_string(), None, None)
        .await?;

    if format.is_json() {
        return print_json(&results);
    }

    for (index, chunk) in results.chunks.iter().enumerate() {
        print_chunk_summary(index + 1, chunk);
    }

    Ok(())
}

async fn list_memory(
    executor: Arc<dyn CommandExecutor>,
    agent: Option<String>,
    tag: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let chunks = executor.list_memory(agent, tag).await?;

    if format.is_json() {
        return print_json(&chunks);
    }

    render_chunks_table(&chunks)
}

async fn export_memory(
    executor: Arc<dyn CommandExecutor>,
    agent: Option<String>,
    output: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let result = executor.export_memory(agent.clone()).await?;

    let output_path = output.unwrap_or_else(|| result.suggested_filename.clone());
    std::fs::write(&output_path, &result.markdown)?;

    if format.is_json() {
        return print_json(&json!({
            "agent_id": result.agent_id,
            "output": output_path,
            "chunk_count": result.chunk_count,
            "session_count": result.session_count
        }));
    }

    println!("Exported to: {}", output_path);
    Ok(())
}

async fn memory_stats(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let agents = executor.list_agents().await?;
    if agents.is_empty() {
        bail!("No agents available");
    }

    let mut stats = Vec::new();
    for agent in agents {
        let stat = executor.get_memory_stats(Some(agent.id)).await?;
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

async fn clear_memory(
    executor: Arc<dyn CommandExecutor>,
    agent: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let agent_ids = if let Some(agent_id) = agent {
        vec![agent_id]
    } else {
        executor
            .list_agents()
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
        let deleted = executor.clear_memory(Some(agent_id.clone())).await?;
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
