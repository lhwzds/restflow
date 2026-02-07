use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use std::sync::Arc;
use tokio::time::{Duration, sleep};

use crate::cli::{TaskCommands, TaskMessageCommands};
use crate::commands::utils::{format_timestamp, preview_text};
use crate::executor::{
    BackgroundProgressInput, CommandExecutor, ControlBackgroundAgentInput,
    CreateBackgroundAgentInput, ListBackgroundMessageInput, SendBackgroundMessageInput,
    UpdateBackgroundAgentInput,
};
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::{
    AgentTaskStatus, BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec,
    BackgroundMessageSource, MemoryConfig, MemoryScope, TaskEvent, TaskEventType, TaskSchedule,
};
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
            prompt,
            input_template,
            description,
            memory_scope,
            cron,
            timezone,
        } => {
            create_task(
                executor,
                &agent,
                &name,
                input,
                prompt,
                input_template,
                description,
                memory_scope,
                cron,
                timezone,
                format,
            )
            .await
        }
        TaskCommands::Update {
            id,
            name,
            agent,
            description,
            input,
            prompt,
            input_template,
            memory_scope,
            cron,
            timezone,
        } => {
            update_task(
                executor,
                &id,
                name,
                agent,
                description,
                input,
                prompt,
                input_template,
                memory_scope,
                cron,
                timezone,
                format,
            )
            .await
        }
        TaskCommands::Control { id, action } => control_task(executor, &id, &action, format).await,
        TaskCommands::Progress { id, event_limit } => {
            progress_task(executor, &id, event_limit, format).await
        }
        TaskCommands::Message { command } => run_message_command(executor, command, format).await,
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
    println!(
        "MemoryScope: {}",
        memory_scope_label(&task.memory.memory_scope)
    );
    if let Some(ref description) = task.description {
        println!("Description: {}", description);
    }
    if let Some(ref input) = task.input {
        println!("\nInput:\n{}", input);
    }
    if let Some(ref input_template) = task.input_template {
        println!("\nInput Template:\n{}", input_template);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_task(
    executor: Arc<dyn CommandExecutor>,
    agent_id: &str,
    name: &str,
    input: Option<String>,
    prompt: Option<String>,
    input_template: Option<String>,
    description: Option<String>,
    memory_scope: Option<String>,
    cron: Option<String>,
    timezone: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let schedule = build_schedule(cron, timezone)?;
    let input = resolve_task_input(input, prompt);
    let memory = parse_memory_scope(memory_scope.as_deref())?.map(|scope| MemoryConfig {
        memory_scope: scope,
        ..MemoryConfig::default()
    });

    let task = executor
        .create_background_agent(CreateBackgroundAgentInput {
            spec: BackgroundAgentSpec {
                name: name.to_string(),
                agent_id: agent_id.to_string(),
                description,
                input,
                input_template,
                schedule,
                notification: None,
                execution_mode: None,
                memory,
            },
        })
        .await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Background task created: {} ({})", task.name, task.id);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    name: Option<String>,
    agent: Option<String>,
    description: Option<String>,
    input: Option<String>,
    prompt: Option<String>,
    input_template: Option<String>,
    memory_scope: Option<String>,
    cron: Option<String>,
    timezone: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let schedule = build_schedule_patch(cron, timezone)?;
    let input = resolve_task_input(input, prompt);
    let memory = parse_memory_scope(memory_scope.as_deref())?.map(|scope| MemoryConfig {
        memory_scope: scope,
        ..MemoryConfig::default()
    });

    let task = executor
        .update_background_agent(UpdateBackgroundAgentInput {
            id: id.to_string(),
            patch: BackgroundAgentPatch {
                name,
                description,
                agent_id: agent,
                input,
                input_template,
                schedule,
                notification: None,
                execution_mode: None,
                memory,
            },
        })
        .await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("Background task updated: {} ({})", task.name, task.id);
    Ok(())
}

async fn control_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    action: &str,
    format: OutputFormat,
) -> Result<()> {
    let action = parse_control_action(action)?;
    let action_label = control_action_label(&action).to_string();

    let task = executor
        .control_background_agent(ControlBackgroundAgentInput {
            id: id.to_string(),
            action,
        })
        .await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!(
        "Background task {}: {} ({})",
        action_label, task.name, task.id
    );
    Ok(())
}

