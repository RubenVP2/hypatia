//! User-toggleable features and their lock-free runtime mirror.
//!
//! Every background activity Hypatia performs is tied to one switch or rate here so the user
//! can trade features and responsiveness for CPU/RAM footprint. [`Settings`] is the persisted
//! form; [`Flags`] is what hot loops read.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use hypatia_shared::Lang;
use serde::{Deserialize, Serialize};
use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// Simplified user-facing performance preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceProfile {
    /// Minimum CPU/RAM: initials only, no splash art, slow polling, no Mobile Remote or RC socket.
    UltraLight,
    /// Low footprint: champion icons kept, splashes off, 2s live poll, 1s UI clock, sleep when minimized.
    Eco,
    /// Full experience: all visuals enabled, 1s live poll, 250ms UI clocks.
    #[default]
    Balanced,
    /// Manually tweaked advanced settings that do not match any preset.
    Custom,
}

impl PerformanceProfile {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::UltraLight => "ultra_light",
            Self::Eco => "eco",
            Self::Balanced => "balanced",
            Self::Custom => "custom",
        }
    }

    pub fn from_code(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ultra_light" | "ultra-light" | "ultralight" => Some(Self::UltraLight),
            "eco" => Some(Self::Eco),
            "balanced" | "full" => Some(Self::Balanced),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::UltraLight => lang.pick("Ultra-light", "Ultra-léger"),
            Self::Eco => lang.pick("Eco", "Éco"),
            Self::Balanced => lang.pick("Balanced", "Équilibré"),
            Self::Custom => lang.pick("Custom", "Personnalisé"),
        }
    }

    pub fn badge(self, lang: Lang) -> &'static str {
        match self {
            Self::UltraLight => lang.pick("LITE", "LÉGER"),
            Self::Eco => lang.pick("ECO", "ÉCO"),
            Self::Balanced => "",
            Self::Custom => lang.pick("CUSTOM", "PERSO"),
        }
    }
}

/// Polling period for Live Client Data (`:2999`) while a match is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum LivePollRate {
    #[default]
    Fast = 0, // 1000 ms
    Eco = 1,  // 2000 ms
    Slow = 2, // 4000 ms
}

impl LivePollRate {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Eco,
            2 => Self::Slow,
            _ => Self::Fast,
        }
    }

    pub fn as_index(self) -> i32 {
        self as u8 as i32
    }

    pub fn from_index(idx: i32) -> Option<Self> {
        match idx {
            0 => Some(Self::Fast),
            1 => Some(Self::Eco),
            2 => Some(Self::Slow),
            _ => None,
        }
    }

    pub fn interval(self) -> Duration {
        match self {
            Self::Fast => Duration::from_millis(1000),
            Self::Eco => Duration::from_millis(2000),
            Self::Slow => Duration::from_millis(4000),
        }
    }

    pub fn idle_interval(self) -> Duration {
        match self {
            Self::Fast => Duration::from_millis(1000),
            Self::Eco => Duration::from_millis(2000),
            Self::Slow => Duration::from_millis(3000),
        }
    }
}

/// Scan cadence for Riot process & lockfile discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum DiscoveryRate {
    #[default]
    Normal = 0, // 1000 ms active / 2500 ms idle
    Eco = 1,    // 2000 ms active / 5000 ms idle
    Slow = 2,   // 5000 ms active / 10000 ms idle
}

impl DiscoveryRate {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Eco,
            2 => Self::Slow,
            _ => Self::Normal,
        }
    }

    pub fn as_index(self) -> i32 {
        self as u8 as i32
    }

    pub fn from_index(idx: i32) -> Option<Self> {
        match idx {
            0 => Some(Self::Normal),
            1 => Some(Self::Eco),
            2 => Some(Self::Slow),
            _ => None,
        }
    }

    pub fn active_interval(self) -> Duration {
        match self {
            Self::Normal => Duration::from_millis(1000),
            Self::Eco => Duration::from_millis(2000),
            Self::Slow => Duration::from_millis(5000),
        }
    }

    pub fn idle_interval(self) -> Duration {
        match self {
            Self::Normal => Duration::from_millis(2500),
            Self::Eco => Duration::from_millis(5000),
            Self::Slow => Duration::from_millis(10000),
        }
    }
}

