use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use hypatia_shared::{EndpointInfo, EndpointOrigin, SourceId};
use serde_json::Value;

use crate::riot_http::ReadOnlyRiotClient;
use crate::signal::ChangeSignal;

/// Extracts the first path segment as the namespace (e.g., `/lol-champ-select/v1/session` -> `lol-champ-select`).
pub fn extract_namespace(path: &str) -> String {
    let clean = path.trim_start_matches('/');
    if let Some((first, _)) = clean.split_once('/') {
        first.to_string()
    } else if !clean.is_empty() {
        clean.to_string()
    } else {
        "root".to_string()
    }
}

/// Parses an OpenAPI / Swagger v2 / v3 JSON document and extracts `EndpointInfo` entries.
pub fn parse_openapi_json(source: SourceId, doc: &Value) -> Vec<EndpointInfo> {
    let mut endpoints = Vec::new();

    let paths = match doc.get("paths").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return endpoints,
    };

    let methods = ["get", "post", "put", "delete", "patch", "head", "options"];

    for (path_template, path_item) in paths {
        let item_obj = match path_item.as_object() {
            Some(o) => o,
            None => continue,
        };

        for &m in &methods {
            if let Some(op) = item_obj.get(m).and_then(|v| v.as_object()) {
                let description = op
                    .get("summary")
                    .or_else(|| op.get("description"))
                    .and_then(|d| d.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.to_string());

                endpoints.push(EndpointInfo {
                    source,
                    method: m.to_uppercase(),
                    path_template: path_template.clone(),
                    namespace: extract_namespace(path_template),
                    description,
                    origin: EndpointOrigin::Declared,
                    last_seen: None,
                    event_count: 0,
                    bytes_total: 0,
                });
            }
        }
    }

    endpoints
}

/// Converts a PascalCase function name (e.g. `GetLolGameflowV1GameflowPhase`) into an HTTP method and URI.
pub fn parse_function_name(name: &str) -> Option<(String, String)> {
    let prefixes = [
        ("Get", "GET"),
        ("Post", "POST"),
        ("Put", "PUT"),
        ("Delete", "DELETE"),
        ("Patch", "PATCH"),
    ];

    for (prefix, method) in prefixes {
        if let Some(rest) = name.strip_prefix(prefix) {
            if rest.is_empty() {
                continue;
            }

            // Convert remaining PascalCase into kebab-case path segments
            // e.g. LolGameflowV1GameflowPhase -> /lol-gameflow/v1/gameflow-phase
            let mut path = String::with_capacity(rest.len() + 8);
            path.push('/');

            let mut prev_is_upper = true;
            for c in rest.chars() {
                if c.is_uppercase() {
                    if !prev_is_upper {
                        path.push('-');
                    }
                    path.push(c.to_ascii_lowercase());
                    prev_is_upper = true;
                } else {
                    path.push(c);
                    prev_is_upper = false;
                }
            }

            // Clean up common version prefixes: -v-1- -> /v1/, lol- -> lol-
            let cleaned = path
                .replace("-v-1-", "/v1/")
                .replace("-v-2-", "/v2/")
                .replace("-v-3-", "/v3/")
                .replace("-v-4-", "/v4/")
                .replace("-v-5-", "/v5/")
                .replace("-v-6-", "/v6/")
                .replace("-v1-", "/v1/")
                .replace("-v2-", "/v2/")
                .replace("-v3-", "/v3/")
                .replace("-v4-", "/v4/")
                .replace("-v5-", "/v5/")
                .replace("-v6-", "/v6/");

            return Some((method.to_string(), cleaned));
        }
    }

    None
}

