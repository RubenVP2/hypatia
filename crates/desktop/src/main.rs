#![cfg_attr(all(not(test), not(debug_assertions)), windows_subsystem = "windows")]

mod cli;
mod companion;
mod icons;
mod labels;
mod links;
#[cfg(test)]
mod shots;
mod stats_view;
mod telemetry;
mod updates;
mod win_dpi;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use hypatia_core::remote::{start_remote_server_with_token, QrCodeVisual, RemoteServerHandle};
use hypatia_core::settings::{
    HardwareSpec, PerformanceProfile, ProcessFootprint, ProcessMonitor, Settings, UiClockRate,
    UpdateCheck,
};
use hypatia_core::update::{self, UpdateError, Updater, Version};
use hypatia_core::AppState;
use hypatia_shared::{Lang, SourceId};
use serde_json::Value;
use slint::{
    ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode,
};
use tracing::info;

use crate::cli::Cli;
use crate::companion::Companion;
use crate::labels::format_bytes;
use crate::stats_view::StatsView;
use crate::telemetry::{apply_telemetry_to_window, TelemetryUiState};
use crate::updates::{InstallStep, UpdateState};

slint::include_modules!();

thread_local! {
    /// The UI lives on the Slint event-loop thread; background tasks reach it through
    /// `slint::invoke_from_event_loop(|| with_ui(..))`.
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

fn with_ui(f: impl FnOnce(&Rc<Ui>)) {
    let ui = UI.with(|slot| slot.borrow().clone());
    if let Some(ui) = ui {
        f(&ui);
    }
}

/// Window, core state and view models owned by the event-loop thread.
struct Ui {
    main: MainWindow,
    app: AppState,
    rt: tokio::runtime::Handle,
    cli: Cli,
    hw: HardwareSpec,
    settings: RefCell<Settings>,
    companion: RefCell<Companion>,
    telemetry: RefCell<TelemetryUiState>,
    /// The Stats page; reads Champion Stats only while it is open.
    stats: RefCell<StatsView>,
    /// Refreshes the Telemetry Lab's event-rate readout; runs only while the Lab is open.
    telemetry_clock: Timer,
    /// Extrapolates the active phase clock between two LCU events; runs only while one exists.
    phase_clock: Timer,
    /// Currently scheduled interval of `phase_clock`, so rate changes restart cleanly.
    phase_clock_period: Cell<Option<Duration>>,
    /// Samples Hypatia's RAM/CPU every 1s strictly while the Performance sheet is open.
    perf_clock: Timer,
    perf_monitor: RefCell<Option<ProcessMonitor>>,
    last_footprint: Cell<ProcessFootprint>,
    /// Cached disk usage keyed by `image_revision()` to avoid scanning the cache folder every second.
    cached_disk_stats: RefCell<Option<(u64, usize, u64)>>,
    /// Flags a skipped rebuild while sleeping minimized so restoring catches up immediately.
    deferred_refresh: Cell<bool>,
    /// Lightweight 1s poll active only while sleeping minimized, waking when restored.
    restore_timer: Timer,
    remote: RefCell<Option<RemoteServerHandle>>,
    remote_starting: Cell<bool>,
    /// Saved `(port, token)` while Mobile Remote is auto-paused in game so it resumes without
    /// forcing the player to re-scan the QR code.
    remote_paused: RefCell<Option<(u16, String)>>,
    /// Set when the user manually opens Mobile Remote during a match; cleared after the match.
    remote_in_game_override: Cell<bool>,
    /// Update Check: asks GitHub only while `Settings::update_check` is on.
    updater: Arc<Updater>,
    update: RefCell<UpdateState>,
    /// Asks again every `update::CHECK_INTERVAL`; runs only while update checks are on.
    update_clock: Timer,
    /// One retry `update::RETRY_INTERVAL` after a check that could not reach GitHub.
    update_retry: Timer,
    /// Set once an update is installed: `main` starts this executable after the window closes.
    restart_into: RefCell<Option<PathBuf>>,
}

impl Ui {
    /// True when the main window is minimized and sleep is enabled.
    fn is_ui_sleeping(&self) -> bool {
        self.app.flags.pause_when_minimized() && self.main.window().is_minimized()
    }

    fn ensure_restore_watcher(&self) {
        if !self.restore_timer.running() {
            self.restore_timer
                .start(TimerMode::Repeated, Duration::from_secs(1), || {
                    with_ui(|ui| {
                        if !ui.is_ui_sleeping() {
                            ui.restore_timer.stop();
                            if ui.deferred_refresh.replace(false) {
                                ui.refresh();
                            } else {
                                ui.sync_phase_clock();
                            }
                        }
                    });
                });
        }
    }

    fn stop_phase_clock(&self) {
        self.phase_clock.stop();
        self.phase_clock_period.set(None);
    }

    /// Brings every visible view up to date. Cheap when nothing changed: the companion compares
    /// one `Arc` pointer and the Telemetry Lab one hash.
    fn refresh(&self) {
        // Keep Mobile Remote pause/resume in sync with match phase even while the window sleeps
        // minimized during a game.
        self.sync_remote_for_phase();
        if self.is_ui_sleeping() {
            self.deferred_refresh.set(true);
            self.stop_phase_clock();
            self.ensure_restore_watcher();
            return;
        }
        if self.restore_timer.running() {
            self.restore_timer.stop();
        }
        self.deferred_refresh.set(false);
        self.companion.borrow_mut().refresh(&self.main, &self.app);
        self.refresh_telemetry();
        self.stats.borrow_mut().refresh(&self.main, &self.app);
        let clients = self
            .remote
            .borrow()
            .as_ref()
            .map_or(0, |h| h.connected_clients() as i32);
        self.main.set_remote_clients(clients);
        if self.main.get_settings_open() {
            self.sync_coach_llm_test_status();
        }
        self.sync_phase_clock();
    }

    /// Starts, stops and repaints the phase clock to match what the companion shows.
    fn sync_phase_clock(&self) {
        let clock = self.companion.borrow().clock();
        match clock {
            Some(clock) => {
                self.main.set_clock_total(clock.total_sec);
                self.main.set_clock_down(clock.counts_down());
                if self.is_ui_sleeping() {
                    self.stop_phase_clock();
                    self.ensure_restore_watcher();
                    return;
                }
                self.tick_phase_clock();
                let period = self.app.flags.ui_clock_interval();
                if !self.phase_clock.running() || self.phase_clock_period.get() != Some(period) {
                    self.phase_clock.start(TimerMode::Repeated, period, || {
                        with_ui(|ui| ui.tick_phase_clock());
                    });
                    self.phase_clock_period.set(Some(period));
                }
            }
            None => {
                self.stop_phase_clock();
                self.main.set_clock_seconds(-1);
            }
        }
    }

    fn tick_phase_clock(&self) {
        if self.is_ui_sleeping() {
            self.stop_phase_clock();
            self.deferred_refresh.set(true);
            self.ensure_restore_watcher();
            return;
        }
        if let Some(clock) = self.companion.borrow().clock() {
            let secs = clock.seconds_now(Instant::now());
            self.main.set_clock_seconds(secs);
        }
    }

    fn refresh_telemetry(&self) {
        apply_telemetry_to_window(&self.main, &self.app, &mut self.telemetry.borrow_mut());
    }

    /// Re-renders the Telemetry Lab after `change` mutated its view model.
    fn update_telemetry(&self, change: impl FnOnce(&mut TelemetryUiState)) {
        change(&mut self.telemetry.borrow_mut());
        self.refresh_telemetry();
    }

    fn set_telemetry(&self, on: bool) {
        self.main.set_telemetry_enabled(on);
        self.app.set_telemetry(on);
        if on {
            self.telemetry_clock
                .start(TimerMode::Repeated, Duration::from_secs(1), || {
                    with_ui(|ui| ui.refresh_telemetry());
                });
            self.update_telemetry(|st| st.invalidate_telemetry_cache());
        } else {
            self.telemetry_clock.stop();
        }
        if self.main.get_settings_open() {
            self.update_perf_metrics(false);
        }
    }

    fn set_lang(&self, lang: Lang) {
        self.app.set_lang(lang);
        apply_ui_language(&self.main, lang);
        self.telemetry.borrow_mut().invalidate_telemetry_cache();
        let settings = *self.settings.borrow();
        self.main.set_perf(perf_settings(&settings, self.hw, lang));
        if self.main.get_settings_open() {
            self.update_perf_metrics(false);
        }
        self.push_update_view();
        self.refresh();
    }

    fn set_dark(&self, dark: bool) {
        Theme::get(&self.main).set_dark(dark);
        let mut settings = self.settings.borrow_mut();
        if settings.light_theme == dark {
            settings.light_theme = !dark;
            settings.save();
        }
    }

    fn motion_enabled(&self) -> bool {
        !(self.cli.reduce_motion
            || self.settings.borrow().reduce_motion
            || os_prefers_reduced_motion())
    }

    /// Animations run unless the OS, the settings sheet or `--reduce-motion` turns them off.
    fn apply_motion(&self) {
        let motion = self.motion_enabled();
        Theme::get(&self.main).set_motion(motion);
    }

    fn set_demo_phase(&self, phase: &str) {
        self.app.set_demo_phase(phase);
    }

    fn toggle_demo(&self) {
        if !self.main.get_dev_tools() {
            return;
        }
        let next = !self.main.get_demo();
        self.app.set_demo_active(next, self.cli.demo_rate);
        self.main.set_demo(next);
        self.telemetry.borrow_mut().invalidate_telemetry_cache();
        self.refresh();
        if self.main.get_settings_open() {
            self.update_perf_metrics(false);
        }
    }

    // --- Performance & live footprint monitor --------------------------------------------

    fn on_performance_panel_changed(&self, open: bool) {
        if open {
            if self.perf_monitor.borrow().is_none() {
                *self.perf_monitor.borrow_mut() = Some(ProcessMonitor::new());
            }
            self.update_perf_metrics(true);
            self.perf_clock
                .start(TimerMode::Repeated, Duration::from_secs(1), || {
                    with_ui(|ui| {
                        if ui.main.get_settings_open() && !ui.is_ui_sleeping() {
                            ui.update_perf_metrics(true);
                        }
                    });
                });
        } else {
            self.perf_clock.stop();
            self.perf_monitor.borrow_mut().take();
            self.last_footprint.set(ProcessFootprint::default());
            self.main.set_perf_metrics(PerfMetrics::default());
        }
    }

    /// Applies a Stats page choice, then shows its result.
    fn update_stats(&self, change: impl FnOnce(&mut StatsView, &AppState)) {
        change(&mut self.stats.borrow_mut(), &self.app);
        self.stats.borrow_mut().refresh(&self.main, &self.app);
    }

    fn update_perf_metrics(&self, sample_process: bool) {
        let lang = self.app.lang();
        if sample_process {
            let footprint = self
                .perf_monitor
                .borrow_mut()
                .as_mut()
                .map(ProcessMonitor::sample)
                .unwrap_or_default();
            self.last_footprint.set(footprint);
        }
        let footprint = self.last_footprint.get();

        let (core_entries, core_bytes) = self.app.assets.memory_stats();
        let (ui_entries, ui_bytes) = self.companion.borrow().loader_stats();
        let cache_ram_count = ui_entries.max(core_entries);
        let cache_ram_bytes = core_bytes.saturating_add(ui_bytes);

        let rev = self.app.assets.image_revision();
        let (disk_files, disk_bytes) = {
            let mut cached = self.cached_disk_stats.borrow_mut();
            match *cached {
                Some((cached_rev, files, bytes)) if cached_rev == rev => (files, bytes),
                _ => {
                    let (files, bytes) = self.app.assets.disk_stats();
                    *cached = Some((rev, files, bytes));
                    (files, bytes)
                }
            }
        };

        let settings = *self.settings.borrow();
        let active_extras = ActiveExtras {
            demo: self.app.is_demo_active(),
            observatory: self.main.get_telemetry_enabled(),
            remote: self.remote.borrow().is_some(),
        };

        self.main.set_perf_metrics(build_perf_metrics(
            footprint,
            (cache_ram_count, cache_ram_bytes),
            (disk_files, disk_bytes),
            &settings,
            active_extras,
            lang,
        ));
    }

    fn apply_updated_settings(
        &self,
        settings: &Settings,
        prev: &Settings,
        explicit_mobile_toggle: bool,
    ) {
        settings.save();
        self.app.apply_settings(settings);
        self.main
            .set_perf(perf_settings(settings, self.hw, self.app.lang()));
        if settings.reduce_motion != prev.reduce_motion {
            self.apply_motion();
        }
        if settings.update_check != prev.update_check {
            self.sync_update_checks();
        }
        if settings.mobile_remote != prev.mobile_remote {
            if settings.mobile_remote {
                if !self.cli.no_remote || explicit_mobile_toggle {
                    self.start_remote(false);
                }
            } else {
                self.stop_remote();
            }
        }
        self.refresh();
        if self.main.get_settings_open() {
            self.update_perf_metrics(false);
        }
    }

    fn select_perf_profile(&self, code: &str) {
        let Some(profile) = PerformanceProfile::from_code(code) else {
            return;
        };
        let (prev, next) = {
            let mut settings = self.settings.borrow_mut();
            let prev = *settings;
            if !settings.apply_profile(profile) {
                return;
            }
            (prev, *settings)
        };
        self.apply_updated_settings(&next, &prev, false);
    }

    fn apply_recommended_profile(&self) {
        let (prev, next) = {
            let mut settings = self.settings.borrow_mut();
            let prev = *settings;
            if !settings.apply_profile(self.hw.recommended) {
                return;
            }
            (prev, *settings)
        };
        self.apply_updated_settings(&next, &prev, false);
    }

    fn set_perf_choice(&self, key: &str, index: i32) {
        let (prev, next) = {
            let mut settings = self.settings.borrow_mut();
            let prev = *settings;
            if !settings.set_choice(key, index) {
                return;
            }
            (prev, *settings)
        };
        self.apply_updated_settings(&next, &prev, false);
    }

    fn toggle_setting(&self, key: &str) {
        let (prev, next) = {
            let mut settings = self.settings.borrow_mut();
            let prev = *settings;
            if !settings.toggle(key) {
                return;
            }
            (prev, *settings)
        };
        self.apply_updated_settings(&next, &prev, key == "mobile_remote");
    }

    fn clear_ram_cache(&self) {
        self.app.clear_asset_memory_cache();
        self.companion.borrow_mut().clear_icon_cache(&self.main);
        self.refresh();
        if self.main.get_settings_open() {
            self.update_perf_metrics(false);
        }
    }

    fn clear_disk_cache(&self) {
        self.app.clear_asset_disk_cache();
        self.companion.borrow_mut().clear_icon_cache(&self.main);
        self.cached_disk_stats.borrow_mut().take();
        self.refresh();
        if self.main.get_settings_open() {
            self.update_perf_metrics(false);
        }
    }

    // --- Mobile Remote -------------------------------------------------------------------

    fn show_pairing(&self, qr: &QrCodeVisual) {
        let pixels =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&qr.rgba, qr.width, qr.height);
        self.main.set_remote_qr(Image::from_rgba8(pixels));
        self.main
            .set_remote_url(SharedString::from(qr.url.as_str()));
    }

    /// Starts the LAN server in the background so the window never waits on the network.
    fn start_remote(&self, open_drawer: bool) {
        self.start_remote_with(open_drawer, None);
    }

    fn start_remote_with(&self, open_drawer: bool, resume: Option<(u16, String)>) {
        if self.remote.borrow().is_some() || self.remote_starting.replace(true) {
            return;
        }
        let app = self.app.clone();
        let port = resume.as_ref().map_or(self.cli.remote_port, |(p, _)| *p);
        let token = resume.map(|(_, t)| t);
        self.rt.spawn(async move {
            let started = start_remote_server_with_token(app, port, token).await;
            let _ = slint::invoke_from_event_loop(move || {
                with_ui(|ui| {
                    ui.remote_starting.set(false);
                    match started {
                        Ok(handle) => {
                            if let Ok(qr) = handle.qr_visual() {
                                info!("Mobile Remote ready at {}", qr.url);
                                ui.show_pairing(&qr);
                            }
                            *ui.remote.borrow_mut() = Some(handle);
                            ui.main.set_remote_enabled(true);
                            ui.main.set_remote_paused(false);
                            if open_drawer {
                                ui.main.set_settings_open(false);
                            }
                            ui.main.set_remote_panel_open(open_drawer);
                            if ui.main.get_settings_open() {
                                ui.update_perf_metrics(false);
                            }
                            ui.sync_remote_for_phase();
                        }
                        Err(e) => tracing::error!("Failed to start Mobile Remote: {e}"),
                    }
                });
            });
        });
    }

    fn stop_remote(&self) {
        if let Some(mut handle) = self.remote.borrow_mut().take() {
            handle.stop();
        }
        self.remote_paused.borrow_mut().take();
        self.remote_in_game_override.set(false);
        self.main.set_remote_enabled(false);
        self.main.set_remote_paused(false);
        self.main.set_remote_panel_open(false);
        self.main.set_remote_url(SharedString::default());
        self.main.set_remote_clients(0);
    }

    /// Pauses the Mobile Remote server during an active match when the option is on, keeping
    /// `(port, token)` so the paired phone reconnects automatically once the game ends.
    fn sync_remote_for_phase(&self) {
        let in_game = self.app.is_in_game();
        if !in_game {
            self.remote_in_game_override.set(false);
        }
        let should_pause = in_game
            && self.app.flags.mobile_remote_pause_in_game()
            && !self.remote_in_game_override.get();

        if should_pause {
            if let Some(mut handle) = self.remote.borrow_mut().take() {
                let port = handle.port;
                let token = handle.token();
                handle.stop();
                *self.remote_paused.borrow_mut() = Some((port, token));
                self.main.set_remote_enabled(false);
                self.main.set_remote_paused(true);
                self.main.set_remote_panel_open(false);
                self.main.set_remote_clients(0);
                info!("Mobile Remote paused for live game (will resume on port {port})");
                if self.main.get_settings_open() {
                    self.update_perf_metrics(false);
                }
            }
        } else if self.remote.borrow().is_none() && !self.remote_starting.get() {
            if let Some(saved) = self.remote_paused.borrow_mut().take() {
                info!("Resuming Mobile Remote on port {}", saved.0);
                self.main.set_remote_paused(false);
                self.start_remote_with(false, Some(saved));
            }
        }
    }

    fn toggle_remote_panel(&self) {
        if self.remote.borrow().is_some() {
            self.main.set_settings_open(false);
            self.main
                .set_remote_panel_open(!self.main.get_remote_panel_open());
        } else {
            if self.app.is_in_game() {
                self.remote_in_game_override.set(true);
            }
            let resume = self.remote_paused.borrow_mut().take();
            self.main.set_remote_paused(false);
            self.start_remote_with(true, resume);
        }
    }

    fn rotate_remote_token(&self) {
        let rotated = self.remote.borrow().as_ref().map(|h| h.rotate_token());
        if let Some(Ok(qr)) = rotated {
            self.show_pairing(&qr);
        }
    }

    // --- Update Check --------------------------------------------------------------------

    fn push_update_view(&self) {
        let view = self
            .update
            .borrow()
            .view(Version::current(), self.app.lang());
        if !view.available {
            self.main.set_update_open(false);
        }
        self.main.set_update(view);
    }

    /// Checks now, then every `CHECK_INTERVAL`, while update checks are on. Off, nothing runs
    /// and what an earlier check found is dropped.
    fn sync_update_checks(&self) {
        if !self.app.flags.update_check() {
            self.update_clock.stop();
            self.update_retry.stop();
            self.update.borrow_mut().found(None);
            self.push_update_view();
            return;
        }
        self.check_for_update();
        if !self.update_clock.running() {
            self.update_clock
                .start(TimerMode::Repeated, update::CHECK_INTERVAL, || {
                    with_ui(|ui| ui.check_for_update());
                });
        }
    }

    fn check_for_update(&self) {
        let updater = self.updater.clone();
        self.rt.spawn(async move {
            let found = updater.check(Version::current()).await;
            let _ = slint::invoke_from_event_loop(move || {
                with_ui(|ui| match found {
                    Ok(release) => {
                        ui.update_retry.stop();
                        if let Some(release) = &release {
                            info!("Hypatia {} is available", release.version);
                        }
                        ui.update.borrow_mut().found(release);
                        ui.push_update_view();
                    }
                    // A failed background check stays quiet. GitHub out of reach (often the
                    // network is not up yet at login): try again soon, not in 12 hours.
                    Err(err) => {
                        tracing::debug!("Update check failed: {err:?}");
                        if err.retry_soon() && ui.app.flags.update_check() {
                            ui.update_retry.start(
                                TimerMode::SingleShot,
                                update::RETRY_INTERVAL,
                                || with_ui(|ui| ui.check_for_update()),
                            );
                        }
                    }
                });
            });
        });
    }

    fn answer_update_check(&self, check: bool) {
        self.main.set_update_consent_open(false);
        let (prev, next) = {
            let mut settings = self.settings.borrow_mut();
            let prev = *settings;
            settings.update_check = if check {
                UpdateCheck::On
            } else {
                UpdateCheck::Off
            };
            (prev, *settings)
        };
        self.apply_updated_settings(&next, &prev, false);
    }

    fn push_coach_llm_view(&self) {
        let cfg = self.app.coach_llm.config();
        let (test_state, test_status) =
            coach_test_status(&self.app.coach_llm.connection_test_state());
        self.main.set_coach_llm_cfg(CoachLlmView {
            preset: cfg.preset.as_index(),
            base_url: SharedString::from(cfg.base_url),
            model: SharedString::from(cfg.model),
            api_key: SharedString::from(cfg.api_key),
            test_state,
            test_status,
        });
    }

    fn sync_coach_llm_test_status(&self) {
        let (test_state, test_status) =
            coach_test_status(&self.app.coach_llm.connection_test_state());
        let mut current = self.main.get_coach_llm_cfg();
        if current.test_state != test_state || current.test_status != test_status {
            current.test_state = test_state;
            current.test_status = test_status;
            self.main.set_coach_llm_cfg(current);
        }
    }

    fn select_coach_llm_preset(&self, idx: i32) {
        let preset = hypatia_core::coach_llm::CoachLlmPreset::from_index(idx);
        let mut cfg = hypatia_core::coach_llm::CoachLlmConfig::for_preset(preset);
        if preset == hypatia_core::coach_llm::CoachLlmPreset::Custom {
            cfg.api_key = self.app.coach_llm.config().api_key;
        }
        self.app.coach_llm.save_config(cfg);
        self.push_coach_llm_view();
    }

    fn save_coach_llm_config(&self, base_url: &str, model: &str, api_key: &str) {
        let mut cfg = self.app.coach_llm.config();
        cfg.base_url = base_url.trim().to_string();
        cfg.model = model.trim().to_string();
        cfg.api_key = api_key.trim().to_string();
        self.app.coach_llm.save_config(cfg);
        self.push_coach_llm_view();
    }

    fn test_coach_llm(&self) {
        self.app.coach_llm.test_connection(self.app.lang());
        self.sync_coach_llm_test_status();
    }

    /// Downloads, verifies and installs the release on the player's click, then restarts.
    fn install_update(&self) {
        let release = {
            let state = self.update.borrow();
            match &state.release {
                Some(release) if release.installable() && !state.busy() => release.clone(),
                _ => return,
            }
        };
        let Ok(exe) = std::env::current_exe() else {
            self.finish_install(Err(UpdateError::NotWritable), PathBuf::new());
            return;
        };
        self.update.borrow_mut().step = InstallStep::Downloading;
        self.push_update_view();
        let updater = self.updater.clone();
        self.rt.spawn(async move {
            let result = match updater.download(&release).await {
                Ok(new_exe) => {
                    let target = exe.clone();
                    tokio::task::spawn_blocking(move || update::install_exe(&new_exe, &target))
                        .await
                        .unwrap_or(Err(UpdateError::NotWritable))
                }
                Err(err) => Err(err),
            };
            let _ = slint::invoke_from_event_loop(move || {
                with_ui(|ui| ui.finish_install(result, exe));
            });
        });
    }

    fn finish_install(&self, result: Result<(), UpdateError>, exe: PathBuf) {
        match result {
            Ok(()) => {
                info!("Update installed; restarting Hypatia");
                self.update.borrow_mut().step = InstallStep::Restarting;
                self.push_update_view();
                // Free the Mobile Remote port before the new process binds it.
                self.stop_remote();
                *self.restart_into.borrow_mut() = Some(exe);
                if let Err(err) = slint::quit_event_loop() {
                    tracing::warn!("Could not close the window to restart: {err}");
                }
            }
            Err(err) => {
                tracing::warn!("Update not installed: {err:?}");
                self.update.borrow_mut().step = InstallStep::Failed(err);
                self.push_update_view();
            }
        }
    }

    // --- Telemetry Lab GET tester --------------------------------------------------------

    /// Runs the GET off the UI thread; the window stays responsive even if Riot is slow.
    fn execute_get_call(&self, source: SourceId, path: String) {
        let app = self.app.clone();
        self.rt.spawn(async move {
            let result = app.call_endpoint(source, &path).await;
            let _ = slint::invoke_from_event_loop(move || {
                with_ui(|ui| ui.show_call_result(result));
            });
        });
    }

    fn show_call_result(&self, result: Result<(u16, Arc<Value>, u64), String>) {
        let lang = self.app.lang();
        self.update_telemetry(|st| {
            st.call_version += 1;
            st.invalidate_telemetry_cache();
            match result {
                Ok((status, body, event_id)) => {
                    let pretty =
                        serde_json::to_string_pretty(&*body).unwrap_or_else(|_| "{}".to_string());
                    st.call_status = format!(
                        "HTTP {status} · {} · {} #{event_id}",
                        format_bytes(pretty.len() as u64, lang),
                        lang.pick("Event", "Événement"),
                    );
                    st.call_response = pretty;
                    if event_id > 0 {
                        st.selected_event_id = Some(event_id);
                    }
                }
                Err(err) => {
                    st.call_status = lang
                        .pick("Request Failed", "Échec de la requête")
                        .to_string();
                    st.call_response = err;
                }
            }
        });
    }
}

