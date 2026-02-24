use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Refresh the token when it has less than this much time remaining.
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(30);

/// Fallback token lifetime when the server doesn't provide `expires_in`.
const DEFAULT_TOKEN_LIFETIME: Duration = Duration::from_secs(300);

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

struct TokenState {
    access_token: String,
    expires_at: Instant,
}

pub struct JamfClient {
    pub base_url: String,
    client_id: String,
    client_secret: String,
    token_state: RwLock<TokenState>,
    pub http: Client,
}

impl JamfClient {
    pub async fn connect(base_url: &str, client_id: &str, client_secret: &str) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(1800)) // 30 min for large uploads
            .build()
            .context("Failed to create HTTP client")?;

        let (access_token, expires_at) = Self::fetch_token(&http, base_url, client_id, client_secret).await?;

        Ok(Self {
            base_url: base_url.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            token_state: RwLock::new(TokenState { access_token, expires_at }),
            http,
        })
    }

    async fn fetch_token(http: &Client, base_url: &str, client_id: &str, client_secret: &str) -> Result<(String, Instant)> {
        let token_url = format!("{}/api/oauth/token", base_url);

        let resp = http
            .post(&token_url)
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await
            .context("Failed to reach Jamf Pro for authentication")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Authentication failed (HTTP {}): {}", status, body);
        }

        let token_resp: OAuthTokenResponse = resp
            .json()
            .await
            .context("Failed to parse authentication response")?;

        let lifetime = token_resp
            .expires_in
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TOKEN_LIFETIME);
        let expires_at = Instant::now() + lifetime;

        Ok((token_resp.access_token, expires_at))
    }

    /// Returns a valid bearer token, refreshing it if it is near expiry.
    pub async fn token(&self) -> Result<String> {
        // Fast path: token is still fresh.
        {
            let state = self.token_state.read().await;
            if Instant::now() + TOKEN_REFRESH_MARGIN < state.expires_at {
                return Ok(state.access_token.clone());
            }
        }

        // Slow path: refresh needed.
        let mut state = self.token_state.write().await;
        // Double-check after acquiring write lock (another task may have refreshed).
        if Instant::now() + TOKEN_REFRESH_MARGIN < state.expires_at {
            return Ok(state.access_token.clone());
        }

        let (access_token, expires_at) =
            Self::fetch_token(&self.http, &self.base_url, &self.client_id, &self.client_secret).await?;
        state.access_token = access_token.clone();
        state.expires_at = expires_at;
        Ok(access_token)
    }
}
