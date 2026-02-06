use anyhow::{bail, Context, Result};
use reqwest::multipart;
use std::path::Path;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::api::client::JamfClient;
use crate::models::package::{Package, PackageCreateRequest, PackageSearchResponse};

impl JamfClient {
    /// Find a package by name. Returns None if not found.
    pub async fn find_package(&self, name: &str) -> Result<Option<Package>> {
        let url = format!(
            "{}/api/v1/packages?page=0&page-size=100&filter=packageName%3D%3D%22{}%22",
            self.base_url,
            urlencoding(name)
        );

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to search for package")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to search packages (HTTP {}): {}", status, body);
        }

        let search: PackageSearchResponse = resp
            .json()
            .await
            .context("Failed to parse package search response")?;

        Ok(search.results.into_iter().next())
    }

    /// Update an existing package's metadata in-place.
    pub async fn update_package(&self, id: &str, req: &PackageCreateRequest) -> Result<()> {
        let url = format!("{}/api/v1/packages/{}", self.base_url, id);

        let resp = self
            .http
            .put(&url)
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await
            .context("Failed to update package metadata")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to update package metadata (HTTP {}): {}", status, body);
        }

        Ok(())
    }

    /// Upload a file to an existing package record, with retries.
    pub async fn upload_package(&self, id: &str, file_path: &Path) -> Result<()> {
        let url = format!("{}/api/v1/packages/{}/upload", self.base_url, id);

        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let metadata = tokio::fs::metadata(file_path)
            .await
            .context("Failed to read file metadata")?;
        let file_size = metadata.len();

        let max_attempts = 3;
        for attempt in 1..=max_attempts {
            let file = File::open(file_path)
                .await
                .context("Failed to open package file")?;

            let stream = FramedRead::new(file, BytesCodec::new());
            let body = reqwest::Body::wrap_stream(stream);

            let part = multipart::Part::stream_with_length(body, file_size)
                .file_name(file_name.clone())
                .mime_str("application/octet-stream")
                .context("Failed to set MIME type")?;

            let form = multipart::Form::new().part("file", part);

            let resp = self
                .http
                .post(&url)
                .bearer_auth(&self.token)
                .header("Accept", "application/json")
                .multipart(form)
                .send()
                .await
                .context("Failed to upload package file")?;

            if resp.status().is_success() {
                return Ok(());
            }

            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();

            if attempt < max_attempts && status.is_server_error() {
                eprintln!(
                    "\n  Upload attempt {}/{} failed (HTTP {}), retrying in 10s...",
                    attempt, max_attempts, status
                );
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            } else {
                bail!("Failed to upload package (HTTP {}): {}", status, resp_body);
            }
        }

        unreachable!()
    }

    /// Trigger JCDS inventory recalculation to refresh checksums.
    pub async fn refresh_jcds_inventory(&self) -> Result<()> {
        let url = format!("{}/api/v1/jcds/refresh-inventory", self.base_url);

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to refresh JCDS inventory")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to refresh JCDS inventory (HTTP {}): {}", status, body);
        }

        Ok(())
    }
}

/// Simple percent-encoding for the filter query parameter value.
fn urlencoding(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('"', "%22")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('+', "%2B")
}