#[derive(Clone, Copy, Default)]
struct ActiveExtras {
    demo: bool,
    observatory: bool,
    remote: bool,
}

fn profile_summary(s: &Settings, lang: Lang) -> String {
    match s.profile {
        PerformanceProfile::UltraLight => lang
            .pick(
                "Minimum RAM/CPU: initials only, no splash art, 4s live poll, 1s UI clock, sleep when minimized, Mobile Remote off.",
                "RAM/CPU minimaux : initiales seules, sans splash arts, jeu 4 s, horloge 1 s, veille réduit, App Mobile coupée.",
            )
            .to_string(),
        PerformanceProfile::Eco => lang
            .pick(
                "Low footprint: champion icons (64 max), no splash backdrops, 2s live poll, 1s UI clock, sleep when minimized.",
                "Empreinte réduite : icônes (64 max), sans splash arts, jeu 2 s, horloge 1 s et veille lorsque réduit.",
            )
            .to_string(),
        PerformanceProfile::Balanced => lang
            .pick(
                "Full visual experience: all icons and splash art (256 max), 1s live poll, and smooth 250ms UI clocks.",
                "Expérience complète : icônes et splash arts (256 max), jeu 1 s et horloges fluides à 250 ms.",
            )
            .to_string(),
        PerformanceProfile::Custom => {
            let live_part = if !s.live_game {
                lang.pick("live off", "jeu coupé").to_string()
            } else {
                let sec = s.live_poll_rate.interval().as_secs();
                if lang.is_fr() {
                    format!("jeu {sec} s")
                } else {
                    format!("live {sec}s")
                }
            };
            let art_part = if !s.game_icons {
                lang.pick("initials only", "initiales seules").to_string()
            } else if s.splash_backdrops {
                format!(
                    "{} ({})",
                    lang.pick("icons + splashes", "icônes + splashs"),
                    s.asset_cache_budget.max_entries()
                )
            } else {
                format!(
                    "{} ({})",
                    lang.pick("icons only", "icônes seules"),
                    s.asset_cache_budget.max_entries()
                )
            };
            let clock_part = match s.clock_rate {
                UiClockRate::Normal => "250 ms UI",
                UiClockRate::Relaxed => "1 s UI",
            };
            let sleep_part = if s.pause_when_minimized {
                lang.pick(" · sleep", " · veille")
            } else {
                ""
            };
            if lang.is_fr() {
                format!("Réglage personnalisé : {live_part} · {art_part} · {clock_part}{sleep_part}.")
            } else {
                format!("Custom setup: {live_part} · {art_part} · {clock_part}{sleep_part}.")
            }
        }
    }
}

