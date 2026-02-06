use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::time::sleep;

use crate::api::client::JamfClient;
use crate::api::packages::PackageDigestSnapshot;
use crate::credentials;
use crate::models::package::PackageCreateRequest;

const DIGEST_POLL_ATTEMPTS: usize = 12;
const DIGEST_POLL_INTERVAL: Duration = Duration::from_secs(5);

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

    let previous_digest = client.get_package_digest_snapshot(&pkg_id).await?;
    match &previous_digest {
        Some(digest) => println!("Current package digest: {}", digest.display_line()),
        None => println!("Current package digest metadata is unavailable via API."),
    }

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
    println!("Inventory refresh requested.");

    if let Some(previous) = previous_digest.as_ref() {
        println!("Waiting for Jamf digest metadata to update...");
        let refreshed_digest = wait_for_digest_change(&client, &pkg_id, previous).await?;
        println!("Digest updated: {}", refreshed_digest.display_line());
    } else {
        let current_digest = client.get_package_digest_snapshot(&pkg_id).await?;
        if let Some(digest) = current_digest {
            println!("Current digest after upload: {}", digest.display_line());
        } else {
            println!("Digest metadata is still unavailable; skipping digest verification.");
        }
    }

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

async fn wait_for_digest_change(
    client: &JamfClient,
    package_id: &str,
    previous: &PackageDigestSnapshot,
) -> Result<PackageDigestSnapshot> {
    let mut latest_snapshot: Option<PackageDigestSnapshot> = None;

    for attempt in 1..=DIGEST_POLL_ATTEMPTS {
        match client.get_package_digest_snapshot(package_id).await? {
            Some(current) => {
                if current.differs_from(previous) {
                    return Ok(current);
                }

                latest_snapshot = Some(current);
                if attempt < DIGEST_POLL_ATTEMPTS {
                    println!(
                        "  Attempt {}/{}: digest unchanged, waiting {}s...",
                        attempt,
                        DIGEST_POLL_ATTEMPTS,
                        DIGEST_POLL_INTERVAL.as_secs()
                    );
                }
            }
            None => {
                if attempt < DIGEST_POLL_ATTEMPTS {
                    println!(
                        "  Attempt {}/{}: digest metadata unavailable, waiting {}s...",
                        attempt,
                        DIGEST_POLL_ATTEMPTS,
                        DIGEST_POLL_INTERVAL.as_secs()
                    );
                }
            }
        }

        if attempt < DIGEST_POLL_ATTEMPTS {
            sleep(DIGEST_POLL_INTERVAL).await;
        }
    }

    let previous_line = previous.display_line();
    if let Some(latest) = latest_snapshot {
        bail!(
            "Upload completed but Jamf digest metadata did not change after {} seconds. Previous digest: {}. Latest digest: {}. If you intentionally uploaded an identical file, this can be expected.",
            DIGEST_POLL_ATTEMPTS as u64 * DIGEST_POLL_INTERVAL.as_secs(),
            previous_line,
            latest.display_line()
        );
    }

    bail!(
        "Upload completed but Jamf digest metadata remained unavailable after {} seconds. Previous digest: {}.",
        DIGEST_POLL_ATTEMPTS as u64 * DIGEST_POLL_INTERVAL.as_secs(),
        previous_line
    );
}
