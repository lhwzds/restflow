use anyhow::Result;
use std::sync::Arc;

use crate::cli::HookCommands;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::AppCore;
use restflow_core::hooks::{AgentTaskHookScheduler, HookExecutor};
use restflow_core::models::{Hook, HookAction, HookContext, HookEvent};

pub async fn run(core: Arc<AppCore>, command: HookCommands, format: OutputFormat) -> Result<()> {
    match command {
        HookCommands::List => list_hooks(core, format).await,
        HookCommands::Create {
            name,
            event,
            action,
            url,
            script,
            channel,
            message,
            agent,
            input,
        } => {
            create_hook(
                core, name, event, action, url, script, channel, message, agent, input, format,
            )
            .await
        }
        HookCommands::Delete { id } => delete_hook(core, &id, format).await,
        HookCommands::Test { id } => test_hook(core, &id, format).await,
    }
}

async fn list_hooks(core: Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let hooks = core.storage.hooks.list()?;

    if format.is_json() {
        return print_json(&hooks);
    }

    if hooks.is_empty() {
        println!("No hooks found");
        return Ok(());
    }

    for hook in hooks {
        println!(
            "{}\t{}\t{}\t{}",
            hook.id,
            hook.name,
            hook.event.as_str(),
            if hook.enabled { "enabled" } else { "disabled" }
        );
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_hook(
    core: Arc<AppCore>,
    name: String,
    event: String,
    action: String,
    url: Option<String>,
    script: Option<String>,
    channel: Option<String>,
    message: Option<String>,
    agent: Option<String>,
    input: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let event = parse_event(&event)?;
    let action = build_action(action, url, script, channel, message, agent, input)?;

    let hook = Hook::new(name, event, action);
    core.storage.hooks.create(&hook)?;

    if format.is_json() {
        return print_json(&hook);
    }

    println!("Hook created: {} ({})", hook.name, hook.id);
    Ok(())
}

async fn delete_hook(core: Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let deleted = core.storage.hooks.delete(id)?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id, "deleted": deleted }));
    }

    if deleted {
        println!("Hook deleted: {}", id);
    } else {
        println!("Hook not found: {}", id);
    }
    Ok(())
}

async fn test_hook(core: Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let hook = core
        .storage
        .hooks
        .get(id)?
        .ok_or_else(|| anyhow::anyhow!("Hook not found: {}", id))?;

    let scheduler = Arc::new(AgentTaskHookScheduler::new(
        core.storage.background_agents.clone(),
    ));
    let executor = HookExecutor::new(Vec::new()).with_task_scheduler(scheduler);

    let context = sample_context(&hook.event);
    executor.execute_hook(&hook, &context).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id, "tested": true }));
    }

    println!("Hook test executed: {}", id);
    Ok(())
}

fn parse_event(value: &str) -> Result<HookEvent> {
    match value.trim().to_ascii_lowercase().as_str() {
        "task_started" | "started" => Ok(HookEvent::TaskStarted),
        "task_completed" | "completed" => Ok(HookEvent::TaskCompleted),
        "task_failed" | "failed" => Ok(HookEvent::TaskFailed),
        "task_cancelled" | "cancelled" | "canceled" => Ok(HookEvent::TaskCancelled),
        "tool_executed" => Ok(HookEvent::ToolExecuted),
        "approval_required" => Ok(HookEvent::ApprovalRequired),
        _ => anyhow::bail!("Unsupported hook event: {}", value),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_action(
    action: String,
    url: Option<String>,
    script: Option<String>,
    channel: Option<String>,
    message: Option<String>,
    agent: Option<String>,
    input: Option<String>,
) -> Result<HookAction> {
    match action.trim().to_ascii_lowercase().as_str() {
        "webhook" => Ok(HookAction::Webhook {
            url: url.ok_or_else(|| anyhow::anyhow!("--url is required for webhook action"))?,
            method: None,
            headers: None,
        }),
        "script" => Ok(HookAction::Script {
            path: script
                .ok_or_else(|| anyhow::anyhow!("--script is required for script action"))?,
            args: None,
            timeout_secs: None,
        }),
        "send_message" | "message" => Ok(HookAction::SendMessage {
            channel_type: channel.unwrap_or_else(|| "telegram".to_string()),
            message_template: message
                .ok_or_else(|| anyhow::anyhow!("--message is required for send_message action"))?,
        }),
        "run_task" => Ok(HookAction::RunTask {
            agent_id: agent.ok_or_else(|| anyhow::anyhow!("--agent is required for run_task"))?,
            input_template: input.unwrap_or_default(),
        }),
        _ => anyhow::bail!("Unsupported hook action: {}", action),
    }
}

fn sample_context(event: &HookEvent) -> HookContext {
    let now = chrono::Utc::now().timestamp_millis();

    match event {
        HookEvent::TaskFailed | HookEvent::TaskCancelled => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook-test-task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(false),
            output: None,
            error: Some("Hook test error".to_string()),
            duration_ms: Some(200),
            timestamp: now,
        },
        _ => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook-test-task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(true),
            output: Some("Hook test output".to_string()),
            error: None,
            duration_ms: Some(200),
            timestamp: now,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event() {
        assert!(matches!(
            parse_event("task_completed").expect("parse"),
            HookEvent::TaskCompleted
        ));
        assert!(parse_event("unknown").is_err());
    }

    #[test]
    fn test_build_run_task_action() {
        let action = build_action(
            "run_task".to_string(),
            None,
            None,
            None,
            None,
            Some("agent-1".to_string()),
            Some("input".to_string()),
        )
        .expect("build action");

        match action {
            HookAction::RunTask {
                agent_id,
                input_template,
            } => {
                assert_eq!(agent_id, "agent-1");
                assert_eq!(input_template, "input");
            }
            _ => panic!("Expected run_task action"),
        }
    }
}
