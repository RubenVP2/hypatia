pub mod lockfile;
pub mod ports;
pub mod process;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hypatia_shared::{SecretRef, SourceId, SourceState, SourceStatus, SystemSnapshot};
use sysinfo::System;
use tokio::sync::watch;
use tracing::{debug, info};

pub use lockfile::*;
pub use ports::*;
pub use process::*;

use crate::redaction::SecretVault;

/// Internal state stored and updated by the `DiscoveryManager`.
#[derive(Debug, Clone)]
pub struct DiscoveryState {
    pub snapshot: SystemSnapshot,
    pub sources: Vec<SourceState>,
    pub lcu_args: Option<LeagueClientArgs>,
    pub since_map: HashMap<SourceId, i64>,
}

/// The DiscoveryManager coordinates periodic scanning of Riot processes,
/// lockfiles, TCP ports, and connection states.
pub struct DiscoveryManager {
    state: Arc<RwLock<DiscoveryState>>,
    vault: SecretVault,
    snapshot_tx: watch::Sender<SystemSnapshot>,
}

impl DiscoveryManager {
    /// Creates a new `DiscoveryManager`, performs an initial scan, and returns
    /// an `Arc<DiscoveryManager>` along with a `watch::Receiver<SystemSnapshot>`.
    pub fn new(vault: SecretVault) -> (Arc<Self>, watch::Receiver<SystemSnapshot>) {
        let mut sys = System::new();
        let mut since_map = HashMap::new();
        let (snapshot, sources, lcu_args) = Self::scan_cycle(&mut sys, &vault, &mut since_map);

        let (snapshot_tx, snapshot_rx) = watch::channel(snapshot.clone());

        let initial_state = DiscoveryState {
            snapshot,
            sources,
            lcu_args,
            since_map,
        };

        let manager = Arc::new(Self {
            state: Arc::new(RwLock::new(initial_state)),
            vault,
            snapshot_tx,
        });

        (manager, snapshot_rx)
    }

    /// Spawns the background discovery loop running at the specified interval.
    pub fn spawn_loop(self: &Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
        let manager = Arc::clone(self);

        tokio::spawn(async move {
            info!(
                "Discovery background loop started with interval {:?}",
                interval
            );
            let mut sys = System::new();

            loop {
                tokio::time::sleep(interval).await;

                let (snapshot, sources, _lcu_args) = {
                    let mut state = manager.state.write().unwrap();
                    let (snap, src, args) =
                        Self::scan_cycle(&mut sys, &manager.vault, &mut state.since_map);
                    state.snapshot = snap.clone();
                    state.sources = src.clone();
                    state.lcu_args = args.clone();
                    (snap, src, args)
                };

                let _ = manager.snapshot_tx.send(snapshot);
                debug!(
                    "Discovery tick completed: {} sources, {} sockets",
                    sources.len(),
                    manager.state.read().unwrap().snapshot.sockets.len()
                );
            }
        })
    }