/// Cadence of UI countdown interpolation and core->UI refresh throttling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum UiClockRate {
    #[default]
    Normal = 0, // 250 ms
    Relaxed = 1, // 1000 ms
}

impl UiClockRate {
    pub fn from_u8(v: u8) -> Self {
        if v == 1 {
            Self::Relaxed
        } else {
            Self::Normal
        }
    }

    pub fn as_index(self) -> i32 {
        self as u8 as i32
    }

    pub fn from_index(idx: i32) -> Option<Self> {
        match idx {
            0 => Some(Self::Normal),
            1 => Some(Self::Relaxed),
            _ => None,
        }
    }

    pub fn tick_interval(self) -> Duration {
        match self {
            Self::Normal => Duration::from_millis(250),
            Self::Relaxed => Duration::from_millis(1000),
        }
    }
}

/// Maximum number of cached images/blobs kept in RAM by both core and desktop loaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum AssetCacheBudget {
    Minimal = 0, // 16 entries
    Compact = 1, // 64 entries
    #[default]
    Full = 2,    // 256 entries
}

impl AssetCacheBudget {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Minimal,
            1 => Self::Compact,
            _ => Self::Full,
        }
    }

    pub fn as_index(self) -> i32 {
        self as u8 as i32
    }

    pub fn from_index(idx: i32) -> Option<Self> {
        match idx {
            0 => Some(Self::Minimal),
            1 => Some(Self::Compact),
            2 => Some(Self::Full),
            _ => None,
        }
    }

    pub fn max_entries(self) -> usize {
        match self {
            Self::Minimal => 16,
            Self::Compact => 64,
            Self::Full => 256,
        }
    }
}

/// Coarse machine capability class used for first-launch and "Auto" profile selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareTier {
    Constrained,
    Standard,
    Capable,
}

/// Detected host hardware summary and recommended Hypatia profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareSpec {
    pub total_ram_bytes: u64,
    pub cpu_threads: usize,
    pub tier: HardwareTier,
    pub recommended: PerformanceProfile,
}

impl HardwareSpec {
    /// Classifies raw `(total_ram_bytes, cpu_threads)` deterministically.
    pub fn classify(total_ram_bytes: u64, cpu_threads: usize) -> Self {
        const GIB: u64 = 1024 * 1024 * 1024;
        // 6.5 GiB accounts for reserved hardware RAM on 8 GiB laptops (~7.6-7.9 GiB reported).
        let low_ram_threshold = 6 * GIB + 512 * 1024 * 1024;
        // 14 GiB accounts for reserved RAM on 16 GiB systems (~15.2-15.9 GiB reported).
        let mid_ram_threshold = 14 * GIB;

        let (tier, recommended) = if (total_ram_threshold_hit(total_ram_bytes, low_ram_threshold))
            || cpu_threads <= 4
        {
            (HardwareTier::Constrained, PerformanceProfile::UltraLight)
        } else if total_ram_threshold_hit(total_ram_bytes, mid_ram_threshold) || cpu_threads <= 6 {
            (HardwareTier::Standard, PerformanceProfile::Eco)
        } else {
            (HardwareTier::Capable, PerformanceProfile::Balanced)
        };

        Self {
            total_ram_bytes,
            cpu_threads,
            tier,
            recommended,
        }
    }

    /// Probes local physical memory and logical CPU count once using lightweight `sysinfo` queries.
    pub fn detect() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        sys.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let total_ram_bytes = sys.total_memory();
        let cpu_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self::classify(total_ram_bytes, cpu_threads)
    }

    pub fn formatted(&self, lang: Lang) -> String {
        let gib = self.total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        if self.total_ram_bytes > 0 {
            if lang.is_fr() {
                format!("{gib:.1} Go RAM · {} cœurs", self.cpu_threads).replace('.', ",")
            } else {
                format!("{gib:.1} GB RAM · {} threads", self.cpu_threads)
            }
        } else if lang.is_fr() {
            format!("{} cœurs CPU", self.cpu_threads)
        } else {
            format!("{} CPU threads", self.cpu_threads)
        }
    }
}

