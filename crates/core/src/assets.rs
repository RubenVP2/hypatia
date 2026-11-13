use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use hypatia_shared::{
    builtin_champion_roles, is_champion_placeholder, normalize_champion_key,
    BuiltinChampionCatalog, ChampionLookup, Lang, RemoteChampionOption, SourceId,
};
use serde_json::Value;
use tracing::{debug, warn};

use crate::discovery::DiscoveryManager;
use crate::riot_http::ReadOnlyRiotClient;
use crate::signal::ChangeSignal;

const MAX_MEM_ENTRIES: usize = 512;
const RETRY_COOLDOWN_TRANSIENT: Duration = Duration::from_secs(20);
const RETRY_COOLDOWN_NOT_FOUND: Duration = Duration::from_secs(300);

/// Local persistent disk + in-memory cache for Riot LCU `/lol-game-data/assets/...` bitmaps
/// and metadata JSONs. 100% local: only talks to `127.0.0.1:{lcu_port}` via `ReadOnlyRiotClient`.
pub struct GameAssetCache {
    cache_dir: PathBuf,
    signal: ChangeSignal,
    metadata_revision: AtomicU64,
    image_revision: AtomicU64,
    bytes_cache: RwLock<HashMap<String, (Vec<u8>, &'static str)>>,
    disk_paths: RwLock<HashMap<String, PathBuf>>,
    in_flight: Mutex<HashSet<String>>,
    failed_until: Mutex<HashMap<String, Instant>>,
    meta_synced_port: tokio::sync::Mutex<Option<u16>>,
    demo_warmed_port: AtomicU16,
    champ_names_by_id: RwLock<HashMap<i64, String>>,
    champ_ids_by_key: RwLock<HashMap<String, i64>>,
    champ_paths: RwLock<HashMap<i64, String>>,
    champ_catalog: RwLock<Vec<RemoteChampionOption>>,
    spell_paths: RwLock<HashMap<i64, String>>,
    perk_paths: RwLock<HashMap<i64, String>>,
    style_paths: RwLock<HashMap<i64, String>>,
    skin_paths: RwLock<HashMap<i64, String>>,
}

impl GameAssetCache {
    /// Creates a cache rooted in the user's standard local application cache folder.
    /// `signal` is bumped whenever an icon or champion metadata becomes available.
    pub fn new(signal: ChangeSignal) -> Self {
        Self::with_dir_and_signal(default_cache_dir(), signal)
    }

    /// Creates a cache rooted in an explicit directory (primarily for deterministic unit tests).
    pub fn with_dir(cache_dir: PathBuf) -> Self {
        Self::with_dir_and_signal(cache_dir, ChangeSignal::new())
    }