/// Parses the `/help?format=Full` JSON response and extracts `EndpointInfo` entries.
pub fn parse_help_format_full(source: SourceId, doc: &Value) -> Vec<EndpointInfo> {
    let mut endpoints = Vec::new();

    // 1. Parse `events` (e.g. `OnJsonApiEvent_lol-gameflow_v1_gameflow-phase`)
    if let Some(events) = doc.get("events") {
        if let Some(arr) = events.as_array() {
            for ev in arr {
                if let Some(name) = ev.get("name").and_then(|n| n.as_str()) {
                    if let Some(suffix) = name.strip_prefix("OnJsonApiEvent_") {
                        let path_template = format!("/{}", suffix.replace('_', "/"));
                        let desc = ev
                            .get("description")
                            .and_then(|d| d.as_str())
                            .filter(|s| !s.trim().is_empty())
                            .map(|s| s.to_string());

                        endpoints.push(EndpointInfo {
                            source,
                            method: "GET".to_string(),
                            path_template: path_template.clone(),
                            namespace: extract_namespace(&path_template),
                            description: desc,
                            origin: EndpointOrigin::Declared,
                            last_seen: None,
                            event_count: 0,
                            bytes_total: 0,
                        });
                    }
                }
            }
        } else if let Some(map) = events.as_object() {
            for (name, desc_val) in map {
                if let Some(suffix) = name.strip_prefix("OnJsonApiEvent_") {
                    let path_template = format!("/{}", suffix.replace('_', "/"));
                    let desc = desc_val
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| s.to_string());

                    endpoints.push(EndpointInfo {
                        source,
                        method: "GET".to_string(),
                        path_template: path_template.clone(),
                        namespace: extract_namespace(&path_template),
                        description: desc,
                        origin: EndpointOrigin::Declared,
                        last_seen: None,
                        event_count: 0,
                        bytes_total: 0,
                    });
                }
            }
        }
    }

    // 2. Parse `functions`
    if let Some(functions) = doc.get("functions") {
        if let Some(arr) = functions.as_array() {
            for f in arr {
                let desc = f
                    .get("description")
                    .and_then(|d| d.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.to_string());

                // If function explicitly provides `http_method` and `url`
                if let (Some(method), Some(url)) = (
                    f.get("http_method").and_then(|m| m.as_str()),
                    f.get("url").and_then(|u| u.as_str()),
                ) {
                    endpoints.push(EndpointInfo {
                        source,
                        method: method.to_uppercase(),
                        path_template: url.to_string(),
                        namespace: extract_namespace(url),
                        description: desc,
                        origin: EndpointOrigin::Declared,
                        last_seen: None,
                        event_count: 0,
                        bytes_total: 0,
                    });
                } else if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                    // Try parsing function name as HTTP PascalCase
                    if let Some((method, path_template)) = parse_function_name(name) {
                        endpoints.push(EndpointInfo {
                            source,
                            method,
                            path_template: path_template.clone(),
                            namespace: extract_namespace(&path_template),
                            description: desc,
                            origin: EndpointOrigin::Declared,
                            last_seen: None,
                            event_count: 0,
                            bytes_total: 0,
                        });
                    }
                }
            }
        } else if let Some(map) = functions.as_object() {
            for (name, desc_val) in map {
                let desc = desc_val
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.to_string());

                if let Some((method, path_template)) = parse_function_name(name) {
                    endpoints.push(EndpointInfo {
                        source,
                        method,
                        path_template: path_template.clone(),
                        namespace: extract_namespace(&path_template),
                        description: desc,
                        origin: EndpointOrigin::Declared,
                        last_seen: None,
                        event_count: 0,
                        bytes_total: 0,
                    });
                }
            }
        }
    }

    endpoints
}

/// In-memory catalog of declared (Swagger / `/help`) and observed endpoints.
#[derive(Debug, Default)]
pub struct CatalogStore {
    endpoints: Vec<EndpointInfo>,
    /// Per source: path -> positions in `endpoints` (one per HTTP method).
    index: [HashMap<String, Vec<u32>>; SourceId::COUNT],
    /// Port whose declared endpoints were last imported, per source.
    imported_port: [Option<u16>; SourceId::COUNT],
    revision: u64,
}