    /// Performs one single scan of processes, lockfiles, and network ports.
    pub fn scan_cycle(
        sys: &mut System,
        vault: &SecretVault,
        since_map: &mut HashMap<SourceId, i64>,
    ) -> (SystemSnapshot, Vec<SourceState>, Option<LeagueClientArgs>) {
        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        // 1. Process scan
        let (processes, lcu_args) = process::scan_processes(sys);

        // 2. Ports and connections scan for Riot PIDs
        let riot_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
        let (sockets, connections) = ports::collect_sockets_for_pids(&riot_pids);

        let snapshot = SystemSnapshot {
            ts: now_sec,
            processes: processes.clone(),
            sockets,
            connections,
        };

        // 3. Compute SourceStates
        let mut sources = Vec::with_capacity(4);

        // --- Riot Client Source ---
        let rc_proc = processes
            .iter()
            .find(|p| p.source == Some(SourceId::RiotClient));
        let rc_creds = lockfile::discover_riot_client_lockfile(vault);

        let (rc_status, rc_pid, rc_port, rc_proto, rc_cred) = match (rc_proc, rc_creds) {
            (Some(_proc), Some(creds)) => (
                SourceStatus::Live,
                Some(creds.pid),
                Some(creds.port),
                Some(creds.protocol),
                Some(creds.secret_ref),
            ),
            (Some(proc), None) => {
                // Process is running but lockfile could not be read yet
                (
                    SourceStatus::Connecting,
                    Some(proc.pid),
                    None,
                    None,
                    None,
                )
            }
            (None, _) => (SourceStatus::Offline, None, None, None, None),
        };

        let rc_since = update_since(since_map, SourceId::RiotClient, &rc_status, now_ms);
        sources.push(SourceState {
            id: SourceId::RiotClient,
            status: rc_status,
            pid: rc_pid,
            port: rc_port,
            protocol: rc_proto,
            since: rc_since,
            events_total: 0,
            bytes_total: 0,
            credential: rc_cred,
        });

        // --- League Client Source ---
        let lcu_proc = processes
            .iter()
            .find(|p| p.source == Some(SourceId::LeagueClient));

        let lcu_credentials: Option<(u32, u16, String, SecretRef)> = if lcu_proc.is_some() {
            // First priority: inspect command-line args of LeagueClientUx
            if let Some(args) = &lcu_args {
                if let (Some(port), Some(token)) = (args.app_port, &args.remoting_auth_token) {
                    let pid = args.app_pid.unwrap_or_else(|| lcu_proc.unwrap().pid);
                    let sref = vault.store(token.clone(), "lcu_cmdline_token".to_string());
                    Some((pid, port, "https".to_string(), sref))
                } else {
                    // Second priority: lockfile in install-directory or default path
                    lockfile::discover_league_client_lockfile(
                        args.install_directory.as_deref(),
                        vault,
                    )
                    .map(|c| (c.pid, c.port, c.protocol, c.secret_ref))
                }
            } else {
                lockfile::discover_league_client_lockfile(None, vault)
                    .map(|c| (c.pid, c.port, c.protocol, c.secret_ref))
            }
        } else {
            None
        };

        let (lc_status, lc_pid, lc_port, lc_proto, lc_cred) = match (lcu_proc, lcu_credentials) {
            (Some(_), Some((pid, port, proto, cred))) => {
                (SourceStatus::Live, Some(pid), Some(port), Some(proto), Some(cred))
            }
            (Some(proc), None) => {
                (SourceStatus::Connecting, Some(proc.pid), None, None, None)
            }
            (None, _) => (SourceStatus::Offline, None, None, None, None),
        };

        let lc_since = update_since(since_map, SourceId::LeagueClient, &lc_status, now_ms);
        sources.push(SourceState {
            id: SourceId::LeagueClient,
            status: lc_status,
            pid: lc_pid,
            port: lc_port,
            protocol: lc_proto,
            since: lc_since,
            events_total: 0,
            bytes_total: 0,
            credential: lc_cred,
        });

        // --- Live Game Source ---
        let live_proc = processes
            .iter()
            .find(|p| p.source == Some(SourceId::LiveGame));

        let (live_status, live_pid, live_port, live_proto) = match live_proc {
            Some(proc) => (
                SourceStatus::Live,
                Some(proc.pid),
                Some(2999),
                Some("https".to_string()),
            ),
            None => (SourceStatus::Offline, None, None, None),
        };

        let live_since = update_since(since_map, SourceId::LiveGame, &live_status, now_ms);
        sources.push(SourceState {
            id: SourceId::LiveGame,
            status: live_status,
            pid: live_pid,
            port: live_port,
            protocol: live_proto,
            since: live_since,
            events_total: 0,
            bytes_total: 0,
            credential: None,
        });

        // --- System Source ---
        let sys_status = SourceStatus::Live;
        let sys_since = update_since(since_map, SourceId::System, &sys_status, now_ms);
        sources.push(SourceState {
            id: SourceId::System,
            status: sys_status,
            pid: None,
            port: None,
            protocol: None,
            since: sys_since,
            events_total: 0,
            bytes_total: 0,
            credential: None,
        });

        (snapshot, sources, lcu_args)
    }

    /// Returns the latest `SystemSnapshot`.
    pub fn get_system_snapshot(&self) -> SystemSnapshot {
        self.state.read().unwrap().snapshot.clone()
    }

    /// Returns the latest list of `SourceState`s.
    pub fn get_sources(&self) -> Vec<SourceState> {
        self.state.read().unwrap().sources.clone()
    }

    /// Returns the `SourceState` for a specific source ID.
    pub fn get_source(&self, id: SourceId) -> Option<SourceState> {
        self.state
            .read()
            .unwrap()
            .sources
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    /// Returns the parsed LeagueClientUx arguments, if LeagueClientUx is running.
    pub fn lcu_args(&self) -> Option<LeagueClientArgs> {
        self.state.read().unwrap().lcu_args.clone()
    }

    /// Provides access to the associated `SecretVault`.
    pub fn vault(&self) -> &SecretVault {
        &self.vault
    }

    /// Subscribes to new `SystemSnapshot`s.
    pub fn subscribe_snapshot(&self) -> watch::Receiver<SystemSnapshot> {
        self.snapshot_tx.subscribe()
    }
}

fn update_since(
    since_map: &mut HashMap<SourceId, i64>,
    id: SourceId,
    status: &SourceStatus,
    now_ms: i64,
) -> i64 {
    match status {
        SourceStatus::Live | SourceStatus::Connecting | SourceStatus::Discovered => {
            *since_map.entry(id).or_insert(now_ms)
        }
        SourceStatus::Offline | SourceStatus::Degraded { .. } => {
            since_map.remove(&id);
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_manager_initialization() {
        let vault = SecretVault::new();
        let (manager, mut rx) = DiscoveryManager::new(vault);

        let snap = manager.get_system_snapshot();
        let sources = manager.get_sources();

        assert_eq!(sources.len(), 4);
        assert_eq!(sources[0].id, SourceId::RiotClient);
        assert_eq!(sources[1].id, SourceId::LeagueClient);
        assert_eq!(sources[2].id, SourceId::LiveGame);
        assert_eq!(sources[3].id, SourceId::System);

        assert_eq!(*rx.borrow_and_update(), snap);
    }
}