fn perf_settings(s: &Settings, hw: HardwareSpec, lang: Lang) -> PerfSettings {
    PerfSettings {
        profile: SharedString::from(s.profile.as_code()),
        profile_label: SharedString::from(s.profile.label(lang)),
        profile_badge: SharedString::from(s.profile.badge(lang)),
        profile_summary: SharedString::from(profile_summary(s, lang)),
        hardware_summary: SharedString::from(hw.formatted(lang)),
        recommended_profile: SharedString::from(hw.recommended.as_code()),
        recommended_label: SharedString::from(hw.recommended.label(lang)),
        live_poll_idx: s.live_poll_rate.as_index(),
        discovery_rate_idx: s.discovery_rate.as_index(),
        clock_rate_idx: s.clock_rate.as_index(),
        cache_budget_idx: s.asset_cache_budget.as_index(),
        live_game: s.live_game,
        riot_client: s.riot_client,
        replay_api: s.replay_api,
        game_icons: s.game_icons,
        splash_backdrops: s.splash_backdrops,
        demo_prefetch: s.demo_prefetch,
        mobile_remote: s.mobile_remote,
        mobile_remote_pause_in_game: s.mobile_remote_pause_in_game,
        system_footprint: s.system_footprint,
        endpoint_catalog: s.endpoint_catalog,
        eco_mode: s.eco_mode,
        pause_when_minimized: s.pause_when_minimized,
        reduce_motion: s.reduce_motion,
        online_stats: s.online_stats,
        update_check: s.update_check == UpdateCheck::On,
        coach_llm: s.coach_llm,
    }
}

