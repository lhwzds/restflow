use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::{BackgroundAgentCommands, OutputFormat};
use crate::executor::CommandExecutor;
use crate::output::json::print_json;
use crate::output::table::print_table;
use restflow_core::models::{
    BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec, TaskSchedule,
};

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: BackgroundAgentCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        BackgroundAgentCommands::List { status } => {
            list_background_agents(executor, status, format).await
        }
        BackgroundAgentCommands::Show { id } => show_background_agent(executor, &id, format).await,
        BackgroundAgentCommands::Create {
            name,
            agent,
            schedule,
            schedule_value,
            input,
            input_template,
            timeout,
            notify,
        } => {
            create_background_agent(
                executor,
                name,
                agent,
                schedule,
                schedule_value,
                input,
                input_template,
                timeout,
                notify,
                format,
            )
            .await
        }
        BackgroundAgentCommands::Update {
            id,
            name,
            input,
            schedule,
            schedule_value,
            timeout,
        } => {
            update_background_agent(
                executor,
                &id,
                name,
                input,
                schedule,
                schedule_value,
                timeout,
                format,
            )
            .await
        }
        BackgroundAgentCommands::Delete { id } => {
            delete_background_agent(executor, &id, format).await
        }
        BackgroundAgentCommands::Control { id, action } => {
            control_background_agent(executor, &id, &action, format).await
        }
        BackgroundAgentCommands::Progress { id, limit } => {
            show_progress(executor, &id, limit, format).await
        }
        BackgroundAgentCommands::Send { id, message } => {
            send_message(executor, &id, &message, format).await
        }
    }
}

async fn list_background_agents(
    executor: Arc<dyn CommandExecutor>,
    status: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let agents = executor.list_background_agents(status).await?;

    if format.is_json() {
        return print_json(&agents);
    }

    if agents.is_empty() {
        println!("No background agents found.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Agent", "Status", "Next Run"]);

    for agent in agents {
        let short_id = &agent.id[..8.min(agent.id.len())];
        let next_run = agent
            .next_run_at
            .map(|ts| format_timestamp(ts))
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(agent.name),
            Cell::new(&agent.agent_id[..8.min(agent.agent_id.len())]),
            Cell::new(format!("{:?}", agent.status).to_lowercase()),
            Cell::new(next_run),
        ]);
    }

    print_table(table)
}

async fn show_background_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let agent = executor.get_background_agent(id).await?;

    if format.is_json() {
        return print_json(&agent);
    }

    println!("ID:          {}", agent.id);
    println!("Name:        {}", agent.name);
    println!("Agent:       {}", agent.agent_id);
    println!("Status:      {:?}", agent.status);
    println!("Schedule:    {:?}", agent.schedule);
    if let Some(input) = &agent.input {
        println!("Input:       {}", truncate(input, 100));
    }
    if let Some(timeout) = agent.timeout_secs {
        println!("Timeout:     {}s", timeout);
    }
    println!("Created:     {}", format_timestamp(agent.created_at));
    println!("Updated:     {}", format_timestamp(agent.updated_at));
    if let Some(last_run) = agent.last_run_at {
        println!("Last Run:    {}", format_timestamp(last_run));
    }
    if let Some(next_run) = agent.next_run_at {
        println!("Next Run:    {}", format_timestamp(next_run));
    }
    println!("Success:     {}", agent.success_count);
    println!("Failed:      {}", agent.failure_count);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_background_agent(
    executor: Arc<dyn CommandExecutor>,
    name: String,
    agent_id: String,
    schedule_type: String,
    schedule_value: Option<String>,
    input: Option<String>,
    input_template: Option<String>,
    timeout_secs: Option<u64>,
    notify: bool,
    format: OutputFormat,
) -> Result<()> {
    let schedule = parse_schedule(&schedule_type, schedule_value)?;

    let notification = if notify {
        Some(restflow_core::models::NotificationConfig {
            notify_on_failure_only: false,
            include_output: true,
            broadcast_steps: false,
        })
    } else {
        None
    };

    let spec = BackgroundAgentSpec {
        name,
        agent_id,
        description: None,
        input,
        input_template,
        schedule,
        notification,
        execution_mode: None,
        timeout_secs,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: vec![],
        continuation: None,
    };

    let agent = executor.create_background_agent(spec).await?;

    if format.is_json() {
        return print_json(&agent);
    }

    println!(
        "Background agent created: {} ({})",
        agent.name,
        &agent.id[..8]
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_background_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: Option<String>,
    input: Option<String>,
    schedule_type: Option<String>,
    schedule_value: Option<String>,
    timeout_secs: Option<u64>,
    format: OutputFormat,
) -> Result<()> {
    let schedule = if let (Some(st), sv) = (schedule_type, schedule_value) {
        Some(parse_schedule(&st, sv)?)
    } else {
        None
    };

    let patch = BackgroundAgentPatch {
        name,
        description: None,
        agent_id: None,
        input,
        input_template: None,
        schedule,
        notification: None,
        execution_mode: None,
        timeout_secs,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: None,
        continuation: None,
    };

    let agent = executor.update_background_agent(id, patch).await?;

    if format.is_json() {
        return print_json(&agent);
    }

    println!("Background agent updated: {} ({})", agent.name, &agent.id[..8]);
    Ok(())
}

async fn delete_background_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.delete_background_agent(id).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({"deleted": true}));
    }

    println!("Background agent deleted: {}", id);
    Ok(())
}

