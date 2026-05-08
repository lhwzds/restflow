use anyhow::Result;
use comfy_table::{Cell, Table};
use std::sync::Arc;

use crate::cli::SkillCommands;
use crate::commands::utils::format_timestamp;
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use runtime::services::skills as skill_service;
use serde_json::json;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: SkillCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        SkillCommands::List => list_skills(executor, format).await,
        SkillCommands::Show { id } => show_skill(executor, &id, format).await,
        SkillCommands::Export { id, output } => export_skill(executor, &id, output, format).await,
    }
}

async fn list_skills(executor: Arc<dyn CommandExecutor>, format: OutputFormat) -> Result<()> {
    let skills = executor.list_skills().await?;

    if format.is_json() {
        return print_json(&skills);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Updated", "Tags"]);

    for skill in skills {
        let tags = skill
            .tags
            .as_ref()
            .map(|values| values.join(", "))
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(skill.id),
            Cell::new(skill.name),
            Cell::new(format_timestamp(Some(skill.updated_at))),
            Cell::new(tags),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let skill = executor
        .get_skill(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("ID:          {}", skill.id);
    println!("Name:        {}", skill.name);
    println!(
        "Description: {}",
        skill.description.clone().unwrap_or_else(|| "-".to_string())
    );
    println!(
        "Tags:        {}",
        skill.tags.clone().unwrap_or_default().join(", ")
    );
    println!("Updated:     {}", format_timestamp(Some(skill.updated_at)));
    println!("\nContent:\n{}", skill.content);

    Ok(())
}

async fn export_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    output: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let skill = executor
        .get_skill(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

    let markdown = skill_service::export_skill_to_markdown(&skill);
    let path = output.unwrap_or_else(|| format!("{}.md", id));
    std::fs::write(&path, markdown)?;

    if format.is_json() {
        return print_json(&json!({ "id": id, "output": path }));
    }

    println!("Exported to: {}", path);
    Ok(())
}
