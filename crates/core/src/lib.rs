pub mod assets;
pub mod batcher;
pub mod catalog;
pub mod connectors;
pub mod discovery;
pub mod fixtures;
pub mod lcu_actions;
pub mod redaction;
pub mod remote;
pub mod riot_http;
pub mod store;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use hypatia_shared::{
    project_match, Lang, MatchInputs, ServerMsg, Snapshot, SourceId, SummarySnapshot,
};
use tokio::sync::{broadcast, watch};
use tracing::info;

use crate::assets::{load_persisted_or_detect_lang, GameAssetCache};
use crate::batcher::spawn_batcher;
use crate::catalog::CatalogStore;
use crate::connectors::{spawn_live_client_poller, spawn_replay_poller, spawn_wamp_connector_full};
use crate::discovery::DiscoveryManager;
use crate::fixtures::spawn_demo_generator_with_remote;
use crate::lcu_actions::DemoRemoteState;
use crate::redaction::SecretVault;
use crate::riot_http::ReadOnlyRiotClient;
use crate::store::EventStore;

#[derive(Clone)]
pub struct AppState {
    pub discovery: Arc<DiscoveryManager>,
    pub vault: SecretVault,
    pub store: Arc<RwLock<EventStore>>,
    pub catalog: Arc<RwLock<CatalogStore>>,
    pub riot_client: ReadOnlyRiotClient,
    pub assets: Arc<GameAssetCache>,
    pub ws_broadcast_tx: broadcast::Sender<ServerMsg>,
    pub demo_remote: Arc<RwLock<DemoRemoteState>>,
    pub is_demo: bool,
    demo_spawned: Arc<AtomicBool>,
    lang_tx: watch::Sender<Lang>,
}

impl AppState {
    /// Starts discovery, the event store and every connector (or the synthetic
    /// generator in demo mode) using persisted or OS-detected language. Must be called from a Tokio runtime.
    pub fn start(demo: bool, demo_rate: f32) -> Self {
        Self::start_with_lang(demo, demo_rate, load_persisted_or_detect_lang())
    }

    /// Starts Hypatia core with an explicit initial UI/API language.
    pub fn start_with_lang(demo: bool, demo_rate: f32, initial_lang: Lang) -> Self {
        let vault = SecretVault::new();
        let (discovery, _snapshot_rx) = if demo {
            info!("Demo mode activated (synthetic data, rate: {:.1} evt/s)", demo_rate);
            DiscoveryManager::new_demo(vault.clone())
        } else {
            let (disc, rx) = DiscoveryManager::new(vault.clone());
            disc.spawn_loop(Duration::from_millis(1000));
            (disc, rx)
        };

        let (store_inner, event_rx) = EventStore::new();
        let store = Arc::new(RwLock::new(store_inner));
        let (ws_broadcast_tx, _ws_broadcast_rx) = broadcast::channel(1024);
        let riot_client = ReadOnlyRiotClient::new();
        let assets = Arc::new(GameAssetCache::new());
        let catalog = Arc::new(RwLock::new(CatalogStore::new()));
        let demo_remote = Arc::new(RwLock::new(DemoRemoteState::default()));
        let demo_spawned = Arc::new(AtomicBool::new(demo));
        let (lang_tx, _lang_rx) = watch::channel(initial_lang);

        spawn_batcher(event_rx, store.clone(), discovery.clone(), ws_broadcast_tx.clone());

        if demo {
            spawn_demo_generator_with_remote(
                store.clone(),
                catalog.clone(),
                vault.clone(),
                demo_rate,
                Some(demo_remote.clone()),
            );
        } else {
            for source in [SourceId::RiotClient, SourceId::LeagueClient] {
                spawn_wamp_connector_full(
                    source,
                    discovery.clone(),
                    store.clone(),
                    vault.clone(),
                    Some(catalog.clone()),
                    Some(riot_client.clone()),
                    Some(ws_broadcast_tx.clone()),
                );
            }
            spawn_live_client_poller(
                discovery.clone(),
                store.clone(),
                catalog.clone(),
                vault.clone(),
                riot_client.clone(),
                ws_broadcast_tx.clone(),
            );
            spawn_replay_poller(
                discovery.clone(),
                store.clone(),
                catalog.clone(),
                vault.clone(),
                riot_client.clone(),
                ws_broadcast_tx.clone(),
            );
        }

        Self {
            discovery,
            vault,
            store,
            catalog,
            riot_client,
            assets,
            ws_broadcast_tx,
            demo_remote,
            is_demo: demo,
            demo_spawned,
            lang_tx,
        }
    }