fn total_ram_threshold_hit(total_ram_bytes: u64, threshold: u64) -> bool {
    total_ram_bytes > 0 && total_ram_bytes < threshold
}

/// Instantaneous memory/CPU reading for Hypatia's own process.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessFootprint {
    pub memory_bytes: u64,
    /// Normalized percentage across logical CPUs (`0.0..=100.0`) matching OS task managers.
    pub cpu_percent: f32,
    pub cpu_ready: bool,
}

/// On-demand process sampler for Hypatia itself; allocated only while the Performance sheet is open.
pub struct ProcessMonitor {
    sys: System,
    pid: Pid,
    cpu_divisor: f32,
    samples: u32,
}

impl ProcessMonitor {
    pub fn new() -> Self {
        let pid = Pid::from_u32(std::process::id());
        let cpu_divisor = std::thread::available_parallelism()
            .map(|n| n.get().max(1) as f32)
            .unwrap_or(1.0);
        Self {
            sys: System::new(),
            pid,
            cpu_divisor,
            samples: 0,
        }
    }

    /// Samples RSS memory and delta CPU usage for the current process only.
    pub fn sample(&mut self) -> ProcessFootprint {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            false,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
        self.samples = self.samples.saturating_add(1);
        let Some(proc) = self.sys.process(self.pid) else {
            return ProcessFootprint::default();
        };
        let cpu_percent = (proc.cpu_usage() / self.cpu_divisor).clamp(0.0, 100.0);
        ProcessFootprint {
            memory_bytes: proc.memory(),
            cpu_percent,
            cpu_ready: self.samples >= 2,
        }
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Active performance preset (`Balanced`, `Eco`, `UltraLight`, or `Custom`).
    pub profile: PerformanceProfile,
    /// Poll Live Client Data (`:2999`) while a game runs (live board).
    pub live_game: bool,
    /// Polling frequency for Live Client Data (`1s`, `2s`, or `4s`).
    pub live_poll_rate: LivePollRate,
    /// Scan frequency for Riot process & lockfile discovery.
    pub discovery_rate: DiscoveryRate,
    /// Tick & core-to-UI refresh cadence (`250ms` or `1s`).
    pub clock_rate: UiClockRate,
    /// Keep a WebSocket open to the Riot Client (chat presence).
    pub riot_client: bool,
    /// Probe the Replay API every 5 s while a game runs (Telemetry Lab only).
    pub replay_api: bool,
    /// Download, cache and decode champion / spell / profile icons.
    pub game_icons: bool,
    /// Show cached champion splash art and Summoner's Rift client artwork (needs `game_icons`).
    pub splash_backdrops: bool,
    /// Maximum number of images/blobs kept in memory (`16`, `64`, or `256`).
    pub asset_cache_budget: AssetCacheBudget,
    /// Automatically prefetch ~36 demo showcase images when League Client connects.
    pub demo_prefetch: bool,
    /// Start the Mobile Remote LAN server at launch.
    pub mobile_remote: bool,
    /// Scan Riot process CPU/RAM and sockets while the Telemetry Lab is open.
    pub system_footprint: bool,
    /// Import the Swagger / `/help` endpoint catalog while the Telemetry Lab is open.
    pub endpoint_catalog: bool,
    /// Legacy eco toggle kept in sync with presets and rate settings.
    pub eco_mode: bool,
    /// Throttle UI ticks and skip screen rebuilds when the main window is minimized.
    pub pause_when_minimized: bool,
    /// Day (light) printing instead of Night.
    pub light_theme: bool,
    /// Turn every UI animation off, on top of the OS reduced-motion preference.
    pub reduce_motion: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self::for_profile(PerformanceProfile::Balanced)
    }
}