fn coach_test_status(state: &hypatia_core::coach_llm::ConnectionTestState) -> (i32, SharedString) {
    match state {
        hypatia_core::coach_llm::ConnectionTestState::Idle => (0, SharedString::default()),
        hypatia_core::coach_llm::ConnectionTestState::Testing => (1, SharedString::default()),
        hypatia_core::coach_llm::ConnectionTestState::Ok(msg) => {
            (2, SharedString::from(msg.as_str()))
        }
        hypatia_core::coach_llm::ConnectionTestState::Failed(msg) => {
            (3, SharedString::from(msg.as_str()))
        }
    }
}

fn build_perf_metrics(
    footprint: ProcessFootprint,
    cache_ram: (usize, u64),
    cache_disk: (usize, u64),
    s: &Settings,
    extras: ActiveExtras,
    lang: Lang,
) -> PerfMetrics {
    let ram_text = if footprint.memory_bytes > 0 {
        format_bytes(footprint.memory_bytes, lang)
    } else {
        "—".to_string()
    };
    let cpu_text = if footprint.cpu_ready {
        let raw = format!("{:.1}%", footprint.cpu_percent);
        if lang.is_fr() {
            raw.replace('.', ",")
        } else {
            raw
        }
    } else {
        lang.pick("Sampling...", "Mesure...").to_string()
    };
    let cache_ram_text = format!("{} · {}", cache_ram.0, format_bytes(cache_ram.1, lang));
    let cache_disk_text = format!("{} · {}", cache_disk.0, format_bytes(cache_disk.1, lang));

    let live_s = s.live_poll_rate.interval().as_secs();
    let disc_s = s.discovery_rate.active_interval().as_secs();
    let ui_ms = s.clock_rate.tick_interval().as_millis();
    let budget = s.asset_cache_budget.max_entries();
    let sleep_tag = if s.pause_when_minimized {
        lang.pick("sleep on", "veille active")
    } else {
        lang.pick("sleep off", "veille coupée")
    };
    let cadence_text = if s.live_game {
        if lang.is_fr() {
            format!(
                "Jeu {live_s}s · Scans {disc_s}s · UI {ui_ms}ms · Budget {budget} · {sleep_tag}"
            )
        } else {
            format!(
                "Live {live_s}s · Scan {disc_s}s · UI {ui_ms}ms · Budget {budget} · {sleep_tag}"
            )
        }
    } else if lang.is_fr() {
        format!("Jeu coupé · Scans {disc_s}s · UI {ui_ms}ms · Budget {budget} · {sleep_tag}")
    } else {
        format!("Live off · Scan {disc_s}s · UI {ui_ms}ms · Budget {budget} · {sleep_tag}")
    };

    let mut active = Vec::new();
    if extras.demo {
        active.push(lang.pick("Demo", "Démo"));
    }
    if extras.observatory {
        active.push(lang.pick("Observatory", "Observatoire"));
    }
    if extras.remote {
        active.push(lang.pick("Mobile Remote", "App Mobile"));
    }
    let status_note = if active.is_empty() {
        lang.pick(
            "Sampling stops automatically when you close this sheet.",
            "Les mesures s'arrêtent automatiquement à la fermeture de ce panneau.",
        )
        .to_string()
    } else if lang.is_fr() {
        format!(
            "Actifs : {} · Les mesures s'arrêtent à la fermeture.",
            active.join(", ")
        )
    } else {
        format!(
            "Active: {} · Sampling stops when you close this sheet.",
            active.join(", ")
        )
    };

    PerfMetrics {
        ram_text: SharedString::from(ram_text),
        cpu_text: SharedString::from(cpu_text),
        cache_ram_text: SharedString::from(cache_ram_text),
        cache_disk_text: SharedString::from(cache_disk_text),
        cadence_text: SharedString::from(cadence_text),
        status_note: SharedString::from(status_note),
    }
}