    /// Returns the active display/protocol language.
    pub fn lang(&self) -> Lang {
        *self.lang_tx.borrow()
    }

    /// Updates the active language and notifies subscribers (such as the mobile remote server).
    pub fn set_lang(&self, lang: Lang) {
        let _ = self.lang_tx.send_if_modified(|cur| {
            if *cur == lang {
                false
            } else {
                *cur = lang;
                true
            }
        });
    }

    /// Subscribes to live language changes.
    pub fn subscribe_lang(&self) -> watch::Receiver<Lang> {
        self.lang_tx.subscribe()
    }

    /// Returns whether synthetic Demo mode is currently active.
    pub fn is_demo_active(&self) -> bool {
        self.is_demo || self.discovery.is_demo_override()
    }

    /// Activates or deactivates synthetic Demo mode at runtime.
    pub fn set_demo_active(&self, active: bool, demo_rate: f32) {
        self.discovery.set_demo_override(active);
        if active && !self.demo_spawned.swap(true, Ordering::SeqCst) {
            spawn_demo_generator_with_remote(
                self.store.clone(),
                self.catalog.clone(),
                self.vault.clone(),
                demo_rate,
                Some(self.demo_remote.clone()),
            );
        }
    }

    /// Assembles a complete current system Snapshot.
    pub fn get_snapshot(&self) -> Snapshot {
        let mut sources = self.discovery.get_sources();
        {
            let guard = self.store.read().unwrap();
            for s in &mut sources {
                let (events_total, bytes_total) = guard.source_stats(s.id);
                s.events_total = events_total;
                s.bytes_total = bytes_total;
            }
        }
        let guard = self.store.read().unwrap();
        Snapshot {
            sources,
            phase: guard.phase(),
            recent_events: guard.get_recent_meta(500),
            system: self.discovery.get_system_snapshot(),
        }
    }

    /// Assembles a synthesized SummarySnapshot from cached URI states.
    pub fn get_summary(&self) -> SummarySnapshot {
        let mut sources = self.discovery.get_sources();
        let guard = self.store.read().unwrap();
        for s in &mut sources {
            let (events_total, bytes_total) = guard.source_stats(s.id);
            s.events_total = events_total;
            s.bytes_total = bytes_total;
        }

        let lcu = |uri: &str| guard.get_state(SourceId::LeagueClient, uri).map(|u| u.current);
        let summoner_val = lcu("/lol-summoner/v1/current-summoner");
        let ranked_val = lcu("/lol-ranked/v1/current-ranked-stats");
        let lobby_val = lcu("/lol-lobby/v2/lobby");
        let search_val = lcu("/lol-matchmaking/v1/search");
        let ready_val = lcu("/lol-matchmaking/v1/ready-check");
        let cs_val = lcu("/lol-champ-select/v1/session");
        let eog_val = lcu("/lol-end-of-game/v1/eog-stats-block");
        let presences_val = guard
            .get_state(SourceId::RiotClient, "/chat/v4/presences")
            .map(|u| u.current);
        let live = |uri: &str| guard.get_state(SourceId::LiveGame, uri).map(|u| u.current);
        let allgame_val = live("/liveclientdata/allgamedata");
        let active_val = live("/liveclientdata/activeplayer");
        let events_val = live("/liveclientdata/eventdata");
        let phase = guard.phase();
        drop(guard);

        let inputs = MatchInputs {
            phase,
            sources,
            summoner: summoner_val.as_ref(),
            ranked: ranked_val.as_ref(),
            presences: presences_val.as_ref(),
            lobby: lobby_val.as_ref(),
            matchmaking_search: search_val.as_ref(),
            ready_check: ready_val.as_ref(),
            champ_select: cs_val.as_ref(),
            live_allgame: allgame_val.as_ref(),
            live_active: active_val.as_ref(),
            live_events: events_val.as_ref(),
            end_of_game: eog_val.as_ref(),
        };

        project_match(&inputs, &*self.assets, self.lang())
    }