    fn with_dir_and_signal(cache_dir: PathBuf, signal: ChangeSignal) -> Self {
        let cache = Self {
            cache_dir,
            signal,
            metadata_revision: AtomicU64::new(0),
            image_revision: AtomicU64::new(0),
            bytes_cache: RwLock::new(HashMap::new()),
            disk_paths: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            failed_until: Mutex::new(HashMap::new()),
            meta_synced_port: tokio::sync::Mutex::new(None),
            demo_warmed_port: AtomicU16::new(0),
            champ_names_by_id: RwLock::new(HashMap::new()),
            champ_ids_by_key: RwLock::new(HashMap::new()),
            champ_paths: RwLock::new(HashMap::new()),
            champ_catalog: RwLock::new(Vec::new()),
            spell_paths: RwLock::new(HashMap::new()),
            perk_paths: RwLock::new(HashMap::new()),
            style_paths: RwLock::new(HashMap::new()),
            skin_paths: RwLock::new(HashMap::new()),
        };
        cache.load_cached_metadata_from_disk();
        cache
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Bumped each time a newly fetched image lands in the disk cache.
    pub fn image_revision(&self) -> u64 {
        self.image_revision.load(Ordering::Relaxed)
    }

    /// Bumped whenever champion metadata (names, aliases) is (re)loaded.
    pub fn metadata_revision(&self) -> u64 {
        self.metadata_revision.load(Ordering::Relaxed)
    }

    /// Registers a skin tile path discovered from `/lol-champ-select/v1/skin-carousel-skins`.
    pub fn register_skin_path(&self, skin_id: i64, raw_path: &str) {
        if skin_id <= 0 {
            return;
        }
        if let Some(clean) = sanitize_lcu_asset_path(raw_path) {
            self.skin_paths.write().unwrap().insert(skin_id, clean);
        }
    }

    /// Synchronous check for a cached image file path on disk (safe inside tight UI render timers).
    pub fn cached_image_path(&self, category: &str, id: i64) -> Option<PathBuf> {
        if !is_valid_asset_id(category, id) {
            return None;
        }
        let key = asset_key(category, id);
        if let Some(p) = self.disk_paths.read().unwrap().get(&key) {
            if p.exists() {
                return Some(p.clone());
            }
        }
        for ext in ["png", "jpg"] {
            let candidate = self.file_path(category, id, ext);
            if candidate.is_file() {
                self.disk_paths.write().unwrap().insert(key, candidate.clone());
                return Some(candidate);
            }
        }
        None
    }

    /// Triggers a non-blocking Tokio background fetch if the asset is not cached locally
    /// and a live LCU instance is available. Safe to call repeatedly from a UI loop.
    pub fn prefetch_if_missing(
        self: &Arc<Self>,
        category: &'static str,
        id: i64,
        discovery: &Arc<DiscoveryManager>,
        client: &ReadOnlyRiotClient,
    ) {
        if !is_valid_asset_id(category, id) {
            return;
        }
        if self.cached_image_path(category, id).is_some() {
            return;
        }
        let Some((port, password)) = resolve_live_lcu_creds(discovery) else {
            return;
        };

        let key = asset_key(category, id);
        if self.is_in_cooldown(&key) {
            return;
        }
        {
            let mut guard = self.in_flight.lock().unwrap();
            if !guard.insert(key.clone()) {
                return;
            }
        }

        let this = Arc::clone(self);
        let client = client.clone();
        tokio::spawn(async move {
            this.ensure_metadata_synced_with_creds(port, &password, &client).await;
            let _ = this.fetch_from_lcu_with_creds(category, id, port, &password, &client).await;
            this.in_flight.lock().unwrap().remove(&key);
        });
    }

    /// Non-blocking trigger so Desktop UI can sync LCU metadata (`champion-summary.json`, etc.)
    /// proactively even before any individual bitmap is resolved or `--remote` polls.
    pub fn trigger_metadata_sync(
        self: &Arc<Self>,
        discovery: &Arc<DiscoveryManager>,
        client: &ReadOnlyRiotClient,
    ) {
        let Some((port, password)) = resolve_live_lcu_creds(discovery) else {
            return;
        };
        if let Ok(guard) = self.meta_synced_port.try_lock() {
            if *guard == Some(port) {
                return;
            }
        } else {
            return;
        }
        let meta_key = format!("__meta__:{port}");
        if self.is_in_cooldown(&meta_key) {
            return;
        }
        {
            let mut inflight = self.in_flight.lock().unwrap();
            if !inflight.insert(meta_key.clone()) {
                return;
            }
        }
        let this = Arc::clone(self);
        let client = client.clone();
        tokio::spawn(async move {
            let ok = this
                .ensure_metadata_synced_with_creds(port, &password, &client)
                .await;
            if !ok {
                this.mark_failed(&meta_key, Duration::from_secs(5));
            }
            this.in_flight.lock().unwrap().remove(&meta_key);
        });
    }

    /// Prefetches the showcase assets used by `--demo` (champion icons, spells, and
    /// splash arts) once per LCU port while the real League Client is running so offline
    /// Demo previews immediately display authentic cached Riot artwork.
    pub fn prefetch_demo_showcase(
        self: &Arc<Self>,
        discovery: &Arc<DiscoveryManager>,
        client: &ReadOnlyRiotClient,
    ) {
        let Some((port, password)) = resolve_live_lcu_creds(discovery) else {
            return;
        };
        if self.demo_warmed_port.load(Ordering::Relaxed) == port {
            return;
        }
        let key = format!("__demo_warmup__:{port}");
        {
            let mut inflight = self.in_flight.lock().unwrap();
            if !inflight.insert(key.clone()) {
                return;
            }
        }
        self.demo_warmed_port.store(port, Ordering::Relaxed);

        let this = Arc::clone(self);
        let client = client.clone();
        tokio::spawn(async move {
            this.ensure_metadata_synced_with_creds(port, &password, &client).await;
            const GROUPS: &[(&str, &[i64])] = &[
                ("champion", &[266, 64, 103, 222, 412, 122, 234, 86, 238, 145, 111, 157, 35, 777, 233]),
                ("spell", &[4, 7, 11, 12, 14]),
                ("splash", &[103027, 103000, 266000, 64000, 222000, 412000]),
            ];
            for &(cat, ids) in GROUPS {
                for &id in ids {
                    if this.cached_image_path(cat, id).is_none() {
                        let _ = this.fetch_from_lcu_with_creds(cat, id, port, &password, &client).await;
                    }
                }
            }
            this.in_flight.lock().unwrap().remove(&key);
        });
    }

    /// Ensures LCU metadata JSONs (`champion-summary`, `summoner-spells`, `perks`, `perkstyles`)
    /// have been fetched for the active LCU session port.
    pub async fn ensure_metadata_synced(
        &self,
        discovery: &DiscoveryManager,
        client: &ReadOnlyRiotClient,
    ) {
        let Some((port, password)) = resolve_live_lcu_creds(discovery) else {
            return;
        };
        let _ = self
            .ensure_metadata_synced_with_creds(port, &password, client)
            .await;
    }

    /// Returns `(mime_type, bytes)` for an asset from memory cache, disk cache, or live LCU.
    pub async fn get_or_fetch_bytes(
        &self,
        category: &str,
        id: i64,
        discovery: &DiscoveryManager,
        client: &ReadOnlyRiotClient,
    ) -> Option<(&'static str, Vec<u8>)> {
        if !is_valid_asset_id(category, id) {
            return None;
        }
        let key = asset_key(category, id);

        // 1. In-memory bytes cache
        if let Some((bytes, mime)) = self.bytes_cache.read().unwrap().get(&key).cloned() {
            return Some((mime, bytes));
        }

        // 2. Disk cache
        if let Some(path) = self.cached_image_path(category, id) {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                if let Some((_ext, mime)) = detect_image_format(&bytes) {
                    self.put_mem_cache(key, bytes.clone(), mime);
                    return Some((mime, bytes));
                }
            }
        }

        // 3. Live LCU fetch (never in demo mode)
        if self.is_in_cooldown(&key) {
            return None;
        }
        let (port, password) = resolve_live_lcu_creds(discovery)?;
        let _ = self
            .ensure_metadata_synced_with_creds(port, &password, client)
            .await;
        self.fetch_from_lcu_with_creds(category, id, port, &password, client)
            .await
    }

