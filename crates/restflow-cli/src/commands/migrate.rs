use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, TableDefinition};
use restflow_core::paths;
use restflow_storage::{CliConfig, SystemConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::cli::MigrateArgs;

type Converter = fn(&[u8]) -> Result<Vec<u8>>;

const LEGACY_CONFIG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("system_config");

pub async fn run(args: MigrateArgs) -> Result<()> {
    println!("RestFlow Configuration Migration");
    println!("=================================\n");

    let restflow_dir = paths::ensure_restflow_dir()?;
    let legacy_paths = LegacyPaths::discover(restflow_dir);
    println!("Target directory: {}", legacy_paths.restflow_dir.display());
    println!(
        "Unified config target: {}\n",
        legacy_paths.global_config.display()
    );

    let config_plan = build_config_migration_plan(&legacy_paths, args.force)?;
    let file_migrations = collect_file_migrations(&legacy_paths, args.force);

    if let Some(old_mcp_db) = legacy_paths
        .old_mcp_db
        .as_ref()
        .filter(|path| path.exists())
    {
        println!("⚠️  Found old MCP database: {}", old_mcp_db.display());
        println!("   This database is no longer used (MCP now shares the main database).");
        println!("   You can safely delete it after verifying your data.\n");
    }

    if legacy_paths.old_auth.exists() {
        println!(
            "⚠️  Found old auth_profiles.json: {}",
            legacy_paths.old_auth.display()
        );
        println!("   Auth profiles are now stored in the database.");
        println!("   Run the application once to trigger automatic migration.\n");
    }

    if legacy_paths.global_config.exists() && !args.force {
        let retired = collect_retired_config_sources(&legacy_paths);
        if !retired.is_empty() {
            println!(
                "ℹ️  {} already exists and remains the runtime source of truth.",
                legacy_paths.global_config.display()
            );
            for source in retired {
                println!("   Ignored legacy source: {}", source);
            }
            println!();
        }
    }

    let migration_count = file_migrations.len() + usize::from(config_plan.is_some());
    if migration_count == 0 {
        println!("✅ No migrations needed. Configuration is up to date.");
        return Ok(());
    }

    println!("Found {} item(s) to migrate:\n", migration_count);
    let mut index = 1usize;
    if let Some(plan) = &config_plan {
        println!("{}. Unified runtime configuration", index);
        println!("   To: {}", plan.target.display());
        for source in &plan.imported_sources {
            println!("   Import: {}", source);
        }
        for note in &plan.notes {
            println!("   Note: {}", note);
        }
        println!();
        index += 1;
    }

    for migration in &file_migrations {
        println!("{}. {}", index, migration.description);
        println!("   From: {}", migration.source.display());
        println!("   To: {}", migration.target.display());
        println!();
        index += 1;
    }

    if args.dry_run {
        println!("Dry run - no changes made.");
        return Ok(());
    }

    println!("Proceeding with migration...\n");
    if let Some(plan) = &config_plan {
        print!("Migrating unified runtime configuration... ");
        match plan.execute() {
            Ok(()) => println!("OK"),
            Err(err) => println!("FAILED: {err}"),
        }
    }

    for migration in &file_migrations {
        print!("Migrating {}... ", migration.description);
        if migration.target.exists() && !args.force {
            println!("SKIPPED (target exists, use --force to overwrite)");
            continue;
        }

        match migration.execute() {
            Ok(()) => println!("OK"),
            Err(err) => println!("FAILED: {err}"),
        }
    }

    println!("\n✅ Migration complete.");
    println!(
        "\nRuntime configuration now reads from: {}",
        legacy_paths.global_config.display()
    );
    if let Some(plan) = &config_plan {
        for source in &plan.imported_sources {
            println!(" - Imported: {source}");
        }
        println!(
            " - Legacy DB config and legacy CLI files no longer participate once config.toml exists."
        );
    }
    println!("\nYou can now safely remove old configuration directories:");
    println!(" - ~/.config/restflow/");
    println!(" - ~/Library/Application Support/com.restflow.app/ (macOS)");

    Ok(())
}

#[derive(Debug, Clone)]
struct LegacyPaths {
    restflow_dir: PathBuf,
    global_config: PathBuf,
    current_db: PathBuf,
    legacy_db: Option<PathBuf>,
    legacy_cli_toml: Option<PathBuf>,
    legacy_cli_json: PathBuf,
    old_mcp_db: Option<PathBuf>,
    old_key: PathBuf,
    new_key: PathBuf,
    old_auth: PathBuf,
}

impl LegacyPaths {
    fn discover(restflow_dir: PathBuf) -> Self {
        let global_config = restflow_dir.join("config.toml");
        let current_db = restflow_dir.join("restflow.db");
        let legacy_cli_json = restflow_dir.join("config.json");
        let old_key = restflow_dir.join("secret-master-key.json");
        let new_key = restflow_dir.join("master.key");
        let old_auth = restflow_dir.join("auth_profiles.json");

        let legacy_cli_toml =
            dirs::config_dir().map(|dir| dir.join("restflow").join("config.toml"));
        let legacy_db =
            dirs::data_dir().map(|dir| dir.join("com.restflow.app").join("restflow.db"));
        let old_mcp_db =
            dirs::data_dir().map(|dir| dir.join("com.restflow.app").join("restflow-mcp.db"));

        Self {
            restflow_dir,
            global_config,
            current_db,
            legacy_db,
            legacy_cli_toml,
            legacy_cli_json,
            old_mcp_db,
            old_key,
            new_key,
            old_auth,
        }
    }
}

#[derive(Debug, Clone)]
struct ConfigMigrationPlan {
    target: PathBuf,
    system: SystemConfig,
    cli: CliConfig,
    imported_sources: Vec<String>,
    notes: Vec<String>,
}

impl ConfigMigrationPlan {
    fn execute(&self) -> Result<()> {
        write_unified_config(&self.target, &self.system, &self.cli)
    }
}

#[derive(Debug, Clone)]
struct Migration {
    source: PathBuf,
    target: PathBuf,
    description: &'static str,
    converter: Option<Converter>,
}

impl Migration {
    fn execute(&self) -> Result<()> {
        if let Some(parent) = self.target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = std::fs::read(&self.source)?;
        let output = match self.converter {
            Some(convert) => convert(&content)?,
            None => content,
        };
        std::fs::write(&self.target, output)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
struct UnifiedConfigFile {
    #[serde(flatten)]
    system: SystemConfig,
    cli: CliConfig,
}

#[derive(Debug, Deserialize)]
struct LegacyCliTomlConfig {
    default: Option<LegacyCliTomlDefaultConfig>,
    api_keys: Option<LegacyCliTomlApiKeys>,
}

#[derive(Debug, Deserialize)]
struct LegacyCliTomlDefaultConfig {
    agent: Option<String>,
    model: Option<String>,
    #[allow(dead_code)]
    db_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LegacyCliTomlApiKeys {
    anthropic: Option<String>,
    openai: Option<String>,
    deepseek: Option<String>,
}

fn build_config_migration_plan(
    paths: &LegacyPaths,
    force: bool,
) -> Result<Option<ConfigMigrationPlan>> {
    if paths.global_config.exists() && !force {
        return Ok(None);
    }

    let mut system = SystemConfig::default();
    let mut cli = CliConfig::default();
    let mut imported_sources = Vec::new();
    let mut notes = Vec::new();
    let mut has_source = false;

    if let Some((source, imported)) = read_system_source(paths)? {
        system = imported;
        imported_sources.push(format!("system config from {}", source.display()));
        has_source = true;
    }

    if paths.legacy_cli_json.exists() {
        if let Some(imported) = read_legacy_cli_json_config(&paths.legacy_cli_json)? {
            cli = imported;
            imported_sources.push(format!(
                "CLI settings from {}",
                paths.legacy_cli_json.display()
            ));
            has_source = true;
        }
    } else if let Some(path) = paths.legacy_cli_toml.as_ref().filter(|path| path.exists())
        && let Some((imported, contains_api_keys)) = read_legacy_cli_toml_config(path)?
    {
        cli = imported;
        imported_sources.push(format!("CLI settings from {}", path.display()));
        if contains_api_keys {
            notes.push(format!(
                "API keys found in {} are not copied into config.toml; store them with `restflow secret set` instead.",
                path.display()
            ));
        }
        has_source = true;
    }

    if !has_source {
        return Ok(None);
    }

    Ok(Some(ConfigMigrationPlan {
        target: paths.global_config.clone(),
        system,
        cli,
        imported_sources,
        notes,
    }))
}

fn collect_retired_config_sources(paths: &LegacyPaths) -> Vec<String> {
    let mut sources = Vec::new();
    if paths.legacy_cli_json.exists() {
        sources.push(paths.legacy_cli_json.display().to_string());
    }
    if let Some(path) = paths.legacy_cli_toml.as_ref().filter(|path| path.exists()) {
        sources.push(path.display().to_string());
    }
    if paths.current_db.exists() {
        sources.push(format!("{} (system config)", paths.current_db.display()));
    }
    if let Some(path) = paths.legacy_db.as_ref().filter(|path| path.exists()) {
        sources.push(format!("{} (system config)", path.display()));
    }
    sources
}

fn read_system_source(paths: &LegacyPaths) -> Result<Option<(PathBuf, SystemConfig)>> {
    if let Some(config) = read_legacy_db_system_config(&paths.current_db)? {
        return Ok(Some((paths.current_db.clone(), config)));
    }

    if let Some(path) = paths.legacy_db.as_ref().filter(|path| path.exists())
        && let Some(config) = read_legacy_db_system_config(path)?
    {
        return Ok(Some((path.clone(), config)));
    }

    Ok(None)
}

fn collect_file_migrations(paths: &LegacyPaths, force: bool) -> Vec<Migration> {
    let mut migrations = Vec::new();

    if let Some(old_db) = paths.legacy_db.as_ref()
        && old_db.exists()
        && (!paths.current_db.exists() || force)
    {
        migrations.push(Migration {
            source: old_db.clone(),
            target: paths.current_db.clone(),
            description: "Application database",
            converter: None,
        });
    }

    if paths.old_key.exists() && (!paths.new_key.exists() || force) {
        migrations.push(Migration {
            source: paths.old_key.clone(),
            target: paths.new_key.clone(),
            description: "Master encryption key",
            converter: Some(convert_master_key),
        });
    }

    migrations
}

fn read_legacy_db_system_config(path: &Path) -> Result<Option<SystemConfig>> {
    if !path.exists() {
        return Ok(None);
    }

    let temp_path = std::env::temp_dir().join(format!(
        "restflow-config-import-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::copy(path, &temp_path).with_context(|| {
        format!(
            "Failed to copy legacy database {} into a temporary import file",
            path.display()
        )
    })?;

    let result = (|| -> Result<Option<SystemConfig>> {
        let db = Database::create(&temp_path)
            .with_context(|| format!("Failed to open legacy database {}", path.display()))?;
        let read_txn = db.begin_read()?;
        let table = match read_txn.open_table(LEGACY_CONFIG_TABLE) {
            Ok(table) => table,
            Err(err) => {
                let message = err.to_string();
                if message.contains("does not exist") {
                    return Ok(None);
                }
                return Err(anyhow::Error::new(err).context(format!(
                    "Failed to open legacy config table from database {}",
                    path.display()
                )));
            }
        };

        if let Some(data) = table.get("system")? {
            let config = serde_json::from_slice(data.value()).with_context(|| {
                format!("Failed to parse system config from {}", path.display())
            })?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    })();

    let _ = std::fs::remove_file(&temp_path);
    result
}

fn read_legacy_cli_json_config(path: &Path) -> Result<Option<CliConfig>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read legacy CLI config from {}", path.display()))?;
    let config = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse legacy CLI config from {}", path.display()))?;
    Ok(Some(config))
}

fn read_legacy_cli_toml_config(path: &Path) -> Result<Option<(CliConfig, bool)>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read legacy CLI config from {}", path.display()))?;
    let legacy: LegacyCliTomlConfig = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse legacy CLI config from {}", path.display()))?;

    let contains_api_keys = legacy.api_keys.as_ref().is_some_and(|keys| {
        keys.anthropic.is_some() || keys.openai.is_some() || keys.deepseek.is_some()
    });

    Ok(Some((
        CliConfig {
            version: 1,
            default: restflow_storage::config::CliDefaultConfig {
                agent: legacy
                    .default
                    .as_ref()
                    .and_then(|value| value.agent.clone()),
                model: legacy.default.and_then(|value| value.model),
            },
            sandbox: restflow_storage::config::CliSandboxConfig::default(),
        },
        contains_api_keys,
    )))
}

fn write_unified_config(path: &Path, system: &SystemConfig, cli: &CliConfig) -> Result<()> {
    system.validate()?;
    let payload = UnifiedConfigFile {
        system: system.clone(),
        cli: cli.clone(),
    };
    let contents =
        toml::to_string_pretty(&payload).context("Failed to serialize unified config.toml")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create target config directory {}",
                parent.display()
            )
        })?;
    }

    std::fs::write(path, contents)
        .with_context(|| format!("Failed to write unified config to {}", path.display()))?;
    Ok(())
}

