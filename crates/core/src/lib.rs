pub mod art;
pub mod assets;
pub mod catalog;
pub mod connectors;
pub mod diff;
pub mod discovery;
pub mod fixtures;
pub mod lcu_actions;
pub mod redaction;
pub mod remote;
pub mod riot_http;
pub mod settings;
pub mod signal;
pub mod store;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use hypatia_shared::{
    project_match, EventKind, Lang, MatchInputs, Snapshot, SourceId, SummarySnapshot,
};
use serde_json::Value;
use tracing::info;

use crate::assets::{load_persisted_or_detect_lang, GameAssetCache};
use crate::catalog::CatalogStore;
use crate::connectors::{
    spawn_live_client_poller, spawn_replay_poller, spawn_wamp_connector, ConnectorContext,
};
pub use crate::diff::compute_json_diff_localized;
use crate::discovery::DiscoveryManager;
use crate::fixtures::spawn_demo_generator;
use crate::lcu_actions::DemoRemoteState;
use crate::redaction::SecretVault;
use crate::riot_http::ReadOnlyRiotClient;
use crate::settings::{Flags, Settings};
use crate::signal::ChangeSignal;
use crate::store::EventStore;

/// Inputs the Match Projection depends on; equal keys guarantee an identical projection.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ProjectionKey {
    state: u64,
    discovery: u64,
    champions: u64,
    lang: Lang,
}

/// Last projection together with the inputs it was computed from.
type ProjectionCache = Mutex<Option<(ProjectionKey, Arc<SummarySnapshot>)>>;

#[derive(Clone)]
pub struct AppState {
    pub discovery: Arc<DiscoveryManager>,
    pub vault: SecretVault,
    pub store: Arc<RwLock<EventStore>>,
    pub catalog: Arc<RwLock<CatalogStore>>,
    pub riot_client: ReadOnlyRiotClient,
    pub assets: Arc<GameAssetCache>,
    pub demo_remote: Arc<RwLock<DemoRemoteState>>,
    /// Runtime feature switches read by every background task.
    pub flags: Arc<Flags>,
    /// Bumped whenever anything a view could display has changed.
    pub signal: ChangeSignal,
    demo_spawned: Arc<AtomicBool>,
    lang: Arc<AtomicU8>,
    projection: Arc<ProjectionCache>,
}

impl AppState {
    /// Starts Hypatia core with default settings and the persisted or OS-detected language.
    /// Must be called from a Tokio runtime.
    pub fn start(demo: bool, demo_rate: f32) -> Self {
        Self::start_with_lang(demo, demo_rate, load_persisted_or_detect_lang())
    }

    /// Starts Hypatia core with default settings and an explicit language.
    pub fn start_with_lang(demo: bool, demo_rate: f32, lang: Lang) -> Self {
        Self::start_with(demo, demo_rate, lang, &Settings::default())
    }

    /// Starts discovery, the event store and every connector (plus the synthetic generator in
    /// demo mode). Nothing is recorded beyond the Match Projection until
    /// [`Self::set_telemetry`] opens the Telemetry Lab.
    pub fn start_with(demo: bool, demo_rate: f32, lang: Lang, settings: &Settings) -> Self {
        let signal = ChangeSignal::new();
        let vault = SecretVault::new();
        let flags = Arc::new(Flags::new(settings));

        let discovery = if demo {
            info!("Demo mode activated (synthetic data, rate: {demo_rate:.1} evt/s)");
            DiscoveryManager::new_demo(vault.clone(), signal.clone())
        } else {
            DiscoveryManager::new(vault.clone(), signal.clone())
        };
        discovery.spawn_loop(flags.clone());

        let mut store = EventStore::new(signal.clone());
        store.set_capture(false);

        let state = Self {
            discovery,
            vault,
            store: Arc::new(RwLock::new(store)),
            catalog: Arc::new(RwLock::new(CatalogStore::new())),
            riot_client: ReadOnlyRiotClient::new(),
            assets: Arc::new(GameAssetCache::new(signal.clone())),
            demo_remote: Arc::new(RwLock::new(DemoRemoteState::default())),
            flags,
            signal,
            demo_spawned: Arc::new(AtomicBool::new(false)),
            lang: Arc::new(AtomicU8::new(lang as u8)),
            projection: Arc::new(Mutex::new(None)),
        };

        let ctx = state.connector_context();
        for source in [SourceId::RiotClient, SourceId::LeagueClient] {
            spawn_wamp_connector(source, ctx.clone());
        }
        spawn_live_client_poller(ctx.clone());
        spawn_replay_poller(ctx);
        if demo {
            state.ensure_demo_generator(demo_rate);
        }
        state
    }

