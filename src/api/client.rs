use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

pub struct JamfClient {
    pub base_url: String,
    pub token: String,
    pub http: Client,
}

impl JamfClient {
    pub async fn connect(base_url: &str, client_id: &str, client_secret: &str) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(1800)) // 30 min for large uploads
            .build()
            .context("Failed to create HTTP client")?;

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

        Ok(Self {
            base_url: base_url.to_string(),
            token: token_resp.access_token,
            http,
        })
    }
}