    async fn ensure_metadata_synced_with_creds(
        &self,
        port: u16,
        password: &str,
        client: &ReadOnlyRiotClient,
    ) -> bool {
        let mut synced = self.meta_synced_port.lock().await;
        if *synced == Some(port) {
            return true;
        }

        let meta_specs = [
            ("champion-summary.json", "/lol-game-data/assets/v1/champion-summary.json"),
            ("summoner-spells.json", "/lol-game-data/assets/v1/summoner-spells.json"),
            ("perks.json", "/lol-game-data/assets/v1/perks.json"),
            ("perkstyles.json", "/lol-game-data/assets/v1/perkstyles.json"),
        ];

        let mut champ_summary_ok = false;
        for (file_name, lcu_path) in meta_specs {
            match client.get_bytes(port, Some(password), lcu_path).await {
                Ok((status, _, bytes)) if status.is_success() && !bytes.is_empty() => {
                    if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                        self.ingest_metadata(file_name, &json);
                        if file_name == "champion-summary.json" {
                            champ_summary_ok = !self.champ_paths.read().unwrap().is_empty();
                        }
                        let meta_path = self.cache_dir.join("meta").join(file_name);
                        if let Some(parent) = meta_path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        let _ = tokio::fs::write(&meta_path, &bytes).await;
                    }
                }
                Ok((status, _, _)) => {
                    debug!("LCU asset metadata {} returned HTTP {}", lcu_path, status);
                }
                Err(e) => {
                    warn!("Failed syncing LCU asset metadata {}: {}", lcu_path, e);
                    return false;
                }
            }
        }

        if champ_summary_ok {
            *synced = Some(port);
        }
        champ_summary_ok
    }

