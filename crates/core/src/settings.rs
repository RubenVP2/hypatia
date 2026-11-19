//! User-toggleable features and their lock-free runtime mirror.
//!
//! Every background activity Hypatia performs is tied to one switch here so the user can trade
//! features for footprint. [`Settings`] is the persisted form; [`Flags`] is what hot loops read.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use hypatia_shared::Lang;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Poll Live Client Data (`:2999`) at 1 Hz while a game runs (live board).
    pub live_game: bool,
    /// Keep a WebSocket open to the Riot Client (chat presence).
    pub riot_client: bool,
    /// Probe the Replay API every 5 s while a game runs (Telemetry Lab only).
    pub replay_api: bool,
    /// Download, cache and decode champion / spell / profile icons.
    pub game_icons: bool,
    /// Start the Mobile Remote LAN server at launch.
    pub mobile_remote: bool,
    /// Scan Riot process CPU/RAM and sockets while the Telemetry Lab is open.
    pub system_footprint: bool,
    /// Import the Swagger / `/help` endpoint catalog while the Telemetry Lab is open.
    pub endpoint_catalog: bool,
    /// Halve refresh rates (UI, discovery) to minimise wake-ups.
    pub eco_mode: bool,
    /// Day (light) printing instead of Night.
    pub light_theme: bool,
    /// Turn every UI animation off, on top of the OS reduced-motion preference.
    pub reduce_motion: bool,
    /// Show cached champion splash art and Summoner's Rift client artwork (needs `game_icons`).
    pub splash_backdrops: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            live_game: true,
            riot_client: true,
            replay_api: false,
            game_icons: true,
            mobile_remote: true,
            system_footprint: true,
            endpoint_catalog: true,
            eco_mode: false,
            light_theme: false,
            reduce_motion: false,
            splash_backdrops: true,
        }
    }
}

impl Settings {
    pub fn default_path() -> PathBuf {
        default_config_dir().join("settings.json")
    }

    /// Loads settings from `path`; a missing or malformed file yields the defaults.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }

    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    pub fn save(&self) {
        if let Err(e) = self.save_to(&Self::default_path()) {
            tracing::warn!("Failed to persist settings: {e}");
        }
    }

    pub fn default_lang_path() -> PathBuf {
        default_config_dir().join("lang")
    }

    /// Reads a persisted language preference from `path`, falling back to OS locale detection.
    pub fn load_lang_from(path: &Path) -> Lang {
        if let Ok(raw) = std::fs::read_to_string(path) {
            if let Some(lang) = Lang::from_code(&raw) {
                return lang;
            }
        }
        detect_os_lang()
    }

    /// Persists the active language code (`"en"` or `"fr"`) to `path`.
    pub fn save_lang_to(path: &Path, lang: Lang) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, lang.as_code())
    }

    /// Loads the persisted language preference, falling back to OS locale detection.
    pub fn load_lang() -> Lang {
        Self::load_lang_from(&Self::default_lang_path())
    }

    /// Persists the active language preference to the user's config folder.
    pub fn save_lang(lang: Lang) {
        if let Err(e) = Self::save_lang_to(&Self::default_lang_path(), lang) {
            tracing::warn!("Failed to persist language: {e}");
        }
    }

    /// Toggles the setting named `key`; returns `false` for an unknown key.
    pub fn toggle(&mut self, key: &str) -> bool {
        let field = match key {
            "live_game" => &mut self.live_game,
            "riot_client" => &mut self.riot_client,
            "replay_api" => &mut self.replay_api,
            "game_icons" => &mut self.game_icons,
            "mobile_remote" => &mut self.mobile_remote,
            "system_footprint" => &mut self.system_footprint,
            "endpoint_catalog" => &mut self.endpoint_catalog,
            "eco_mode" => &mut self.eco_mode,
            "light_theme" => &mut self.light_theme,
            "reduce_motion" => &mut self.reduce_motion,
            "splash_backdrops" => &mut self.splash_backdrops,
            _ => return false,
        };
        *field = !*field;
        true
    }
}

