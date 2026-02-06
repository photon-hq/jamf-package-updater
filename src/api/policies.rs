use anyhow::{bail, Context, Result};

use crate::api::client::JamfClient;
use crate::models::policy::{AffectedPolicy, PolicyListResponse};

impl JamfClient {
    /// Fetch the list of all policy IDs and names.
    pub async fn list_policies(&self) -> Result<Vec<(i64, String)>> {
        let url = format!("{}/JSSResource/policies", self.base_url);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to list policies")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to list policies (HTTP {}): {}", status, body);
        }

        let list: PolicyListResponse = resp
            .json()
            .await
            .context("Failed to parse policy list response")?;

        Ok(list
            .policies
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect())
    }

    /// Fetch the full XML for a single policy.
    pub async fn get_policy_xml(&self, id: i64) -> Result<String> {
        let url = format!("{}/JSSResource/policies/id/{}", self.base_url, id);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/xml")
            .send()
            .await
            .with_context(|| format!("Failed to fetch policy {}", id))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to fetch policy {} (HTTP {}): {}", id, status, body);
        }

        resp.text()
            .await
            .with_context(|| format!("Failed to read policy {} body", id))
    }

    /// Find all policies that reference a package by packageName or fileName.
    /// The policy XML <name> field may contain either the display name or the file name.
    pub async fn find_policies_with_package(
        &self,
        package_name: &str,
        file_name: &str,
    ) -> Result<Vec<AffectedPolicy>> {
        let policies = self.list_policies().await?;
        let total = policies.len();
        let mut affected = Vec::new();

        for (i, (id, name)) in policies.iter().enumerate() {
            eprint!("\r  Scanning policy {}/{}...", i + 1, total);

            let xml = self.get_policy_xml(*id).await?;

            if let Some(pkg_config) = extract_section(&xml, "package_configuration") {
                let matches = pkg_config.contains(&format!("<name>{}</name>", package_name))
                    || pkg_config.contains(&format!("<name>{}</name>", file_name));

                if matches {
                    affected.push(AffectedPolicy {
                        id: *id,
                        name: name.clone(),
                    });
                }
            }
        }
        eprintln!(); // newline after progress

        Ok(affected)
    }
}

/// Extract the content between <tag>...</tag> from XML.
fn extract_section<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let end = xml.find(&close)?;
    Some(&xml[start..end + close.len()])
}
