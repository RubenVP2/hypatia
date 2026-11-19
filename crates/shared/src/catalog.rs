use crate::source::SourceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum EndpointOrigin {
    Declared,
    Observed,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointInfo {
    pub source: SourceId,
    pub method: String,
    pub path_template: String,
    pub namespace: String,
    pub description: Option<String>,
    pub origin: EndpointOrigin,
    pub last_seen: Option<i64>,
    pub event_count: u64,
    pub bytes_total: u64,
}

impl EndpointInfo {
    /// Returns true if this endpoint is a GET request without required path parameters (i.e. `{...}`)
    pub fn is_callable_get(&self) -> bool {
        self.method.eq_ignore_ascii_case("GET") && !self.path_template.contains('{')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_serde() {
        let ep = EndpointInfo {
            source: SourceId::LeagueClient,
            method: "GET".to_string(),
            path_template: "/lol-gameflow/v1/gameflow-phase".to_string(),
            namespace: "lol-gameflow".to_string(),
            description: Some("Current gameflow phase".to_string()),
            origin: EndpointOrigin::Both,
            last_seen: Some(1700000000),
            event_count: 5,
            bytes_total: 250,
        };

        assert!(ep.is_callable_get());

        let ep_param = EndpointInfo {
            path_template: "/lol-summoner/v1/summoners/{id}".to_string(),
            ..ep.clone()
        };
        assert!(!ep_param.is_callable_get());

        let json = serde_json::to_string(&ep).unwrap();
        let back: EndpointInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ep);
    }
}