#[derive(Deserialize)]
struct RawSettingsProbe {
    #[serde(default)]
    profile: Option<PerformanceProfile>,
    #[serde(default)]
    live_poll_rate: Option<LivePollRate>,
    #[serde(default)]
    discovery_rate: Option<DiscoveryRate>,
    #[serde(default)]
    clock_rate: Option<UiClockRate>,
    #[serde(default)]
    eco_mode: Option<bool>,
}

impl Settings {
    /// Builds a clean preset for `profile` (`Custom` resolves to `Balanced` defaults).
    pub fn for_profile(profile: PerformanceProfile) -> Self {
        match profile {
            PerformanceProfile::UltraLight => Self {
                profile: PerformanceProfile::UltraLight,
                live_game: true,
                live_poll_rate: LivePollRate::Slow,
                discovery_rate: DiscoveryRate::Slow,
                clock_rate: UiClockRate::Relaxed,
                riot_client: false,
                replay_api: false,
                game_icons: false,
                splash_backdrops: false,
                asset_cache_budget: AssetCacheBudget::Minimal,
                demo_prefetch: false,
                mobile_remote: false,
                system_footprint: false,
                endpoint_catalog: false,
                eco_mode: true,
                pause_when_minimized: true,
                light_theme: false,
                reduce_motion: true,
            },
            PerformanceProfile::Eco => Self {
                profile: PerformanceProfile::Eco,
                live_game: true,
                live_poll_rate: LivePollRate::Eco,
                discovery_rate: DiscoveryRate::Eco,
                clock_rate: UiClockRate::Relaxed,
                riot_client: true,
                replay_api: false,
                game_icons: true,
                splash_backdrops: false,
                asset_cache_budget: AssetCacheBudget::Compact,
                demo_prefetch: false,
                mobile_remote: true,
                system_footprint: true,
                endpoint_catalog: true,
                eco_mode: true,
                pause_when_minimized: true,
                light_theme: false,
                reduce_motion: false,
            },
            PerformanceProfile::Balanced | PerformanceProfile::Custom => Self {
                profile: PerformanceProfile::Balanced,
                live_game: true,
                live_poll_rate: LivePollRate::Fast,
                discovery_rate: DiscoveryRate::Normal,
                clock_rate: UiClockRate::Normal,
                riot_client: true,
                replay_api: false,
                game_icons: true,
                splash_backdrops: true,
                asset_cache_budget: AssetCacheBudget::Full,
                demo_prefetch: false,
                mobile_remote: true,
                system_footprint: true,
                endpoint_catalog: true,
                eco_mode: false,
                pause_when_minimized: false,
                light_theme: false,
                reduce_motion: false,
            },
        }
    }

    pub fn default_path() -> PathBuf {
        default_config_dir().join("settings.json")
    }

    /// Loads settings from `path`; a missing or malformed file yields the defaults.
    pub fn load_from(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        let Ok(mut parsed) = serde_json::from_slice::<Self>(&bytes) else {
            return Self::default();
        };
        if let Ok(probe) = serde_json::from_slice::<RawSettingsProbe>(&bytes) {
            // Upgrade legacy files that only stored `eco_mode: true` cleanly.
            if probe.profile.is_none() && probe.eco_mode == Some(true) {
                if probe.live_poll_rate.is_none() {
                    parsed.live_poll_rate = LivePollRate::Eco;
                }
                if probe.discovery_rate.is_none() {
                    parsed.discovery_rate = DiscoveryRate::Eco;
                }
                if probe.clock_rate.is_none() {
                    parsed.clock_rate = UiClockRate::Relaxed;
                }
            }
            if probe.profile.is_none() {
                parsed.sync_profile();
            }
        }
        parsed
    }

    /// Loads persisted settings, or on first launch (no `settings.json` yet) applies the
    /// hardware-recommended profile and writes it to `path`.
    pub fn load_or_init_from(path: &Path, hw: HardwareSpec) -> (Self, bool) {
        if path.exists() {
            (Self::load_from(path), false)
        } else {
            let s = Self::for_profile(hw.recommended);
            let _ = s.save_to(path);
            (s, true)
        }
    }

