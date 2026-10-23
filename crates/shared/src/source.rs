use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use crate::secret::SecretRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceId {
    RiotClient,
    LeagueClient,
    LiveGame,
    System,
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RiotClient => write!(f, "RiotClient"),
            Self::LeagueClient => write!(f, "LeagueClient"),
            Self::LiveGame => write!(f, "LiveGame"),
            Self::System => write!(f, "System"),
        }
    }
}

impl FromStr for SourceId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RiotClient" | "riotclient" | "riot_client" => Ok(Self::RiotClient),
            "LeagueClient" | "leagueclient" | "league_client" | "lcu" => Ok(Self::LeagueClient),
            "LiveGame" | "livegame" | "live_game" | "game" => Ok(Self::LiveGame),
            "System" | "system" => Ok(Self::System),
            _ => Err(format!("Unknown SourceId: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum SourceStatus {
    Offline,
    Discovered,
    Connecting,
    Live,
    Degraded { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceState {
    pub id: SourceId,
    pub status: SourceStatus,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub protocol: Option<String>,
    pub since: i64, // ms epoch
    pub events_total: u64,
    pub bytes_total: u64,
    pub credential: Option<SecretRef>, // jamais la valeur
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiotClientLockfile {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub password: String,
    pub protocol: String,
}

impl RiotClientLockfile {
    pub fn parse(content: &str) -> Option<Self> {
        let parts: Vec<&str> = content.trim().split(':').collect();
        if parts.len() < 5 {
            return None;
        }
        Some(Self {
            name: parts[0].to_string(),
            pid: parts[1].parse().ok()?,
            port: parts[2].parse().ok()?,
            password: parts[3].to_string(),
            protocol: parts[4].to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_id_display_and_parse() {
        assert_eq!(SourceId::RiotClient.to_string(), "RiotClient");
        assert_eq!(SourceId::LeagueClient.to_string(), "LeagueClient");
        assert_eq!(SourceId::LiveGame.to_string(), "LiveGame");
        assert_eq!(SourceId::System.to_string(), "System");

        assert_eq!("lcu".parse::<SourceId>().unwrap(), SourceId::LeagueClient);
        assert_eq!("RiotClient".parse::<SourceId>().unwrap(), SourceId::RiotClient);
        assert_eq!("game".parse::<SourceId>().unwrap(), SourceId::LiveGame);
        assert_eq!("system".parse::<SourceId>().unwrap(), SourceId::System);
        assert!("invalid".parse::<SourceId>().is_err());
    }

    #[test]
    fn test_source_status_serde() {
        let live = SourceStatus::Live;
        let live_json = serde_json::to_string(&live).unwrap();
        assert_eq!(live_json, r#"{"t":"Live"}"#);

        let degraded = SourceStatus::Degraded {
            reason: "timeout".to_string(),
        };
        let deg_json = serde_json::to_string(&degraded).unwrap();
        assert_eq!(deg_json, r#"{"t":"Degraded","c":{"reason":"timeout"}}"#);

        let back: SourceStatus = serde_json::from_str(&deg_json).unwrap();
        assert_eq!(back, degraded);
    }
}
