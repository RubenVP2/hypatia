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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use hypatia_shared::{
    project_match, EventKind, Lang, MatchInputs, RemoteCommand, Snapshot, SourceId, SummarySnapshot,
};
use serde_json::Value;
use tracing::info;

use crate::assets::GameAssetCache;
use crate::catalog::CatalogStore;
use crate::connectors::{
    spawn_live_client_poller, spawn_replay_poller, spawn_wamp_connector, ConnectorContext,
};
pub use crate::diff::compute_json_diff_localized;
use crate::discovery::DiscoveryManager;
use crate::fixtures::spawn_demo_generator;
pub use crate::fixtures::DemoRemoteState;
use crate::lcu_actions::{
    affected_lcu_uris, resolve_lcu_command_localized, LcuActionClient, RemoteActionError,
};
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
    lang_path: Option<PathBuf>,
    projection: Arc<ProjectionCache>,
}

impl AppState {
    /// Starts Hypatia core with default settings and the persisted or OS-detected language.
    /// Must be called from a Tokio runtime.
    pub fn start(demo: bool, demo_rate: f32) -> Self {
        Self::start_inner(
            demo,
            demo_rate,
            Settings::load_lang(),
            &Settings::default(),
            Some(Settings::default_lang_path()),
        )
    }

    /// Starts Hypatia core with default settings and an explicit in-memory language (no disk lang persistence).
    pub fn start_with_lang(demo: bool, demo_rate: f32, lang: Lang) -> Self {
        Self::start_inner(demo, demo_rate, lang, &Settings::default(), None)
    }

    /// Starts discovery, the event store and every connector (plus the synthetic generator in
    /// demo mode). Nothing is recorded beyond the Match Projection until
    /// [`Self::set_telemetry`] opens the Telemetry Lab.
    pub fn start_with(demo: bool, demo_rate: f32, lang: Lang, settings: &Settings) -> Self {
        Self::start_inner(
            demo,
            demo_rate,
            lang,
            settings,
            Some(Settings::default_lang_path()),
        )
    }

    /// Overrides the file path used to persist language changes (`None` disables persistence).
    pub fn with_lang_path(mut self, path: Option<PathBuf>) -> Self {
        self.lang_path = path;
        self
    }

