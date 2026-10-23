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
    pub preview: String, // <= 120 car., masqué
    pub has_secrets: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamePhase {
    pub phase: String,
    pub since: i64, // ms epoch
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
    }
}
