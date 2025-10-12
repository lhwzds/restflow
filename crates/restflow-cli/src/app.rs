use anyhow::Result;
use restflow_core::{
    AppCore,
    node::agent::{AgentNode, ApiKeyConfig},
    paths, services,
};
use std::sync::Arc;

/// Build the embedded RestFlow core and ensure an OpenAI API key is available.
pub async fn prepare_core() -> Result<Arc<AppCore>> {
    let core = init_core().await?;
    ensure_api_key(&core).await?;
    Ok(core)
}

async fn init_core() -> Result<Arc<AppCore>> {
    let db_path = paths::ensure_database_path_string()?;
    Ok(Arc::new(AppCore::new(&db_path).await?))
}

async fn ensure_api_key(core: &Arc<AppCore>) -> Result<()> {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return Ok(());
    }

    if services::secrets::has_secret(core, "OPENAI_API_KEY").await? {
        return Ok(());
    }

    println!("OpenAI API key is required. It will be encrypted and stored locally.\n");

    let key = loop {
        let input = rpassword::prompt_password("API Key (sk-...): ")?;
        if input.starts_with("sk-") && input.len() > 20 {
            break input;
        }
        println!("Invalid format, please try again.");
    };

    println!("Validating API key...");

    let test_agent = AgentNode::new(
        "gpt-4o-mini".to_string(),
        "You are a test assistant.".to_string(),
        Some(0.1),
        Some(ApiKeyConfig::Direct(key.clone())),
    );

    match test_agent.execute("Say 'OK'", None).await {
        Ok(_) => {
            services::secrets::set_secret(
                core,
                "OPENAI_API_KEY",
                &key,
                Some("OpenAI API Key for RestFlow CLI".to_string()),
            )
            .await?;
            println!("API key verified and saved.\n");
            Ok(())
        }
        Err(e) => anyhow::bail!("API key validation failed: {}", e),
    }
}
