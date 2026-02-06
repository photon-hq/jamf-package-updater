use anyhow::{Context, Result, bail};
use reqwest::multipart;
use serde_json::Value;
use std::path::Path;
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::api::client::JamfClient;
use crate::models::package::{Package, PackageCreateRequest, PackageSearchResponse};

#[derive(Debug, Clone, Default)]
pub struct PackageDigestSnapshot {
    pub md5_hash: Option<String>,
    pub hash_type: Option<String>,
    pub hash_value: Option<String>,
    pub file_size: Option<u64>,
}

impl PackageDigestSnapshot {
    pub fn is_empty(&self) -> bool {
        self.md5_hash.is_none()
            && self.hash_type.is_none()
            && self.hash_value.is_none()
            && self.file_size.is_none()
    }

    pub fn differs_from(&self, old: &Self) -> bool {
        field_changed(old.md5_hash.as_deref(), self.md5_hash.as_deref())
            || field_changed(old.hash_type.as_deref(), self.hash_type.as_deref())
            || field_changed(old.hash_value.as_deref(), self.hash_value.as_deref())
            || field_changed(old.file_size.as_ref(), self.file_size.as_ref())
    }

    pub fn display_line(&self) -> String {
        let md5 = self.md5_hash.as_deref().unwrap_or("unknown");
        let hash_type = self.hash_type.as_deref().unwrap_or("unknown");
        let hash_value = self.hash_value.as_deref().unwrap_or("unknown");
        let file_size = self
            .file_size
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!(
            "md5={}, hash={} {}, file_size={}",
            md5, hash_type, hash_value, file_size
        )
    }
}

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
            bail!(
                "Failed to update package metadata (HTTP {}): {}",
                status,
                body
            );
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
            bail!(
                "Failed to refresh JCDS inventory (HTTP {}): {}",
                status,
                body
            );
        }

        Ok(())
    }

    /// Read package digest/checksum fields as currently reported by Jamf Pro.
    pub async fn get_package_digest_snapshot(
        &self,
        id: &str,
    ) -> Result<Option<PackageDigestSnapshot>> {
        let url = format!("{}/api/v1/packages/{}", self.base_url, id);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to read package details")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Failed to read package details (HTTP {}): {}", status, body);
        }

        let payload: Value = resp
            .json()
            .await
            .context("Failed to parse package details response")?;

        let snapshot = PackageDigestSnapshot {
            md5_hash: find_first_string(
                &payload,
                &["md5Hash", "md5", "md5Checksum", "md5Sum", "MD5"],
            ),
            hash_type: find_first_string(&payload, &["hashType", "checksumType"]),
            hash_value: find_first_string(&payload, &["hashValue", "checksum", "hash"]),
            file_size: find_first_u64(&payload, &["fileSize", "size", "fileSizeBytes"]),
        };

        if snapshot.is_empty() {
            Ok(None)
        } else {
            Ok(Some(snapshot))
        }
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

fn field_changed<T: PartialEq + ?Sized>(old: Option<&T>, new: Option<&T>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => old != new,
        _ => false,
    }
}

fn find_first_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(found) = map.get(*key).and_then(value_to_string) {
                    return Some(found);
                }
            }
            for nested in map.values() {
                if let Some(found) = find_first_string(nested, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_first_string(item, keys)),
        _ => None,
    }
}

fn find_first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(found) = map.get(*key).and_then(value_to_u64) {
                    return Some(found);
                }
            }
            for nested in map.values() {
                if let Some(found) = find_first_u64(nested, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_first_u64(item, keys)),
        _ => None,
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PackageDigestSnapshot, find_first_string, find_first_u64};

    #[test]
    fn parses_digest_fields_from_nested_json() {
        let payload = json!({
            "packageName": "demo",
            "distributionPointFileInfo": {
                "md5Hash": "abc123",
                "hashType": "SHA3_512",
                "hashValue": "def456",
                "fileSize": 42
            }
        });

        let snapshot = PackageDigestSnapshot {
            md5_hash: find_first_string(&payload, &["md5Hash", "md5"]),
            hash_type: find_first_string(&payload, &["hashType"]),
            hash_value: find_first_string(&payload, &["hashValue"]),
            file_size: find_first_u64(&payload, &["fileSize"]),
        };

        assert_eq!(snapshot.md5_hash.as_deref(), Some("abc123"));
        assert_eq!(snapshot.hash_type.as_deref(), Some("SHA3_512"));
        assert_eq!(snapshot.hash_value.as_deref(), Some("def456"));
        assert_eq!(snapshot.file_size, Some(42));
    }
}
