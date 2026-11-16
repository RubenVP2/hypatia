#![cfg_attr(all(not(test), not(debug_assertions)), windows_subsystem = "windows")]

mod cli;
mod companion;
mod icons;
mod labels;
mod telemetry;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use hypatia_core::remote::{start_remote_server, QrCodeVisual, RemoteServerHandle};
use hypatia_core::settings::Settings;
use hypatia_core::AppState;
use hypatia_shared::{Lang, SourceId};
use serde_json::Value;
use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode};
use tracing::info;

use crate::cli::Cli;
use crate::companion::Companion;
use crate::labels::format_bytes;
use crate::telemetry::{apply_telemetry_to_window, TelemetryUiState};

slint::include_modules!();

/// How often a running phase clock (queue, ready check, draft, game) repaints its seconds.
/// Four ticks a second keeps the displayed second within 250 ms of the truth; a tick that
/// lands on the same second sets an unchanged property and repaints nothing.
const CLOCK_TICK: Duration = Duration::from_millis(250);

/// Minimum delay between two UI refreshes triggered by core changes.
const REFRESH_THROTTLE: Duration = Duration::from_millis(250);
const REFRESH_THROTTLE_ECO: Duration = Duration::from_millis(1000);

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

/// Windows, core state and view models owned by the event-loop thread.
struct Ui {
    main: MainWindow,
    mini: MiniWidget,
    app: AppState,
    rt: tokio::runtime::Handle,
    cli: Cli,
    settings: RefCell<Settings>,
    companion: RefCell<Companion>,
    telemetry: RefCell<TelemetryUiState>,
    /// Refreshes the Telemetry Lab's event-rate readout; runs only while the Lab is open.
    telemetry_clock: Timer,
    /// Extrapolates the active phase clock between two LCU events; runs only while one exists.
    phase_clock: Timer,
    widget_open: Cell<bool>,
    remote: RefCell<Option<RemoteServerHandle>>,
    remote_starting: Cell<bool>,
}

impl Ui {
    /// Brings every visible view up to date. Cheap when nothing changed: the companion compares
    /// one `Arc` pointer and the Telemetry Lab one hash.
    fn refresh(&self) {
        self.companion
            .borrow_mut()
            .refresh(&self.main, &self.mini, &self.app, self.widget_open.get());
        self.refresh_telemetry();
        let clients = self
            .remote
            .borrow()
            .as_ref()
            .map_or(0, |h| h.connected_clients() as i32);
        self.main.set_remote_clients(clients);
        self.sync_phase_clock();
    }

    /// Starts, stops and repaints the phase clock to match what the companion shows.
    fn sync_phase_clock(&self) {
        let clock = self.companion.borrow().clock();
        match clock {
            Some(clock) => {
                self.main.set_clock_total(clock.total_sec);
                self.main.set_clock_down(clock.counts_down());
                self.mini.set_clock_total(clock.total_sec);
                self.mini.set_clock_down(clock.counts_down());
                self.tick_phase_clock();
                if !self.phase_clock.running() {
                    self.phase_clock.start(TimerMode::Repeated, CLOCK_TICK, || {
                        with_ui(|ui| ui.tick_phase_clock());
                    });
                }
            }
            None => {
                self.phase_clock.stop();
                self.main.set_clock_seconds(-1);
                self.mini.set_clock_seconds(-1);
            }
        }
    }

    fn tick_phase_clock(&self) {
        if let Some(clock) = self.companion.borrow().clock() {
            let secs = clock.seconds_now(Instant::now());
            self.main.set_clock_seconds(secs);
            self.mini.set_clock_seconds(secs);
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
    }

    fn set_lang(&self, lang: Lang) {
        self.app.set_lang(lang);
        apply_ui_language(&self.main, lang);
        self.telemetry.borrow_mut().invalidate_telemetry_cache();
        self.refresh();
    }

    fn set_dark(&self, dark: bool) {
        Theme::get(&self.main).set_dark(dark);
        Theme::get(&self.mini).set_dark(dark);
        let mut settings = self.settings.borrow_mut();
        if settings.light_theme == dark {
            settings.light_theme = !dark;
            settings.save();
        }
    }

    /// Animations run unless the OS, the settings sheet or `--reduce-motion` turns them off.
    fn apply_motion(&self) {
        let motion = !(self.cli.reduce_motion
            || self.settings.borrow().reduce_motion
            || os_prefers_reduced_motion());
        Theme::get(&self.main).set_motion(motion);
        Theme::get(&self.mini).set_motion(motion);
    }

    fn set_demo_phase(&self, phase: &str) {
        self.app.set_demo_phase(phase);
    }

    fn toggle_demo(&self) {
        let next = !self.main.get_demo();
        self.app.set_demo_active(next, self.cli.demo_rate);
        self.main.set_demo(next);
        self.telemetry.borrow_mut().invalidate_telemetry_cache();
        self.refresh();
    }

    fn toggle_widget(&self) {
        let open = !self.widget_open.get();
        self.widget_open.set(open);
        let _ = if open { self.mini.show() } else { self.mini.hide() };
        self.main.set_widget_open(open);
        self.refresh();
    }

    fn toggle_setting(&self, key: &str) {
        let settings = {
            let mut settings = self.settings.borrow_mut();
            if !settings.toggle(key) {
                return;
            }
            *settings
        };
        settings.save();
        self.app.apply_settings(&settings);
        self.main.set_perf(perf_settings(&settings));
        if key == "reduce_motion" {
            self.apply_motion();
        }
        if key == "mobile_remote" {
            if settings.mobile_remote {
                self.start_remote(false);
            } else {
                self.stop_remote();
            }
        }
        self.refresh();
    }

    // --- Mobile Remote -------------------------------------------------------------------

    fn show_pairing(&self, qr: &QrCodeVisual) {
        let pixels =
            SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&qr.rgba, qr.width, qr.height);
        self.main.set_remote_qr(Image::from_rgba8(pixels));
        self.main.set_remote_url(SharedString::from(qr.url.as_str()));
    }

