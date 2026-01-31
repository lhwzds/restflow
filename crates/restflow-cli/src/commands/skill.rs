use anyhow::Result;
use comfy_table::{Cell, Table};
use std::path::Path;
use std::sync::Arc;

use crate::cli::SkillCommands;
use crate::commands::utils::{format_timestamp, preview_text, slugify};
use crate::output::{json::print_json, OutputFormat};
use restflow_core::models::Skill;
use restflow_core::registry::{MarketplaceProvider, SkillRegistry, SkillSearchQuery};
use restflow_core::services::skills as skill_service;
use restflow_core::AppCore;
use serde_json::json;

pub async fn run(core: Arc<AppCore>, command: SkillCommands, format: OutputFormat) -> Result<()> {
    match command {
        SkillCommands::List => list_skills(&core, format).await,
        SkillCommands::Show { id } => show_skill(&core, &id, format).await,
        SkillCommands::Create { name } => create_skill(&core, &name, format).await,
        SkillCommands::Delete { id } => delete_skill(&core, &id, format).await,
        SkillCommands::Import { path } => import_skill(&core, &path, format).await,
        SkillCommands::Export { id, output } => export_skill(&core, &id, output, format).await,
        SkillCommands::Search { query } => search_skills(&query, format).await,
        SkillCommands::Install { name } => install_skill(&core, &name, format).await,
    }
}

async fn list_skills(core: &Arc<AppCore>, format: OutputFormat) -> Result<()> {
    let skills = skill_service::list_skills(core).await?;

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

async fn show_skill(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    let skill = skill_service::get_skill(core, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Skill not found: {}", id))?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("ID:          {}", skill.id);
    println!("Name:        {}", skill.name);
    println!("Description: {}", skill.description.clone().unwrap_or_else(|| "-".to_string()));
    println!("Tags:        {}", skill.tags.clone().unwrap_or_default().join(", "));
    println!("Updated:     {}", format_timestamp(Some(skill.updated_at)));
    println!("\nContent:\n{}", skill.content);

    Ok(())
}

async fn create_skill(core: &Arc<AppCore>, name: &str, format: OutputFormat) -> Result<()> {
    let id = slugify(name);
    let content = format!("# {}\n", name);
    let skill = Skill::new(id.clone(), name.to_string(), None, None, content);

    skill_service::create_skill(core, skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill created: {} ({})", skill.name, skill.id);
    Ok(())
}

async fn delete_skill(core: &Arc<AppCore>, id: &str, format: OutputFormat) -> Result<()> {
    skill_service::delete_skill(core, id).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "id": id }));
    }

    println!("Skill deleted: {id}");
    Ok(())
}

async fn import_skill(core: &Arc<AppCore>, path: &str, format: OutputFormat) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let filename = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("imported-skill");
    let id = slugify(filename);

    let skill = skill_service::import_skill_from_markdown(&id, &content)?;
    skill_service::create_skill(core, skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill imported: {} ({})", skill.name, skill.id);
    Ok(())
}

async fn export_skill(
    core: &Arc<AppCore>,
    id: &str,
    output: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let skill = skill_service::get_skill(core, id)
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

async fn search_skills(query: &str, format: OutputFormat) -> Result<()> {
    let mut registry = SkillRegistry::with_defaults();
    registry.add_provider(Arc::new(MarketplaceProvider::new()));

    let query = SkillSearchQuery {
        query: Some(query.to_string()),
        limit: Some(20),
        ..SkillSearchQuery::default()
    };

    let results = registry.search(&query).await;

    if format.is_json() {
        return print_json(&results);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Version", "Description"]);

    for result in results {
        let description = result
            .manifest
            .description
            .clone()
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(result.manifest.id),
            Cell::new(result.manifest.name),
            Cell::new(result.manifest.version.to_string()),
            Cell::new(preview_text(&description, 60)),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn install_skill(core: &Arc<AppCore>, name: &str, format: OutputFormat) -> Result<()> {
    let mut registry = SkillRegistry::with_defaults();
    registry.add_provider(Arc::new(MarketplaceProvider::new()));

    let installed = registry.install(name).await?;

    let tags = if installed.manifest.keywords.is_empty() {
        None
    } else {
        Some(installed.manifest.keywords.clone())
    };

    let skill = Skill::new(
        installed.manifest.id.clone(),
        installed.manifest.name.clone(),
        installed.manifest.description.clone(),
        tags,
        installed.content.clone(),
    );

    let existing = skill_service::get_skill(core, &installed.manifest.id).await?;
    if let Some(mut existing_skill) = existing {
        existing_skill.update(
            Some(skill.name),
            Some(skill.description),
            Some(skill.tags),
            Some(skill.content),
        );
        skill_service::update_skill(core, &installed.manifest.id, &existing_skill).await?;
    } else {
        skill_service::create_skill(core, skill.clone()).await?;
    }

    if format.is_json() {
        return print_json(&installed);
    }

    println!("Skill installed: {} ({})", installed.manifest.name, installed.manifest.id);
    Ok(())
}
