use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::api::client::JamfClient;
use crate::credentials;
use crate::models::package::PackageCreateRequest;

pub async fn run(path: &Path, name: Option<&str>) -> Result<()> {
    // 1. Resolve package name
    let file_name = path
        .file_name()
        .context("Invalid file path")?
        .to_string_lossy()
        .to_string();

    let package_name = match name {
        Some(n) => n.to_string(),
        None => path
            .file_stem()
            .context("Cannot determine package name from file path")?
            .to_string_lossy()
            .to_string(),
    };

    // Validate file extension
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext != "pkg" && ext != "dmg" {
        bail!("File must be a .pkg or .dmg (got .{})", ext);
    }

    if !path.exists() {
        bail!("File not found: {}", path.display());
    }

    println!("Package name: {}", package_name);
    println!("File: {}", path.display());

    // 2. Load credentials
    let creds = credentials::load_credentials()?;
    println!("Jamf Pro URL: {}", creds.url);

    // 3. Authenticate
    println!("Authenticating...");
    let client = JamfClient::connect(&creds.url, &creds.client_id, &creds.client_secret).await?;
    println!("Authenticated.");

    // 4. Find existing package
    println!("Searching for package '{}'...", package_name);
    let old_package = client
        .find_package(&package_name)
        .await?
        .with_context(|| format!("Package '{}' not found in Jamf Pro", package_name))?;

    let pkg_id = old_package.id.clone();
    println!(
        "Found package '{}' (ID: {}, file: {})",
        package_name, pkg_id, old_package.file_name
    );

    // 5. Scan policies for references to this package
    println!("Scanning policies...");
    let affected_policies = client
        .find_policies_with_package(&package_name, &old_package.file_name)
        .await?;
    println!(
        "Found {} {} referencing this package.",
        affected_policies.len(),
        if affected_policies.len() == 1 {
            "policy"
        } else {
            "policies"
        }
    );
    for p in &affected_policies {
        println!("  - {} (ID: {})", p.name, p.id);
    }

    // 6. Update package metadata in-place (keep same ID, update fileName)
    println!("Updating package metadata...");
    let update_req = PackageCreateRequest::from_old(&old_package, &file_name);
    client.update_package(&pkg_id, &update_req).await?;
    println!("Metadata updated.");

    // 7. Upload the new file
    println!("Uploading {}...", file_name);
    client.upload_package(&pkg_id, path).await?;
    println!("Upload complete.");

    // 8. Refresh JCDS inventory to recalculate checksums
    println!("Refreshing package inventory (recalculating checksums)...");
    client.refresh_jcds_inventory().await?;
    println!("Inventory refreshed.");

    println!(
        "Package '{}' (ID: {}) updated successfully.",
        package_name, pkg_id
    );
    if !affected_policies.is_empty() {
        println!(
            "{} {} will automatically use the new package.",
            affected_policies.len(),
            if affected_policies.len() == 1 {
                "policy"
            } else {
                "policies"
            }
        );
    }

    Ok(())
}
