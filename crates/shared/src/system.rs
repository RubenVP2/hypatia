use crate::source::SourceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub exe: Option<String>,
    pub started: i64,
    pub cpu: f32,
    pub mem_bytes: u64,
    pub source: Option<SourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListenSocket {
    pub pid: u32,
    pub local: String,
    pub port: u16,
    pub proto: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteConn {
    pub pid: u32,
    pub local: String,
    pub remote: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub ts: i64,
    pub processes: Vec<ProcessInfo>,
    pub sockets: Vec<ListenSocket>,
    pub connections: Vec<RemoteConn>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_snapshot_serde() {
        let snap = SystemSnapshot {
            ts: 1700000000,
            processes: vec![ProcessInfo {
                pid: 1234,
                name: "LeagueClientUx.exe".to_string(),
                exe: Some("C:\\Games\\LeagueClientUx.exe".to_string()),
                started: 1699990000,
                cpu: 1.5,
                mem_bytes: 500_000_000,
                source: Some(SourceId::LeagueClient),
            }],
            sockets: vec![ListenSocket {
                pid: 1234,
                local: "127.0.0.1".to_string(),
                port: 54321,
                proto: "tcp".to_string(),
            }],
            connections: vec![RemoteConn {
                pid: 1234,
                local: "127.0.0.1:54321".to_string(),
                remote: "192.168.1.1:443".to_string(),
                state: "ESTABLISHED".to_string(),
            }],
        };

        let json = serde_json::to_string(&snap).unwrap();
        let back: SystemSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
    }
}