    async fn fetch_from_lcu_with_creds(
        &self,
        category: &str,
        id: i64,
        port: u16,
        password: &str,
        client: &ReadOnlyRiotClient,
    ) -> Option<(&'static str, Vec<u8>)> {
        let key = asset_key(category, id);
        let candidates = self.candidate_lcu_paths(category, id);
        if candidates.is_empty() {
            self.mark_failed(&key, RETRY_COOLDOWN_TRANSIENT);
            return None;
        }

        let mut saw_404 = false;
        for lcu_path in candidates {
            match client.get_bytes(port, Some(password), &lcu_path).await {
                Ok((status, _, bytes)) if status.is_success() => {
                    if let Some((ext, mime)) = detect_image_format(&bytes) {
                        let path = self.file_path(category, id, ext);
                        if let Some(parent) = path.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        let tmp = path.with_extension("tmp");
                        if tokio::fs::write(&tmp, &bytes).await.is_ok() {
                            let _ = tokio::fs::rename(&tmp, &path).await;
                        } else {
                            let _ = tokio::fs::write(&path, &bytes).await;
                        }
                        self.disk_paths.write().unwrap().insert(key.clone(), path);
                        self.put_mem_cache(key.clone(), bytes.clone(), mime);
                        self.failed_until.lock().unwrap().remove(&key);
                        self.image_revision.fetch_add(1, Ordering::Relaxed);
                        self.signal.bump();
                        return Some((mime, bytes));
                    } else {
                        debug!("Skipping non-PNG/JPEG payload from LCU {}", lcu_path);
                    }
                }
                Ok((status, _, _)) if status.as_u16() == 404 => {
                    saw_404 = true;
                }
                Ok((status, _, _)) => {
                    debug!("LCU asset {} returned HTTP {}", lcu_path, status);
                }
                Err(e) => {
                    debug!("LCU asset {} fetch failed: {}", lcu_path, e);
                }
            }
        }

        self.mark_failed(
            &key,
            if saw_404 {
                RETRY_COOLDOWN_NOT_FOUND
            } else {
                RETRY_COOLDOWN_TRANSIENT
            },
        );
        None
    }

    fn candidate_lcu_paths(&self, category: &str, id: i64) -> Vec<String> {
        let mut out = Vec::new();
        match category {
            "champion" => {
                if let Some(p) = self.champ_paths.read().unwrap().get(&id) {
                    out.push(p.clone());
                }
                let canonical = format!("/lol-game-data/assets/v1/champion-icons/{id}.png");
                if !out.contains(&canonical) {
                    out.push(canonical);
                }
            }
            "profile" => {
                out.push(format!("/lol-game-data/assets/v1/profile-icons/{id}.jpg"));
                out.push(format!("/lol-game-data/assets/v1/profile-icons/{id}.png"));
            }
            "spell" => {
                if let Some(p) = self.spell_paths.read().unwrap().get(&id) {
                    out.push(p.clone());
                }
                for p in fallback_spell_paths(id) {
                    if !out.iter().any(|existing| existing == p) {
                        out.push((*p).to_string());
                    }
                }
            }
            "perk" => {
                if let Some(p) = self.perk_paths.read().unwrap().get(&id) {
                    out.push(p.clone());
                }
            }
            "style" => {
                if let Some(p) = self.style_paths.read().unwrap().get(&id) {
                    out.push(p.clone());
                }
                if let Some(p) = fallback_style_path(id) {
                    let s = p.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
            "skin" => {
                if let Some(p) = self.skin_paths.read().unwrap().get(&id) {
                    out.push(p.clone());
                }
                if id >= 1000 {
                    let champ_id = id / 1000;
                    let canonical = format!("/lol-game-data/assets/v1/champion-tiles/{champ_id}/{id}.jpg");
                    if !out.contains(&canonical) {
                        out.push(canonical);
                    }
                }
            }
            "splash" => {
                let (champ_id, skin_id) =
                    if id >= 1000 { (id / 1000, id) } else { (id, id * 1000) };
                let base_id = champ_id * 1000;
                for sid in [skin_id, base_id] {
                    for path in [
                        format!("/lol-game-data/assets/v1/champion-splashes/{champ_id}/{sid}.jpg"),
                        format!(
                            "/lol-game-data/assets/v1/champion-splashes/uncentered/{champ_id}/{sid}.jpg"
                        ),
                    ] {
                        if !out.contains(&path) {
                            out.push(path);
                        }
                    }
                }
                let tile =
                    format!("/lol-game-data/assets/v1/champion-tiles/{champ_id}/{skin_id}.jpg");
                if !out.contains(&tile) {
                    out.push(tile);
                }
            }
            _ => {}
        }
        out
    }

    fn load_cached_metadata_from_disk(&self) {
        let meta_dir = self.cache_dir.join("meta");
        for file_name in [
            "champion-summary.json",
            "summoner-spells.json",
            "perks.json",
            "perkstyles.json",
        ] {
            let p = meta_dir.join(file_name);
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                    self.ingest_metadata(file_name, &json);
                }
            }
        }
    }

