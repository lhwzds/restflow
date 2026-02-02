use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use std::sync::Arc;
use std::time::Instant;

use crate::cli::AgentCommands;
use crate::commands::utils::{format_timestamp, parse_model, read_stdin_to_string};
use crate::output::{OutputFormat, json::print_json};
use restflow_ai::{
    AgentConfig, AgentExecutor, AgentState, AgentStatus, AnthropicClient, LlmClient, OpenAIClient,
    Role, ToolRegistry,
};
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager, AuthProvider};
use restflow_core::storage::SecretStorage;
use redb::Database;
use restflow_core::memory::{ChatSessionMirror, MessageMirror};
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, ApiKeyConfig, ExecutionDetails, ExecutionStep, Provider,
    ToolCallInfo,
};
use restflow_core::paths;
use restflow_core::services::tool_registry::create_tool_registry;
use restflow_core::{AppCore, services::agent as agent_service};
use serde_json::json;
use tracing::warn;

pub async fn run(core: Arc<AppCore>, command: AgentCommands, format: OutputFormat) -> Result<()> {
    match command {
        AgentCommands::List => list_agents(&core, format).await,
        AgentCommands::Show { id } => show_agent(&core, &id, format).await,
        AgentCommands::Create {
            name,
            model,
            prompt,
        } => create_agent(&core, &name, model, prompt, format).await,
        AgentCommands::Update { id, name, model } => {
            update_agent(&core, &id, name, model, format).await
        }
        AgentCommands::Delete { id } => delete_agent(&core, &id, format).await,
        AgentCommands::Exec { id, input, session } => {
            exec_agent(&core, &id, input, session, format).await
        }
    }
}

async fn list_agents(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let agents = agent_service::list_agents(core).await?;

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

async fn show_agent(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let agent = agent_service::get_agent(core, id).await?;

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
    core: &Arc<AppCore>,
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

    let created = agent_service::create_agent(core, name.to_string(), agent_node).await?;

    if format.is_json() {
        return print_json(&created);
    }

    println!("Agent created: {} ({})", created.name, created.id);
    Ok(())
}

async fn update_agent(
    core: &Arc<AppCore>,
    id: &str,
    name: Option<String>,
    model: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut existing = agent_service::get_agent(core, id).await?;

    if let Some(model) = model {
        existing.agent.model = Some(parse_model(&model)?);
    }

    let updated = agent_service::update_agent(core, id, name, Some(existing.agent)).await?;

    if format.is_json() {
        return print_json(&updated);
    }

    println!("Agent updated: {} ({})", updated.name, updated.id);
    Ok(())
}

async fn delete_agent(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    agent_service::delete_agent(core, id).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "id": id }));
    }

    println!("Agent deleted: {id}");
    Ok(())
}

async fn exec_agent(
    core: &Arc<AppCore>,
    id: &str,
    input: Option<String>,
    session: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let agent = agent_service::get_agent(core, id).await?;
    let input = match input {
        Some(value) => value,
        None => read_stdin_to_string()?,
    };

    if input.is_empty() {
        bail!("Input is required to execute an agent");
    }

    let started = Instant::now();
    let response = run_agent_with_executor(
        &agent.agent,
        &input,
        Some(&core.storage.secrets),
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.shared_space.clone(),
    )
    .await?;
    let duration_ms = started.elapsed().as_millis() as i64;

    if let Some(ref session_id) = session {
        let mirror = ChatSessionMirror::new(Arc::new(core.storage.chat_sessions.clone()));

        if let Err(e) = mirror.mirror_user(session_id, &input).await {
            warn!(error = %e, "Failed to mirror user message");
        }

        let tokens = response
            .execution_details
            .as_ref()
            .map(|details| details.total_tokens);

        if let Err(e) = mirror
            .mirror_assistant(session_id, &response.response, tokens)
            .await
        {
            warn!(error = %e, "Failed to mirror assistant message");
        }
    }

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

fn convert_to_execution_steps(state: &AgentState) -> Vec<ExecutionStep> {
    state
        .messages
        .iter()
        .map(|msg| {
            let step_type = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => {
                    if msg.tool_calls.is_some() {
                        "tool_call"
                    } else {
                        "assistant"
                    }
                }
                Role::Tool => "tool_result",
            };

            let tool_calls = msg.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|tc| ToolCallInfo {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    })
                    .collect()
            });

            ExecutionStep {
                step_type: step_type.to_string(),
                content: msg.content.clone(),
                tool_calls,
            }
        })
        .collect()
}

fn status_to_string(status: &AgentStatus) -> String {
    match status {
        AgentStatus::Running => "running".to_string(),
        AgentStatus::Completed => "completed".to_string(),
        AgentStatus::Failed { error } => format!("failed: {}", error),
        AgentStatus::MaxIterations => "max_iterations".to_string(),
    }
}