/// Windows "Show animations in Windows" off means the user asked for reduced motion.
#[cfg(windows)]
fn os_prefers_reduced_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };
    let mut enabled: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL into the pointed-to i32.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        )
    };
    ok != 0 && enabled == 0
}

#[cfg(not(windows))]
fn os_prefers_reduced_motion() -> bool {
    false
}

/// Switches every Slint window to `lang` through the bundled gettext catalogs.
fn apply_ui_language(main: &MainWindow, lang: Lang) {
    let code = if lang.is_fr() { "fr" } else { "en" };
    if let Err(err) = slint::select_bundled_translation(code) {
        tracing::warn!("Could not select UI language '{code}': {err}");
    }
    main.set_language(code.into());
}

fn initial_lang(cli: &Cli) -> Lang {
    match cli.lang.as_deref() {
        Some(raw) => Lang::from_code(raw).unwrap_or_else(|| {
            tracing::warn!("Unknown --lang value '{raw}', falling back to saved/OS locale");
            Settings::load_lang()
        }),
        None => Settings::load_lang(),
    }
}

/// Connects every Slint callback to its handler on [`Ui`].
fn wire_callbacks(ui: &Rc<Ui>) {
    let main = &ui.main;

    main.on_set_dark(|dark| with_ui(|ui| ui.set_dark(dark)));
    main.on_select_language(|code| {
        if let Some(lang) = Lang::from_code(&code) {
            with_ui(|ui| ui.set_lang(lang));
        }
    });
    main.on_toggle_telemetry(|| with_ui(|ui| ui.set_telemetry(!ui.main.get_telemetry_enabled())));
    main.on_toggle_demo(|| with_ui(|ui| ui.toggle_demo()));
    main.on_open_link(|url| {
        links::open_external(&url);
    });
    main.on_select_demo_phase(|phase| {
        with_ui(|ui| {
            ui.set_demo_phase(&phase);
            ui.telemetry.borrow_mut().invalidate_telemetry_cache();
            ui.refresh();
        })
    });
    main.on_toggle_setting(|key| with_ui(|ui| ui.toggle_setting(&key)));
    main.on_select_perf_profile(|code| with_ui(|ui| ui.select_perf_profile(&code)));
    main.on_apply_recommended_profile(|| with_ui(|ui| ui.apply_recommended_profile()));
    main.on_set_perf_choice(|key, idx| with_ui(|ui| ui.set_perf_choice(&key, idx)));
    main.on_clear_ram_cache(|| with_ui(|ui| ui.clear_ram_cache()));
    main.on_clear_disk_cache(|| with_ui(|ui| ui.clear_disk_cache()));
    main.on_performance_panel_changed(|open| {
        with_ui(|ui| ui.on_performance_panel_changed(open));
    });

    main.on_answer_update_check(|check| with_ui(|ui| ui.answer_update_check(check)));
    main.on_install_update(|| with_ui(|ui| ui.install_update()));
    main.on_request_end_debrief(|focus| {
        with_ui(|ui| {
            companion::request_end_debrief(&ui.app, hypatia_shared::CoachFocus::from_index(focus))
        })
    });
    main.on_ask_end_coach(|q| with_ui(|ui| companion::ask_end_coach(&ui.app, q.to_string())));
    main.on_select_coach_llm_preset(|idx| with_ui(|ui| ui.select_coach_llm_preset(idx)));
    main.on_save_coach_llm_config(|url, model, key| {
        with_ui(|ui| ui.save_coach_llm_config(&url, &model, &key))
    });
    main.on_test_coach_llm(|| with_ui(|ui| ui.test_coach_llm()));

    main.on_open_stats(|| {
        with_ui(|ui| {
            ui.main.set_stats_open(true);
            ui.update_stats(|_, _| {});
        })
    });
    stats_view::wire(&main.global::<StatsBridge>());

    main.on_toggle_remote_panel(|| with_ui(|ui| ui.toggle_remote_panel()));
    main.on_rotate_remote_token(|| with_ui(|ui| ui.rotate_remote_token()));
    main.on_disable_remote(|| with_ui(|ui| ui.stop_remote()));

    main.on_select_telemetry_event(|id| {
        with_ui(|ui| {
            ui.update_telemetry(|st| {
                if id > 0 {
                    st.selected_event_id = Some(id as u64);
                }
            })
        })
    });
    main.on_toggle_reveal_secrets(|| {
        with_ui(|ui| ui.update_telemetry(|st| st.secrets_revealed = !st.secrets_revealed))
    });
    main.on_clear_telemetry_events(|| {
        with_ui(|ui| {
            ui.app.clear_events();
            ui.update_telemetry(|st| {
                st.selected_event_id = None;
                st.clear_event_rows();
                st.cached_inspector = Default::default();
                st.invalidate_telemetry_cache();
            })
        })
    });
    main.on_set_endpoint_filter(|filter| {
        with_ui(|ui| {
            ui.update_telemetry(|st| {
                st.endpoint_filter = filter.to_string();
                st.last_render_sig = None;
            })
        })
    });
    main.on_execute_get_call(|src_idx, path| {
        let source = match src_idx {
            1 => SourceId::RiotClient,
            2 => SourceId::LiveGame,
            _ => SourceId::LeagueClient,
        };
        with_ui(|ui| ui.execute_get_call(source, path.to_string()));
    });
}

