use anyhow::{Result, anyhow, bail};
use comfy_table::{Cell, Table};
use restflow_core::AppCore;
use restflow_core::auth::{
    AuthManagerConfig, AuthProfileManager, AuthProvider, Credential, CredentialSource,
    ManagerSummary, ProfileHealth, SecureCredential,
};
use restflow_core::daemon::{IpcClient, is_daemon_available};
use restflow_core::paths;
use restflow_storage::AuthProfileStorage;
use std::path::Path;
use std::sync::Arc;

use crate::cli::AuthCommands;
use crate::output::{OutputFormat, json::print_json};

pub async fn run(core: Arc<AppCore>, command: AuthCommands, format: OutputFormat) -> Result<()> {
    let socket_path = paths::socket_path()?;
    if is_daemon_available(&socket_path).await {
        return run_ipc(&socket_path, command, format).await;
    }

    let manager = create_manager(core)?;
    if let Ok(data_dir) = paths::ensure_restflow_dir() {
        let old_json = data_dir.join("auth_profiles.json");
        if let Err(e) = manager.migrate_from_json(&old_json).await {
            tracing::warn!(error = %e, "Failed to migrate auth profiles from JSON");
        }
    }
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

async fn run_ipc(socket_path: &Path, command: AuthCommands, format: OutputFormat) -> Result<()> {
    let mut client = IpcClient::connect(socket_path).await?;

    match command {
        AuthCommands::Status => status_ipc(&mut client, format).await,
        AuthCommands::Discover => discover_ipc(&mut client, format).await,
        AuthCommands::List => list_profiles_ipc(&mut client, format).await,
        AuthCommands::Show { id } => show_profile_ipc(&mut client, &id, format).await,
        AuthCommands::Add {
            provider,
            key,
            name,
        } => add_profile_ipc(&mut client, &provider, &key, name, format).await,
        AuthCommands::Remove { id } => remove_profile_ipc(&mut client, &id, format).await,
    }
}

fn create_manager(core: Arc<AppCore>) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig::default();
    let secrets = Arc::new(core.storage.secrets.clone());
    let profile_storage = AuthProfileStorage::new(core.storage.get_db())?;

    Ok(AuthProfileManager::with_storage(
        config,
        secrets,
        Some(profile_storage),
    ))
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

async fn status_ipc(client: &mut IpcClient, format: OutputFormat) -> Result<()> {
    let profiles = client.list_auth_profiles().await?;
    let summary = summary_from_profiles(&profiles);

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

async fn discover_ipc(client: &mut IpcClient, format: OutputFormat) -> Result<()> {
    let summary = client.discover_auth().await?;

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

async fn list_profiles_ipc(client: &mut IpcClient, format: OutputFormat) -> Result<()> {
    let mut profiles = client.list_auth_profiles().await?;
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

    println!(
        "Credential:   {}",
        format_secure_credential(&profile.credential)
    );

    if let Some(email) = profile.credential.get_email() {
        println!("Email:        {}", email);
    }

    Ok(())
}

async fn show_profile_ipc(client: &mut IpcClient, id: &str, format: OutputFormat) -> Result<()> {
    let resolved_id = resolve_profile_id_ipc(client, id).await?;
    let profile = client.get_auth_profile(resolved_id).await?;

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

    println!(
        "Credential:   {}",
        format_secure_credential(&profile.credential)
    );

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
    let id = manager
        .add_profile_from_credential(display_name, credential, CredentialSource::Manual, provider)
        .await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": id }));
    }

    println!("Profile added: {id}");
    Ok(())
}

async fn add_profile_ipc(
    client: &mut IpcClient,
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
    let profile = client
        .add_auth_profile(display_name, credential, CredentialSource::Manual, provider)
        .await?;

    if format.is_json() {
        return print_json(&serde_json::json!({ "id": profile.id }));
    }

    println!("Profile added: {}", profile.id);
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

async fn remove_profile_ipc(client: &mut IpcClient, id: &str, format: OutputFormat) -> Result<()> {
    let resolved_id = resolve_profile_id_ipc(client, id).await?;
    let removed = client.remove_auth_profile(resolved_id).await?;

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
        "openai-codex" | "openai_codex" | "codex" => Ok(AuthProvider::OpenAICodex),
        "google" | "gemini" => Ok(AuthProvider::Google),
        "other" => Ok(AuthProvider::Other),
        _ => bail!(
            "Unsupported provider: {value}. Valid options: anthropic, claude-code, openai, openai-codex, google, other"
        ),
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

fn format_secure_credential(credential: &SecureCredential) -> String {
    match credential {
        SecureCredential::ApiKey { secret_ref, .. } => format!("API key (ref: {})", secret_ref),
        SecureCredential::Token {
            secret_ref,
            expires_at,
            ..
        } => match expires_at {
            Some(expiry) => format!("Token (ref: {}, expires {})", secret_ref, expiry),
            None => format!("Token (ref: {})", secret_ref),
        },
        SecureCredential::OAuth {
            access_token_ref,
            expires_at,
            ..
        } => match expires_at {
            Some(expiry) => format!("OAuth (ref: {}, expires {})", access_token_ref, expiry),
            None => format!("OAuth (ref: {})", access_token_ref),
        },
    }
}

fn summary_from_profiles(profiles: &[restflow_core::auth::AuthProfile]) -> ManagerSummary {
    let total = profiles.len();
    let enabled = profiles.iter().filter(|p| p.enabled).count();
    let available = profiles.iter().filter(|p| p.is_available()).count();
    let in_cooldown = profiles
        .iter()
        .filter(|p| p.health == ProfileHealth::Cooldown)
        .count();
    let disabled = profiles
        .iter()
        .filter(|p| p.health == ProfileHealth::Disabled)
        .count();

    let mut by_provider = std::collections::HashMap::new();
    let mut by_source = std::collections::HashMap::new();

    for profile in profiles {
        *by_provider.entry(profile.provider.to_string()).or_insert(0) += 1;
        *by_source.entry(profile.source.to_string()).or_insert(0) += 1;
    }

    ManagerSummary {
        total,
        enabled,
        available,
        in_cooldown,
        disabled,
        by_provider,
        by_source,
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

async fn resolve_profile_id_ipc(client: &mut IpcClient, id: &str) -> Result<String> {
    let profiles = client.list_auth_profiles().await?;

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
