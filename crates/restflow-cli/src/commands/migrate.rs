use anyhow::Result;
use std::path::PathBuf;

use crate::cli::MigrateArgs;
use restflow_core::paths;

type Converter = fn(&[u8]) -> Result<Vec<u8>>;

pub async fn run(args: MigrateArgs) -> Result<()> {
    println!("RestFlow Configuration Migration");
    println!("=================================\n");

    let restflow_dir = paths::ensure_restflow_dir()?;
    println!("Target directory: {}\n", restflow_dir.display());

    let mut migrations = Vec::new();

    if let Some(config_dir) = dirs::config_dir() {
        let old_config = config_dir.join("restflow").join("config.toml");
        if old_config.exists() {
            migrations.push(Migration {
                source: old_config,
                target: restflow_dir.join("config.json"),
                description: "CLI configuration",
                converter: Some(convert_toml_to_json),
            });
        }
    }

    if let Some(data_dir) = dirs::data_dir() {
        let old_db = data_dir.join("com.restflow.app").join("restflow.db");
        let new_db = restflow_dir.join("restflow.db");
        if old_db.exists() && !new_db.exists() {
            migrations.push(Migration {
                source: old_db,
                target: new_db,
                description: "Application database",
                converter: None,
            });
        }

        let old_mcp_db = data_dir.join("com.restflow.app").join("restflow-mcp.db");
        if old_mcp_db.exists() {
            println!("⚠️  Found old MCP database: {}", old_mcp_db.display());
            println!("   This database is no longer used (MCP now shares main database)");
            println!("   You can safely delete it after verifying your data.\n");
        }
    }

    let old_key = restflow_dir.join("secret-master-key.json");
    let new_key = restflow_dir.join("master.key");
    if old_key.exists() && !new_key.exists() {
        migrations.push(Migration {
            source: old_key,
            target: new_key,
            description: "Master encryption key",
            converter: Some(convert_master_key),
        });
    }

    let old_auth = restflow_dir.join("auth_profiles.json");
    if old_auth.exists() {
        println!("⚠️  Found old auth_profiles.json: {}", old_auth.display());
        println!("   Auth profiles are now stored in the database.");
        println!("   Run the application once to trigger automatic migration.\n");
    }

    if migrations.is_empty() {
        println!("✅ No migrations needed. Configuration is up to date.");
        return Ok(());
    }

    println!("Found {} item(s) to migrate:\n", migrations.len());
    for (i, migration) in migrations.iter().enumerate() {
        println!("{}. {}", i + 1, migration.description);
        println!("   From: {}", migration.source.display());
        println!("   To: {}", migration.target.display());
        println!();
    }

    if args.dry_run {
        println!("Dry run - no changes made.");
        return Ok(());
    }

    println!("Proceeding with migration...\n");
    for migration in migrations {
        print!("Migrating {}... ", migration.description);
        if migration.target.exists() && !args.force {
            println!("SKIPPED (target exists, use --force to overwrite)");
            continue;
        }

        match migration.execute() {
            Ok(_) => println!("OK"),
            Err(err) => println!("FAILED: {}", err),
        }
    }

    println!("\n✅ Migration complete.");
    println!("\nYou can now safely remove old configuration directories:");
    println!(" - ~/.config/restflow/");
    println!(" - ~/Library/Application Support/com.restflow.app/ (macOS)");

    Ok(())
}

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

fn convert_toml_to_json(toml_bytes: &[u8]) -> Result<Vec<u8>> {
    let toml_str = std::str::from_utf8(toml_bytes)?;
    let value: toml::Value = toml::from_str(toml_str)?;
    let json = serde_json::to_string_pretty(&value)?;
    Ok(json.into_bytes())
}

fn convert_master_key(json_bytes: &[u8]) -> Result<Vec<u8>> {
    #[derive(serde::Deserialize)]
    struct OldFormat {
        key: String,
    }

    let old: OldFormat = serde_json::from_slice(json_bytes)?;
    Ok(old.key.into_bytes())
}
