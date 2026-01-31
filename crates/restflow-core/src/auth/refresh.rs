//! OAuth token refreshers.

use crate::auth::{AuthProvider, Credential};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::Deserialize;

const ANTHROPIC_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const ANTHROPIC_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";

#[derive(Debug, Clone)]
pub struct RefreshedCredential {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait OAuthRefresher: Send + Sync {
    fn provider(&self) -> AuthProvider;

    async fn refresh(&self, credential: &Credential) -> Result<RefreshedCredential>;
}

#[derive(Debug, Clone)]
pub struct AnthropicRefresher {
    client: Client,
}

impl Default for AnthropicRefresher {
    fn default() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicRefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

#[async_trait]
impl OAuthRefresher for AnthropicRefresher {
    fn provider(&self) -> AuthProvider {
        AuthProvider::Anthropic
    }

    async fn refresh(&self, credential: &Credential) -> Result<RefreshedCredential> {
        let refresh_token = match credential {
            Credential::OAuth {
                refresh_token: Some(token),
                ..
            } => token,
            _ => {
                return Err(anyhow!(
                    "OAuth credential missing refresh token for provider {}",
                    self.provider()
                ));
            }
        };

        let response = self
            .client
            .post(ANTHROPIC_TOKEN_URL)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", ANTHROPIC_CLIENT_ID),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .context("Failed to send refresh token request")?
            .error_for_status()
            .context("Refresh token request failed")?
            .json::<AnthropicRefreshResponse>()
            .await
            .context("Failed to parse refresh token response")?;

        let expires_at = response
            .expires_in
            .map(|seconds| Utc::now() + Duration::seconds(seconds));

        Ok(RefreshedCredential {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_at,
        })
    }
}
