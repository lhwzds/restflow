use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use std::sync::Arc;
use std::time::Instant;

use crate::cli::AgentCommands;
use crate::commands::utils::{format_timestamp, parse_model, read_stdin_to_string};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::AgentNode;

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
        } => create_agent(executor, &name, model, prompt, format).await,
        AgentCommands::Update { id, name, model } => {
            update_agent(executor, &id, name, model, format).await
        }
        AgentCommands::Delete { id } => delete_agent(executor, &id, format).await,
        AgentCommands::Exec { id, input, session } => {
            exec_agent(executor, &id, input, session, format).await
        }
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
            .map(|m| m.as_str())
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
        println!("Model:       {}", model.as_str());
        println!("Provider:    {:?}", model.provider());
    } else {
        println!("Model:       (not set - will auto-select based on auth profile)");
    }
    println!("Created:     {}", format_timestamp(agent.created_at));
    println!("Updated:     {}", format_timestamp(agent.updated_at));
    println!("Tools:       {}", format_tools(&agent.agent.tools));

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
    format: OutputFormat,
) -> Result<()> {
    let mut agent_node = match model {
        Some(value) => AgentNode::with_model(parse_model(&value)?),
        None => AgentNode::new(),
    };
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

async fn update_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: Option<String>,
    model: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut existing = executor.get_agent(id).await?;

    if let Some(model) = model {
        existing.agent.model = Some(parse_model(&model)?);
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

async fn exec_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    input: Option<String>,
    session: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let input = match input {
        Some(value) => value,
        None => read_stdin_to_string()?,
    };

    if input.is_empty() {
        bail!("Input is required to execute an agent");
    }

    let started = Instant::now();
    let response = executor.execute_agent(id, input, session).await?;
    let duration_ms = started.elapsed().as_millis() as i64;

    if format.is_json() {
        return print_json(&response);
    }

    println!("{}", response.response);
    println!("\nDuration: {} ms", duration_ms);
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