/// Lock-free mirror of [`Settings`] plus the Telemetry Lab open/closed state.
#[derive(Debug, Default)]
pub struct Flags {
    live_game: AtomicBool,
    riot_client: AtomicBool,
    replay_api: AtomicBool,
    game_icons: AtomicBool,
    splash_backdrops: AtomicBool,
    system_footprint: AtomicBool,
    endpoint_catalog: AtomicBool,
    eco_mode: AtomicBool,
    telemetry: AtomicBool,
}

impl Flags {
    pub fn new(settings: &Settings) -> Self {
        let flags = Self::default();
        flags.apply(settings);
        flags
    }

    pub fn apply(&self, s: &Settings) {
        self.live_game.store(s.live_game, Ordering::Relaxed);
        self.riot_client.store(s.riot_client, Ordering::Relaxed);
        self.replay_api.store(s.replay_api, Ordering::Relaxed);
        self.game_icons.store(s.game_icons, Ordering::Relaxed);
        self.splash_backdrops.store(s.splash_backdrops, Ordering::Relaxed);
        self.system_footprint.store(s.system_footprint, Ordering::Relaxed);
        self.endpoint_catalog.store(s.endpoint_catalog, Ordering::Relaxed);
        self.eco_mode.store(s.eco_mode, Ordering::Relaxed);
    }

    pub fn set_telemetry(&self, on: bool) {
        self.telemetry.store(on, Ordering::Relaxed);
    }

    pub fn live_game(&self) -> bool {
        self.live_game.load(Ordering::Relaxed)
    }

    pub fn riot_client(&self) -> bool {
        self.riot_client.load(Ordering::Relaxed)
    }

    pub fn game_icons(&self) -> bool {
        self.game_icons.load(Ordering::Relaxed)
    }

    pub fn splash_backdrops(&self) -> bool {
        self.game_icons() && self.splash_backdrops.load(Ordering::Relaxed)
    }

    pub fn eco_mode(&self) -> bool {
        self.eco_mode.load(Ordering::Relaxed)
    }

    /// Telemetry Lab is open: record the full event history and the complete WAMP stream.
    pub fn telemetry(&self) -> bool {
        self.telemetry.load(Ordering::Relaxed)
    }

    pub fn replay_api(&self) -> bool {
        self.telemetry() && self.replay_api.load(Ordering::Relaxed)
    }

    pub fn system_footprint(&self) -> bool {
        self.telemetry() && self.system_footprint.load(Ordering::Relaxed)
    }

    pub fn endpoint_catalog(&self) -> bool {
        self.telemetry() && self.endpoint_catalog.load(Ordering::Relaxed)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_tolerates_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        assert_eq!(Settings::load_from(&path), Settings::default());

        let mut s = Settings::default();
        assert!(s.toggle("game_icons"));
        assert!(s.toggle("eco_mode"));
        assert!(!s.toggle("nope"));
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), s);

        std::fs::write(&path, br#"{"live_game": false}"#).unwrap();
        let partial = Settings::load_from(&path);
        assert!(!partial.live_game);
        assert!(partial.riot_client, "missing keys fall back to defaults");
        assert!(partial.splash_backdrops);
    }

    #[test]
    fn telemetry_only_features_require_the_lab_to_be_open() {
        let flags = Flags::new(&Settings {
            replay_api: true,
            ..Settings::default()
        });
        assert!(!flags.system_footprint() && !flags.endpoint_catalog() && !flags.replay_api());
        flags.set_telemetry(true);
        assert!(flags.system_footprint() && flags.endpoint_catalog() && flags.replay_api());
    }

    #[test]
    fn roundtrips_persisted_lang_via_settings() {
        let dir = tempfile::tempdir().unwrap();
        let lang_path = dir.path().join("nested").join("lang");
        Settings::save_lang_to(&lang_path, Lang::Fr).unwrap();
        assert_eq!(Settings::load_lang_from(&lang_path), Lang::Fr);
        Settings::save_lang_to(&lang_path, Lang::En).unwrap();
        assert_eq!(Settings::load_lang_from(&lang_path), Lang::En);
        assert!(Settings::default_lang_path().ends_with("lang"));
    }
}
