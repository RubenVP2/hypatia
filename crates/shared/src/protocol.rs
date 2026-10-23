use serde::{Deserialize, Serialize};
use crate::event::{EventMeta, GamePhase};
use crate::source::{SourceId, SourceState};
use crate::system::SystemSnapshot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub sources: Vec<SourceState>,
    pub phase: Option<GamePhase>,
    pub recent_events: Vec<EventMeta>,
    pub system: SystemSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum ServerMsg {
    Hello {
        version: String,
        snapshot: Snapshot,
    },
    Batch {
        events: Vec<EventMeta>,
        sources: Vec<SourceState>,
        rate: f32,
    },
    Phase(GamePhase),
    CatalogChanged {
        source: SourceId,
    },
    System(SystemSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum ClientMsg {
    Pause(bool),
    Ping,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_msg_serde() {
        let pause = ClientMsg::Pause(true);
        let pause_json = serde_json::to_string(&pause).unwrap();
        assert_eq!(pause_json, r#"{"t":"Pause","c":true}"#);
        let back: ClientMsg = serde_json::from_str(&pause_json).unwrap();
        assert_eq!(back, pause);

        let ping = ClientMsg::Ping;
        let ping_json = serde_json::to_string(&ping).unwrap();
        assert_eq!(ping_json, r#"{"t":"Ping"}"#);
        let back_ping: ClientMsg = serde_json::from_str(&ping_json).unwrap();
        assert_eq!(back_ping, ping);
    }

    #[test]
    fn test_server_msg_serde() {
        let phase_msg = ServerMsg::Phase(GamePhase {
            phase: "ChampSelect".to_string(),
            since: 1700000000,
        });
        let json = serde_json::to_string(&phase_msg).unwrap();
        assert_eq!(
            json,
            r#"{"t":"Phase","c":{"phase":"ChampSelect","since":1700000000}}"#
        );
        let back: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(back, phase_msg);

        let cat_msg = ServerMsg::CatalogChanged {
            source: SourceId::LeagueClient,
        };
        let cat_json = serde_json::to_string(&cat_msg).unwrap();
        assert_eq!(
            cat_json,
            r#"{"t":"CatalogChanged","c":{"source":"LeagueClient"}}"#
        );
        let back_cat: ServerMsg = serde_json::from_str(&cat_json).unwrap();
        assert_eq!(back_cat, cat_msg);
    }
}