async fn progress_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    event_limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let progress = executor
        .get_background_progress(BackgroundProgressInput {
            id: id.to_string(),
            event_limit: Some(event_limit.max(1)),
        })
        .await?;

    if format.is_json() {
        return print_json(&progress);
    }

    println!("Task:       {}", progress.background_agent_id);
    println!("Status:     {}", task_status_label(&progress.status));
    println!(
        "Stage:      {}",
        progress.stage.as_deref().unwrap_or("unknown")
    );
    println!("Last Run:   {}", format_timestamp(progress.last_run_at));
    println!("Next Run:   {}", format_timestamp(progress.next_run_at));
    println!("Successes:  {}", progress.success_count);
    println!("Failures:   {}", progress.failure_count);
    println!("Tokens:     {}", progress.total_tokens_used);
    println!("Cost (USD): {:.6}", progress.total_cost_usd);
    println!("Queued Msg: {}", progress.pending_message_count);

    if !progress.recent_events.is_empty() {
        println!("\nRecent Events:");
        for event in &progress.recent_events {
            print_task_event(event);
        }
    }

    Ok(())
}

async fn run_message_command(
    executor: Arc<dyn CommandExecutor>,
    command: TaskMessageCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        TaskMessageCommands::Send {
            id,
            message,
            source,
        } => send_message(executor, &id, &message, &source, format).await,
        TaskMessageCommands::List { id, limit } => {
            list_messages(executor, &id, limit, format).await
        }
    }
}

async fn send_message(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    message: &str,
    source: &str,
    format: OutputFormat,
) -> Result<()> {
    let source = parse_message_source(source)?;

    let sent = executor
        .send_background_message(SendBackgroundMessageInput {
            id: id.to_string(),
            message: message.to_string(),
            source: Some(source),
        })
        .await?;

    if format.is_json() {
        return print_json(&sent);
    }

    println!("Message queued: {}", sent.id);
    Ok(())
}

async fn list_messages(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let messages = executor
        .list_background_messages(ListBackgroundMessageInput {
            id: id.to_string(),
            limit: Some(limit.max(1)),
        })
        .await?;

    if format.is_json() {
        return print_json(&messages);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Source", "Status", "Created", "Message"]);

    for item in messages {
        table.add_row(vec![
            Cell::new(short_id(&item.id)),
            Cell::new(message_source_label(&item.source)),
            Cell::new(message_status_label(&item.status)),
            Cell::new(format_timestamp(Some(item.created_at))),
            Cell::new(preview_text(&item.message, 80)),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn pause_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    control_task(executor, id, "pause", format).await
}

async fn resume_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    control_task(executor, id, "resume", format).await
}

async fn cancel_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let deleted = executor.delete_background_agent(id).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": deleted, "id": id }));
    }

    if deleted {
        println!("Background task deleted: {id}");
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
    control_task(executor, id, "run_now", format).await
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

fn parse_control_action(input: &str) -> Result<BackgroundAgentControlAction> {
    match input.trim().to_lowercase().as_str() {
        "start" => Ok(BackgroundAgentControlAction::Start),
        "pause" => Ok(BackgroundAgentControlAction::Pause),
        "resume" => Ok(BackgroundAgentControlAction::Resume),
        "stop" => Ok(BackgroundAgentControlAction::Stop),
        "run_now" | "run-now" | "runnow" | "run" => Ok(BackgroundAgentControlAction::RunNow),
        _ => bail!("Unknown control action: {input}"),
    }
}

fn parse_message_source(input: &str) -> Result<BackgroundMessageSource> {
    match input.trim().to_lowercase().as_str() {
        "user" => Ok(BackgroundMessageSource::User),
        "agent" => Ok(BackgroundMessageSource::Agent),
        "system" => Ok(BackgroundMessageSource::System),
        _ => bail!("Unknown message source: {input}"),
    }
}

fn parse_memory_scope(input: Option<&str>) -> Result<Option<MemoryScope>> {
    match input.map(|value| value.trim().to_lowercase()) {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if value == "shared_agent" => Ok(Some(MemoryScope::SharedAgent)),
        Some(value) if value == "per_task" => Ok(Some(MemoryScope::PerTask)),
        Some(value) => bail!("Unknown memory scope: {value}"),
    }
}

fn control_action_label(action: &BackgroundAgentControlAction) -> &'static str {
    match action {
        BackgroundAgentControlAction::Start => "started",
        BackgroundAgentControlAction::Pause => "paused",
        BackgroundAgentControlAction::Resume => "resumed",
        BackgroundAgentControlAction::Stop => "stopped",
        BackgroundAgentControlAction::RunNow => "scheduled",
    }
}

fn message_source_label(source: &BackgroundMessageSource) -> &'static str {
    match source {
        BackgroundMessageSource::User => "user",
        BackgroundMessageSource::Agent => "agent",
        BackgroundMessageSource::System => "system",
    }
}

fn message_status_label(status: &restflow_core::models::BackgroundMessageStatus) -> &'static str {
    match status {
        restflow_core::models::BackgroundMessageStatus::Queued => "queued",
        restflow_core::models::BackgroundMessageStatus::Delivered => "delivered",
        restflow_core::models::BackgroundMessageStatus::Consumed => "consumed",
        restflow_core::models::BackgroundMessageStatus::Failed => "failed",
    }
}