    fn connector_context(&self) -> ConnectorContext {
        ConnectorContext {
            discovery: self.discovery.clone(),
            store: self.store.clone(),
            catalog: self.catalog.clone(),
            vault: self.vault.clone(),
            client: self.riot_client.clone(),
            flags: self.flags.clone(),
            signal: self.signal.clone(),
        }
    }

    fn ensure_demo_generator(&self, demo_rate: f32) {
        if !self.demo_spawned.swap(true, Ordering::SeqCst) {
            spawn_demo_generator(
                self.store.clone(),
                self.catalog.clone(),
                self.vault.clone(),
                demo_rate,
                self.demo_remote.clone(),
                self.discovery.demo_flag(),
            );
        }
    }

    /// Returns the active display/protocol language.
    pub fn lang(&self) -> Lang {
        if self.lang.load(Ordering::Relaxed) == Lang::Fr as u8 {
            Lang::Fr
        } else {
            Lang::En
        }
    }

    /// Updates the active language and notifies every view.
    pub fn set_lang(&self, lang: Lang) {
        if self.lang.swap(lang as u8, Ordering::Relaxed) != lang as u8 {
            self.signal.bump();
        }
    }

    /// Applies user settings to every running background task.
    pub fn apply_settings(&self, settings: &Settings) {
        self.flags.apply(settings);
        self.discovery.wake();
        self.signal.bump();
    }

    /// Opens or closes the Telemetry Lab: toggles full event capture, the complete WAMP
    /// stream, endpoint catalog import and system footprint scans. Closing frees the history.
    pub fn set_telemetry(&self, on: bool) {
        self.flags.set_telemetry(on);
        self.store.write().unwrap().set_capture(on);
        self.discovery.wake();
    }

    /// Returns whether synthetic Demo mode is currently active.
    pub fn is_demo_active(&self) -> bool {
        self.discovery.is_demo_override()
    }

    /// Activates or deactivates synthetic Demo mode at runtime.
    ///
    /// Every transition wipes the observed state so synthetic payloads never linger on screen
    /// after Demo stops (and real ones never mix into the Demo). Real connectors re-bootstrap
    /// their state when they reconnect; the generator re-seeds its baseline when it resumes.
    pub fn set_demo_active(&self, active: bool, demo_rate: f32) {
        if self.is_demo_active() == active {
            return;
        }
        self.discovery.set_demo_override(active);
        *self.demo_remote.write().unwrap() = DemoRemoteState::default();
        self.store.write().unwrap().reset_state();
        if active {
            self.ensure_demo_generator(demo_rate);
        }
    }

    /// Assembles the Telemetry Lab snapshot (sources with traffic counters, newest events,
    /// system footprint).
    pub fn get_snapshot(&self) -> Snapshot {
        let mut sources = self.discovery.get_sources();
        let store = self.store.read().unwrap();
        for s in &mut sources {
            (s.events_total, s.bytes_total) = store.source_stats(s.id);
        }
        Snapshot {
            sources,
            phase: store.phase(),
            recent_events: store.get_recent_meta(500),
            system: self.discovery.get_system_snapshot(),
        }
    }

    /// Returns the current Match Projection.
    ///
    /// The projection is recomputed only when one of its inputs changed (a projected Riot
    /// payload, discovery, champion metadata or language); otherwise the cached snapshot is
    /// returned, so callers can compare `Arc` pointers to detect "nothing changed".
    pub fn get_summary(&self) -> Arc<SummarySnapshot> {
        let lang = self.lang();
        let mut cache = self.projection.lock().unwrap();

        let store = self.store.read().unwrap();
        let key = ProjectionKey {
            state: store.state_revision(),
            discovery: self.discovery.revision(),
            champions: self.assets.metadata_revision(),
            lang,
        };
        if let Some((cached_key, snapshot)) = cache.as_ref() {
            if *cached_key == key {
                return Arc::clone(snapshot);
            }
        }

        let lcu = |uri: &str| store.current(SourceId::LeagueClient, uri);
        let live = |uri: &str| store.current(SourceId::LiveGame, uri);
        let summoner = lcu("/lol-summoner/v1/current-summoner");
        let ranked = lcu("/lol-ranked/v1/current-ranked-stats");
        let lobby = lcu("/lol-lobby/v2/lobby");
        let search = lcu("/lol-matchmaking/v1/search");
        let ready_check = lcu("/lol-matchmaking/v1/ready-check");
        let champ_select = lcu("/lol-champ-select/v1/session");
        let end_of_game = lcu("/lol-end-of-game/v1/eog-stats-block");
        let presences = store.current(SourceId::RiotClient, "/chat/v4/presences");
        let allgame = live("/liveclientdata/allgamedata");
        let active = live("/liveclientdata/activeplayer");
        let events = live("/liveclientdata/eventdata");
        let phase = store.phase();
        drop(store);

        let inputs = MatchInputs {
            phase,
            sources: self.discovery.get_sources(),
            summoner: summoner.as_deref(),
            ranked: ranked.as_deref(),
            presences: presences.as_deref(),
            lobby: lobby.as_deref(),
            matchmaking_search: search.as_deref(),
            ready_check: ready_check.as_deref(),
            champ_select: champ_select.as_deref(),
            live_allgame: allgame.as_deref(),
            live_active: active.as_deref(),
            live_events: events.as_deref(),
            end_of_game: end_of_game.as_deref(),
        };
        let snapshot = Arc::new(project_match(&inputs, &*self.assets, lang));
        *cache = Some((key, Arc::clone(&snapshot)));
        snapshot
    }

