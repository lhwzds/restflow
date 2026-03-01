use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use serde_json::json;

use crate::cli::{AllowlistAction, SecurityCommands};
use crate::commands::utils::short_id;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::security::{
    ApprovalStatus, CommandPattern, PendingApproval, SecurityPolicy,
};
use restflow_core::paths;

const POLICY_FILE: &str = "security_policy.json";
const APPROVALS_FILE: &str = "security_approvals.json";

pub async fn run(command: SecurityCommands, format: OutputFormat) -> Result<()> {
    match command {
        SecurityCommands::Approvals => list_pending_approvals(format).await,
        SecurityCommands::Approve { id } => approve_request(&id, format).await,
        SecurityCommands::Reject { id } => reject_request(&id, format).await,
        SecurityCommands::Allowlist { action } => manage_allowlist(action, format).await,
    }
}

async fn list_pending_approvals(format: OutputFormat) -> Result<()> {
    let approvals = load_approvals()?;
    let pending: Vec<PendingApproval> = approvals
        .into_iter()
        .filter(|approval| approval.status == ApprovalStatus::Pending)
        .collect();

    if format.is_json() {
        return print_json(&pending);
    }

    if pending.is_empty() {
        println!("No pending approvals.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Command", "Task", "Agent", "Expires"]);

    for approval in pending {
        table.add_row(vec![
            Cell::new(short_id(&approval.id)),
            Cell::new(approval.command),
            Cell::new(approval.task_id),
            Cell::new(approval.agent_id),
            Cell::new(approval.expires_at),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn approve_request(id: &str, format: OutputFormat) -> Result<()> {
    update_approval_status(id, ApprovalStatus::Approved, format)
}

async fn reject_request(id: &str, format: OutputFormat) -> Result<()> {
    update_approval_status(id, ApprovalStatus::Rejected, format)
}

fn update_approval_status(id: &str, status: ApprovalStatus, format: OutputFormat) -> Result<()> {
    let mut approvals = load_approvals()?;
    let index = resolve_approval_index(&approvals, id)?;

    match status {
        ApprovalStatus::Approved => approvals[index].approve(),
        ApprovalStatus::Rejected => approvals[index].reject(None),
        _ => {}
    }

    let updated_id = approvals[index].id.clone();
    let updated_status = approvals[index].status;

    save_approvals(&approvals)?;

    if format.is_json() {
        return print_json(&json!({ "id": updated_id, "status": updated_status }));
    }

    println!("Updated approval {id}: {:?}", updated_status);
    Ok(())
}

async fn manage_allowlist(action: AllowlistAction, format: OutputFormat) -> Result<()> {
    match action {
        AllowlistAction::Show => show_allowlist(format).await,
        AllowlistAction::Add {
            pattern,
            description,
        } => add_allowlist(pattern, description, format).await,
        AllowlistAction::Remove { index } => remove_allowlist(index, format).await,
    }
}

async fn show_allowlist(format: OutputFormat) -> Result<()> {
    let policy = load_policy()?;

    if format.is_json() {
        return print_json(&policy.allowlist);
    }

    if policy.allowlist.is_empty() {
        println!("Allowlist is empty.");
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["Index", "Pattern", "Description"]);

    for (index, pattern) in policy.allowlist.iter().enumerate() {
        table.add_row(vec![
            Cell::new(index),
            Cell::new(pattern.pattern.clone()),
            Cell::new(pattern.description.clone().unwrap_or_default()),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn add_allowlist(
    pattern: String,
    description: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let mut policy = load_policy()?;

    let command_pattern = match description {
        Some(text) => CommandPattern::with_description(pattern, text),
        None => CommandPattern::new(pattern),
    };

    policy.allowlist.push(command_pattern);
    save_policy(&policy)?;

    if format.is_json() {
        return print_json(&policy.allowlist);
    }

    println!("Allowlist pattern added.");
    Ok(())
}

async fn remove_allowlist(index: usize, format: OutputFormat) -> Result<()> {
    let mut policy = load_policy()?;

    if index >= policy.allowlist.len() {
        bail!("Allowlist index out of range: {index}");
    }

    policy.allowlist.remove(index);
    save_policy(&policy)?;

    if format.is_json() {
        return print_json(&policy.allowlist);
    }

    println!("Allowlist pattern removed.");
    Ok(())
}

fn load_policy() -> Result<SecurityPolicy> {
    let path = policy_path()?;
    if !path.exists() {
        return Ok(SecurityPolicy::default());
    }

    let bytes = std::fs::read(path)?;
    let policy: SecurityPolicy = serde_json::from_slice(&bytes)?;
    Ok(policy)
}

fn save_policy(policy: &SecurityPolicy) -> Result<()> {
    let path = policy_path()?;
    let bytes = serde_json::to_vec_pretty(policy)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn load_approvals() -> Result<Vec<PendingApproval>> {
    let path = approvals_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let bytes = std::fs::read(path)?;
    let approvals: Vec<PendingApproval> = serde_json::from_slice(&bytes)?;
    Ok(approvals)
}

fn save_approvals(approvals: &[PendingApproval]) -> Result<()> {
    let path = approvals_path()?;
    let bytes = serde_json::to_vec_pretty(approvals)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

fn policy_path() -> Result<std::path::PathBuf> {
    Ok(paths::ensure_restflow_dir()?.join(POLICY_FILE))
}

fn approvals_path() -> Result<std::path::PathBuf> {
    Ok(paths::ensure_restflow_dir()?.join(APPROVALS_FILE))
}

fn resolve_approval_index(approvals: &[PendingApproval], id: &str) -> Result<usize> {
    // Try exact match first
    if let Some(index) = approvals.iter().position(|a| a.id == id) {
        return Ok(index);
    }

    // Try prefix match
    let matches: Vec<_> = approvals
        .iter()
        .enumerate()
        .filter(|(_, approval)| approval.id.starts_with(id))
        .map(|(i, _)| i)
        .collect();

    if matches.is_empty() {
        bail!("Approval not found: {id}");
    }

    if matches.len() > 1 {
        bail!("Approval id '{id}' is ambiguous");
    }

    Ok(matches[0])
}