fn convert_master_key(json_bytes: &[u8]) -> Result<Vec<u8>> {
    #[derive(serde::Deserialize)]
    struct OldFormat {
        key: String,
    }

    let old: OldFormat = serde_json::from_slice(json_bytes)?;
    Ok(old.key.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::tempdir;

    #[test]
    fn read_legacy_cli_toml_config_extracts_default_settings() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("config.toml");
        fs::write(
            &path,
            r#"[default]
agent = "legacy-agent"
model = "legacy-model"

[api_keys]
anthropic = "hidden"
"#,
        )
        .unwrap();

        let (cli, contains_api_keys) = read_legacy_cli_toml_config(&path)
            .unwrap()
            .expect("legacy cli config should parse");

        assert_eq!(cli.default.agent.as_deref(), Some("legacy-agent"));
        assert_eq!(cli.default.model.as_deref(), Some("legacy-model"));
        assert!(contains_api_keys);
    }

    #[test]
    fn read_legacy_db_system_config_extracts_system_row() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("legacy.db");
        let db = Database::create(&db_path).unwrap();

        let config = SystemConfig {
            worker_count: 19,
            agent: restflow_storage::AgentDefaults {
                max_iterations: 155,
                ..restflow_storage::AgentDefaults::default()
            },
            ..SystemConfig::default()
        };
        let payload = serde_json::to_vec(&config).unwrap();

        let write_txn = db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(LEGACY_CONFIG_TABLE).unwrap();
            table.insert("system", payload.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();

        let loaded = read_legacy_db_system_config(&db_path)
            .unwrap()
            .expect("legacy system config should exist");
        assert_eq!(loaded.worker_count, 19);
        assert_eq!(loaded.agent.max_iterations, 155);
    }

    #[test]
    fn build_config_migration_plan_prefers_json_cli_source() {
        let temp_dir = tempdir().unwrap();
        let restflow_dir = temp_dir.path().join("restflow");
        fs::create_dir_all(&restflow_dir).unwrap();

        let json_path = restflow_dir.join("config.json");
        fs::write(
            &json_path,
            r#"{
  "version": 1,
  "default": { "agent": "json-agent", "model": "json-model" },
  "sandbox": { "enabled": false, "env": { "isolate": false, "allow": [], "block": [] }, "limits": { "timeout_secs": 120, "max_output_bytes": 1048576 } }
}"#,
        )
        .unwrap();

        let legacy_toml = temp_dir.path().join("legacy-config.toml");
        fs::write(
            &legacy_toml,
            r#"[default]
agent = "toml-agent"
model = "toml-model"
"#,
        )
        .unwrap();

        let paths = LegacyPaths {
            restflow_dir: restflow_dir.clone(),
            global_config: restflow_dir.join("config.toml"),
            current_db: restflow_dir.join("restflow.db"),
            legacy_db: None,
            legacy_cli_toml: Some(legacy_toml),
            legacy_cli_json: json_path,
            old_mcp_db: None,
            old_key: restflow_dir.join("secret-master-key.json"),
            new_key: restflow_dir.join("master.key"),
            old_auth: restflow_dir.join("auth_profiles.json"),
        };

        let plan = build_config_migration_plan(&paths, false)
            .unwrap()
            .expect("migration plan should exist");
        assert_eq!(plan.cli.default.agent.as_deref(), Some("json-agent"));
        assert_eq!(plan.cli.default.model.as_deref(), Some("json-model"));
    }

    #[test]
    fn write_unified_config_serializes_system_and_cli_sections() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("config.toml");
        let system = SystemConfig {
            worker_count: 27,
            ..SystemConfig::default()
        };
        let cli = CliConfig {
            version: 1,
            default: restflow_storage::config::CliDefaultConfig {
                agent: Some("unified-agent".to_string()),
                model: Some("unified-model".to_string()),
            },
            ..CliConfig::default()
        };

        write_unified_config(&path, &system, &cli).unwrap();
        let written = fs::read_to_string(&path).unwrap();

        assert!(written.contains("worker_count = 27"));
        assert!(written.contains("[cli.default]"));
        assert!(written.contains("agent = \"unified-agent\""));
        assert!(written.contains("model = \"unified-model\""));
    }
}
