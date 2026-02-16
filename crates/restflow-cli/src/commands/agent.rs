use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::{AgentCommands, CodexExecutionModeArg};
use crate::commands::utils::{format_timestamp, parse_model};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::{AgentNode, CodexCliExecutionMode};

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
            model,
            prompt,
            codex_execution_mode,
            codex_reasoning_effort,
        } => {
            create_agent(
                executor,
                &name,
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
            model,
            codex_execution_mode,
            codex_reasoning_effort,
        } => {
            update_agent(
                executor,
                &id,
                name,
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
    table.set_header(vec!["ID", "Name", "Model", "Updated"]);

    for agent in agents {
        let model_str = agent
            .agent
            .model
            .as_ref()
            .map(|m| m.as_serialized_str())
            .unwrap_or("(not set)");
        table.add_row(vec![
            Cell::new(short_id(&agent.id)),
            Cell::new(agent.name),
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
    if let Some(model) = &agent.agent.model {
        println!("Model:       {}", model.as_serialized_str());
        println!("Provider:    {:?}", model.provider());
    } else {
        println!("Model:       (not set - will auto-select based on auth profile)");
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

async fn create_agent(
    executor: Arc<dyn CommandExecutor>,
    name: &str,
    model: Option<String>,
    prompt: Option<String>,
    codex_execution_mode: Option<CodexExecutionModeArg>,
    codex_reasoning_effort: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut agent_node = match model {
        Some(value) => AgentNode::with_model(parse_model(&value)?),
        None => AgentNode::new(),
    };
    if let Some(prompt) = prompt {
        agent_node = agent_node.with_prompt(prompt);
    }
    if let Some(mode) = codex_execution_mode {
        agent_node = agent_node.with_codex_cli_execution_mode(to_codex_mode(mode));
    }
    if let Some(effort) = codex_reasoning_effort {
        agent_node = agent_node.with_codex_cli_reasoning_effort(effort);
    }

    let created = executor.create_agent(name.to_string(), agent_node).await?;

    if format.is_json() {
        return print_json(&created);
    }

    println!("Agent created: {} ({})", created.name, created.id);
    Ok(())
}

async fn update_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: Option<String>,
    model: Option<String>,
    codex_execution_mode: Option<CodexExecutionModeArg>,
    codex_reasoning_effort: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut existing = executor.get_agent(id).await?;

    if let Some(model) = model {
        existing.agent.model = Some(parse_model(&model)?);
    }
    if let Some(mode) = codex_execution_mode {
        existing.agent.codex_cli_execution_mode = Some(to_codex_mode(mode));
    }
    if let Some(effort) = codex_reasoning_effort {
        existing.agent.codex_cli_reasoning_effort = Some(effort);
    }

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

fn short_id(value: &str) -> String {
    value.chars().take(8).collect::<String>()
}

fn format_tools(tools: &Option<Vec<String>>) -> String {
    match tools {
        Some(tool_list) if !tool_list.is_empty() => tool_list.join(", "),
        _ => "-".to_string(),
    }
}

fn to_codex_mode(mode: CodexExecutionModeArg) -> CodexCliExecutionMode {
    match mode {
        CodexExecutionModeArg::Safe => CodexCliExecutionMode::Safe,
        CodexExecutionModeArg::Bypass => CodexCliExecutionMode::Bypass,
    }
}
