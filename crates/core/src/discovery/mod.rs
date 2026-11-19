pub mod lockfile;
pub mod ports;
pub mod process;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};
use std::thread::Thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hypatia_shared::{
    ListenSocket, ProcessInfo, RemoteConn, SecretRef, SourceId, SourceState, SourceStatus,
    SystemSnapshot,
};
use sysinfo::System;
use tracing::info;

pub use lockfile::*;
pub use ports::*;
pub use process::*;

use crate::redaction::SecretVault;
use crate::settings::Flags;
use crate::signal::ChangeSignal;

/// Latest discovery result.
#[derive(Debug, Clone)]
pub struct DiscoveryState {
    pub snapshot: SystemSnapshot,
    pub sources: Vec<SourceState>,
    pub lcu_args: Option<LeagueClientArgs>,
}

/// Reusable OS handles and bookkeeping carried from one scan to the next.
struct Scanner {
    sys: Option<System>,
    sys_init_failed: bool,
    since: HashMap<SourceId, i64>,
}

impl Scanner {
    fn ensure_sys(&mut self) -> Option<&mut System> {
        if self.sys.is_none() && !self.sys_init_failed {
            match System::new() {
                Ok(sys) => self.sys = Some(sys),
                Err(err) => {
                    tracing::warn!("Failed to initialize sysinfo process scanner: {err}");
                    self.sys_init_failed = true;
                }
            }
        }
        self.sys.as_mut()
    }
}

/// Follows Riot processes, lockfiles and (optionally) their sockets, publishing a new
/// [`DiscoveryState`] only when something actually changed.
pub struct DiscoveryManager {
    state: RwLock<DiscoveryState>,
    scanner: Mutex<Scanner>,
    vault: SecretVault,
    signal: ChangeSignal,
    revision: AtomicU64,
    demo_override: Arc<AtomicBool>,
    loop_thread: OnceLock<Thread>,
}

fn epoch() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
}

impl DiscoveryManager {
    /// Creates a manager and performs an initial (lightweight) scan.
    pub fn new(vault: SecretVault, signal: ChangeSignal) -> Arc<Self> {
        let mut scanner = Scanner {
            sys: None,
            sys_init_failed: false,
            since: HashMap::new(),
        };
        scanner.ensure_sys();
        let state = Self::scan_cycle_opt(scanner.sys.as_mut(), &vault, &mut scanner.since, false);
        Self::with_state(state, scanner, vault, signal, false)
    }

    /// Creates a manager that has found nothing and never scans: tests that need "League is not
    /// running" stay true even when the developer's own client is open.
    #[cfg(test)]
    pub(crate) fn new_offline(vault: SecretVault, signal: ChangeSignal) -> Arc<Self> {
        let scanner = Scanner {
            sys: None,
            sys_init_failed: false,
            since: HashMap::new(),
        };
        let state = DiscoveryState {
            snapshot: SystemSnapshot {
                ts: 0,
                processes: Vec::new(),
                sockets: Vec::new(),
                connections: Vec::new(),
            },
            sources: Vec::new(),
            lcu_args: None,
        };
        Self::with_state(state, scanner, vault, signal, false)
    }

    /// Creates a manager pre-populated with realistic synthetic sources and processes.
    pub fn new_demo(vault: SecretVault, signal: ChangeSignal) -> Arc<Self> {
        let scanner = Scanner {
            sys: None,
            sys_init_failed: false,
            since: HashMap::new(),
        };
        let state = Self::build_demo_state(&vault);
        Self::with_state(state, scanner, vault, signal, true)
    }