    /// Parses cached/live Riot `lol-game-data` metadata JSONs into fast in-memory lookup maps.
    pub fn ingest_metadata(&self, file_name: &str, json: &Value) {
        match file_name {
            "champion-summary.json" => {
                let Some(arr) = json.as_array() else { return; };
                let mut names = self.champ_names_by_id.write().unwrap();
                let mut keys = self.champ_ids_by_key.write().unwrap();
                let mut paths = self.champ_paths.write().unwrap();
                let mut catalog = Vec::with_capacity(arr.len());

                for entry in arr {
                    let id = entry.get("id").and_then(|v| v.as_i64()).unwrap_or(-1);
                    if id <= 0 {
                        continue;
                    }
                    let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    names.insert(id, name.to_string());
                    keys.insert(normalize_champion_key(name), id);
                    if let Some(alias) = entry.get("alias").and_then(|v| v.as_str()) {
                        keys.insert(normalize_champion_key(alias), id);
                    }
                    if let Some(raw_path) = entry.get("squarePortraitPath").and_then(|v| v.as_str()) {
                        if let Some(clean) = sanitize_lcu_asset_path(raw_path) {
                            paths.insert(id, clean);
                        }
                    }
                    let builtin_roles = builtin_champion_roles(id);
                    let roles = if !builtin_roles.is_empty() {
                        builtin_roles.iter().map(|r| (*r).to_string()).collect()
                    } else {
                        map_cdragon_roles_to_lanes(entry.get("roles"))
                    };
                    catalog.push(RemoteChampionOption {
                        id,
                        name: name.to_string(),
                        roles,
                        pickable: true,
                        bannable: true,
                    });
                }
                catalog.sort_by(|a, b| a.name.cmp(&b.name));
                if !catalog.is_empty() {
                    *self.champ_catalog.write().unwrap() = catalog;
                }
                drop((names, keys, paths));
                self.metadata_revision.fetch_add(1, Ordering::Relaxed);
                self.signal.bump();
            }
            "summoner-spells.json" => {
                let Some(arr) = json.as_array() else { return; };
                let mut map = self.spell_paths.write().unwrap();
                for entry in arr {
                    let Some(id) = entry.get("id").and_then(|v| v.as_i64()) else { continue; };
                    if let Some(raw_path) = entry.get("iconPath").and_then(|v| v.as_str()) {
                        if let Some(clean) = sanitize_lcu_asset_path(raw_path) {
                            map.insert(id, clean);
                        }
                    }
                }
            }
            "perks.json" => {
                let Some(arr) = json.as_array() else { return; };
                let mut map = self.perk_paths.write().unwrap();
                for entry in arr {
                    let Some(id) = entry.get("id").and_then(|v| v.as_i64()) else { continue; };
                    if let Some(raw_path) = entry.get("iconPath").and_then(|v| v.as_str()) {
                        if let Some(clean) = sanitize_lcu_asset_path(raw_path) {
                            map.insert(id, clean);
                        }
                    }
                }
            }
            "perkstyles.json" => {
                let styles = json
                    .get("styles")
                    .and_then(|v| v.as_array())
                    .or_else(|| json.as_array());
                let Some(arr) = styles else { return; };
                let mut map = self.style_paths.write().unwrap();
                for entry in arr {
                    let Some(id) = entry.get("id").and_then(|v| v.as_i64()) else { continue; };
                    if let Some(raw_path) = entry.get("iconPath").and_then(|v| v.as_str()) {
                        if let Some(clean) = sanitize_lcu_asset_path(raw_path) {
                            map.insert(id, clean);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn file_path(&self, category: &str, id: i64, ext: &str) -> PathBuf {
        self.cache_dir.join(category).join(format!("{id}.{ext}"))
    }

    fn put_mem_cache(&self, key: String, bytes: Vec<u8>, mime: &'static str) {
        let mut guard = self.bytes_cache.write().unwrap();
        if guard.len() >= MAX_MEM_ENTRIES && !guard.contains_key(&key) {
            if let Some(evict_key) = guard.keys().next().cloned() {
                guard.remove(&evict_key);
            }
        }
        guard.insert(key, (bytes, mime));
    }

    fn is_in_cooldown(&self, key: &str) -> bool {
        let mut guard = self.failed_until.lock().unwrap();
        if let Some(until) = guard.get(key) {
            if Instant::now() < *until {
                return true;
            }
            guard.remove(key);
        }
        false
    }

    fn mark_failed(&self, key: &str, ttl: Duration) {
        self.failed_until
            .lock()
            .unwrap()
            .insert(key.to_string(), Instant::now() + ttl);
    }
}

pub fn default_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("hypatia").join("assets-cache");
    }
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("hypatia").join("assets-cache");
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home)
            .join(".cache")
            .join("hypatia")
            .join("assets-cache");
    }
    std::env::temp_dir().join("hypatia-assets-cache")
}

/// Returns the per-user Hypatia config directory (`%LOCALAPPDATA%\hypatia` on Windows).
pub fn default_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("hypatia");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("hypatia");
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home).join(".config").join("hypatia");
    }
    std::env::temp_dir().join("hypatia")
}