impl CatalogStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// All endpoints across all sources, in insertion order.
    pub fn endpoints(&self) -> &[EndpointInfo] {
        &self.endpoints
    }

    pub fn len(&self) -> usize {
        self.endpoints.len()
    }

    pub fn is_empty(&self) -> bool {
        self.endpoints.is_empty()
    }

    /// Bumped whenever an endpoint is added or its origin changes (not on traffic counters).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Claims the declared-endpoint import for `source` on `port`; `false` if already done.
    pub fn claim_import(&mut self, source: SourceId, port: u16) -> bool {
        let slot = &mut self.imported_port[source.index()];
        if *slot == Some(port) {
            return false;
        }
        *slot = Some(port);
        true
    }

    fn position(&self, source: SourceId, method: &str, path: &str) -> Option<usize> {
        self.index[source.index()]
            .get(path)?
            .iter()
            .map(|&i| i as usize)
            .find(|&i| self.endpoints[i].method.eq_ignore_ascii_case(method))
    }

    fn push(&mut self, endpoint: EndpointInfo) {
        self.index[endpoint.source.index()]
            .entry(endpoint.path_template.clone())
            .or_default()
            .push(self.endpoints.len() as u32);
        self.endpoints.push(endpoint);
        self.revision += 1;
    }

    /// Merges declared endpoints into the catalog. Returns the number of newly added endpoints.
    pub fn add_declared(&mut self, new_endpoints: Vec<EndpointInfo>) -> usize {
        let mut added = 0;
        for new_ep in new_endpoints {
            match self.position(new_ep.source, &new_ep.method, &new_ep.path_template) {
                Some(i) => {
                    let existing = &mut self.endpoints[i];
                    if existing.description.is_none() && new_ep.description.is_some() {
                        existing.description = new_ep.description;
                    }
                    if existing.origin == EndpointOrigin::Observed {
                        existing.origin = EndpointOrigin::Both;
                        self.revision += 1;
                    }
                }
                None => {
                    self.push(new_ep);
                    added += 1;
                }
            }
        }
        added
    }

    /// Records an observed call or event. Returns true if an endpoint was added or its origin
    /// changed.
    pub fn record_observed(
        &mut self,
        source: SourceId,
        method: &str,
        uri: &str,
        bytes: u64,
    ) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        if let Some(i) = self.position(source, method, uri) {
            let existing = &mut self.endpoints[i];
            existing.last_seen = Some(now_ms);
            existing.event_count += 1;
            existing.bytes_total += bytes;
            if existing.origin == EndpointOrigin::Declared {
                existing.origin = EndpointOrigin::Both;
                self.revision += 1;
                return true;
            }
            return false;
        }

        self.push(EndpointInfo {
            source,
            method: method.to_ascii_uppercase(),
            path_template: uri.to_string(),
            namespace: extract_namespace(uri),
            description: None,
            origin: EndpointOrigin::Observed,
            last_seen: Some(now_ms),
            event_count: 1,
            bytes_total: bytes,
        });
        true
    }
}

/// Discovers endpoints for a source, trying in order `/swagger/v3/openapi.json`,
/// `/swagger/v2/swagger.json`, `/help?format=Full` and `/help`.
pub async fn discover_endpoints(
    client: &ReadOnlyRiotClient,
    source: SourceId,
    port: u16,
    password: Option<&str>,
) -> Vec<EndpointInfo> {
    type Parser = fn(SourceId, &Value) -> Vec<EndpointInfo>;
    let probes: [(&str, Parser); 4] = [
        ("/swagger/v3/openapi.json", parse_openapi_json),
        ("/swagger/v2/swagger.json", parse_openapi_json),
        ("/help?format=Full", parse_help_format_full),
        ("/help", parse_help_format_full),
    ];

    for (path, parse) in probes {
        let Ok((status, _, body)) = client.get(port, password, path).await else {
            continue;
        };
        if !status.is_success() {
            continue;
        }
        let endpoints = parse(source, &body);
        if !endpoints.is_empty() {
            tracing::info!(
                "{source}: discovered {} endpoints via {path}",
                endpoints.len()
            );
            return endpoints;
        }
    }

    tracing::warn!("{source}: no endpoints discovered via Swagger or /help");
    Vec::new()
}

