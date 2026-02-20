use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use md5::{Digest, Md5};
use tokio::io::AsyncReadExt;
use tokio::time::sleep;

use crate::api::client::JamfClient;
use crate::api::packages::PackageDigestSnapshot;
use crate::credentials;
use crate::models::package::PackageCreateRequest;

const DIGEST_POLL_ATTEMPTS: usize = 12;
const DIGEST_POLL_INTERVAL: Duration = Duration::from_secs(5);

pub async fn run(path: &Path, name: Option<&str>, priority: Option<i32>) -> Result<()> {
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

    // 4. Find existing package — or create a new record if it doesn't exist yet
    println!("Searching for package '{}'...", package_name);
    let (package, is_new) = match client.find_package(&package_name).await? {
        Some(pkg) => {
            println!(
                "Found package '{}' (ID: {}, file: {})",
                package_name, pkg.id, pkg.file_name
            );
            (pkg, false)
        }
        None => {
            println!("Package not found — creating new package record...");
            let req = PackageCreateRequest::new_default(&package_name, &file_name, priority);
            let created = client.create_package(&req).await?;
            println!("Created package '{}' (ID: {}).", package_name, created.id);
            let pkg_id = created.id;
            // The create endpoint only returns an id+href; build a minimal
            // Package from the request data so the rest of the flow works.
            let pkg = crate::models::package::Package {
                id: pkg_id,
                package_name: req.package_name,
                file_name: req.file_name,
                category_id: req.category_id,
                priority: req.priority,
                fill_user_template: req.fill_user_template,
                fill_existing_users: req.fill_existing_users,
                reboot_required: req.reboot_required,
                os_install: req.os_install,
                suppress_updates: req.suppress_updates,
                suppress_from_dock: req.suppress_from_dock,
                suppress_eula: req.suppress_eula,
                suppress_registration: req.suppress_registration,
            };
            (pkg, true)
        }
    };

    let pkg_id = package.id.clone();

    // For existing packages: check digest, skip if unchanged, scan policies, update metadata.
    // For new packages: skip all of this — there is no existing payload or policy reference.
    let previous_digest: Option<PackageDigestSnapshot> = if !is_new {
        let digest = client.get_package_digest_snapshot(&pkg_id).await?;
        match &digest {
            Some(d) => println!("Current package digest: {}", d.display_line()),
            None => println!("Current package digest metadata is unavailable via API."),
        }

        // Exit early when Jamf already has the same payload (MD5 match).
        if let Some(remote_md5) = digest.as_ref().and_then(|d| d.md5_hash.as_deref()) {
            let local_md5 = compute_file_md5(path).await?;
            println!("Local file MD5: {}", local_md5);
            if remote_md5.eq_ignore_ascii_case(&local_md5) {
                println!("Package payload already matches Jamf (MD5 unchanged).");
                println!(
                    "Package '{}' (ID: {}) is already up to date. Skipping update.",
                    package_name, pkg_id
                );
                return Ok(());
            }
        }

        // Scan policies for references to this package
        println!("Scanning policies...");
        let affected_policies = client
            .find_policies_with_package(&package_name, &package.file_name)
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

        // Update package metadata in-place (keep same ID, update fileName)
        println!("Updating package metadata...");
        let update_req = PackageCreateRequest::from_old(&package, &file_name, priority);
        client.update_package(&pkg_id, &update_req).await?;
        println!("Metadata updated.");

        digest
    } else {
        None
    };

    // Upload the file
    println!("Uploading {}...", file_name);
    client.upload_package(&pkg_id, path).await?;
    println!("Upload complete.");

    // Refresh JCDS inventory to recalculate checksums
    println!("Refreshing package inventory (recalculating checksums)...");
    client.refresh_jcds_inventory().await?;
    println!("Inventory refresh requested.");

    if let Some(previous) = previous_digest.as_ref() {
        println!("Waiting for Jamf digest metadata to update...");
        match wait_for_digest_change(&client, &pkg_id, previous).await {
            Ok(refreshed_digest) => {
                println!("Digest updated: {}", refreshed_digest.display_line());
            }
            Err(_) => {
                // Digest didn't change — check whether the remote now matches
                // the local file.  Rebuilds from identical source often produce
                // files with different outer MD5s but identical payload content,
                // so Jamf's stored digest stays the same.  Treat this as
                // success when the remote MD5 matches the file we just uploaded.
                let local_md5 = compute_file_md5(path).await?;
                let remote_md5 = client
                    .get_package_digest_snapshot(&pkg_id)
                    .await?
                    .and_then(|d| d.md5_hash);

                if remote_md5
                    .as_deref()
                    .is_some_and(|r| r.eq_ignore_ascii_case(&local_md5))
                {
                    println!(
                        "Digest unchanged but remote MD5 matches the uploaded file — content is identical."
                    );
                } else {
                    bail!(
                        "Upload completed but Jamf digest metadata did not update \
                         after {} seconds and the remote MD5 ({}) does not match the \
                         local file MD5 ({}). Previous digest: {}.",
                        DIGEST_POLL_ATTEMPTS as u64 * DIGEST_POLL_INTERVAL.as_secs(),
                        remote_md5.as_deref().unwrap_or("unavailable"),
                        local_md5,
                        previous.display_line()
                    );
                }
            }
        }
    } else {
        println!("Waiting for Jamf digest metadata to become available...");
        let digest = wait_for_digest_availability(&client, &pkg_id).await?;
        println!("Digest updated: {}", digest.display_line());
    }

    println!("Inventory refreshed.");

    if is_new {
        println!(
            "Package '{}' (ID: {}) created and uploaded successfully.",
            package_name, pkg_id
        );
    } else {
        println!(
            "Package '{}' (ID: {}) updated successfully.",
            package_name, pkg_id
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
                if current.content_updated_from(previous) {
                    return Ok(current);
                }

                latest_snapshot = Some(current);
                if attempt < DIGEST_POLL_ATTEMPTS {
                    println!(
                        "  Attempt {}/{}: digest value not updated yet, waiting {}s...",
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

async fn wait_for_digest_availability(
    client: &JamfClient,
    package_id: &str,
) -> Result<PackageDigestSnapshot> {
    let mut latest_snapshot: Option<PackageDigestSnapshot> = None;

    for attempt in 1..=DIGEST_POLL_ATTEMPTS {
        match client.get_package_digest_snapshot(package_id).await? {
            Some(current) => {
                if current.has_verifiable_content() {
                    return Ok(current);
                }

                latest_snapshot = Some(current);
                if attempt < DIGEST_POLL_ATTEMPTS {
                    println!(
                        "  Attempt {}/{}: digest fields incomplete, waiting {}s...",
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

    if let Some(latest) = latest_snapshot {
        bail!(
            "Upload completed but Jamf digest fields remained incomplete after {} seconds. Latest digest: {}.",
            DIGEST_POLL_ATTEMPTS as u64 * DIGEST_POLL_INTERVAL.as_secs(),
            latest.display_line()
        );
    }

    bail!(
        "Upload completed but Jamf digest metadata remained unavailable after {} seconds.",
        DIGEST_POLL_ATTEMPTS as u64 * DIGEST_POLL_INTERVAL.as_secs()
    );
}

async fn compute_file_md5(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("Failed to open file for MD5: {}", path.display()))?;
    let mut hasher = Md5::new();
    let mut buf = [0_u8; 8192];

    loop {
        let n = file
            .read(&mut buf)
            .await
            .with_context(|| format!("Failed reading file for MD5: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