/// Wakes the UI whenever core state changes, at most once per configured throttle interval.
/// While nothing changes this task sleeps on the signal, so an idle Hypatia never repaints or re-projects.
fn spawn_refresh_bridge(rt: &tokio::runtime::Handle, app: &AppState) {
    let mut changes = app.signal.subscribe();
    let flags = app.flags.clone();
    rt.spawn(async move {
        while changes.changed().await.is_ok() {
            if slint::invoke_from_event_loop(|| with_ui(|ui| ui.refresh())).is_err() {
                break;
            }
            tokio::time::sleep(flags.ui_clock_interval()).await;
        }
    });
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let cli = Cli::parse();

    // All background work is I/O-bound and tiny: two workers are plenty.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("hypatia-io")
        .enable_all()
        .build()
        .expect("Failed to start Tokio runtime");
    let _guard = rt.enter();

    let lang = initial_lang(&cli);
    let hw = HardwareSpec::detect();
    let (settings, first_launch) = Settings::load_or_init(hw);
    if first_launch {
        info!(
            "First launch hardware profile: {:?} ({}) -> {:?}",
            hw.tier,
            hw.formatted(Lang::En),
            settings.profile
        );
    }
    let app = AppState::start_with(cli.demo, cli.demo_rate, lang, &settings);
    spawn_update_cleanup(rt.handle());
    let updater = Arc::new(Updater::new(app.flags.clone()));

    let main = MainWindow::new().expect("Failed to create MainWindow");
    main.set_demo(cli.demo);
    // Demo Mode is for developers: debug builds, or a release launched with `--demo`.
    main.set_dev_tools(cfg!(debug_assertions) || cli.demo);
    main.set_perf(perf_settings(&settings, hw, lang));
    main.set_perf_metrics(PerfMetrics::default());
    // Asked once; Demo runs (screenshots, development) leave the answer for a real launch.
    main.set_update_consent_open(settings.update_check == UpdateCheck::Ask && !cli.demo);

    let ui = Rc::new(Ui {
        main,
        app,
        rt: rt.handle().clone(),
        cli,
        hw,
        settings: RefCell::new(settings),
        companion: RefCell::default(),
        telemetry: RefCell::default(),
        stats: RefCell::default(),
        telemetry_clock: Timer::default(),
        phase_clock: Timer::default(),
        phase_clock_period: Cell::new(None),
        perf_clock: Timer::default(),
        perf_monitor: RefCell::new(None),
        last_footprint: Cell::new(ProcessFootprint::default()),
        cached_disk_stats: RefCell::new(None),
        deferred_refresh: Cell::new(false),
        restore_timer: Timer::default(),
        remote: RefCell::new(None),
        remote_starting: Cell::new(false),
        remote_paused: RefCell::new(None),
        remote_in_game_override: Cell::new(false),
        updater,
        update: RefCell::default(),
        update_clock: Timer::default(),
        update_retry: Timer::default(),
        restart_into: RefCell::new(None),
    });
    UI.with(|slot| *slot.borrow_mut() = Some(ui.clone()));
    wire_callbacks(&ui);

    let dark = !(ui.cli.light || ui.settings.borrow().light_theme);
    ui.set_dark(dark);
    ui.apply_motion();
    apply_ui_language(&ui.main, lang);
    if let Some(phase) = ui.cli.pinned_phase() {
        ui.set_demo_phase(phase);
    }
    if let Some(tab) = ui.cli.stats_tab() {
        ui.main.set_stats_open(true);
        ui.stats.borrow_mut().select_tab(tab);
    }
    if ui.cli.telemetry {
        ui.set_telemetry(true);
    }
    if !ui.cli.no_remote && (settings.mobile_remote || ui.cli.remote) {
        if ui.cli.remote && ui.app.is_in_game() {
            ui.remote_in_game_override.set(true);
        }
        ui.start_remote(ui.cli.remote);
    }

    ui.refresh();
    ui.main.show().expect("Failed to show MainWindow");
    if !win_dpi::install(ui.main.window()) {
        let _ = slint::invoke_from_event_loop(|| {
            with_ui(|ui| {
                if !win_dpi::install(ui.main.window()) {
                    tracing::warn!("Could not install Windows DPI subclass on main window");
                }
            });
        });
    }
    spawn_refresh_bridge(rt.handle(), &ui.app);
    ui.sync_update_checks();
    ui.push_coach_llm_view();

    ui.main.run().expect("Slint event loop exited with error");

    let restart = ui.restart_into.borrow_mut().take();
    drop(ui);
    UI.with(|slot| slot.borrow_mut().take());
    rt.shutdown_background();
    if let Some(exe) = restart {
        if let Err(err) = std::process::Command::new(&exe)
            .args(std::env::args_os().skip(1))
            .spawn()
        {
            tracing::error!("Could not restart Hypatia after the update: {err}");
        }
    }
}