    fn with_state(
        state: DiscoveryState,
        scanner: Scanner,
        vault: SecretVault,
        signal: ChangeSignal,
        demo: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: RwLock::new(state),
            scanner: Mutex::new(scanner),
            vault,
            signal,
            revision: AtomicU64::new(0),
            demo_override: Arc::new(AtomicBool::new(demo)),
            loop_thread: OnceLock::new(),
        })
    }

    fn build_demo_state(vault: &SecretVault) -> DiscoveryState {
        let now = epoch();
        let now_sec = now.as_secs() as i64;
        let now_ms = now.as_millis() as i64;

        let rc_cred = vault.store(
            "demo-riot-client-token-xyz789".to_string(),
            "rc_demo_token".to_string(),
        );
        let lcu_cred = vault.store(
            "demo-league-client-token-abc123".to_string(),
            "lcu_demo_token".to_string(),
        );

        let processes = vec![
            ProcessInfo {
                pid: 14200,
                name: "RiotClientServices.exe".to_string(),
                exe: Some("C:\\Riot Games\\Riot Client\\RiotClientServices.exe".to_string()),
                started: now_sec - 120,
                cpu: 0.3,
                mem_bytes: 88_500_000,
                source: Some(SourceId::RiotClient),
            },
            ProcessInfo {
                pid: 18452,
                name: "LeagueClientUx.exe".to_string(),
                exe: Some("C:\\Riot Games\\League of Legends\\LeagueClientUx.exe".to_string()),
                started: now_sec - 60,
                cpu: 1.2,
                mem_bytes: 345_000_000,
                source: Some(SourceId::LeagueClient),
            },
            ProcessInfo {
                pid: 19100,
                name: "LeagueCrashHandler64.exe".to_string(),
                exe: Some(
                    "C:\\Riot Games\\League of Legends\\LeagueCrashHandler64.exe".to_string(),
                ),
                started: now_sec - 60,
                cpu: 0.0,
                mem_bytes: 14_000_000,
                source: Some(SourceId::LeagueClient),
            },
            ProcessInfo {
                pid: 21004,
                name: "League of Legends.exe".to_string(),
                exe: Some(
                    "C:\\Riot Games\\League of Legends\\Game\\League of Legends.exe".to_string(),
                ),
                started: now_sec - 30,
                cpu: 4.8,
                mem_bytes: 1_850_000_000,
                source: Some(SourceId::LiveGame),
            },
        ];

        let sockets = vec![
            ListenSocket {
                pid: 14200,
                local: "127.0.0.1".to_string(),
                port: 54012,
                proto: "TCP".to_string(),
            },
            ListenSocket {
                pid: 18452,
                local: "127.0.0.1".to_string(),
                port: 58921,
                proto: "TCP".to_string(),
            },
            ListenSocket {
                pid: 21004,
                local: "127.0.0.1".to_string(),
                port: 2999,
                proto: "TCP".to_string(),
            },
        ];

        let connections = vec![
            RemoteConn {
                pid: 14200,
                local: "127.0.0.1:54012".to_string(),
                remote: "127.0.0.1:54013".to_string(),
                state: "ESTABLISHED".to_string(),
            },
            RemoteConn {
                pid: 18452,
                local: "127.0.0.1:58921".to_string(),
                remote: "127.0.0.1:58922".to_string(),
                state: "ESTABLISHED".to_string(),
            },
            RemoteConn {
                pid: 18452,
                local: "192.168.1.42:50123".to_string(),
                remote: "162.249.79.1:443".to_string(),
                state: "ESTABLISHED".to_string(),
            },
        ];

        let snapshot = SystemSnapshot {
            ts: now_sec,
            processes,
            sockets,
            connections,
        };

        let sources = vec![
            SourceState {
                id: SourceId::RiotClient,
                status: SourceStatus::Live,
                pid: Some(14200),
                port: Some(54012),
                protocol: Some("https".to_string()),
                since: now_ms - 120_000,
                events_total: 0,
                bytes_total: 0,
                credential: Some(rc_cred),
            },
            SourceState {
                id: SourceId::LeagueClient,
                status: SourceStatus::Live,
                pid: Some(18452),
                port: Some(58921),
                protocol: Some("https".to_string()),
                since: now_ms - 60_000,
                events_total: 0,
                bytes_total: 0,
                credential: Some(lcu_cred),
            },
            SourceState {
                id: SourceId::LiveGame,
                status: SourceStatus::Live,
                pid: Some(21004),
                port: Some(2999),
                protocol: Some("https".to_string()),
                since: now_ms - 30_000,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            },
            SourceState {
                id: SourceId::System,
                status: SourceStatus::Live,
                pid: None,
                port: None,
                protocol: None,
                since: now_ms - 120_000,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            },
        ];

        let lcu_args = Some(LeagueClientArgs {
            app_port: Some(58921),
            remoting_auth_token: Some("demo-league-client-token-abc123".to_string()),
            install_directory: Some(std::path::PathBuf::from(
                "C:\\Riot Games\\League of Legends",
            )),
            riotclient_app_port: Some(54012),
            riotclient_auth_token: Some("demo-riot-client-token-xyz789".to_string()),
            app_pid: Some(18452),
        });

        DiscoveryState {
            snapshot,
            sources,
            lcu_args,
        }
    }

    /// Enables or disables synthetic demo discovery state at runtime.
    pub fn set_demo_override(&self, enabled: bool) {
        self.demo_override.store(enabled, Ordering::SeqCst);
        if enabled {
            *self.state.write().unwrap() = Self::build_demo_state(&self.vault);
            self.published();
        } else {
            self.rescan(false);
        }
    }

    pub fn is_demo_override(&self) -> bool {
        self.demo_override.load(Ordering::SeqCst)
    }

    /// Shared flag telling whether synthetic demo data is active.
    pub fn demo_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.demo_override)
    }

    /// Scans now and publishes the result if it differs from the current state.
    /// Returns whether any Riot process is running.
    fn rescan(&self, detailed: bool) -> bool {
        let fresh = {
            let mut scanner = self.scanner.lock().unwrap();
            scanner.ensure_sys();
            let Scanner { sys, since, .. } = &mut *scanner;
            Self::scan_cycle_opt(sys.as_mut(), &self.vault, since, detailed)
        };
        let active = !fresh.snapshot.processes.is_empty();
        let changed = {
            let mut state = self.state.write().unwrap();
            let changed = state.sources != fresh.sources
                || state.lcu_args != fresh.lcu_args
                || state.snapshot.processes != fresh.snapshot.processes
                || state.snapshot.sockets != fresh.snapshot.sockets
                || state.snapshot.connections != fresh.snapshot.connections;
            if changed {
                *state = fresh;
            }
            changed
        };
        if changed {
            self.published();
        }
        active
    }

    fn published(&self) {
        self.revision.fetch_add(1, Ordering::Relaxed);
        self.signal.bump();
    }

    /// Bumped every time a different [`DiscoveryState`] is published.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    /// True when `id` is `Live` on this machine (never in synthetic demo mode).
    pub fn is_live_real(&self, id: SourceId) -> bool {
        !self.is_demo_override() && self.is_live(id)
    }

    /// Requests an immediate scan from the background loop (e.g. after a settings change).
    pub fn wake(&self) {
        if let Some(thread) = self.loop_thread.get() {
            thread.unpark();
        }
    }

    /// Starts the background discovery loop on a dedicated, mostly parked OS thread.
    ///
    /// The loop scans every second while a Riot process runs and backs off when none does;
    /// it exits once the manager is dropped.
    pub fn spawn_loop(self: &Arc<Self>, flags: Arc<Flags>) {
        let weak: Weak<Self> = Arc::downgrade(self);
        let spawned = std::thread::Builder::new()
            .name("hypatia-discovery".into())
            .spawn(move || {
                info!("Discovery loop started");
                let mut interval = flags.discovery_rate().active_interval();
                loop {
                    std::thread::park_timeout(interval);
                    let Some(manager) = weak.upgrade() else {
                        break;
                    };
                    // Synthetic demo state is static: nothing to scan while it is active.
                    let active =
                        !manager.is_demo_override() && manager.rescan(flags.system_footprint());
                    let rate = flags.discovery_rate();
                    interval = if active {
                        rate.active_interval()
                    } else {
                        rate.idle_interval()
                    };
                }
            });
        match spawned {
            Ok(handle) => {
                let _ = self.loop_thread.set(handle.thread().clone());
            }
            Err(e) => tracing::error!("Failed to start discovery thread: {e}"),
        }
    }

    /// Performs one scan of processes and lockfiles. With `detailed`, also samples per-process
    /// CPU/RAM and enumerates sockets (only needed by the Telemetry Lab system view).
    pub fn scan_cycle(
        sys: &mut System,
        vault: &SecretVault,
        since_map: &mut HashMap<SourceId, i64>,
        detailed: bool,
    ) -> DiscoveryState {
        Self::scan_cycle_opt(Some(sys), vault, since_map, detailed)
    }

    fn scan_cycle_opt(
        sys: Option<&mut System>,
        vault: &SecretVault,
        since_map: &mut HashMap<SourceId, i64>,
        detailed: bool,
    ) -> DiscoveryState {
        let now = epoch();
        let now_ms = now.as_millis() as i64;

        let (processes, lcu_args) = match sys {
            Some(sys) => process::scan_processes(sys, detailed),
            None => (Vec::new(), None),
        };
        let (sockets, connections) = if detailed {
            let riot_pids: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
            ports::collect_sockets_for_pids(&riot_pids)
        } else {
            (Vec::new(), Vec::new())
        };

        let proc_of = |id: SourceId| processes.iter().find(|p| p.source == Some(id));
        let mut source = |id: SourceId, proc: Option<&ProcessInfo>, endpoint: Option<Endpoint>| {
            let status = match (proc, &endpoint) {
                (None, _) if id != SourceId::System => SourceStatus::Offline,
                (Some(_), None) if id != SourceId::LiveGame => SourceStatus::Connecting,
                _ => SourceStatus::Live,
            };
            let since = update_since(since_map, id, &status, now_ms);
            let live = status == SourceStatus::Live;
            let endpoint = endpoint.filter(|_| live);
            SourceState {
                id,
                pid: endpoint.as_ref().map(|e| e.pid).or(proc.map(|p| p.pid)),
                port: endpoint.as_ref().map(|e| e.port),
                protocol: endpoint.as_ref().map(|e| e.protocol.clone()),
                since,
                events_total: 0,
                bytes_total: 0,
                credential: endpoint.and_then(|e| e.credential),
                status,
            }
        };

        // Riot Client: credentials always come from its lockfile.
        let rc_proc = proc_of(SourceId::RiotClient);
        let rc_endpoint = rc_proc
            .and_then(|_| lockfile::discover_riot_client_lockfile(vault))
            .map(Endpoint::from);

        // League Client: command-line arguments first, lockfile as a fallback.
        let lcu_proc = proc_of(SourceId::LeagueClient);
        let lcu_endpoint = lcu_proc.and_then(|proc| {
            let args = lcu_args.as_ref();
            match args.and_then(|a| Some((a.app_port?, a.remoting_auth_token.as_ref()?))) {
                Some((port, token)) => Some(Endpoint {
                    pid: args.and_then(|a| a.app_pid).unwrap_or(proc.pid),
                    port,
                    protocol: "https".to_string(),
                    credential: Some(vault.store(token.clone(), "lcu_cmdline_token".to_string())),
                }),
                None => lockfile::discover_league_client_lockfile(
                    args.and_then(|a| a.install_directory.as_deref()),
                    vault,
                )
                .map(Endpoint::from),
            }
        });

        let live_proc = proc_of(SourceId::LiveGame);
        let live_endpoint = live_proc.map(|p| Endpoint {
            pid: p.pid,
            port: 2999,
            protocol: "https".to_string(),
            credential: None,
        });

        let sources = vec![
            source(SourceId::RiotClient, rc_proc, rc_endpoint),
            source(SourceId::LeagueClient, lcu_proc, lcu_endpoint),
            source(SourceId::LiveGame, live_proc, live_endpoint),
            source(SourceId::System, None, None),
        ];

        DiscoveryState {
            snapshot: SystemSnapshot {
                ts: now.as_secs() as i64,
                processes,
                sockets,
                connections,
            },
            sources,
            lcu_args,
        }
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

    /// True when `id` is currently `Live`.
    pub fn is_live(&self, id: SourceId) -> bool {
        self.state
            .read()
            .unwrap()
            .sources
            .iter()
            .any(|s| s.id == id && s.status == SourceStatus::Live)
    }

    /// Port and plaintext credential of a live authenticated source (League or Riot Client).
    pub fn credentials(&self, id: SourceId) -> Option<(u16, String)> {
        let (port, secret_id) = {
            let state = self.state.read().unwrap();
            let src = state.sources.iter().find(|s| s.id == id)?;
            (src.port?, src.credential.as_ref()?.id)
        };
        Some((port, self.vault.reveal(secret_id)?))
    }

    /// Returns the parsed LeagueClientUx arguments, if LeagueClientUx is running.
    pub fn lcu_args(&self) -> Option<LeagueClientArgs> {
        self.state.read().unwrap().lcu_args.clone()
    }

    /// Provides access to the associated `SecretVault`.
    pub fn vault(&self) -> &SecretVault {
        &self.vault
    }
}