    /// Clears all buffered events in the EventStore.
    pub fn clear_events(&self) {
        self.store.write().unwrap().clear_events();
    }

    /// Returns a clone of `val` with any `{"$secret": ...}` entries revealed from the vault.
    pub fn reveal_secrets_in_value(&self, val: &serde_json::Value) -> serde_json::Value {
        crate::redaction::reveal_value(val, &self.vault)
    }

    /// Performs a read-only GET call against the specified Riot source (or returns cached/synthetic
    /// state in Demo mode), records it as `EventKind::Call` in the store and catalog, and returns
    /// `(http_status, redacted_json, event_id)`.
    pub async fn call_endpoint(
        &self,
        source_id: SourceId,
        path: &str,
    ) -> Result<(u16, serde_json::Value, u64), String> {
        let lang = self.lang();
        let clean_path = path.trim();
        if !clean_path.starts_with('/') {
            return Err(lang
                .pick(
                    "Path must start with '/' (e.g. /lol-summoner/v1/current-summoner)",
                    "Le chemin doit commencer par '/' (ex. /lol-summoner/v1/current-summoner)",
                )
                .to_string());
        }
        if source_id == SourceId::System {
            return Err(lang
                .pick(
                    "Cannot execute HTTP GET on System source",
                    "Impossible d'exécuter un HTTP GET sur la source Système",
                )
                .to_string());
        }

        // If in demo mode (or if source is not live but we have cached demo state), serve from store/synthetic
        if self.is_demo_active() {
            let cached = {
                let guard = self.store.read().unwrap();
                guard.get_state(source_id, clean_path).map(|u| u.current)
            };
            let body = cached.unwrap_or_else(|| {
                serde_json::json!({
                    "demo": true,
                    "source": source_id.to_string(),
                    "path": clean_path,
                    "note": lang.pick("Synthetic demo response for GET tester", "Réponse de démonstration synthétique pour le testeur GET")
                })
            });
            let meta = {
                let mut guard = self.store.write().unwrap();
                guard.insert_event(
                    source_id,
                    hypatia_shared::EventKind::Call,
                    clean_path.to_string(),
                    body.clone(),
                    &self.vault,
                )
            };
            {
                let mut cat = self.catalog.write().unwrap();
                cat.record_observed(source_id, "GET", clean_path, meta.size as u64);
            }
            let redacted = self
                .store
                .read()
                .unwrap()
                .get_event(meta.id)
                .map(|e| e.payload)
                .unwrap_or(body);
            return Ok((200, redacted, meta.id));
        }

        let (port, auth_password) = match source_id {
            SourceId::RiotClient => {
                let src = self.discovery.get_source(SourceId::RiotClient);
                let port = src.as_ref().and_then(|s| s.port);
                let cred = src.as_ref().and_then(|s| s.credential.clone());
                let pwd = cred.and_then(|c| self.vault.reveal(c.id));
                match (port, pwd) {
                    (Some(p), Some(pwd)) => (p, Some(pwd)),
                    _ => return Err(lang
                        .pick(
                            "RiotClient is not running or lockfile credentials unavailable",
                            "RiotClient n'est pas en cours d'exécution ou les identifiants du lockfile sont indisponibles",
                        )
                        .to_string()),
                }
            }
            SourceId::LeagueClient => {
                let src = self.discovery.get_source(SourceId::LeagueClient);
                let port = src.as_ref().and_then(|s| s.port);
                let cred = src.as_ref().and_then(|s| s.credential.clone());
                let pwd = cred.and_then(|c| self.vault.reveal(c.id));
                match (port, pwd) {
                    (Some(p), Some(pwd)) => (p, Some(pwd)),
                    _ => return Err(lang
                        .pick(
                            "LeagueClient (LCU) is not running or lockfile credentials unavailable",
                            "LeagueClient (LCU) n'est pas en cours d'exécution ou les identifiants du lockfile sont indisponibles",
                        )
                        .to_string()),
                }
            }
            SourceId::LiveGame => (2999, None),
            SourceId::System => unreachable!(),
        };

        let (status, _headers, mut body) = self
            .riot_client
            .get(port, auth_password.as_deref(), clean_path)
            .await
            .map_err(|e| format!("{}: {}", lang.pick("GET request failed", "Échec de la requête GET"), e))?;

        let meta = {
            let mut guard = self.store.write().unwrap();
            guard.insert_event(
                source_id,
                hypatia_shared::EventKind::Call,
                clean_path.to_string(),
                body.clone(),
                &self.vault,
            )
        };

        {
            let mut cat = self.catalog.write().unwrap();
            cat.record_observed(source_id, "GET", clean_path, meta.size as u64);
        }

        let redacted = self
            .store
            .read()
            .unwrap()
            .get_event(meta.id)
            .map(|e| e.payload)
            .unwrap_or_else(|| {
                crate::redaction::redact_value(&mut body, &self.vault);
                body
            });

        Ok((status.as_u16(), redacted, meta.id))
    }
}

