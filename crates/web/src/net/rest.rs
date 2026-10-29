use gloo_net::http::Request;
use hypatia_shared::{EndpointInfo, SourceId, StoredEvent, SummarySnapshot, SystemSnapshot};
use serde::Deserialize;

fn get_api_base() -> String {
    let window = web_sys::window();
    let host = window
        .as_ref()
        .and_then(|w| w.location().host().ok())
        .filter(|h| !h.is_empty())
        .unwrap_or_default();

    if host.contains("8080") {
        "http://127.0.0.1:7878".to_string()
    } else {
        String::new()
    }
}

pub async fn fetch_event_detail(id: u64) -> Result<StoredEvent, String> {
    let url = format!("{}/api/events/{}", get_api_base(), id);
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    resp.json::<StoredEvent>()
        .await
        .map_err(|e| format!("Deserialization error: {e}"))
}

pub async fn fetch_catalog(source: Option<SourceId>) -> Result<Vec<EndpointInfo>, String> {
    let base = get_api_base();
    let url = match source {
        Some(s) => format!("{}/api/catalog/{:?}", base, s),
        None => format!("{}/api/catalog", base),
    };

    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    resp.json::<Vec<EndpointInfo>>()
        .await
        .map_err(|e| format!("Deserialization error: {e}"))
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallResponse {
    pub status: u16,
    pub data: serde_json::Value,
}

pub async fn call_endpoint(source: SourceId, path: &str) -> Result<CallResponse, String> {
    let base = get_api_base();
    // Encode the query path properly
    let encoded_path = js_sys::encode_uri_component(path);
    let url = format!(
        "{}/api/call/{:?}?path={}",
        base,
        source,
        encoded_path.as_string().unwrap_or_else(|| path.to_string())
    );

    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(format!("Error {status}: {body_text}"));
    }

    resp.json::<CallResponse>()
        .await
        .map_err(|e| format!("Response error: {e}"))
}

#[derive(Debug, Deserialize)]
struct RevealResponse {
    #[allow(dead_code)]
    pub id: u64,
    pub value: String,
}

pub async fn reveal_secret(id: u64) -> Result<String, String> {
    let url = format!("{}/api/secrets/{}/reveal", get_api_base(), id);
    let resp = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    let reveal = resp
        .json::<RevealResponse>()
        .await
        .map_err(|e| format!("Secret decoding error: {e}"))?;

    Ok(reveal.value)
}

pub async fn fetch_system() -> Result<SystemSnapshot, String> {
    let url = format!("{}/api/system", get_api_base());
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    resp.json::<SystemSnapshot>()
        .await
        .map_err(|e| format!("Deserialization error: {e}"))
}

pub async fn fetch_summary() -> Result<SummarySnapshot, String> {
    let url = format!("{}/api/summary", get_api_base());
    let resp = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP status {}", resp.status()));
    }

    resp.json::<SummarySnapshot>()
        .await
        .map_err(|e| format!("Deserialization error: {e}"))
}