pub fn default_lang_file_path() -> PathBuf {
    default_config_dir().join("lang")
}

/// Reads a persisted language preference from `path`, falling back to OS locale detection.
pub fn load_lang_from_path(path: &Path) -> Lang {
    if let Ok(raw) = std::fs::read_to_string(path) {
        if let Some(lang) = Lang::from_code(&raw) {
            return lang;
        }
    }
    detect_os_lang()
}

/// Persists the active language code (`"en"` or `"fr"`) to `path`.
pub fn save_lang_to_path(path: &Path, lang: Lang) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, lang.as_code())
}

pub fn load_persisted_or_detect_lang() -> Lang {
    load_lang_from_path(&default_lang_file_path())
}

pub fn save_persisted_lang(lang: Lang) {
    let _ = save_lang_to_path(&default_lang_file_path(), lang);
}

fn detect_env_lang() -> Option<Lang> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(key) {
            if let Some(lang) = Lang::from_code(&val) {
                return Some(lang);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn detect_os_lang() -> Lang {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(lp_locale_name: *mut u16, cch_locale_name: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    // SAFETY: `buf` is a stack buffer matching Windows LOCALE_NAME_MAX_LENGTH (85 UTF-16 code units).
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if len > 1 {
        let raw = String::from_utf16_lossy(&buf[..(len as usize - 1)]);
        if let Some(lang) = Lang::from_code(raw.trim_end_matches('\0')) {
            return lang;
        }
    }
    detect_env_lang().unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn detect_os_lang() -> Lang {
    detect_env_lang().unwrap_or_default()
}

/// Validates and detects PNG or JPEG image magic headers.
pub fn detect_image_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG_MAGIC) {
        return Some(("png", "image/png"));
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some(("jpg", "image/jpeg"));
    }
    None
}

/// Validates an LCU static asset path strictly stays under `/lol-game-data/assets/`.
pub fn sanitize_lcu_asset_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains("..") || trimmed.contains("://") {
        return None;
    }
    if trimmed.bytes().any(|b| b.is_ascii_control() || b == b'\\' || b == b' ') {
        return None;
    }
    let normalized = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    if normalized.to_ascii_lowercase().starts_with("/lol-game-data/assets/") {
        Some(normalized)
    } else {
        None
    }
}

fn asset_key(category: &str, id: i64) -> String {
    format!("{category}/{id}")
}

fn is_valid_asset_id(category: &str, id: i64) -> bool {
    match category {
        "profile" => id >= 0,
        "champion" | "spell" | "perk" | "style" | "skin" | "splash" => id > 0,
        _ => false,
    }
}

fn resolve_live_lcu_creds(discovery: &DiscoveryManager) -> Option<(u16, String)> {
    if discovery.is_demo_override() {
        return None;
    }
    discovery.credentials(SourceId::LeagueClient).or_else(|| {
        let args = discovery.lcu_args()?;
        Some((args.app_port?, args.remoting_auth_token?))
    })
}

fn fallback_spell_paths(id: i64) -> &'static [&'static str] {
    match id {
        1 => &["/lol-game-data/assets/data/spells/icons2d/summoner_boost.png"],
        3 => &["/lol-game-data/assets/data/spells/icons2d/summoner_exhaust.png"],
        4 => &["/lol-game-data/assets/data/spells/icons2d/summoner_flash.png"],
        6 => &["/lol-game-data/assets/data/spells/icons2d/summoner_haste.png"],
        7 => &["/lol-game-data/assets/data/spells/icons2d/summoner_heal.png"],
        11 => &["/lol-game-data/assets/data/spells/icons2d/summoner_smite.png"],
        12 => &[
            "/lol-game-data/assets/data/spells/icons2d/summoner_teleport_new.png",
            "/lol-game-data/assets/data/spells/icons2d/summoner_teleport.png",
        ],
        13 => &["/lol-game-data/assets/data/spells/icons2d/summonermana.png"],
        14 => &["/lol-game-data/assets/data/spells/icons2d/summonerignite.png"],
        21 => &["/lol-game-data/assets/data/spells/icons2d/summonerbarrier.png"],
        32 => &["/lol-game-data/assets/data/spells/icons2d/summoner_mark.png"],
        _ => &[],
    }
}