/// Computes a human-readable line-by-line JSON diff between an optional previous JSON value
/// and a current JSON value in the requested language.
pub fn compute_json_diff_localized(
    prev: Option<&serde_json::Value>,
    curr: &serde_json::Value,
    lang: Lang,
) -> String {
    let curr_pretty = serde_json::to_string_pretty(curr).unwrap_or_else(|_| "{}".to_string());
    let Some(prev_val) = prev else {
        return format!(
            "{}\n{}\n{}",
            lang.pick(
                "--- (no previous state for this URI — initial snapshot)",
                "--- (aucun état précédent pour cette URI — instantané initial)",
            ),
            lang.pick("+++ current", "+++ actuel"),
            curr_pretty
                .lines()
                .map(|l| format!("+ {}", l))
                .collect::<Vec<_>>()
                .join("\n")
        );
    };
    if prev_val == curr {
        return format!(
            "{}\n\n{}",
            lang.pick(
                "(No changes between previous and current payload)",
                "(Aucune modification entre le payload précédent et l'actuel)",
            ),
            curr_pretty
        );
    }

    let prev_pretty = serde_json::to_string_pretty(prev_val).unwrap_or_else(|_| "{}".to_string());
    let prev_lines: Vec<&str> = prev_pretty.lines().collect();
    let curr_lines: Vec<&str> = curr_pretty.lines().collect();

    let mut out = Vec::new();
    out.push(lang.pick("--- previous payload", "--- payload précédent").to_string());
    out.push(lang.pick("+++ current payload", "+++ payload actuel").to_string());

    let max_len = prev_lines.len().max(curr_lines.len());
    let mut i = 0;
    let mut j = 0;
    while i < prev_lines.len() || j < curr_lines.len() {
        match (prev_lines.get(i), curr_lines.get(j)) {
            (Some(&p), Some(&c)) if p == c => {
                out.push(format!("  {}", c));
                i += 1;
                j += 1;
            }
            (Some(&p), Some(&c)) => {
                // Look ahead up to 4 lines for resync
                if let Some(offset) = (1..=4).find(|&off| curr_lines.get(j + off) == Some(&p)) {
                    for k in 0..offset {
                        out.push(format!("+ {}", curr_lines[j + k]));
                    }
                    j += offset;
                } else if let Some(offset) = (1..=4).find(|&off| prev_lines.get(i + off) == Some(&c)) {
                    for k in 0..offset {
                        out.push(format!("- {}", prev_lines[i + k]));
                    }
                    i += offset;
                } else {
                    out.push(format!("- {}", p));
                    out.push(format!("+ {}", c));
                    i += 1;
                    j += 1;
                }
            }
            (Some(&p), None) => {
                out.push(format!("- {}", p));
                i += 1;
            }
            (None, Some(&c)) => {
                out.push(format!("+ {}", c));
                j += 1;
            }
            (None, None) => break,
        }
        if out.len() > max_len * 2 + 32 {
            break;
        }
    }

    out.join("\n")
}
