use anyhow::{anyhow, bail, Result};
use comfy_table::{Cell, Table};
use restflow_core::auth::{
    AuthManagerConfig, AuthProfile, AuthProfileManager, AuthProvider, Credential, CredentialSource,
};
use restflow_core::paths;

use crate::cli::AuthCommands;
use crate::output::{json::print_json, OutputFormat};

pub async fn run(command: AuthCommands, format: OutputFormat) -> Result<()> {
    let manager = create_manager()?;
    manager.initialize().await?;

    match command {
        AuthCommands::Status => status(&manager, format).await,
        AuthCommands::Discover => discover(&manager, format).await,
        AuthCommands::List => list_profiles(&manager, format).await,
        AuthCommands::Show { id } => show_profile(&manager, &id, format).await,
        AuthCommands::Add {
            provider,
            key,
            name,
        } => add_profile(&manager, &provider, &key, name, format).await,
        AuthCommands::Remove { id } => remove_profile(&manager, &id, format).await,
    }
}

fn create_manager() -> Result<AuthProfileManager> {
    let mut config = AuthManagerConfig::default();
    let profiles_path = paths::ensure_data_dir()?.join("auth_profiles.json");
    config.profiles_path = Some(profiles_path);
    Ok(AuthProfileManager::with_config(config))
}

async fn status(manager: &AuthProfileManager, format: OutputFormat) -> Result<()> {
    let summary = manager.get_summary().await;

    if format.is_json() {
        return print_json(&summary);
    }

    println!("Authentication Status");
    println!("=====================");
    println!("Total profiles:   {}", summary.total);
    println!("Enabled:          {}", summary.enabled);
    println!("Available:        {}", summary.available);
    println!("In cooldown:      {}", summary.in_cooldown);
    println!("Disabled:         {}", summary.disabled);
    println!();
    println!("By Provider:");
    for (provider, count) in summary.by_provider {
        println!("  {provider}: {count}");
    }
    println!();
    println!("By Source:");
    for (source, count) in summary.by_source {
        println!("  {source}: {count}");
    }

    Ok(())
}

async fn discover(manager: &AuthProfileManager, format: OutputFormat) -> Result<()> {
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

async fn list_profiles(manager: &AuthProfileManager, format: OutputFormat) -> Result<()> {
    let mut profiles = manager.list_profiles().await;
    profiles.sort_by(|a, b| {
        a.provider
            .to_string()
            .cmp(&b.provider.to_string())
            .then_with(|| a.name.cmp(&b.name))
    });

    if format.is_json() {
        return print_json(&profiles);
    }

    let mut table = Table::new();
    table.set_header(vec![
        "ID",
        "Name",
        "Provider",
        "Source",
        "Health",
        "Available",
    ]);

    for profile in &profiles {
        table.add_row(vec![
            Cell::new(short_id(&profile.id)),
            Cell::new(&profile.name),
            Cell::new(profile.provider.to_string()),
            Cell::new(profile.source.to_string()),
            Cell::new(format_health(&profile.health)),
            Cell::new(format_available(profile.is_available())),
        ]);
    }

    crate::output::table::print_table(table)
}

async fn show_profile(manager: &AuthProfileManager, id: &str, format: OutputFormat) -> Result<()> {
    let resolved_id = resolve_profile_id(manager, id).await?;
    let profile = manager
        .get_profile(&resolved_id)
        .await
        .ok_or_else(|| anyhow!("Profile not found: {}", id))?;

    if format.is_json() {
        return print_json(&profile);
    }

    println!("ID:           {}", profile.id);
    println!("Name:         {}", profile.name);
    println!("Provider:     {}", profile.provider);
    println!("Source:       {}", profile.source);
    println!("Health:       {}", format_health(&profile.health));
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

    println!("Credential:   {}", format_credential(&profile.credential));

    if let Some(email) = profile.credential.get_email() {
        println!("Email:        {}", email);
    }

    Ok(())
}

async fn add_profile(
    manager: &AuthProfileManager,
    provider: &str,
    key: &str,
    name: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let provider = parse_provider(provider)?;
    let display_name = name.unwrap_or_else(|| format!("{} (manual)", provider));
    let credential = Credential::ApiKey {
        key: key.to_string(),
        email: None,
    };
    let profile = AuthProfile::new(display_name, credential, CredentialSource::Manual, provider);
    let id = manager.add_profile(profile).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id }));
    }

    println!("Profile added: {id}");
    Ok(())
}

async fn remove_profile(
    manager: &AuthProfileManager,
    id: &str,
    format: OutputFormat,
) -> Result<()> {
    let resolved_id = resolve_profile_id(manager, id).await?;
    let removed = manager.remove_profile(&resolved_id).await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "deleted": true, "id": removed.id }));
    }

    println!("Profile removed: {} ({})", removed.name, removed.id);
    Ok(())
}

fn parse_provider(value: &str) -> Result<AuthProvider> {
    match value.to_lowercase().as_str() {
        "anthropic" => Ok(AuthProvider::Anthropic),
        "claude-code" | "claudecode" => Ok(AuthProvider::ClaudeCode),
        "openai" => Ok(AuthProvider::OpenAI),
        "google" | "gemini" => Ok(AuthProvider::Google),
        "other" => Ok(AuthProvider::Other),
        _ => bail!("Unsupported provider: {value}. Valid options: anthropic, claude-code, openai, google, other"),
    }
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect::<String>()
}

fn format_health(health: &restflow_core::auth::ProfileHealth) -> String {
    format!("{health:?}")
}

fn format_available(value: bool) -> String {
    if value {
        "yes".to_string()
    } else {
        "no".to_string()
    }
}

fn format_credential(credential: &Credential) -> String {
    match credential {
        Credential::ApiKey { .. } => format!("API key ({})", credential.masked()),
        Credential::Token { expires_at, .. } => match expires_at {
            Some(expiry) => format!("Token ({}, expires {})", credential.masked(), expiry),
            None => format!("Token ({})", credential.masked()),
        },
        Credential::OAuth { expires_at, .. } => match expires_at {
            Some(expiry) => format!("OAuth ({}, expires {})", credential.masked(), expiry),
            None => format!("OAuth ({})", credential.masked()),
        },
    }
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
