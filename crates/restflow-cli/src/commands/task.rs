use anyhow::{bail, Result};
use comfy_table::{Cell, Table};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use crate::cli::TaskCommands;
use crate::commands::utils::{format_timestamp, preview_text};
use crate::output::{json::print_json, OutputFormat};
use restflow_core::models::{AgentTaskStatus, TaskEvent, TaskEventType, TaskSchedule};
use restflow_core::AppCore;
use serde_json::json;

pub async fn run(core: Arc<AppCore>, command: TaskCommands, format: OutputFormat) -> Result<()> {
    match command {
        TaskCommands::List { status } => list_tasks(&core, status, format).await,
        TaskCommands::Show { id } => show_task(&core, &id, format).await,
        TaskCommands::Create {
            agent,
            name,
            input,
            cron,
        } => create_task(&core, &agent, &name, input, cron, format).await,
        TaskCommands::Pause { id } => pause_task(&core, &id, format).await,
        TaskCommands::Resume { id } => resume_task(&core, &id, format).await,
        TaskCommands::Cancel { id } => cancel_task(&core, &id, format).await,
        TaskCommands::Watch { id } => watch_task(&core, &id, format).await,
        TaskCommands::Run { id } => run_task(&core, &id, format).await,
    }
}

async fn list_tasks(core: &Arc<AppCore>, status: Option<String>, format: OutputFormat) -> Result<()> {
    let tasks = if let Some(value) = status {
        let status = parse_task_status(&value)?;
        core.storage.agent_tasks.list_tasks_by_status(status)?
    } else {
        core.storage.agent_tasks.list_tasks()?
    };

    if format.is_json() {
        return print_json(&tasks);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Agent", "Status", "Last Run", "Next Run"]);

    for task in tasks {
        table.add_row(vec![
            Cell::new(short_id(&task.id)),
            Cell::new(task.name),
            Cell::new(short_id(&task.agent_id)),
            Cell::new(task_status_label(&task.status)),
            Cell::new(format_timestamp(task.last_run_at)),
            Cell::new(format_timestamp(task.next_run_at)),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_task(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let task = core
        .storage
        .agent_tasks
        .get_task(id)?
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("ID:          {}", task.id);
    println!("Name:        {}", task.name);
    println!("Agent:       {}", task.agent_id);
    println!("Status:      {}", task_status_label(&task.status));
    println!("Schedule:    {}", format_schedule(&task.schedule));
    println!("Created:     {}", format_timestamp(Some(task.created_at)));
    println!("Updated:     {}", format_timestamp(Some(task.updated_at)));
    println!("Last Run:    {}", format_timestamp(task.last_run_at));
    println!("Next Run:    {}", format_timestamp(task.next_run_at));
    println!("Successes:   {}", task.success_count);
    println!("Failures:    {}", task.failure_count);
    if let Some(ref input) = task.input {
        println!("\nInput:\n{}", input);
    }

    Ok(())
}

async fn create_task(
    core: &Arc<AppCore>,
    agent_id: &str,
    name: &str,
    input: Option<String>,
    cron: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let schedule = match cron {
        Some(expression) => TaskSchedule::Cron {
            expression,
            timezone: None,
        },
        None => TaskSchedule::default(),
    };

    let mut task = core
        .storage
        .agent_tasks
        .create_task(name.to_string(), agent_id.to_string(), schedule)?;

    if let Some(text) = input {
        task.input = Some(text);
        core.storage.agent_tasks.update_task(&task)?;
    }

    if format.is_json() {
        return print_json(&task);
    }

    println!("Task created: {} ({})", task.name, task.id);
    Ok(())
}

async fn pause_task(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let task = core.storage.agent_tasks.pause_task(id)?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Task paused: {} ({})", task.name, task.id);
    Ok(())
}

async fn resume_task(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let task = core.storage.agent_tasks.resume_task(id)?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Task resumed: {} ({})", task.name, task.id);
    Ok(())
}

async fn cancel_task(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let deleted = core.storage.agent_tasks.delete_task(id)?;

    if format.is_json() {
        return print_json(&json!({ "deleted": deleted, "id": id }));
    }

    if deleted {
        println!("Task cancelled: {id}");
    } else {
        println!("Task not found: {id}");
    }
    Ok(())
}

async fn watch_task(core: &Arc<AppCore>, id: &str, _format: OutputFormat) -> Result<()> {
    let task = core
        .storage
        .agent_tasks
        .get_task(id)?
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

    println!("Watching task: {} ({})", task.name, task.id);
    println!("Agent: {}", task.agent_id);
    println!("Status: {}", task_status_label(&task.status));
    println!("Press Ctrl+C to stop watching.\n");

    let mut events = core.storage.agent_tasks.list_events_for_task(id)?;

    if events.is_empty() {
        println!("No events yet.");
    } else {
        for event in events.iter().rev() {
            print_task_event(event);
        }
    }

    let mut last_seen_id = events.first().map(|event| event.id.clone());

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nStopped watching");
                return Ok(());
            }
            _ = sleep(Duration::from_secs(1)) => {
                events = core.storage.agent_tasks.list_events_for_task(id)?;

                let mut new_events = Vec::new();
                if let Some(ref last_id) = last_seen_id {
                    if let Some(pos) = events.iter().position(|event| event.id == *last_id) {
                        if pos > 0 {
                            new_events.extend(events[..pos].iter().rev());
                        }
                    } else {
                        new_events.extend(events.iter().rev());
                    }
                } else {
                    new_events.extend(events.iter().rev());
                }

                for event in new_events {
                    print_task_event(event);
                    if matches!(event.event_type, TaskEventType::Completed | TaskEventType::Failed) {
                        return Ok(());
                    }
                }

                last_seen_id = events.first().map(|event| event.id.clone());

                if let Some(task) = core.storage.agent_tasks.get_task(id)?
                    && matches!(task.status, AgentTaskStatus::Completed | AgentTaskStatus::Failed)
                {
                    return Ok(());
                }
            }
        }
    }
}

async fn run_task(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    if format.is_json() {
        bail!("JSON output is not supported for task run yet");
    }

    let task = core
        .storage
        .agent_tasks
        .get_task(id)?
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

    if task
        .input
        .as_ref()
        .is_none_or(|input| input.trim().is_empty())
    {
        bail!("Task input is required to run");
    }

    if crate::daemon::is_daemon_running() {
        bail!("Daemon is running. Stop it before running tasks inline.");
    }

    let mut runner = crate::daemon::CliTaskRunner::new(core.clone());
    runner.start().await?;
    runner.run_task_now(id).await?;

    let watch_result = watch_task(core, id, format).await;
    runner.stop().await?;
    watch_result
}

fn parse_task_status(input: &str) -> Result<AgentTaskStatus> {
    match input.trim().to_lowercase().as_str() {
        "active" => Ok(AgentTaskStatus::Active),
        "paused" => Ok(AgentTaskStatus::Paused),
        "running" => Ok(AgentTaskStatus::Running),
        "completed" => Ok(AgentTaskStatus::Completed),
        "failed" => Ok(AgentTaskStatus::Failed),
        _ => bail!("Unknown status: {input}"),
    }
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect::<String>()
}

fn task_status_label(status: &AgentTaskStatus) -> String {
    match status {
        AgentTaskStatus::Active => "active".to_string(),
        AgentTaskStatus::Paused => "paused".to_string(),
        AgentTaskStatus::Running => "running".to_string(),
        AgentTaskStatus::Completed => "completed".to_string(),
        AgentTaskStatus::Failed => "failed".to_string(),
    }
}

fn format_schedule(schedule: &TaskSchedule) -> String {
    match schedule {
        TaskSchedule::Once { run_at } => format!("once at {}", format_timestamp(Some(*run_at))),
        TaskSchedule::Interval {
            interval_ms,
            start_at,
        } => {
            let start_label = start_at
                .map(|ts| format_timestamp(Some(ts)))
                .unwrap_or_else(|| "now".to_string());
            format!("every {} ms (start: {})", interval_ms, start_label)
        }
        TaskSchedule::Cron { expression, timezone } => {
            let tz = timezone.clone().unwrap_or_else(|| "local".to_string());
            format!("cron '{}' ({})", expression, tz)
        }
    }
}

fn print_task_event(event: &TaskEvent) {
    let icon = match event.event_type {
        TaskEventType::Created => "📝",
        TaskEventType::Started => "🚀",
        TaskEventType::Completed => "✅",
        TaskEventType::Failed => "❌",
        TaskEventType::Paused => "⏸️",
        TaskEventType::Resumed => "▶️",
        TaskEventType::NotificationSent => "📣",
        TaskEventType::NotificationFailed => "⚠️",
    };

    let time = format_timestamp(Some(event.timestamp));
    let message = event.message.as_deref().unwrap_or("");
    println!("{} [{}] {:?} {}", icon, time, event.event_type, message);

    if let Some(ref output) = event.output {
        println!("{}", preview_text(output, 400));
    }
}