fn memory_scope_label(scope: &MemoryScope) -> &'static str {
    match scope {
        MemoryScope::SharedAgent => "shared_agent",
        MemoryScope::PerTask => "per_task",
    }
}

fn build_schedule(cron: Option<String>, timezone: Option<String>) -> Result<TaskSchedule> {
    match cron {
        Some(expression) => Ok(TaskSchedule::Cron {
            expression,
            timezone,
        }),
        None => {
            if timezone.is_some() {
                bail!("--timezone requires --cron");
            }
            Ok(TaskSchedule::default())
        }
    }
}

fn build_schedule_patch(
    cron: Option<String>,
    timezone: Option<String>,
) -> Result<Option<TaskSchedule>> {
    match (cron, timezone) {
        (None, None) => Ok(None),
        (Some(expression), timezone) => Ok(Some(TaskSchedule::Cron {
            expression,
            timezone,
        })),
        (None, Some(_)) => bail!("--timezone requires --cron when updating schedule"),
    }
}

fn resolve_task_input(input: Option<String>, prompt: Option<String>) -> Option<String> {
    prompt.or(input)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_task_input_prefers_prompt() {
        let input = resolve_task_input(
            Some("from-input".to_string()),
            Some("from-prompt".to_string()),
        );
        assert_eq!(input.as_deref(), Some("from-prompt"));
    }

    #[test]
    fn test_build_schedule_accepts_cron_with_timezone() {
        let schedule = build_schedule(
            Some("0 9 * * *".to_string()),
            Some("Asia/Shanghai".to_string()),
        )
        .expect("schedule should be built");
        match schedule {
            TaskSchedule::Cron {
                expression,
                timezone,
            } => {
                assert_eq!(expression, "0 9 * * *");
                assert_eq!(timezone.as_deref(), Some("Asia/Shanghai"));
            }
            _ => panic!("expected cron schedule"),
        }
    }

    #[test]
    fn test_build_schedule_patch_requires_cron_for_timezone() {
        let result = build_schedule_patch(None, Some("UTC".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_control_action_aliases() {
        assert!(matches!(
            parse_control_action("run-now").expect("action should parse"),
            BackgroundAgentControlAction::RunNow
        ));
    }

    #[test]
    fn test_parse_memory_scope_accepts_known_values() {
        assert!(matches!(
            parse_memory_scope(Some("shared_agent")).expect("scope should parse"),
            Some(MemoryScope::SharedAgent)
        ));
        assert!(matches!(
            parse_memory_scope(Some("per_task")).expect("scope should parse"),
            Some(MemoryScope::PerTask)
        ));
    }
}