    pub fn load_or_init(hw: HardwareSpec) -> (Self, bool) {
        Self::load_or_init_from(&Self::default_path(), hw)
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

    /// Applies a named profile preset while keeping the user's `light_theme` choice.
    pub fn apply_profile(&mut self, profile: PerformanceProfile) -> bool {
        if profile == PerformanceProfile::Custom {
            self.profile = PerformanceProfile::Custom;
            return false;
        }
        let light = self.light_theme;
        *self = Self::for_profile(profile);
        self.light_theme = light;
        true
    }

    /// Recomputes `profile` by checking whether performance fields match one of the presets.
    pub fn sync_profile(&mut self) {
        for preset in [
            PerformanceProfile::UltraLight,
            PerformanceProfile::Eco,
            PerformanceProfile::Balanced,
        ] {
            if self.matches_preset(preset) {
                self.profile = preset;
                return;
            }
        }
        self.profile = PerformanceProfile::Custom;
    }

    fn matches_preset(&self, preset: PerformanceProfile) -> bool {
        let mut p = Self::for_profile(preset);
        p.light_theme = self.light_theme;
        p.profile = self.profile;
        *self == p
    }

    /// Updates a multi-choice performance setting by index and syncs `profile`.
    pub fn set_choice(&mut self, key: &str, index: i32) -> bool {
        match key {
            "live_poll_rate" => {
                let Some(v) = LivePollRate::from_index(index) else {
                    return false;
                };
                self.live_poll_rate = v;
                self.sync_eco_flag();
            }
            "discovery_rate" => {
                let Some(v) = DiscoveryRate::from_index(index) else {
                    return false;
                };
                self.discovery_rate = v;
                self.sync_eco_flag();
            }
            "clock_rate" => {
                let Some(v) = UiClockRate::from_index(index) else {
                    return false;
                };
                self.clock_rate = v;
                self.sync_eco_flag();
            }
            "asset_cache_budget" => {
                let Some(v) = AssetCacheBudget::from_index(index) else {
                    return false;
                };
                self.asset_cache_budget = v;
            }
            _ => return false,
        }
        self.sync_profile();
        true
    }

    fn sync_eco_flag(&mut self) {
        self.eco_mode = self.live_poll_rate != LivePollRate::Fast
            || self.discovery_rate != DiscoveryRate::Normal
            || self.clock_rate != UiClockRate::Normal;
    }

    /// Toggles the setting named `key`; returns `false` for an unknown key.
    pub fn toggle(&mut self, key: &str) -> bool {
        if key == "eco_mode" {
            let next = !self.eco_mode;
            self.eco_mode = next;
            if next {
                if self.live_poll_rate == LivePollRate::Fast {
                    self.live_poll_rate = LivePollRate::Eco;
                }
                if self.discovery_rate == DiscoveryRate::Normal {
                    self.discovery_rate = DiscoveryRate::Eco;
                }
                if self.clock_rate == UiClockRate::Normal {
                    self.clock_rate = UiClockRate::Relaxed;
                }
            } else {
                self.live_poll_rate = LivePollRate::Fast;
                self.discovery_rate = DiscoveryRate::Normal;
                self.clock_rate = UiClockRate::Normal;
            }
            self.sync_profile();
            return true;
        }

        let field = match key {
            "live_game" => &mut self.live_game,
            "riot_client" => &mut self.riot_client,
            "replay_api" => &mut self.replay_api,
            "game_icons" => &mut self.game_icons,
            "mobile_remote" => &mut self.mobile_remote,
            "system_footprint" => &mut self.system_footprint,
            "endpoint_catalog" => &mut self.endpoint_catalog,
            "light_theme" => &mut self.light_theme,
            "reduce_motion" => &mut self.reduce_motion,
            "splash_backdrops" => &mut self.splash_backdrops,
            "demo_prefetch" => &mut self.demo_prefetch,
            "pause_when_minimized" => &mut self.pause_when_minimized,
            _ => return false,
        };
        *field = !*field;
        if key != "light_theme" {
            self.sync_profile();
        }
        true
    }
}

/// Lock-free mirror of [`Settings`] plus the Telemetry Lab open/closed state.
#[derive(Debug, Default)]
pub struct Flags {
    live_game: AtomicBool,
    live_poll_rate: AtomicU8,
    discovery_rate: AtomicU8,
    clock_rate: AtomicU8,
    riot_client: AtomicBool,
    replay_api: AtomicBool,
    game_icons: AtomicBool,
    splash_backdrops: AtomicBool,
    asset_cache_budget: AtomicU8,
    demo_prefetch: AtomicBool,
    system_footprint: AtomicBool,
    endpoint_catalog: AtomicBool,
    eco_mode: AtomicBool,
    pause_when_minimized: AtomicBool,
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
        let legacy_eco = s.eco_mode
            && s.live_poll_rate == LivePollRate::Fast
            && s.discovery_rate == DiscoveryRate::Normal
            && s.clock_rate == UiClockRate::Normal;
        let live_poll = if legacy_eco {
            LivePollRate::Eco
        } else {
            s.live_poll_rate
        };
        let disc_rate = if legacy_eco {
            DiscoveryRate::Eco
        } else {
            s.discovery_rate
        };
        let clock_rate = if legacy_eco {
            UiClockRate::Relaxed
        } else {
            s.clock_rate
        };
        self.live_poll_rate.store(live_poll as u8, Ordering::Relaxed);
        self.discovery_rate.store(disc_rate as u8, Ordering::Relaxed);
        self.clock_rate.store(clock_rate as u8, Ordering::Relaxed);
        self.riot_client.store(s.riot_client, Ordering::Relaxed);
        self.replay_api.store(s.replay_api, Ordering::Relaxed);
        self.game_icons.store(s.game_icons, Ordering::Relaxed);
        self.splash_backdrops.store(s.splash_backdrops, Ordering::Relaxed);
        self.asset_cache_budget
            .store(s.asset_cache_budget as u8, Ordering::Relaxed);
        self.demo_prefetch.store(s.demo_prefetch, Ordering::Relaxed);
        self.system_footprint.store(s.system_footprint, Ordering::Relaxed);
        self.endpoint_catalog.store(s.endpoint_catalog, Ordering::Relaxed);
        self.eco_mode.store(s.eco_mode, Ordering::Relaxed);
        self.pause_when_minimized
            .store(s.pause_when_minimized, Ordering::Relaxed);
    }

