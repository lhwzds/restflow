use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::{AgentCommands, CodexExecutionModeArg};
use crate::commands::utils::{format_timestamp, short_id};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use types::AgentNode;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: AgentCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        AgentCommands::List => list_agents(executor, format).await,
        AgentCommands::Show { id } => show_agent(executor, &id, format).await,
        AgentCommands::Create {
            name,
            provider,
            model,
            prompt,
            codex_execution_mode,
            codex_reasoning_effort,
        } => {
            create_agent(
                executor,
                &name,
                provider,
                model,
                prompt,
                codex_execution_mode,
                codex_reasoning_effort,
                format,
            )
            .await
        }
        AgentCommands::Update {
            id,
            name,
            provider,
            model,
            codex_execution_mode,
            codex_reasoning_effort,
        } => {
            update_agent(
                executor,
                &id,
                name,
                provider,
                model,
                codex_execution_mode,
                codex_reasoning_effort,
                format,
            )
            .await
        }
        AgentCommands::Delete { id } => delete_agent(executor, &id, format).await,
    }
}

async fn list_agents(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let agents = executor.list_agents().await?;

    if format.is_json() {
        return print_json(&agents);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Provider", "Model", "Updated"]);

    for agent in agents {
        let model_ref = agent.agent.resolved_model_ref();
        let model_str = model_ref
            .map(|model_ref| model_ref.model.as_serialized_str())
            .unwrap_or("(not set)");
        let provider_str = model_ref
            .map(|model_ref| model_ref.provider.as_canonical_str())
            .unwrap_or("auto");
        table.add_row(vec![
            Cell::new(short_id(&agent.id)),
            Cell::new(agent.name),
            Cell::new(provider_str),
            Cell::new(model_str),
            Cell::new(format_timestamp(agent.updated_at)),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let agent = executor.get_agent(id).await?;

    if format.is_json() {
        return print_json(&agent);
    }

    println!("ID:          {}", agent.id);
    println!("Name:        {}", agent.name);
    if let Some(model_ref) = agent.agent.resolved_model_ref() {
        println!("Model:       {}", model_ref.model.as_serialized_str());
        println!("Provider:    {}", model_ref.provider.as_canonical_str());
    } else {
        println!("Model:       (not set - will auto-select based on configured credentials)");
    }
    println!("Created:     {}", format_timestamp(agent.created_at));
    println!("Updated:     {}", format_timestamp(agent.updated_at));
    println!("Tools:       {}", format_tools(&agent.agent.tools));
    if let Some(mode) = agent.agent.codex_cli_execution_mode {
        println!("Codex Mode:  {}", mode.as_str());
    }
    if let Some(effort) = &agent.agent.codex_cli_reasoning_effort {
        println!("Codex Effort: {}", effort);
    }

    if let Some(prompt) = agent.agent.prompt {
        println!("\nSystem Prompt:\n{prompt}");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_agent(
    executor: Arc<dyn CommandExecutor>,
    name: &str,
    provider: Option<String>,
    model: Option<String>,
    prompt: Option<String>,
    codex_execution_mode: Option<CodexExecutionModeArg>,
    codex_reasoning_effort: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    reject_agent_model_options(
        provider.as_deref(),
        model.as_deref(),
        codex_execution_mode.as_ref(),
        codex_reasoning_effort.as_deref(),
    )?;

    let mut agent_node = AgentNode::new();
    if let Some(prompt) = prompt {
        agent_node = agent_node.with_prompt(prompt);
    }

    let created = executor.create_agent(name.to_string(), agent_node).await?;

    if format.is_json() {
        return print_json(&created);
    }

    println!("Agent created: {} ({})", created.name, created.id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    codex_execution_mode: Option<CodexExecutionModeArg>,
    codex_reasoning_effort: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let existing = executor.get_agent(id).await?;

    reject_agent_model_options(
        provider.as_deref(),
        model.as_deref(),
        codex_execution_mode.as_ref(),
        codex_reasoning_effort.as_deref(),
    )?;

    let updated = executor
        .update_agent(id, name, Some(existing.agent))
        .await?;

    if format.is_json() {
        return print_json(&updated);
    }

    println!("Agent updated: {} ({})", updated.name, updated.id);
    Ok(())
}

async fn delete_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.delete_agent(id).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "deleted": true, "id": id }));
    }

    println!("Agent deleted: {id}");
    Ok(())
}

fn format_tools(tools: &Option<Vec<String>>) -> String {
    match tools {
        Some(tool_list) if !tool_list.is_empty() => tool_list.join(", "),
        _ => "-".to_string(),
    }
}

fn reject_agent_model_options(
    provider: Option<&str>,
    model: Option<&str>,
    codex_execution_mode: Option<&CodexExecutionModeArg>,
    codex_reasoning_effort: Option<&str>,
) -> Result<()> {
    if provider.is_none()
        && model.is_none()
        && codex_execution_mode.is_none()
        && codex_reasoning_effort.is_none()
    {
        return Ok(());
    }

    bail!(
        "Agent files no longer persist model or auth settings. Configure the runtime model separately instead of using --provider, --model, --codex-execution-mode, or --codex-reasoning-effort on agent commands."
    )
}