async fn control_background_agent(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    action: &str,
    format: OutputFormat,
) -> Result<()> {
    let parsed_action = parse_control_action(action)?;
    executor.control_background_agent(id, parsed_action.clone()).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({"success": true}));
    }

    println!("Background agent {} action: {:?}", id, parsed_action);
    Ok(())
}

async fn show_progress(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    event_limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let progress = executor
        .get_background_agent_progress(id, Some(event_limit))
        .await?;

    if format.is_json() {
        return print_json(&progress);
    }

    println!("Background Agent: {}", id);
    println!("Status: {:?}", progress.status);
    if let Some(stage) = &progress.stage {
        println!("Stage: {}", stage);
    }
    println!("Events (last {}):", progress.recent_events.len());
    for event in progress.recent_events {
        let ts = format_timestamp(event.timestamp);
        println!("  [{}] {:?}: {}", ts, event.event_type, event.message.unwrap_or_default());
    }

    Ok(())
}

async fn send_message(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    message: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.send_background_agent_message(id, message).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({"sent": true}));
    }

    println!("Message sent to background agent: {}", id);
    Ok(())
}

fn parse_schedule(schedule_type: &str, schedule_value: Option<String>) -> Result<TaskSchedule> {
    let value = schedule_value.ok_or_else(|| {
        anyhow::anyhow!("Schedule value is required for schedule type: {}", schedule_type)
    })?;

    match schedule_type.to_lowercase().as_str() {
        "once" => {
            let run_at = value.parse::<i64>()?;
            Ok(TaskSchedule::Once { run_at })
        }
        "interval" => {
            let interval_ms = value.parse::<i64>()?;
            Ok(TaskSchedule::Interval {
                interval_ms,
                start_at: None,
            })
        }
        "cron" => Ok(TaskSchedule::Cron {
            expression: value,
            timezone: None,
        }),
        _ => anyhow::bail!(
            "Invalid schedule type: {}. Use: once, interval, cron",
            schedule_type
        ),
    }
}

fn parse_control_action(action: &str) -> Result<BackgroundAgentControlAction> {
    match action.to_lowercase().as_str() {
        "start" => Ok(BackgroundAgentControlAction::Start),
        "pause" => Ok(BackgroundAgentControlAction::Pause),
        "resume" => Ok(BackgroundAgentControlAction::Resume),
        "stop" => Ok(BackgroundAgentControlAction::Stop),
        "run_now" => Ok(BackgroundAgentControlAction::RunNow),
        _ => anyhow::bail!(
            "Invalid action: {}. Use: start, pause, resume, stop, run_now",
            action
        ),
    }
}

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ts)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}
