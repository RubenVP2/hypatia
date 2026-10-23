use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
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
    fn test_lockfile_parse() {
        let raw = "Riot Client:14852:12840:npfPQc0M_o4xbD9KJdfN4w:https\n";
        let parsed = RiotClientLockfile::parse(raw).expect("should parse lockfile");
        assert_eq!(parsed.name, "Riot Client");
        assert_eq!(parsed.pid, 14852);
        assert_eq!(parsed.port, 12840);
        assert_eq!(parsed.password, "npfPQc0M_o4xbD9KJdfN4w");
        assert_eq!(parsed.protocol, "https");
    }

    #[test]
    fn test_health_response_serialization() {
        let health = HealthResponse {
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&health).expect("serialization failed");
        assert_eq!(json, r#"{"status":"ok"}"#);
    }
}
