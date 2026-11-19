use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::prelude::*;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;

/// Builds a loopback HTTPS client accepting local self-signed Riot certificates.
pub(crate) fn build_loopback_client() -> reqwest::Client {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to initialize loopback reqwest client")
}

/// Formats a Riot LCU/RC remoting auth token as an `Authorization` header value.
pub(crate) fn format_riot_auth_header(token: &str) -> String {
    if token.starts_with("Basic ") || token.starts_with("Bearer ") {
        token.to_string()
    } else {
        format!("Basic {}", BASE64_STANDARD.encode(format!("riot:{token}")))
    }
}

/// Strict read-only HTTP client dedicated to communicating with local Riot APIs (127.0.0.1).
///
/// INVARIANT: This client ONLY exposes a `get` method. No POST, PUT, DELETE, PATCH or
/// any other mutating HTTP methods are implemented or exposed.
#[derive(Clone)]
pub struct ReadOnlyRiotClient {
    inner: reqwest::Client,
}

impl Default for ReadOnlyRiotClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadOnlyRiotClient {
    /// Creates a new `ReadOnlyRiotClient` configured with a 5-second timeout
    /// and accepting self-signed certificates strictly for local 127.0.0.1 connections.
    pub fn new() -> Self {
        Self {
            inner: build_loopback_client(),
        }
    }

    /// Performs a strict binary GET request to https://127.0.0.1:{port}{path_and_query}.
    ///
    /// Validations:
    /// - `path_and_query` MUST start with `/`
    /// - `path_and_query` MUST NOT contain `://`
    /// - `path_and_query` MUST NOT contain `..`
    ///
    /// The target host is hardcoded to `127.0.0.1` by construction.
    pub async fn get_bytes(
        &self,
        port: u16,
        auth_token: Option<&str>,
        path_and_query: &str,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>)> {
        if !path_and_query.starts_with('/') {
            bail!("Path must start with '/': '{}'", path_and_query);
        }
        if path_and_query.contains("://") {
            bail!("Path must not contain '://': '{}'", path_and_query);
        }
        if path_and_query.contains("..") {
            bail!("Path must not contain '..': '{}'", path_and_query);
        }

        let url = format!("https://127.0.0.1:{}{}", port, path_and_query);
        let mut req = self.inner.get(&url);

        if let Some(token) = auth_token {
            req = req.header("Authorization", format_riot_auth_header(token));
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("Failed to send GET request to {}", url))?;

        let status = StatusCode::from_u16(resp.status().as_u16())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let headers = resp.headers().clone();

        let bytes = resp
            .bytes()
            .await
            .with_context(|| format!("Failed to read response body from {}", url))?;

        Ok((status, headers, bytes.to_vec()))
    }

    /// Performs a strict GET request to https://127.0.0.1:{port}{path_and_query} and decodes JSON/text.
    pub async fn get(
        &self,
        port: u16,
        auth_token: Option<&str>,
        path_and_query: &str,
    ) -> Result<(StatusCode, HeaderMap, serde_json::Value)> {
        let (status, headers, bytes) = self.get_bytes(port, auth_token, path_and_query).await?;

        let body: serde_json::Value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            match serde_json::from_slice(&bytes) {
                Ok(val) => val,
                Err(_) => {
                    // Fallback to string if not valid JSON
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    serde_json::Value::String(text)
                }
            }
        };

        Ok((status, headers, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_path_validation_rejects_invalid_paths() {
        let client = ReadOnlyRiotClient::new();

        // Must start with '/'
        assert!(client.get(1234, None, "swagger/v3").await.is_err());
        assert!(client.get(1234, None, "").await.is_err());
        assert!(client
            .get_bytes(1234, None, "lol-game-data/assets")
            .await
            .is_err());

        // Must not contain '://'
        assert!(client
            .get(1234, None, "/http://evil.com/api")
            .await
            .is_err());
        assert!(client
            .get_bytes(1234, None, "/https://evil.com/icon.png")
            .await
            .is_err());

        // Must not contain '..'
        assert!(client.get(1234, None, "/../../etc/passwd").await.is_err());
        assert!(client
            .get_bytes(1234, None, "/lol-game-data/../secret")
            .await
            .is_err());
    }
}
