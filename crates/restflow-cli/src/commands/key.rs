use anyhow::{Result, anyhow, bail};
use comfy_table::{Cell, Table};
use restflow_core::auth::{
    AuthManagerConfig, AuthProfileManager, AuthProvider, Credential, CredentialSource,
    SecureCredential,
};
use restflow_core::AppCore;
use restflow_storage::AuthProfileStorage;
use std::sync::Arc;

use crate::cli::KeyCommands;
use crate::output::{OutputFormat, json::print_json};

pub async fn run(core: Arc<AppCore>, command: KeyCommands, format: OutputFormat) -> Result<()> {
    let manager = create_manager(core)?;
    manager.initialize().await?;

    match command {
        KeyCommands::Add { provider, key, name } => add_key(&manager, &provider, &key, name, format).await,
        KeyCommands::List { provider } => list_keys(&manager, provider.as_deref(), format).await,
        KeyCommands::Show { id } => show_key(&manager, &id, format).await,
        KeyCommands::Use { id } => use_key(&manager, &id, format).await,
        KeyCommands::Remove { id } => remove_key(&manager, &id, format).await,
        KeyCommands::Test { id } => test_key(&manager, &id, format).await,
        KeyCommands::Discover => discover_keys(&manager, format).await,
    }
}

fn create_manager(core: Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig::default();
    let secrets = Arc::new(core.storage.secrets.clone());
    let db = core.storage.get_db();
    let profile_storage = AuthProfileStorage::new(db)?;

    Ok(AuthProfileManager::with_storage(config, secrets, Some(profile_storage)))
}

async fn add_key(
    manager: &AuthProfileManager,
    provider: &str,
    key: &str,
    name: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let provider = parse_provider(provider)?;
    let display_name = name.unwrap_or_else(|| format!("{} key", provider));
    let credential = if key.starts_with("sk-ant-oat") {
        Credential::OAuth {
            access_token: key.to_string(),
            refresh_token: None,
            expires_at: None,
            email: None,
        }
    } else {
        Credential::ApiKey {
            key: key.to_string(),
            email: None,
        }
    };

    let id = manager
        .add_profile_from_credential(display_name, credential, CredentialSource::Manual, provider)
        .await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id }));
    }

    println!("Added key: {}", short_id(&id));
    Ok(())
}

