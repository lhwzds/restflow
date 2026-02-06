use anyhow::Result;
use comfy_table::{Cell, Table};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cli::SkillCommands;
use crate::commands::utils::{format_timestamp, preview_text, slugify};
use crate::executor::CommandExecutor;
use crate::output::{OutputFormat, json::print_json};
use restflow_core::loader::git_source::GitSkillSource;
use restflow_core::loader::skill_folder::{SkillFolderLoader, discover_skill_dirs};
use restflow_core::loader::skill_package::SkillPackageImporter;
use restflow_core::models::{Skill, StorageMode};
use restflow_core::paths;
use restflow_core::registry::{MarketplaceProvider, SkillRegistry, SkillSearchQuery};
use restflow_core::services::skills as skill_service;
use serde_json::json;

pub async fn run(
    executor: Arc<dyn CommandExecutor>,
    command: SkillCommands,
    format: OutputFormat,
) -> Result<()> {
    match command {
        SkillCommands::List => list_skills(executor, format).await,
        SkillCommands::Show { id } => show_skill(executor, &id, format).await,
        SkillCommands::Create { name } => create_skill(executor, &name, format).await,
        SkillCommands::Delete { id } => delete_skill(executor, &id, format).await,
        SkillCommands::Import { path } => import_skill(executor, &path, format).await,
        SkillCommands::Export { id, output } => export_skill(executor, &id, output, format).await,
        SkillCommands::Search { query } => search_skills(&query, format).await,
        SkillCommands::Install {
            source,
            path,
            scope,
        } => install_skill(executor, &source, path.as_deref(), &scope, format).await,
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

async fn create_skill(
    executor: Arc<dyn CommandExecutor>,
    name: &str,
    format: OutputFormat,
) -> Result<()> {
    let id = slugify(name);
    let content = format!("# {}\n", name);
    let skill = Skill::new(id.clone(), name.to_string(), None, None, content);

    executor.create_skill(skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill created: {} ({})", skill.name, skill.id);
    Ok(())
}

async fn delete_skill(
    executor: Arc<dyn CommandExecutor>,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    executor.delete_skill(id).await?;

    if format.is_json() {
        return print_json(&json!({ "deleted": true, "id": id }));
    }

    println!("Skill deleted: {id}");
    Ok(())
}

async fn import_skill(
    executor: Arc<dyn CommandExecutor>,
    path: &str,
    format: OutputFormat,
) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    let filename = Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("imported-skill");
    let id = slugify(filename);

    let skill = skill_service::import_skill_from_markdown(&id, &content)?;
    executor.create_skill(skill.clone()).await?;

    if format.is_json() {
        return print_json(&skill);
    }

    println!("Skill imported: {} ({})", skill.name, skill.id);
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

async fn install_skill(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    subpath: Option<&str>,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    if is_git_source(source) {
        return install_from_git(executor, source, subpath, scope, format).await;
    }

    if is_skill_package(source) {
        return install_from_package(executor, source, scope, format).await;
    }

    let path = Path::new(source);
    if path.exists() {
        return install_from_local_path(executor, path, scope, format).await;
    }

    install_from_marketplace(executor, source, format).await
}

async fn install_from_marketplace(
    executor: Arc<dyn CommandExecutor>,
    name: &str,
    format: OutputFormat,
) -> Result<()> {
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

    let existing = executor.get_skill(&installed.manifest.id).await?;
    if let Some(mut existing_skill) = existing {
        existing_skill.update(
            Some(skill.name),
            Some(skill.description),
            Some(skill.tags),
            Some(skill.content),
        );
        executor
            .update_skill(&installed.manifest.id, existing_skill)
            .await?;
    } else {
        executor.create_skill(skill.clone()).await?;
    }

    if format.is_json() {
        return print_json(&installed);
    }

    println!(
        "Skill installed from marketplace: {} ({})",
        installed.manifest.name, installed.manifest.id
    );
    Ok(())
}

async fn install_from_git(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    subpath: Option<&str>,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let (temp_dir, skill_dirs) = GitSkillSource::clone_and_discover(source, subpath).await?;
    let _guard = temp_dir;
    install_from_dirs(executor, source, &skill_dirs, scope, format).await
}

async fn install_from_package(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let path = Path::new(source);
    let (temp_dir, skill_dirs) = SkillPackageImporter::import(path)?;
    let _guard = temp_dir;
    install_from_dirs(executor, source, &skill_dirs, scope, format).await
}

async fn install_from_local_path(
    executor: Arc<dyn CommandExecutor>,
    path: &Path,
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    let skill_dirs = discover_skill_dirs(path)?;
    install_from_dirs(
        executor,
        path.to_string_lossy().as_ref(),
        &skill_dirs,
        scope,
        format,
    )
    .await
}

async fn install_from_dirs(
    executor: Arc<dyn CommandExecutor>,
    source: &str,
    skill_dirs: &[PathBuf],
    scope: &str,
    format: OutputFormat,
) -> Result<()> {
    if skill_dirs.is_empty() {
        return Err(anyhow::anyhow!("No skills found in source: {}", source));
    }

    let target_base = resolve_scope_dir(scope)?;
    let loader = SkillFolderLoader::new(PathBuf::new());

    let mut installed_ids = Vec::new();
    for skill_dir in skill_dirs {
        let mut skill = loader.load_skill_folder(skill_dir)?;
        let target_dir = target_base.join(&skill.id);
        copy_skill_dir(skill_dir, &target_dir)?;

        let skill_id = skill.id.clone();
        skill.folder_path = Some(target_dir.to_string_lossy().to_string());
        skill.storage_mode = StorageMode::FileSystemOnly;
        upsert_skill(&executor, skill).await?;
        installed_ids.push(skill_id);
    }

    if format.is_json() {
        return print_json(&json!({
            "source": source,
            "scope": scope,
            "installed": installed_ids,
        }));
    }

    println!(
        "Installed {} skill(s) from {} into {} scope",
        installed_ids.len(),
        source,
        scope
    );
    Ok(())
}

async fn upsert_skill(executor: &Arc<dyn CommandExecutor>, mut skill: Skill) -> Result<()> {
    let existing = executor.get_skill(&skill.id).await?;
    if let Some(existing_skill) = existing {
        skill.created_at = existing_skill.created_at;
        skill.updated_at = chrono::Utc::now().timestamp_millis();
        executor.update_skill(&skill.id, skill).await?;
    } else {
        executor.create_skill(skill).await?;
    }
    Ok(())
}

fn resolve_scope_dir(scope: &str) -> Result<PathBuf> {
    match scope {
        "user" => paths::user_skills_dir(),
        "workspace" => paths::workspace_skills_dir(),
        _ => Err(anyhow::anyhow!("Invalid scope: {}", scope)),
    }
}

fn is_git_source(source: &str) -> bool {
    source.starts_with("https://")
        || source.starts_with("http://")
        || source.ends_with(".git")
        || source.starts_with("git@")
}

fn is_skill_package(source: &str) -> bool {
    source.ends_with(".skill") || source.ends_with(".zip")
}

fn copy_skill_dir(source: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;

    for entry in walkdir::WalkDir::new(source).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(source).unwrap_or(path);
        let dest = target.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &dest)?;
        }
    }

    Ok(())
}
