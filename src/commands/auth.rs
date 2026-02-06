use anyhow::Result;

use crate::credentials;

pub fn run(client_id: &str, client_secret: &str, url: &str) -> Result<()> {
    credentials::store_credentials(client_id, client_secret, url)?;
    println!("Credentials stored successfully.");
    Ok(())
}