    /// Starts the LAN server in the background so the window never waits on the network.
    fn start_remote(&self, open_drawer: bool) {
        if self.remote.borrow().is_some() || self.remote_starting.replace(true) {
            return;
        }
        let app = self.app.clone();
        let port = self.cli.remote_port;
        self.rt.spawn(async move {
            let started = start_remote_server(app, port).await;
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
                            if open_drawer {
                                ui.main.set_settings_open(false);
                            }
                            ui.main.set_remote_panel_open(open_drawer);
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
        self.main.set_remote_enabled(false);
        self.main.set_remote_panel_open(false);
        self.main.set_remote_url(SharedString::default());
        self.main.set_remote_clients(0);
    }

    fn toggle_remote_panel(&self) {
        if self.remote.borrow().is_some() {
            self.main.set_settings_open(false);
            self.main
                .set_remote_panel_open(!self.main.get_remote_panel_open());
        } else {
            self.start_remote(true);
        }
    }

    fn rotate_remote_token(&self) {
        let rotated = self.remote.borrow().as_ref().map(|h| h.rotate_token());
        if let Some(Ok(qr)) = rotated {
            self.show_pairing(&qr);
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

fn perf_settings(s: &Settings) -> PerfSettings {
    PerfSettings {
        live_game: s.live_game,
        riot_client: s.riot_client,
        replay_api: s.replay_api,
        game_icons: s.game_icons,
        mobile_remote: s.mobile_remote,
        system_footprint: s.system_footprint,
        endpoint_catalog: s.endpoint_catalog,
        eco_mode: s.eco_mode,
        reduce_motion: s.reduce_motion,
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
    main.on_select_demo_phase(|phase| {
        with_ui(|ui| {
            ui.set_demo_phase(&phase);
            ui.telemetry.borrow_mut().invalidate_telemetry_cache();
            ui.refresh();
        })
    });
    main.on_toggle_widget(|| with_ui(|ui| ui.toggle_widget()));
    main.on_toggle_setting(|key| with_ui(|ui| ui.toggle_setting(&key)));

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

/// Wakes the UI whenever core state changes, at most once per throttle period. While nothing
/// changes this task sleeps on the signal, so an idle Hypatia never repaints or re-projects.
fn spawn_refresh_bridge(rt: &tokio::runtime::Handle, app: &AppState) {
    let mut changes = app.signal.subscribe();
    let flags = app.flags.clone();
    rt.spawn(async move {
        while changes.changed().await.is_ok() {
            if slint::invoke_from_event_loop(|| with_ui(|ui| ui.refresh())).is_err() {
                break;
            }
            tokio::time::sleep(if flags.eco_mode() {
                REFRESH_THROTTLE_ECO
            } else {
                REFRESH_THROTTLE
            })
            .await;
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
    let settings = Settings::load();
    let app = AppState::start_with(cli.demo, cli.demo_rate, lang, &settings);

    let main = MainWindow::new().expect("Failed to create MainWindow");
    let mini = MiniWidget::new().expect("Failed to create MiniWidget");
    main.set_demo(cli.demo);
    main.set_perf(perf_settings(&settings));

    let ui = Rc::new(Ui {
        main,
        mini,
        app,
        rt: rt.handle().clone(),
        cli,
        settings: RefCell::new(settings),
        companion: RefCell::default(),
        telemetry: RefCell::default(),
        telemetry_clock: Timer::default(),
        phase_clock: Timer::default(),
        widget_open: Cell::new(false),
        remote: RefCell::new(None),
        remote_starting: Cell::new(false),
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
    if ui.cli.telemetry {
        ui.set_telemetry(true);
    }
    if ui.cli.widget {
        ui.toggle_widget();
    }
    if !ui.cli.no_remote && (settings.mobile_remote || ui.cli.remote) {
        ui.start_remote(ui.cli.remote);
    }

    ui.refresh();
    ui.main.show().expect("Failed to show MainWindow");
    spawn_refresh_bridge(rt.handle(), &ui.app);

    ui.main.run().expect("Slint event loop exited with error");

    UI.with(|slot| slot.borrow_mut().take());
    rt.shutdown_background();
}
