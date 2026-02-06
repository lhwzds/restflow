use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use std::sync::Arc;
use tokio::time::{Duration, sleep};

use crate::cli::TaskCommands;
use crate::commands::utils::{format_timestamp, preview_text};
use crate::executor::{CommandExecutor, CreateTaskInput};
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::{AgentTaskStatus, TaskEvent, TaskEventType, TaskSchedule};
use serde_json::json;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: TaskCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        TaskCommands::List { status } => list_tasks(executor, status, format).await,
        TaskCommands::Show { id } => show_task(executor, &id, format).await,
        TaskCommands::Create {
            agent,
            name,
            input,
            cron,
        } => create_task(executor, &agent, &name, input, cron, format).await,
        TaskCommands::Pause { id } => pause_task(executor, &id, format).await,
        TaskCommands::Resume { id } => resume_task(executor, &id, format).await,
        TaskCommands::Cancel { id } => cancel_task(executor, &id, format).await,
        TaskCommands::Watch { id } => watch_task(executor, &id, format).await,
        TaskCommands::Run { id } => run_task(executor, &id, format).await,
    }
}

async fn list_tasks(
    executor: Arc<dyn CommandExecutor>,
    status: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let tasks = if let Some(value) = status {
        let status = parse_task_status(&value)?;
        executor.list_tasks_by_status(status).await?
    } else {
        executor.list_tasks().await?
    };

    if format.is_json() {
        return print_json(&tasks);
    }

    let mut table = Table::new();
    table.set_header(vec![
        "ID", "Name", "Agent", "Status", "Last Run", "Next Run",
    ]);

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

async fn show_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let task = executor
        .get_task(id)
        .await?
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
    executor: Arc<dyn CommandExecutor>,
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

    let task = executor
        .create_task(CreateTaskInput {
            name: name.to_string(),
            agent_id: agent_id.to_string(),
            schedule,
            input,
        })
        .await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Task created: {} ({})", task.name, task.id);
    Ok(())
}

async fn pause_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let task = executor.pause_task(id).await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Task paused: {} ({})", task.name, task.id);
    Ok(())
}

async fn resume_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let task = executor.resume_task(id).await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Task resumed: {} ({})", task.name, task.id);
    Ok(())
}

async fn cancel_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let deleted = executor.delete_task(id).await?;

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

async fn watch_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    _format: OutputFormat,
) -> Result<()> {
    let task = executor
        .get_task(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

    println!("Watching task: {} ({})", task.name, task.id);
    println!("Agent: {}", task.agent_id);
    println!("Status: {}", task_status_label(&task.status));
    println!("Press Ctrl+C to stop watching.\n");

    let mut events = executor.get_task_history(id).await?;

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
                events = executor.get_task_history(id).await?;

                let mut new_events = Vec::new();
                if let Some(ref last_id) = last_seen_id {
                    for event in &events {
                        if &event.id == last_id {
                            break;
                        }
                        new_events.push(event.clone());
                    }
                } else {
                    new_events = events.clone();
                }

                for event in new_events.iter().rev() {
                    print_task_event(event);
                }

                last_seen_id = events.first().map(|event| event.id.clone());

                // Check if task completed or failed
                if let Some(task) = executor.get_task(id).await?
                    && matches!(task.status, AgentTaskStatus::Completed | AgentTaskStatus::Failed)
                {
                    return Ok(());
                }
            }
        }
    }
}

async fn run_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    if format.is_json() {
        bail!("JSON output is not supported for task run yet");
    }

    let task = executor
        .get_task(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Task not found: {}", id))?;

    if task
        .input
        .as_ref()
        .is_none_or(|input| input.trim().is_empty())
    {
        bail!("Task input is required to run");
    }

    // Run task requires direct core access for inline execution
    let core = executor.core().ok_or_else(|| {
        anyhow::anyhow!("Cannot run tasks inline when daemon is running. Stop the daemon first.")
    })?;

    if matches!(
        restflow_core::daemon::check_daemon_status()?,
        restflow_core::daemon::DaemonStatus::Running { .. }
    ) {
        bail!("Daemon is running. Stop it before running tasks inline.");
    }

    let mut runner = crate::daemon::CliTaskRunner::new(core.clone());
    runner.start().await?;
    runner.run_task_now(id).await?;

    let watch_result = watch_task(executor, id, format).await;
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
            format!("every {} ms, starting at {}", interval_ms, start_label)
        }
        TaskSchedule::Cron {
            expression,
            timezone,
        } => {
            let tz_label = timezone.as_deref().unwrap_or("UTC");
            format!("cron: {} ({})", expression, tz_label)
        }
    }
}

fn print_task_event(event: &TaskEvent) {
    let timestamp = format_timestamp(Some(event.timestamp));
    let event_type = match event.event_type {
        TaskEventType::Created => "created",
        TaskEventType::Started => "started",
        TaskEventType::Completed => "completed",
        TaskEventType::Failed => "failed",
        TaskEventType::Paused => "paused",
        TaskEventType::Resumed => "resumed",
        TaskEventType::NotificationSent => "notification_sent",
        TaskEventType::NotificationFailed => "notification_failed",
        TaskEventType::Compaction => "compaction",
    };

    print!("[{}] {}", timestamp, event_type);
    if let Some(ref message) = event.message {
        print!(": {}", preview_text(message, 80));
    }
    println!();
}
