use anyhow::Result;
use comfy_table::{Cell, Table};
use std::collections::HashSet;
use std::sync::Arc;

use crate::cli::{BackgroundAgentCommands, OutputFormat};
use crate::commands::utils::format_timestamp;
use crate::executor::CommandExecutor;
use crate::output::json::print_json;
use crate::output::table::print_table;
use restflow_core::models::{
    BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSpec, ChatMessage, ChatRole,
    TaskSchedule,
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
        BackgroundAgentCommands::ConvertSession {
            session_id,
            name,
            input,
            schedule,
            schedule_value,
            timeout,
            run_now,
        } => {
            convert_session_to_background_agent(
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
        BackgroundAgentCommands::RunLog { id, run_id, limit } => {
            show_run_log(executor, &id, run_id.as_deref(), limit, format).await
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
            .map(|ts| format_timestamp(Some(ts)))
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
    println!("Created:     {}", format_timestamp(Some(agent.created_at)));
    println!("Updated:     {}", format_timestamp(Some(agent.updated_at)));
    if let Some(last_run) = agent.last_run_at {
        println!("Last Run:    {}", format_timestamp(Some(last_run)));
    }
    if let Some(next_run) = agent.next_run_at {
        println!("Next Run:    {}", format_timestamp(Some(next_run)));
    }
    println!("Success:     {}", agent.success_count);
    println!("Failed:      {}", agent.failure_count);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn convert_session_to_background_agent(
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
    let session = executor.get_session(session_id).await?;
    let schedule = if let Some(schedule_type) = schedule_type {
        Some(parse_schedule(&schedule_type, schedule_value)?)
    } else {
        None
    }
    .unwrap_or_else(default_conversion_schedule);

    let name = derive_conversion_name(name, &session.name, &session.id);
    let input = derive_conversion_input(input, &session.messages)?;

    let spec = BackgroundAgentSpec {
        name,
        agent_id: session.agent_id.clone(),
        chat_session_id: Some(session.id.clone()),
        description: Some(format!("Converted from chat session {}", session.id)),
        input: Some(input),
        input_template: None,
        schedule,
        notification: None,
        execution_mode: None,
        timeout_secs,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: vec![],
        continuation: None,
    };

    let agent = executor.create_background_agent(spec).await?;
    if run_now {
        executor
            .control_background_agent(&agent.id, BackgroundAgentControlAction::RunNow)
            .await?;
    }

    if format.is_json() {
        return print_json(&serde_json::json!({
            "task": agent,
            "source_session": {
                "id": session.id,
                "agent_id": session.agent_id,
            },
            "run_now": run_now,
        }));
    }

    println!(
        "Converted session {} -> background agent {} ({})",
        session_id,
        agent.name,
        &agent.id[..8.min(agent.id.len())]
    );
    if run_now {
        println!("Triggered immediate run.");
    }
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

    let agent = executor.update_background_agent(id, patch).await?;

    if format.is_json() {
        return print_json(&agent);
    }

    println!(
        "Background agent updated: {} ({})",
        agent.name,
        &agent.id[..8]
    );
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
    executor
        .control_background_agent(id, parsed_action.clone())
        .await?;

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
    executor.send_background_agent_message(id, message).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({"sent": true}));
    }

    println!("Message sent to background agent: {}", id);
    Ok(())
}

async fn show_run_log(
    executor: Arc<dyn CommandExecutor>,
    task_id: &str,
    run_id: Option<&str>,
    limit: usize,
    format: OutputFormat,
) -> Result<()> {
    let task = executor.get_background_agent(task_id).await?;
    let session_id = if task.chat_session_id.trim().is_empty() {
        task.id.clone()
    } else {
        task.chat_session_id.clone()
    };

    if let Some(run_id) = run_id {
        let traces = executor
            .list_tool_traces(&session_id, Some(run_id.to_string()), Some(limit))
            .await?;

        if format.is_json() {
            return print_json(&serde_json::json!({
                "task_id": task_id,
                "session_id": session_id,
                "run_id": run_id,
                "total_events": traces.len(),
                "events": traces,
            }));
        }

        println!("Task: {}", task_id);
        println!("Session: {}", session_id);
        println!("Run (turn_id): {}", run_id);
        println!("Events: {} (showing up to {})", traces.len(), limit);
        for trace in traces {
            println!("{}", serde_json::to_string(&trace)?);
        }
        return Ok(());
    }

    let traces = executor.list_tool_traces(&session_id, None, None).await?;
    let mut seen = HashSet::new();
    let mut runs = Vec::new();
    for trace in traces.iter().rev() {
        if seen.insert(trace.turn_id.clone()) {
            runs.push(trace.turn_id.clone());
        }
    }

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

fn default_conversion_schedule() -> TaskSchedule {
    let now = chrono::Utc::now().timestamp_millis();
    TaskSchedule::Once {
        run_at: now.saturating_add(1_000),
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn derive_conversion_name(name: Option<String>, session_name: &str, session_id: &str) -> String {
    if let Some(name) = normalize_optional_text(name) {
        return name;
    }
    let base = session_name.trim();
    if base.is_empty() {
        format!("Background from {}", session_id)
    } else {
        format!("Background: {}", base)
    }
}

fn derive_conversion_input(input: Option<String>, messages: &[ChatMessage]) -> Result<String> {
    if let Some(input) = normalize_optional_text(input) {
        return Ok(input);
    }

    messages
        .iter()
        .rev()
        .find(|message| message.role == ChatRole::User)
        .and_then(|message| normalize_optional_text(Some(message.content.clone())))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot convert session: no non-empty user message found; use --input to provide one."
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let input = derive_conversion_input(None, &messages).expect("input should be resolved");
        assert_eq!(input, "latest request");
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}