fn fallback_style_path(id: i64) -> Option<&'static str> {
    match id {
        8000 => Some("/lol-game-data/assets/v1/perk-images/Styles/7201_Precision.png"),
        8100 => Some("/lol-game-data/assets/v1/perk-images/Styles/7200_Domination.png"),
        8200 => Some("/lol-game-data/assets/v1/perk-images/Styles/7202_Sorcery.png"),
        8300 => Some("/lol-game-data/assets/v1/perk-images/Styles/7203_Whimsy.png"),
        8400 => Some("/lol-game-data/assets/v1/perk-images/Styles/7204_Resolve.png"),
        _ => None,
    }
}

fn map_cdragon_roles_to_lanes(roles_val: Option<&Value>) -> Vec<String> {
    let mut lanes = Vec::new();
    let Some(arr) = roles_val.and_then(|v| v.as_array()) else {
        return lanes;
    };
    for r in arr.iter().filter_map(|v| v.as_str()) {
        let mapped = match r.to_ascii_lowercase().as_str() {
            "fighter" | "tank" => &["TOP"][..],
            "mage" | "assassin" => &["MIDDLE"][..],
            "marksman" => &["BOTTOM"][..],
            "support" => &["UTILITY"][..],
            _ => &[],
        };
        for lane in mapped {
            if !lanes.iter().any(|l| l == lane) {
                lanes.push((*lane).to_string());
            }
        }
    }
    lanes
}

impl ChampionLookup for GameAssetCache {
    fn resolve_champion_id(&self, name: &str) -> i64 {
        if is_champion_placeholder(name) {
            return 0;
        }
        let key = normalize_champion_key(name);
        if let Some(&id) = self.champ_ids_by_key.read().unwrap().get(&key) {
            return id;
        }
        BuiltinChampionCatalog.resolve_champion_id(name)
    }

    fn resolve_champion_name_localized(&self, champion_id: i64, lang: Lang) -> String {
        if champion_id <= 0 {
            return lang.pick("Selecting...", "Sélection...").to_string();
        }
        if let Some(name) = self.champ_names_by_id.read().unwrap().get(&champion_id) {
            return name.clone();
        }
        BuiltinChampionCatalog.resolve_champion_name_localized(champion_id, lang)
    }

    fn champion_catalog(&self) -> Vec<RemoteChampionOption> {
        let synced = self.champ_catalog.read().unwrap();
        if synced.is_empty() {
            return BuiltinChampionCatalog.champion_catalog();
        }
        let mut merged: HashMap<i64, RemoteChampionOption> = BuiltinChampionCatalog
            .champion_catalog()
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        for opt in synced.iter() {
            merged
                .entry(opt.id)
                .and_modify(|existing| {
                    existing.name = opt.name.clone();
                    if !opt.roles.is_empty() {
                        existing.roles = opt.roles.clone();
                    }
                })
                .or_insert_with(|| opt.clone());
        }
        let mut out: Vec<RemoteChampionOption> = merged.into_values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_cache_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hypatia_asset_test_{}_{}",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn validates_magic_bytes_and_lcu_asset_paths() {
        assert_eq!(detect_image_format(b"\x89PNG\r\n\x1a\nrest"), Some(("png", "image/png")));
        assert_eq!(detect_image_format(b"\xff\xd8\xff\xe0data"), Some(("jpg", "image/jpeg")));
        assert_eq!(detect_image_format(b"<html>not an image</html>"), None);

        assert_eq!(
            sanitize_lcu_asset_path("/lol-game-data/assets/v1/champion-icons/103.png"),
            Some("/lol-game-data/assets/v1/champion-icons/103.png".to_string())
        );
        assert_eq!(
            sanitize_lcu_asset_path("lol-game-data/assets/v1/profile-icons/0.jpg"),
            Some("/lol-game-data/assets/v1/profile-icons/0.jpg".to_string())
        );
        assert_eq!(sanitize_lcu_asset_path("/lol-summoner/v1/current-summoner"), None);
        assert_eq!(sanitize_lcu_asset_path("/lol-game-data/assets/../../etc/passwd"), None);
        assert_eq!(sanitize_lcu_asset_path("https://cdn.example.com/103.png"), None);
    }

