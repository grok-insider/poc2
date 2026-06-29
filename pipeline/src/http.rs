//! HTTP client used by all sources.
//!
//! - Polite User-Agent identifying the project.
//! - Reasonable timeout.
//! - JSON-aware fetch helper.

use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;

use crate::error::{PipelineError, PipelineResult};

const USER_AGENT: &str = concat!(
    "poc2-pipeline/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/anomalyco/poc2; data builder; contact: github issues)"
);

pub fn make_client() -> Client {
    Client::builder()
        .user_agent(USER_AGENT)
        .gzip(true)
        .timeout(Duration::from_secs(60))
        .build()
        .expect("reqwest client init")
}

/// Fetch a URL and parse its body as JSON into `T`.
pub async fn fetch_json<T: DeserializeOwned>(client: &Client, url: &str) -> PipelineResult<T> {
    let url_str = url.to_string();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| PipelineError::Http {
            url: url_str.clone(),
            source: e,
        })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(PipelineError::HttpStatus {
            url: url_str,
            status: status.as_u16(),
            body: body.chars().take(500).collect(),
        });
    }
    let bytes = resp.bytes().await.map_err(|e| PipelineError::Http {
        url: url_str.clone(),
        source: e,
    })?;
    serde_json::from_slice(&bytes).map_err(|e| PipelineError::JsonParse {
        url: url_str,
        source: e,
    })
}

/// Fetch a URL and return its body as a trimmed UTF-8 string.
///
/// Used for tiny plain-text endpoints such as the community patch-version
/// pointer (`poe-tool-dev/latest-patch-version`), where the body is a bare
/// version string rather than JSON.
pub async fn fetch_text(client: &Client, url: &str) -> PipelineResult<String> {
    let url_str = url.to_string();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| PipelineError::Http {
            url: url_str.clone(),
            source: e,
        })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(PipelineError::HttpStatus {
            url: url_str,
            status: status.as_u16(),
            body: body.chars().take(500).collect(),
        });
    }
    let text = resp.text().await.map_err(|e| PipelineError::Http {
        url: url_str,
        source: e,
    })?;
    Ok(text.trim().to_string())
}

/// SHA-256 hex digest of a byte slice, for source-revision tracking.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Fetch a URL as raw bytes (used when we want the SHA256 in addition to
/// the parsed body).
pub async fn fetch_bytes(client: &Client, url: &str) -> PipelineResult<Vec<u8>> {
    let url_str = url.to_string();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| PipelineError::Http {
            url: url_str.clone(),
            source: e,
        })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(PipelineError::HttpStatus {
            url: url_str,
            status: status.as_u16(),
            body: body.chars().take(500).collect(),
        });
    }
    let bytes = resp.bytes().await.map_err(|e| PipelineError::Http {
        url: url_str,
        source: e,
    })?;
    Ok(bytes.to_vec())
}