async fn resolve_api_key(
    agent_node: &AgentNode,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    provider: Provider,
) -> Result<String> {
    if let Some(config) = &agent_node.api_key_config {
        match config {
            ApiKeyConfig::Direct(key) => {
                if !key.is_empty() {
                    return Ok(key.clone());
                }
            }
            ApiKeyConfig::Secret(secret_name) => {
                if let Some(storage) = secret_storage {
                    return storage
                        .get_secret(secret_name)?
                        .ok_or_else(|| anyhow::anyhow!("Secret '{}' not found", secret_name));
                }
                bail!("Secret storage not available");
            }
        }
    }

    if let Some(key) = resolve_api_key_from_profiles(provider).await? {
        return Ok(key);
    }

    bail!("No API key configured");
}

async fn resolve_api_key_from_profiles(provider: Provider) -> Result<Option<String>> {
    let mut config = AuthManagerConfig::default();
    let data_dir = paths::ensure_data_dir()?;
    let profiles_path = data_dir.join("auth_profiles.json");
    config.profiles_path = Some(profiles_path);

    // Create SecretStorage
    let db_path = data_dir.join("restflow.db");
    let db = Arc::new(Database::create(&db_path)?);
    let secrets = Arc::new(SecretStorage::new(db)?);

    let manager = AuthProfileManager::with_config(config, secrets);
    manager.initialize().await?;

    let selection = match provider {
        Provider::Anthropic => {
            if let Some(selection) = manager.select_profile(AuthProvider::Anthropic).await {
                Some(selection)
            } else {
                manager.select_profile(AuthProvider::ClaudeCode).await
            }
        }
        Provider::OpenAI => manager.select_profile(AuthProvider::OpenAI).await,
        Provider::DeepSeek => None,
    };

    match selection {
        Some(sel) => Ok(Some(sel.profile.get_api_key(manager.resolver())?)),
        None => Ok(None),
    }
}

async fn run_agent_with_executor(
    agent_node: &AgentNode,
    input: &str,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    skill_storage: restflow_core::storage::skill::SkillStorage,
    memory_storage: restflow_core::storage::memory::MemoryStorage,
    chat_storage: restflow_core::storage::chat_session::ChatSessionStorage,
    shared_space_storage: restflow_core::storage::SharedSpaceStorage,
) -> Result<AgentExecuteResponse> {
    // Get model (required for execution)
    let model = agent_node.require_model().map_err(|e| anyhow::anyhow!(e))?;

    let api_key = resolve_api_key(agent_node, secret_storage, model.provider()).await?;

    let llm: Arc<dyn LlmClient> = match model.provider() {
        Provider::OpenAI => Arc::new(OpenAIClient::new(&api_key).with_model(model.as_str())),
        Provider::Anthropic => Arc::new(AnthropicClient::new(&api_key).with_model(model.as_str())),
        Provider::DeepSeek => Arc::new(
            OpenAIClient::new(&api_key)
                .with_model(model.as_str())
                .with_base_url("https://api.deepseek.com/v1"),
        ),
    };

    let full_registry = create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
        shared_space_storage,
        None,
    );

    let tools = if let Some(ref tool_names) = agent_node.tools {
        if tool_names.is_empty() {
            Arc::new(ToolRegistry::new())
        } else {
            let mut filtered_registry = ToolRegistry::new();
            for name in tool_names {
                if let Some(tool) = full_registry.get(name) {
                    filtered_registry.register_arc(tool);
                } else {
                    warn!(tool_name = %name, "Configured tool not found in registry, skipping");
                }
            }
            Arc::new(filtered_registry)
        }
    } else {
        Arc::new(ToolRegistry::new())
    };

    let mut config = AgentConfig::new(input);

    if let Some(ref prompt) = agent_node.prompt {
        config = config.with_system_prompt(prompt);
    }

    if model.supports_temperature()
        && let Some(temp) = agent_node.temperature
    {
        config = config.with_temperature(temp as f32);
    }

    let executor = AgentExecutor::new(llm, tools);
    let result = executor.run(config).await?;

    let response = result.answer.unwrap_or_else(|| {
        if let Some(ref err) = result.error {
            format!("Error: {}", err)
        } else {
            "No response generated".to_string()
        }
    });

    let execution_details = ExecutionDetails {
        iterations: result.iterations,
        total_tokens: result.total_tokens,
        steps: convert_to_execution_steps(&result.state),
        status: status_to_string(&result.state.status),
    };

    Ok(AgentExecuteResponse {
        response,
        execution_details: Some(execution_details),
    })
}

#[allow(dead_code)]
pub async fn execute_agent_for_task(
    core: &Arc<AppCore>,
    agent_id: &str,
    input: &str,
) -> Result<AgentExecuteResponse> {
    let agent = agent_service::get_agent(core, agent_id).await?;

    run_agent_with_executor(
        &agent.agent,
        input,
        Some(&core.storage.secrets),
        core.storage.skills.clone(),
        core.storage.memory.clone(),
        core.storage.chat_sessions.clone(),
        core.storage.shared_space.clone(),
    )
    .await
}