    fn start_inner(
        demo: bool,
        demo_rate: f32,
        lang: Lang,
        settings: &Settings,
        lang_path: Option<PathBuf>,
    ) -> Self {
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

        let assets = Arc::new(GameAssetCache::new(signal.clone()));
        assets.set_max_mem_entries(if settings.game_icons {
            settings.asset_cache_budget.max_entries()
        } else {
            0
        });

        let state = Self {
            discovery,
            vault,
            store: Arc::new(RwLock::new(store)),
            catalog: Arc::new(RwLock::new(CatalogStore::new())),
            riot_client: ReadOnlyRiotClient::new(),
            assets,
            demo_remote: Arc::new(RwLock::new(DemoRemoteState::default())),
            flags,
            signal,
            demo_spawned: Arc::new(AtomicBool::new(false)),
            lang: Arc::new(AtomicU8::new(lang as u8)),
            lang_path,
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
            assets: self.assets.clone(),
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
                self.lang.clone(),
                self.assets.clone(),
            );
        }
    }

    /// Pins the synthetic Demo simulator directly to `phase`.
    pub fn set_demo_phase(&self, phase: &str) {
        if self.is_demo_active() {
            let _ = self.demo_remote.write().unwrap().set_phase(self, phase);
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

    /// Updates the active language, persists it if configured, and notifies every view.
    pub fn set_lang(&self, lang: Lang) {
        if self.lang.swap(lang as u8, Ordering::Relaxed) != lang as u8 {
            if let Some(ref path) = self.lang_path {
                if let Err(e) = Settings::save_lang_to(path, lang) {
                    tracing::warn!("Failed to persist language: {e}");
                }
            }
            if self.is_demo_active() {
                self.demo_remote.read().unwrap().sync_to_store(self);
            }
            self.signal.bump();
        }
    }

    /// Returns the local cached image path for `(category, id)` if game icons are enabled,
    /// scheduling a non-blocking LCU prefetch when missing.
    pub fn resolve_cached_asset(&self, category: &'static str, id: i64) -> Option<PathBuf> {
        if !self.flags.game_icons() {
            return None;
        }
        if let Some(path) = self.assets.cached_image_path(category, id) {
            return Some(path);
        }
        self.assets
            .prefetch_if_missing(category, id, &self.discovery, &self.riot_client);
        None
    }

    /// Returns `(mime_type, bytes)` for an asset from memory cache, disk cache, or live LCU.
    pub async fn fetch_asset_bytes(
        &self,
        category: &str,
        id: i64,
    ) -> Option<(&'static str, Vec<u8>)> {
        self.assets
            .get_or_fetch_bytes(category, id, &self.discovery, &self.riot_client)
            .await
    }

    /// Inserts an observed LCU `Update` payload into `EventStore` and updates asset indexes
    /// at ingest time (e.g. skin tile paths).
    pub(crate) fn insert_lcu_update(&self, uri: &str, body: Value) {
        if uri == "/lol-champ-select/v1/skin-carousel-skins" {
            self.assets.register_skin_paths_from_json(&body);
        }
        let mut store = self.store.write().unwrap();
        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            uri.to_string(),
            body,
            &self.vault,
        );
    }

    /// Executes an allowlisted [`RemoteCommand`] against either the unified Demo simulator or
    /// the live LCU client, then refreshes only the affected resources in `EventStore`.
    pub async fn execute_remote_command(
        &self,
        action_client: &LcuActionClient,
        cmd: &RemoteCommand,
    ) -> Result<(), RemoteActionError> {
        if self.is_demo_active() {
            return self
                .demo_remote
                .write()
                .unwrap()
                .apply_command(self, cmd)
                .map_err(|e| RemoteActionError::BadRequest(e.to_string()));
        }

        let lang = self.lang();
        let editable_page_id = matches!(cmd, RemoteCommand::SaveRunePage { .. })
            .then(|| self.get_summary().editable_rune_page_id)
            .flatten();
        let calls = resolve_lcu_command_localized(cmd, editable_page_id, lang)
            .map_err(|e| RemoteActionError::BadRequest(e.to_string()))?;

        let lcu = self.discovery.lcu_args().ok_or_else(|| {
            RemoteActionError::ClientUnavailable(
                lang.pick(
                    "League of Legends client is not running",
                    "Le client League of Legends n'est pas lancé",
                )
                .to_string(),
            )
        })?;
        let port = lcu.app_port.ok_or_else(|| {
            RemoteActionError::ClientUnavailable(
                lang.pick(
                    "League client port not detected yet",
                    "Port du client League pas encore détecté",
                )
                .to_string(),
            )
        })?;
        let token_ref = lcu.remoting_auth_token.as_deref();

        action_client
            .execute_calls_localized(port, token_ref, &calls, lang)
            .await
            .map_err(|e| RemoteActionError::GatewayError(e.to_string()))?;

        for uri in affected_lcu_uris(cmd) {
            if let Ok((status, _, body)) = self.riot_client.get(port, token_ref, uri).await {
                if status.is_success() {
                    self.insert_lcu_update(uri, body);
                }
            }
        }
        Ok(())
    }

    /// Applies user settings to every running background task and memory cache budget.
    pub fn apply_settings(&self, settings: &Settings) {
        self.flags.apply(settings);
        if !settings.game_icons {
            self.assets.set_max_mem_entries(0);
        } else {
            self.assets
                .set_max_mem_entries(settings.asset_cache_budget.max_entries());
            if !settings.splash_backdrops {
                self.assets.evict_categories(&[
                    "splash",
                    crate::art::CATEGORY,
                    crate::art::BANNER_CATEGORY,
                ]);
            }
        }
        self.discovery.wake();
        self.signal.bump();
    }

    /// Drops cached encoded asset blobs from core memory while keeping disk files intact.
    pub fn clear_asset_memory_cache(&self) {
        self.assets.clear_memory_cache();
    }

    /// Removes cached image files from disk (`assets-cache/<category>`) and clears memory blobs
    /// while preserving LCU metadata JSONs.
    pub fn clear_asset_disk_cache(&self) {
        self.assets.clear_disk_cache();
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
        let pickable_champion_ids = lcu("/lol-champ-select/v1/pickable-champion-ids");
        let bannable_champion_ids = lcu("/lol-champ-select/v1/bannable-champion-ids");
        let skin_carousel = lcu("/lol-champ-select/v1/skin-carousel-skins");
        let rune_pages = lcu("/lol-perks/v1/pages");
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
            pickable_champion_ids: pickable_champion_ids.as_deref(),
            bannable_champion_ids: bannable_champion_ids.as_deref(),
            skin_carousel: skin_carousel.as_deref(),
            rune_pages: rune_pages.as_deref(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[tokio::test]
    async fn set_lang_persists_when_configured_and_resolve_cached_asset_respects_flag() {
        let dir = tempfile::tempdir().unwrap();
        let lang_path = dir.path().join("lang");
        let app =
            AppState::start_with_lang(true, 1.0, Lang::En).with_lang_path(Some(lang_path.clone()));
        app.set_lang(Lang::Fr);
        assert_eq!(Settings::load_lang_from(&lang_path), Lang::Fr);

        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(cache_dir.join("champion")).unwrap();
        let icon_file = cache_dir.join("champion").join("103.png");
        std::fs::write(&icon_file, b"\x89PNG\r\n\x1a\nTEST").unwrap();

        let mut app_with_cache = app;
        app_with_cache.assets = Arc::new(GameAssetCache::with_dir(cache_dir));
        assert!(app_with_cache
            .resolve_cached_asset("champion", 103)
            .is_some());

        app_with_cache.apply_settings(&Settings {
            game_icons: false,
            ..Settings::default()
        });
        assert!(app_with_cache
            .resolve_cached_asset("champion", 103)
            .is_none());
    }
}