    pub fn set_telemetry(&self, on: bool) {
        self.telemetry.store(on, Ordering::Relaxed);
    }

    pub fn live_game(&self) -> bool {
        self.live_game.load(Ordering::Relaxed)
    }

    pub fn live_poll_rate(&self) -> LivePollRate {
        LivePollRate::from_u8(self.live_poll_rate.load(Ordering::Relaxed))
    }

    pub fn live_poll_interval(&self) -> Duration {
        self.live_poll_rate().interval()
    }

    pub fn live_idle_interval(&self) -> Duration {
        self.live_poll_rate().idle_interval()
    }

    pub fn discovery_rate(&self) -> DiscoveryRate {
        DiscoveryRate::from_u8(self.discovery_rate.load(Ordering::Relaxed))
    }

    pub fn clock_rate(&self) -> UiClockRate {
        UiClockRate::from_u8(self.clock_rate.load(Ordering::Relaxed))
    }

    pub fn ui_clock_interval(&self) -> Duration {
        self.clock_rate().tick_interval()
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

    pub fn asset_cache_budget(&self) -> AssetCacheBudget {
        AssetCacheBudget::from_u8(self.asset_cache_budget.load(Ordering::Relaxed))
    }

    pub fn demo_prefetch(&self) -> bool {
        self.demo_prefetch.load(Ordering::Relaxed)
    }

    pub fn eco_mode(&self) -> bool {
        self.eco_mode.load(Ordering::Relaxed)
    }

    pub fn pause_when_minimized(&self) -> bool {
        self.pause_when_minimized.load(Ordering::Relaxed)
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
        assert_eq!(s.profile, PerformanceProfile::Custom);
        assert!(!s.toggle("nope"));
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), s);