    /// Clears all buffered events in the EventStore.
    pub fn clear_events(&self) {
        self.store.write().unwrap().clear_events();
    }

    /// Returns a clone of `val` with any `{"$secret": ...}` entries revealed from the vault.
    pub fn reveal_secrets_in_value(&self, val: &Value) -> Value {
        crate::redaction::reveal_value(val, &self.vault)
    }

    /// Records `body` as an `EventKind::Call` event and returns its redacted form and event ID.
    fn record_call(&self, source: SourceId, path: &str, body: Value) -> (Arc<Value>, u64) {
        let mut store = self.store.write().unwrap();
        if !store.wants(source, path) {
            // Telemetry Lab closed: answer the caller without retaining anything.
            let mut body = body;
            crate::redaction::redact_value(&mut body, &self.vault);
            return (Arc::new(body), 0);
        }
        let meta = store.insert_event(source, EventKind::Call, path.to_string(), body, &self.vault);
        let redacted = store.current(source, path).unwrap_or_default();
        drop(store);
        self.catalog
            .write()
            .unwrap()
            .record_observed(source, "GET", path, meta.size as u64);
        (redacted, meta.id)
    }

    /// Performs a read-only GET against a Riot source (or serves cached/synthetic state in Demo
    /// mode), records it as an `EventKind::Call` event and returns
    /// `(http_status, redacted_json, event_id)`.
    pub async fn call_endpoint(
        &self,
        source: SourceId,
        path: &str,
    ) -> Result<(u16, Arc<Value>, u64), String> {
        let lang = self.lang();
        let path = path.trim();
        if !path.starts_with('/') {
            return Err(lang
                .pick(
                    "Path must start with '/' (e.g. /lol-summoner/v1/current-summoner)",
                    "Le chemin doit commencer par '/' (ex. /lol-summoner/v1/current-summoner)",
                )
                .to_string());
        }

        if self.is_demo_active() && source != SourceId::System {
            let cached = self.store.read().unwrap().current(source, path);
            let body = cached.map(|v| (*v).clone()).unwrap_or_else(|| {
                serde_json::json!({
                    "demo": true,
                    "source": source.to_string(),
                    "path": path,
                    "note": lang.pick(
                        "Synthetic demo response for GET tester",
                        "Réponse de démonstration synthétique pour le testeur GET",
                    ),
                })
            });
            let (redacted, id) = self.record_call(source, path, body);
            return Ok((200, redacted, id));
        }

        let (port, password) = match source {
            SourceId::LiveGame => (2999, None),
            SourceId::RiotClient | SourceId::LeagueClient => {
                let (port, password) = self.discovery.credentials(source).ok_or_else(|| {
                    if lang.is_fr() {
                        format!("{source} n'est pas en cours d'exécution ou ses identifiants sont indisponibles")
                    } else {
                        format!("{source} is not running or its credentials are unavailable")
                    }
                })?;
                (port, Some(password))
            }
            SourceId::System => {
                return Err(lang
                    .pick(
                        "Cannot execute HTTP GET on System source",
                        "Impossible d'exécuter un HTTP GET sur la source Système",
                    )
                    .to_string())
            }
        };

        let (status, _headers, body) = self
            .riot_client
            .get(port, password.as_deref(), path)
            .await
            .map_err(|e| {
                format!(
                    "{}: {e}",
                    lang.pick("GET request failed", "Échec de la requête GET")
                )
            })?;

        let (redacted, id) = self.record_call(source, path, body);
        Ok((status.as_u16(), redacted, id))
    }
}
