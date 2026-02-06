use anyhow::{Context, Result};
use std::env;

const SERVICE: &str = "jamf-package-updater";

pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub url: String,
}

pub fn store_credentials(client_id: &str, client_secret: &str, url: &str) -> Result<()> {
    let url = url.trim_end_matches('/');

    keyring::Entry::new(SERVICE, "client_id")
        .context("Failed to create keyring entry for client_id")?
        .set_password(client_id)
        .context("Failed to store client_id in keyring")?;

    keyring::Entry::new(SERVICE, "client_secret")
        .context("Failed to create keyring entry for client_secret")?
        .set_password(client_secret)
        .context("Failed to store client_secret in keyring")?;

    keyring::Entry::new(SERVICE, "url")
        .context("Failed to create keyring entry for url")?
        .set_password(url)
        .context("Failed to store url in keyring")?;

    Ok(())
}

pub fn load_credentials() -> Result<Credentials> {
    // Try environment variables first (for CI / GitHub Actions)
    if let (Ok(client_id), Ok(client_secret), Ok(url)) = (
        env::var("JAMF_CLIENT_ID"),
        env::var("JAMF_CLIENT_SECRET"),
        env::var("JAMF_URL"),
    ) {
        return Ok(Credentials {
            client_id,
            client_secret,
            url: url.trim_end_matches('/').to_string(),
        });
    }

    // Fall back to keyring
    let client_id = keyring::Entry::new(SERVICE, "client_id")
        .context("Failed to access keyring")?
        .get_password()
        .context("No credentials found. Run `jamf-package-updater auth` first or set JAMF_CLIENT_ID, JAMF_CLIENT_SECRET, JAMF_URL environment variables.")?;

    let client_secret = keyring::Entry::new(SERVICE, "client_secret")
        .context("Failed to access keyring")?
        .get_password()
        .context("client_secret not found in keyring")?;

    let url = keyring::Entry::new(SERVICE, "url")
        .context("Failed to access keyring")?
        .get_password()
        .context("url not found in keyring")?;

    Ok(Credentials {
        client_id,
        client_secret,
        url,
    })
}