/// Where and how to reach a discovered source.
struct Endpoint {
    pid: u32,
    port: u16,
    protocol: String,
    credential: Option<SecretRef>,
}

impl From<DiscoveredCredentials> for Endpoint {
    fn from(c: DiscoveredCredentials) -> Self {
        Self {
            pid: c.pid,
            port: c.port,
            protocol: c.protocol,
            credential: Some(c.secret_ref),
        }
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
    fn initial_scan_reports_all_four_sources_in_order() {
        let manager = DiscoveryManager::new(SecretVault::new(), ChangeSignal::new());
        let ids: Vec<SourceId> = manager.get_sources().iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            [
                SourceId::RiotClient,
                SourceId::LeagueClient,
                SourceId::LiveGame,
                SourceId::System
            ]
        );
        assert!(manager.is_live(SourceId::System));
    }

    #[test]
    fn demo_override_publishes_synthetic_state_and_signals() {
        let signal = ChangeSignal::new();
        let manager = DiscoveryManager::new(SecretVault::new(), signal.clone());
        let before = signal.revision();
        manager.set_demo_override(true);
        assert!(signal.revision() > before);
        assert!(manager.is_live(SourceId::LeagueClient));
        let (port, password) = manager.credentials(SourceId::LeagueClient).unwrap();
        assert_eq!(port, 58921);
        assert_eq!(password, "demo-league-client-token-abc123");
    }
}
