use anyhow::{Result, anyhow};
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::NoteCommands;
use crate::commands::utils::{format_timestamp, preview_text};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::models::{NoteQuery, NoteStatus, WorkspaceNotePatch, WorkspaceNoteSpec};
use serde_json::json;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: NoteCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        NoteCommands::List {
            folder,
            status,
            priority,
            tag,
            assignee,
            search,
        } => {
            let query = NoteQuery {
                folder,
                status: parse_status(status)?,
                priority,
                tag,
                assignee,
                search,
            };
            list_notes(executor, query, format).await
        }
        NoteCommands::Folders => list_folders(executor, format).await,
        NoteCommands::Show { id } => show_note(executor, &id, format).await,
        NoteCommands::Create {
            folder,
            title,
            file,
            content,
            priority,
            tags,
        } => {
            let content = load_content(file.as_deref(), content)?;
            let spec = WorkspaceNoteSpec {
                folder,
                title,
                content: content.unwrap_or_default(),
                priority,
                tags,
            };
            create_note(executor, spec, format).await
        }
        NoteCommands::Update {
            id,
            title,
            status,
            priority,
            assignee,
            folder,
            file,
            content,
            tags,
        } => {
            let content = load_content(file.as_deref(), content)?;
            let patch = WorkspaceNotePatch {
                title,
                content,
                priority,
                status: parse_status(status)?,
                tags,
                assignee,
                folder,
            };
            update_note(executor, &id, patch, format).await
        }
        NoteCommands::Delete { id } => delete_note(executor, &id, format).await,
    }
}

async fn list_notes(
    executor: Arc<dyn CommandExecutor>,
    query: NoteQuery,
    format: OutputFormat,
) -> Result<()> {
    let notes = executor.list_notes(query).await?;

    if format.is_json() {
        return print_json(&notes);
    }

    let mut table = Table::new();
    table.set_header(vec![
        "ID", "Folder", "Status", "Priority", "Updated", "Title", "Tags",
    ]);

    for note in notes {
        table.add_row(vec![
            Cell::new(note.id),
            Cell::new(note.folder),
            Cell::new(status_label(note.status)),
            Cell::new(note.priority.unwrap_or_else(|| "-".to_string())),
            Cell::new(format_timestamp(Some(note.updated_at))),
            Cell::new(preview_text(&note.title, 36)),
            Cell::new(note.tags.join(",")),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn list_folders(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let folders = executor.list_note_folders().await?;

    if format.is_json() {
        return print_json(&json!({ "folders": folders }));
    }

    for folder in folders {
        println!("{}", folder);
    }

    Ok(())
}

async fn show_note(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let note = executor
        .get_note(id)
        .await?
        .ok_or_else(|| anyhow!("Workspace note not found: {}", id))?;

    if format.is_json() {
        return print_json(&note);
    }

    println!("ID:       {}", note.id);
    println!("Folder:   {}", note.folder);
    println!("Title:    {}", note.title);
    println!("Status:   {}", status_label(note.status));
    println!(
        "Priority: {}",
        note.priority.clone().unwrap_or_else(|| "-".to_string())
    );
    println!(
        "Assignee: {}",
        note.assignee.clone().unwrap_or_else(|| "-".to_string())
    );
    println!("Tags:     {}", note.tags.join(", "));
    println!("Updated:  {}", format_timestamp(Some(note.updated_at)));
    println!("\nContent:\n{}", note.content);

    Ok(())
}

async fn create_note(
    executor: Arc<dyn CommandExecutor>,
    spec: WorkspaceNoteSpec,
    format: OutputFormat,
) -> Result<()> {
    let note = executor.create_note(spec).await?;

    if format.is_json() {
        return print_json(&note);
    }

    println!("Workspace note created: {}", note.id);
    Ok(())
}

async fn update_note(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    patch: WorkspaceNotePatch,
    format: OutputFormat,
) -> Result<()> {
    let note = executor.update_note(id, patch).await?;

    if format.is_json() {
        return print_json(&note);
    }

    println!("Workspace note updated: {}", note.id);
    Ok(())
}

async fn delete_note(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.delete_note(id).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "id": id }));
    }

    println!("Workspace note deleted: {}", id);
    Ok(())
}

fn load_content(file: Option<&str>, content: Option<String>) -> Result<Option<String>> {
    match (file, content) {
        (Some(_path), Some(_)) => Err(anyhow!("Use either --file or --content, not both")),
        (Some(path), None) => Ok(Some(std::fs::read_to_string(path)?)),
        (None, value) => Ok(value),
    }
}

fn parse_status(status: Option<String>) -> Result<Option<NoteStatus>> {
    let Some(value) = status else {
        return Ok(None);
    };

    let normalized = value.trim().to_ascii_lowercase();
    let parsed = match normalized.as_str() {
        "open" => NoteStatus::Open,
        "in_progress" | "in-progress" => NoteStatus::InProgress,
        "done" => NoteStatus::Done,
        "archived" => NoteStatus::Archived,
        _ => return Err(anyhow!("Invalid status: {}", value)),
    };

    Ok(Some(parsed))
}

fn status_label(status: NoteStatus) -> &'static str {
    match status {
        NoteStatus::Open => "open",
        NoteStatus::InProgress => "in_progress",
        NoteStatus::Done => "done",
        NoteStatus::Archived => "archived",
    }
}