/// Removes the executable an update replaced. The previous process may still be closing, so
/// this retries for a few seconds; local files only, whatever the Update Check setting.
fn spawn_update_cleanup(rt: &tokio::runtime::Handle) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let previous = update::previous_exe_path(&exe);
    if !previous.exists() {
        return;
    }
    rt.spawn(async move {
        for _ in 0..20 {
            update::cleanup_previous(&exe);
            if !previous.exists() {
                info!("Removed the executable replaced by the last update");
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_settings_and_metrics_localize_for_both_languages() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let hw = HardwareSpec::classify(8 * GIB, 8);
        let mut s = Settings::for_profile(PerformanceProfile::Eco);

        let en = perf_settings(&s, hw, Lang::En);
        assert_eq!(en.profile, "eco");
        assert_eq!(en.profile_badge, "ECO");
        assert_eq!(en.recommended_profile, "eco");
        assert_eq!(en.live_poll_idx, 1);
        assert_eq!(en.discovery_rate_idx, 1);
        assert_eq!(en.clock_rate_idx, 1);
        assert_eq!(en.cache_budget_idx, 1);
        assert!(en.pause_when_minimized);

        let fr = perf_settings(&s, hw, Lang::Fr);
        assert_eq!(fr.profile_label, "Éco");
        assert_eq!(fr.profile_badge, "ÉCO");
        assert!(fr.hardware_summary.contains("Go RAM"));

        // Switching one option yields Custom in both languages.
        assert!(s.toggle("splash_backdrops"));
        let custom_en = perf_settings(&s, hw, Lang::En);
        assert_eq!(custom_en.profile, "custom");
        assert_eq!(custom_en.profile_badge, "CUSTOM");
        assert!(custom_en.profile_summary.contains("icons + splashes"));

        let custom_fr = perf_settings(&s, hw, Lang::Fr);
        assert_eq!(custom_fr.profile_badge, "PERSO");
        assert!(custom_fr.profile_summary.contains("icônes + splashs"));

        let metrics_en = build_perf_metrics(
            ProcessFootprint {
                memory_bytes: 64 * 1024 * 1024,
                cpu_percent: 1.4,
                cpu_ready: true,
            },
            (12, 4 * 1024 * 1024),
            (30, 8 * 1024 * 1024),
            &s,
            ActiveExtras {
                demo: true,
                observatory: false,
                remote: true,
            },
            Lang::En,
        );
        assert_eq!(metrics_en.cpu_text, "1.4%");
        assert!(metrics_en.status_note.contains("Demo"));
        assert!(metrics_en.status_note.contains("Mobile Remote"));

        let metrics_fr = build_perf_metrics(
            ProcessFootprint::default(),
            (0, 0),
            (0, 0),
            &s,
            ActiveExtras::default(),
            Lang::Fr,
        );
        assert_eq!(metrics_fr.ram_text, "—");
        assert_eq!(metrics_fr.cpu_text, "Mesure...");
    }
}
