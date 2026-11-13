use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::source::SourceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum EventKind {
    Create,
    Update,
    Delete,
    Poll,
    Call,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventMeta {
    pub id: u64,
    pub ts: i64,
    pub source: SourceId,
    pub kind: EventKind,
    pub uri: String,
    pub size: u32,
    pub preview: String, // <= 120 chars, masked
    pub has_secrets: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamePhase {
    pub phase: String,
    pub since: i64, // ms epoch
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UriState {
    /// Latest redacted payload, shared with the event history (never deep-cloned).
    pub current: Arc<serde_json::Value>,
    pub previous: Option<Arc<serde_json::Value>>,
    pub kind: EventKind,
    pub count: u64,
    pub last_ts: i64,
    pub bytes: u64,
}

/// Stored event with its full payload and the previous one for diffing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredEvent {
    #[serde(rename = "event")]
    pub meta: EventMeta,
    pub payload: Arc<serde_json::Value>,
    pub previous_payload: Option<Arc<serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serde() {
        let meta = EventMeta {
            id: 1,
            ts: 1700000000000,
            source: SourceId::LeagueClient,
            kind: EventKind::Update,
            uri: "/lol-champ-select/v1/session".to_string(),
            size: 1024,
            preview: "{\"myTeam\":[]}".to_string(),
            has_secrets: false,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let back: EventMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);

        let kind = EventKind::Create;
        let k_json = serde_json::to_string(&kind).unwrap();
        assert_eq!(k_json, r#"{"t":"Create"}"#);

        let uri_state = UriState {
            current: Arc::new(serde_json::json!({"status": "active"})),
            previous: Some(Arc::new(serde_json::json!({"status": "idle"}))),
            kind: EventKind::Update,
            count: 42,
            last_ts: 1700000000123,
            bytes: 256,
        };
        let u_json = serde_json::to_string(&uri_state).unwrap();
        let back_uri: UriState = serde_json::from_str(&u_json).unwrap();
        assert_eq!(back_uri, uri_state);

        let stored = StoredEvent {
            meta: meta.clone(),
            payload: Arc::new(serde_json::json!({"phase": "ChampSelect"})),
            previous_payload: Some(Arc::new(serde_json::json!({"phase": "Matchmaking"}))),
        };
        let s_json = serde_json::to_string(&stored).unwrap();
        let back_stored: StoredEvent = serde_json::from_str(&s_json).unwrap();
        assert_eq!(back_stored, stored);
    }
}
