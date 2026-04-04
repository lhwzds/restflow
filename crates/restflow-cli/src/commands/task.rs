use anyhow::Result;
use comfy_table::{Cell, Table};
use restflow_contracts::{DeleteWithIdResponse, request::TaskFromSessionRequest};
use std::sync::Arc;

use crate::cli::{OutputFormat, TaskCommands};
use crate::commands::utils::format_timestamp;
use crate::executor::CommandExecutor;
use crate::output::json::print_json;
use crate::output::table::print_table;
#[cfg(test)]
use restflow_core::models::RunKind;
use restflow_core::models::{
    ExecutionContainerKind, ExecutionContainerRef, RunListQuery, RunSummary, TaskControlAction,
    TaskPatch, TaskSchedule, TaskSpec,
};
#[cfg(test)]
use restflow_core::services::background_agent_conversion::{
    derive_conversion_input, derive_conversion_name,
};

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: TaskCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        TaskCommands::List { status } => list_tasks(executor, status, format).await,
        TaskCommands::Show { id } => show_task(executor, &id, format).await,
        TaskCommands::Create {
            name,
            agent,
            schedule,
            schedule_value,
            input,
            input_template,
            timeout,
            notify,
        } => {
            create_task(
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
        TaskCommands::ConvertSession {
            session_id,
            name,
            input,
            schedule,
            schedule_value,
            timeout,
            run_now,
        } => {
            convert_session_to_task(
                executor,
                &session_id,
                name,
                input,
                schedule,
                schedule_value,
                timeout,
                run_now,
                format,
            )
            .await
        }
        TaskCommands::Update {
            id,
            name,
            input,
            schedule,
            schedule_value,
            timeout,
        } => {
            update_task(
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
        TaskCommands::Delete { id } => delete_task(executor, &id, format).await,
        TaskCommands::Control { id, action } => control_task(executor, &id, &action, format).await,
        TaskCommands::Progress { id, limit } => show_progress(executor, &id, limit, format).await,
        TaskCommands::RunLog { id, run_id, limit } => {
            show_run_log(executor, &id, run_id.as_deref(), limit, format).await
        }
        TaskCommands::Send { id, message } => send_message(executor, &id, &message, format).await,
    }
}

async fn list_tasks(
    executor: Arc<dyn CommandExecutor>,
    status: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let tasks = executor.list_tasks(status).await?;

    if format.is_json() {
        return print_json(&tasks);
    }

    if tasks.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Agent", "Status", "Next Run"]);

    for task in tasks {
        let short_id = &task.id[..8.min(task.id.len())];
        let next_run = task
            .next_run_at
            .map(|ts| format_timestamp(Some(ts)))
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(task.name),
            Cell::new(&task.agent_id[..8.min(task.agent_id.len())]),
            Cell::new(format!("{:?}", task.status).to_lowercase()),
            Cell::new(next_run),
        ]);
    }

    print_table(table)
}