    #[tokio::test]
    async fn reads_disk_cached_assets_and_ingests_metadata() {
        let dir = temp_cache_dir("disk_hit");
        let cache = GameAssetCache::with_dir(dir.clone());

        cache.ingest_metadata(
            "champion-summary.json",
            &json!([
                {"id": -1, "name": "None", "alias": "None"},
                {
                    "id": 999,
                    "name": "Test Champ",
                    "alias": "TestChamp",
                    "roles": ["mage"],
                    "squarePortraitPath": "/lol-game-data/assets/v1/champion-icons/999.png"
                }
            ]),
        );
        assert_eq!(cache.resolve_champion_id("Test Champ"), 999);
        assert_eq!(cache.resolve_champion_id("TestChamp"), 999);
        assert_eq!(cache.resolve_champion_name_localized(999, Lang::En), "Test Champ");
        assert_eq!(cache.resolve_champion_name_localized(0, Lang::En), "Selecting...");
        assert_eq!(cache.resolve_champion_name_localized(0, Lang::Fr), "Sélection...");

        let catalog = cache.champion_catalog();
        assert!(catalog.iter().any(|c| c.id == 999 && c.roles == vec!["MIDDLE".to_string()]));
        assert!(catalog.iter().any(|c| c.id == 64 && c.roles == vec!["JUNGLE".to_string()]));

        let lang_file = dir.join("nested").join("lang");
        save_lang_to_path(&lang_file, Lang::Fr).unwrap();
        assert_eq!(load_lang_from_path(&lang_file), Lang::Fr);
        save_lang_to_path(&lang_file, Lang::En).unwrap();
        assert_eq!(load_lang_from_path(&lang_file), Lang::En);

        cache.ingest_metadata(
            "perks.json",
            &json!([{"id": 8112, "iconPath": "/lol-game-data/assets/v1/perk-images/Styles/Domination/Electrocute/Electrocute.png"}]),
        );
        assert_eq!(
            cache.candidate_lcu_paths("perk", 8112),
            vec!["/lol-game-data/assets/v1/perk-images/Styles/Domination/Electrocute/Electrocute.png".to_string()]
        );
        assert_eq!(
            cache.candidate_lcu_paths("splash", 103027),
            vec![
                "/lol-game-data/assets/v1/champion-splashes/103/103027.jpg",
                "/lol-game-data/assets/v1/champion-splashes/uncentered/103/103027.jpg",
                "/lol-game-data/assets/v1/champion-splashes/103/103000.jpg",
                "/lol-game-data/assets/v1/champion-splashes/uncentered/103/103000.jpg",
                "/lol-game-data/assets/v1/champion-tiles/103/103027.jpg",
            ]
        );
        assert_eq!(
            cache.candidate_lcu_paths("splash", 103),
            vec![
                "/lol-game-data/assets/v1/champion-splashes/103/103000.jpg",
                "/lol-game-data/assets/v1/champion-splashes/uncentered/103/103000.jpg",
                "/lol-game-data/assets/v1/champion-tiles/103/103000.jpg",
            ]
        );

        let png_bytes = b"\x89PNG\r\n\x1a\nFAKEPNGDATA".to_vec();
        std::fs::create_dir_all(dir.join("champion")).unwrap();
        std::fs::write(dir.join("champion").join("103.png"), &png_bytes).unwrap();

        let disk_path = cache.cached_image_path("champion", 103).unwrap();
        assert!(disk_path.ends_with("103.png"));

        let disc = DiscoveryManager::new_demo(crate::redaction::SecretVault::new(), ChangeSignal::new());
        let client = ReadOnlyRiotClient::new();
        let fetched = cache.get_or_fetch_bytes("champion", 103, &disc, &client).await;
        assert_eq!(fetched, Some(("image/png", png_bytes)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