async fn list_keys(
    manager: &AuthProfileManager,
    provider: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let mut profiles = manager.list_profiles().await;
    if let Some(provider) = provider {
        let provider = parse_provider(provider)?;
        profiles.retain(|profile| profile.provider == provider);
    }

    profiles.sort_by(|a, b| {
        a.provider
            .to_string()
            .cmp(&b.provider.to_string())
            .then_with(|| a.name.cmp(&b.name))
    });

    if format.is_json() {
        let items: Vec<_> = profiles
            .iter()
            .map(|profile| {
                serde_json::json!({
                    "id": short_id(&profile.id),
                    "name": &profile.name,
                    "provider": profile.provider.to_string(),
                    "type": credential_type(&profile.credential),
                    "available": profile.is_available(),
                })
            })
            .collect();
        return print_json(&items);
    }

    let mut table = Table::new();
    table.set_header(vec!["ID", "Name", "Provider", "Type", "Available", "Priority"]);

    for profile in &profiles {
        table.add_row(vec![
            Cell::new(short_id(&profile.id)),
            Cell::new(&profile.name),
            Cell::new(profile.provider.to_string()),
            Cell::new(credential_type(&profile.credential)),
            Cell::new(format_available(profile.is_available())),
            Cell::new(profile.priority),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_key(
    manager: &AuthProfileManager,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved_id = resolve_profile_id(manager, id).await?;
    let profile = manager
        .get_profile(&resolved_id)
        .await
        .ok_or_else(|| anyhow!("Profile not found: {}", id))?;

    if format.is_json() {
        return print_json(&serde_json::json!({
            "id": profile.id,
            "short_id": short_id(&profile.id),
            "name": profile.name,
            "provider": profile.provider.to_string(),
            "type": credential_type(&profile.credential),
            "enabled": profile.enabled,
            "available": profile.is_available(),
            "priority": profile.priority,
            "health": format!("{:?}", profile.health),
            "source": profile.source.to_string(),
        }));
    }

    println!("ID:           {}", profile.id);
    println!("Name:         {}", profile.name);
    println!("Provider:     {}", profile.provider);
    println!("Type:         {}", credential_type(&profile.credential));
    println!("Source:       {}", profile.source);
    println!("Health:       {:?}", profile.health);
    println!("Enabled:      {}", profile.enabled);
    println!("Available:    {}", format_available(profile.is_available()));
    println!("Priority:     {}", profile.priority);
    println!("Created:      {}", profile.created_at);

    if let Some(last_used) = profile.last_used_at {
        println!("Last used:    {}", last_used);
    }

    if let Some(last_failed) = profile.last_failed_at {
        println!("Last failed:  {}", last_failed);
    }

    if let Some(cooldown_until) = profile.cooldown_until {
        println!("Cooldown:     {}", cooldown_until);
    }

    Ok(())
}

async fn use_key(
    manager: &AuthProfileManager,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved_id = resolve_profile_id(manager, id).await?;
    let profile = manager
        .get_profile(&resolved_id)
        .await
        .ok_or_else(|| anyhow!("Profile not found: {}", id))?;

    manager
        .update_profile(&profile.id, restflow_core::auth::ProfileUpdate {
            priority: Some(0),
            ..Default::default()
        })
        .await?;

    let all_profiles = manager.list_profiles().await;
    for other in all_profiles {
        if other.provider == profile.provider && other.id != profile.id {
            let new_priority = if other.priority < 1 { 1 } else { other.priority };
            if new_priority != other.priority {
                manager
                    .update_profile(
                        &other.id,
                        restflow_core::auth::ProfileUpdate {
                            priority: Some(new_priority),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
        }
    }

    if format.is_json() {
        return print_json(&serde_json::json!({
            "id": profile.id,
            "default": true
        }));
    }

    println!(
        "Set {} as default for {}",
        short_id(&profile.id),
        profile.provider
    );
    Ok(())
}

async fn remove_key(
    manager: &AuthProfileManager,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved_id = resolve_profile_id(manager, id).await?;
    let removed = manager.remove_profile(&resolved_id).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "deleted": true, "id": removed.id }));
    }

    println!("Key removed: {} ({})", removed.name, short_id(&removed.id));
    Ok(())
}

async fn test_key(
    manager: &AuthProfileManager,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved_id = resolve_profile_id(manager, id).await?;
    let profile = manager
        .get_profile(&resolved_id)
        .await
        .ok_or_else(|| anyhow!("Profile not found: {}", id))?;

    manager.resolver().validate(&profile.credential)?;

    if format.is_json() {
        return print_json(&serde_json::json!({
            "id": profile.id,
            "valid": true
        }));
    }

    println!("Key {} is available for {}", short_id(&profile.id), profile.provider);
    Ok(())
}

async fn discover_keys(manager: &AuthProfileManager, format: OutputFormat) -> Result<()> {
    let summary = manager.discover().await?;

    if format.is_json() {
        return print_json(&summary);
    }

    println!("Discovery complete!");
    println!("  Found: {} profiles", summary.total);
    println!("  Available: {}", summary.available);
    if !summary.errors.is_empty() {
        println!("  Errors: {}", summary.errors.len());
    }

    Ok(())
}

fn parse_provider(value: &str) -> Result<AuthProvider> {
    match value.to_lowercase().as_str() {
        "anthropic" | "claude" => Ok(AuthProvider::Anthropic),
        "claude-code" | "claudecode" | "cc" => Ok(AuthProvider::ClaudeCode),
        "openai" | "gpt" => Ok(AuthProvider::OpenAI),
        "openai-codex" | "openai_codex" | "codex" => Ok(AuthProvider::OpenAICodex),
        "deepseek" => Ok(AuthProvider::Other),
        _ => bail!(
            "Unknown provider: {value}. Use: anthropic, claude-code, openai, openai-codex, deepseek"
        ),
    }
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect::<String>()
}

fn credential_type(credential: &SecureCredential) -> String {
    match credential {
        SecureCredential::ApiKey { .. } => "API Key".to_string(),
        SecureCredential::OAuth { .. } => "OAuth".to_string(),
        SecureCredential::Token { .. } => "Token".to_string(),
    }
}

fn format_available(value: bool) -> String {
    if value { "yes".to_string() } else { "no".to_string() }
}

async fn resolve_profile_id(manager: &AuthProfileManager, id: &str) -> Result<String> {
    let profiles = manager.list_profiles().await;

    if profiles.iter().any(|profile| profile.id == id) {
        return Ok(id.to_string());
    }

    let matches: Vec<_> = profiles
        .iter()
        .filter(|profile| profile.id.starts_with(id))
        .collect();

    if matches.is_empty() {
        bail!("Profile not found: {id}");
    }

    if matches.len() > 1 {
        bail!("Profile id '{id}' is ambiguous");
    }

    Ok(matches[0].id.clone())
}