/// Imports the declared endpoints of `source` in the background, once per port.
pub fn spawn_catalog_discovery(
    source: SourceId,
    port: u16,
    password: Option<String>,
    catalog: Arc<RwLock<CatalogStore>>,
    client: ReadOnlyRiotClient,
    signal: ChangeSignal,
) {
    if !catalog.write().unwrap().claim_import(source, port) {
        return;
    }
    tokio::spawn(async move {
        let endpoints = discover_endpoints(&client, source, port, password.as_deref()).await;
        if !endpoints.is_empty() {
            let added = catalog.write().unwrap().add_declared(endpoints);
            tracing::info!("{source}: added {added} declared endpoints to catalog");
            signal.bump();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_namespace() {
        assert_eq!(
            extract_namespace("/lol-champ-select/v1/session"),
            "lol-champ-select"
        );
        assert_eq!(extract_namespace("/chat/v4/presences"), "chat");
        assert_eq!(extract_namespace("/health"), "health");
        assert_eq!(extract_namespace("/"), "root");
    }

    #[test]
    fn test_parse_openapi_json() {
        let doc = serde_json::json!({
            "paths": {
                "/riotclient/region-locale": {
                    "get": {
                        "summary": "Region and locale info"
                    }
                },
                "/Exit": {
                    "post": {
                        "summary": "Close client"
                    }
                }
            }
        });

        let eps = parse_openapi_json(SourceId::RiotClient, &doc);
        assert_eq!(eps.len(), 2);
        assert!(eps.iter().any(|e| e.method == "GET"
            && e.path_template == "/riotclient/region-locale"
            && e.namespace == "riotclient"));
        assert!(eps
            .iter()
            .any(|e| e.method == "POST" && e.path_template == "/Exit" && e.namespace == "Exit"));
    }

    #[test]
    fn test_parse_help_events() {
        let doc = serde_json::json!({
            "events": [
                {
                    "name": "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase",
                    "description": "Gameflow phase"
                },
                {
                    "name": "OnCallback",
                    "description": "Internal"
                }
            ],
            "functions": []
        });

        let eps = parse_help_format_full(SourceId::LeagueClient, &doc);
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].path_template, "/lol-gameflow/v1/gameflow-phase");
        assert_eq!(eps[0].method, "GET");
        assert_eq!(eps[0].namespace, "lol-gameflow");
        assert_eq!(eps[0].origin, EndpointOrigin::Declared);
    }

    #[test]
    fn test_catalog_store_observed_merging() {
        let mut store = CatalogStore::new();

        // 1. Record an observed endpoint before declaration
        store.record_observed(
            SourceId::LeagueClient,
            "GET",
            "/lol-gameflow/v1/gameflow-phase",
            128,
        );
        let all = store.endpoints();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].origin, EndpointOrigin::Observed);
        assert_eq!(all[0].event_count, 1);
        assert_eq!(all[0].bytes_total, 128);

        // 2. Now discover and add as declared
        let declared = vec![EndpointInfo {
            source: SourceId::LeagueClient,
            method: "GET".to_string(),
            path_template: "/lol-gameflow/v1/gameflow-phase".to_string(),
            namespace: "lol-gameflow".to_string(),
            description: Some("Phase info".to_string()),
            origin: EndpointOrigin::Declared,
            last_seen: None,
            event_count: 0,
            bytes_total: 0,
        }];

        store.add_declared(declared);
        let all2 = store.endpoints();
        assert_eq!(all2.len(), 1);
        assert_eq!(all2[0].origin, EndpointOrigin::Both);
        assert_eq!(all2[0].description, Some("Phase info".to_string()));
    }
}