async fn show_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let task = executor.get_task(id).await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!("ID:          {}", task.id);
    println!("Name:        {}", task.name);
    println!("Agent:       {}", task.agent_id);
    println!("Status:      {:?}", task.status);
    println!("Schedule:    {:?}", task.schedule);
    if let Some(input) = &task.input {
        println!("Input:       {}", truncate(input, 100));
    }
    if let Some(timeout) = task.timeout_secs {
        println!("Timeout:     {}s", timeout);
    }
    println!("Created:     {}", format_timestamp(Some(task.created_at)));
    println!("Updated:     {}", format_timestamp(Some(task.updated_at)));
    if let Some(last_run) = task.last_run_at {
        println!("Last Run:    {}", format_timestamp(Some(last_run)));
    }
    if let Some(next_run) = task.next_run_at {
        println!("Next Run:    {}", format_timestamp(Some(next_run)));
    }
    println!("Success:     {}", task.success_count);
    println!("Failed:      {}", task.failure_count);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn convert_session_to_task(
    executor: Arc<dyn CommandExecutor>,
    session_id: &str,
    name: Option<String>,
    input: Option<String>,
    schedule_type: Option<String>,
    schedule_value: Option<String>,
    timeout_secs: Option<u64>,
    run_now: bool,
    format: OutputFormat,
) -> Result<()> {
    let schedule = if let Some(schedule_type) = schedule_type {
        Some(parse_schedule(&schedule_type, schedule_value)?)
    } else {
        None
    };
    let result = executor
        .convert_session_to_task(TaskFromSessionRequest {
            session_id: session_id.to_string(),
            name,
            schedule: schedule
                .map(restflow_core::daemon::request_mapper::to_contract)
                .transpose()?,
            input,
            timeout_secs,
            durability_mode: None,
            memory: None,
            memory_scope: None,
            resource_limits: None,
            run_now: Some(run_now),
        })
        .await?;

    if format.is_json() {
        return print_json(&result);
    }

    println!(
        "Converted session {} -> task {} ({})",
        session_id,
        result.task.name,
        &result.task.id[..8.min(result.task.id.len())]
    );
    if result.run_now {
        println!("Triggered immediate run.");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_task(
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

    let spec = TaskSpec {
        name,
        agent_id,
        chat_session_id: None,
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

    let task = executor.create_task(spec).await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!(
        "Task created: {} ({})",
        task.name,
        &task.id[..8.min(task.id.len())]
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn update_task(
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

    let patch = TaskPatch {
        name,
        description: None,
        agent_id: None,
        chat_session_id: None,
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

    let task = executor.update_task(id, patch).await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!(
        "Task updated: {} ({})",
        task.name,
        &task.id[..8.min(task.id.len())]
    );
    Ok(())
}

async fn delete_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let result: DeleteWithIdResponse = executor.delete_task(id).await?;

    if format.is_json() {
        return print_json(&result);
    }

    println!("Task deleted: {}", result.id);
    Ok(())
}

async fn control_task(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    action: &str,
    format: OutputFormat,
) -> Result<()> {
    let parsed_action = parse_control_action(action)?;
    let task = executor.control_task(id, parsed_action.clone()).await?;

    if format.is_json() {
        return print_json(&task);
    }

    println!(
        "Task {} action: {:?} -> {:?}",
        id, parsed_action, task.status
    );
    Ok(())
}

async fn show_progress(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    event_limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let progress = executor.get_task_progress(id, Some(event_limit)).await?;

    if format.is_json() {
        return print_json(&progress);
    }

    println!("Task: {}", id);
    println!("Status: {:?}", progress.status);
    if let Some(stage) = &progress.stage {
        println!("Stage: {}", stage);
    }
    println!("Events (last {}):", progress.recent_events.len());
    for event in progress.recent_events {
        let ts = format_timestamp(Some(event.timestamp));
        println!(
            "  [{}] {:?}: {}",
            ts,
            event.event_type,
            event.message.unwrap_or_default()
        );
    }

    Ok(())
}

async fn send_message(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    message: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.send_task_message(id, message).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({"sent": true}));
    }

    println!("Message sent to task: {}", id);
    Ok(())
}

async fn show_run_log(
    executor: Arc<dyn CommandExecutor>,
    task_id: &str,
    run_id: Option<&str>,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let task = executor.get_task(task_id).await?;
    let session_id = if task.chat_session_id.trim().is_empty() {
        task.id.clone()
    } else {
        task.chat_session_id.clone()
    };

    if let Some(run_id) = run_id {
        let timeline = executor.get_execution_run_timeline(run_id).await?;

        if format.is_json() {
            return print_json(&serde_json::json!({
                "task_id": task_id,
                "session_id": session_id,
                "run_id": run_id,
                "total_events": timeline.events.len(),
                "timeline": timeline,
            }));
        }

        println!("Task: {}", task_id);
        println!("Session: {}", session_id);
        println!("Run (turn_id): {}", run_id);
        println!(
            "Events: {} (showing up to {})",
            timeline.events.len(),
            limit
        );
        for event in timeline.events {
            println!("{}", serde_json::to_string(&event)?);
        }
        return Ok(());
    }

    let runs = collect_run_ids(
        &executor
            .list_execution_sessions(RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::BackgroundTask,
                    id: task_id.to_string(),
                },
            })
            .await?,
    );

    if format.is_json() {
        return print_json(&serde_json::json!({
            "task_id": task_id,
            "session_id": session_id,
            "run_count": runs.len(),
            "runs": runs,
        }));
    }

    println!("Task: {}", task_id);
    println!("Session: {}", session_id);
    println!("Runs: {}", runs.len());
    if runs.is_empty() {
        println!("No runs found.");
    } else {
        for run in runs {
            println!("  {}", run);
        }
    }
    Ok(())
}

fn collect_run_ids(summaries: &[RunSummary]) -> Vec<String> {
    summaries
        .iter()
        .filter_map(|summary| summary.run_id.clone())
        .collect()
}

fn parse_schedule(schedule_type: &str, schedule_value: Option<String>) -> Result<TaskSchedule> {
    let value = schedule_value.ok_or_else(|| {
        anyhow::anyhow!(
            "Schedule value is required for schedule type: {}",
            schedule_type
        )
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

fn parse_control_action(action: &str) -> Result<TaskControlAction> {
    match action.to_lowercase().as_str() {
        "start" => Ok(TaskControlAction::Start),
        "pause" => Ok(TaskControlAction::Pause),
        "resume" => Ok(TaskControlAction::Resume),
        "stop" => Ok(TaskControlAction::Stop),
        "run_now" => Ok(TaskControlAction::RunNow),
        _ => anyhow::bail!(
            "Invalid action: {}. Use: start, pause, resume, stop, run_now",
            action
        ),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::models::ChatMessage;

    #[test]
    fn derive_conversion_name_prefers_explicit_name() {
        let name = derive_conversion_name(
            Some("  Explicit Name  ".to_string()),
            "Session Name",
            "session-1",
        );
        assert_eq!(name, "Explicit Name");
    }

    #[test]
    fn derive_conversion_name_falls_back_to_session_name() {
        let name = derive_conversion_name(None, "Session Name", "session-1");
        assert_eq!(name, "Background: Session Name");
    }

    #[test]
    fn derive_conversion_input_uses_latest_non_empty_user_message() {
        let messages = vec![
            ChatMessage::assistant("hello"),
            ChatMessage::user(""),
            ChatMessage::user(" latest request "),
        ];
        let input = derive_conversion_input(None, &messages);
        assert_eq!(input.as_deref(), Some("latest request"));
    }

    #[test]
    fn collect_run_ids_uses_run_summaries_in_order() {
        let first = RunSummary {
            id: "run-session-1".to_string(),
            kind: RunKind::BackgroundRun,
            container_id: "task-1".to_string(),
            root_run_id: Some("run-1".to_string()),
            title: "Run 1".to_string(),
            subtitle: None,
            status: "completed".to_string(),
            updated_at: 1,
            started_at: None,
            ended_at: None,
            session_id: Some("session-1".to_string()),
            run_id: Some("run-1".to_string()),
            task_id: Some("task-1".to_string()),
            parent_run_id: None,
            agent_id: None,
            source_channel: None,
            source_conversation_id: None,
            effective_model: None,
            provider: None,
            event_count: 2,
        };

        let second = RunSummary {
            id: "run-session-2".to_string(),
            kind: RunKind::BackgroundRun,
            container_id: "task-1".to_string(),
            root_run_id: Some("run-2".to_string()),
            title: "Run 2".to_string(),
            subtitle: None,
            status: "completed".to_string(),
            updated_at: 2,
            started_at: None,
            ended_at: None,
            session_id: Some("session-1".to_string()),
            run_id: Some("run-2".to_string()),
            task_id: Some("task-1".to_string()),
            parent_run_id: None,
            agent_id: None,
            source_channel: None,
            source_conversation_id: None,
            effective_model: None,
            provider: None,
            event_count: 2,
        };

        let run_ids = collect_run_ids(&[first, second]);
        assert_eq!(run_ids, vec!["run-1".to_string(), "run-2".to_string()]);
    }
}