        std::fs::write(&path, br#"{"live_game": false}"#).unwrap();
        let partial = Settings::load_from(&path);
        assert!(!partial.live_game);
        assert!(partial.riot_client, "missing keys fall back to defaults");
        assert!(partial.splash_backdrops);
        assert_eq!(partial.profile, PerformanceProfile::Custom);

        // Legacy eco_mode-only config upgrades to Eco rates.
        std::fs::write(&path, br#"{"eco_mode": true}"#).unwrap();
        let legacy_eco = Settings::load_from(&path);
        assert!(legacy_eco.eco_mode);
        assert_eq!(legacy_eco.live_poll_rate, LivePollRate::Eco);
        assert_eq!(legacy_eco.discovery_rate, DiscoveryRate::Eco);
        assert_eq!(legacy_eco.clock_rate, UiClockRate::Relaxed);
    }

    #[test]
    fn classifies_hardware_and_initializes_first_launch_settings() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let low = HardwareSpec::classify(4 * GIB, 4);
        assert_eq!(low.tier, HardwareTier::Constrained);
        assert_eq!(low.recommended, PerformanceProfile::UltraLight);

        let mid = HardwareSpec::classify(8 * GIB, 8);
        assert_eq!(mid.tier, HardwareTier::Standard);
        assert_eq!(mid.recommended, PerformanceProfile::Eco);

        let high = HardwareSpec::classify(16 * GIB, 8);
        assert_eq!(high.tier, HardwareTier::Capable);
        assert_eq!(high.recommended, PerformanceProfile::Balanced);

        // First launch writes recommended profile; second launch preserves user tweaks.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (first, created) = Settings::load_or_init_from(&path, low);
        assert!(created);
        assert_eq!(first.profile, PerformanceProfile::UltraLight);
        assert!(!first.game_icons);

        let (second, created_again) = Settings::load_or_init_from(&path, high);
        assert!(!created_again);
        assert_eq!(second.profile, PerformanceProfile::UltraLight);
    }

    #[test]
    fn presets_preserve_theme_and_custom_tracks_divergence() {
        let mut s = Settings::default();
        s.light_theme = true;
        s.apply_profile(PerformanceProfile::UltraLight);
        assert!(s.light_theme);
        assert_eq!(s.profile, PerformanceProfile::UltraLight);
        assert!(!s.game_icons);
        assert!(!s.splash_backdrops);
        assert_eq!(s.asset_cache_budget, AssetCacheBudget::Minimal);
        assert_eq!(s.live_poll_rate, LivePollRate::Slow);
        assert_eq!(s.discovery_rate, DiscoveryRate::Slow);
        assert!(s.pause_when_minimized);

        // Tweaking one option transitions to Custom, reversing brings preset back.
        assert!(s.toggle("game_icons"));
        assert_eq!(s.profile, PerformanceProfile::Custom);
        assert!(s.toggle("game_icons"));
        assert_eq!(s.profile, PerformanceProfile::UltraLight);

        // Granular choices update Flags intervals accurately, including Fast live poll mixed with Slow discovery.
        assert!(s.set_choice("live_poll_rate", 1));
        assert_eq!(s.live_poll_rate, LivePollRate::Eco);
        let flags = Flags::new(&s);
        assert_eq!(flags.live_poll_interval(), Duration::from_millis(2000));
        assert_eq!(flags.discovery_rate().active_interval(), Duration::from_millis(5000));
        assert_eq!(flags.asset_cache_budget().max_entries(), 16);

        assert!(s.set_choice("live_poll_rate", 0));
        flags.apply(&s);
        assert_eq!(flags.live_poll_interval(), Duration::from_millis(1000));
        assert_eq!(flags.discovery_rate().active_interval(), Duration::from_millis(5000));
    }

    #[test]
    fn process_monitor_reads_current_process_footprint() {
        let mut monitor = ProcessMonitor::new();
        let first = monitor.sample();
        assert!(first.memory_bytes > 0);
        assert!(!first.cpu_ready);
        let second = monitor.sample();
        assert!(second.memory_bytes > 0);
        assert!(second.cpu_ready);
        assert!((0.0..=100.0).contains(&second.cpu_percent));
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
