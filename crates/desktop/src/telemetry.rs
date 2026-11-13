//! Telemetry Lab view model: event stream, inspector, endpoint catalog and system footprint.

use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use hypatia_core::{compute_json_diff_localized, AppState};
use hypatia_shared::{EventKind, Lang, SourceId, SourceStatus};
use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};

use crate::labels::{format_bytes, format_ts_hms, source_short_label};
use crate::{
    MainWindow, TelemetryEndpointItem, TelemetryEventItem, TelemetryProcessItem,
    TelemetrySocketItem, TelemetrySourceItem, TelemetryView,
};

const MAX_DISPLAYED_ENDPOINTS: usize = 150;

/// Long-lived list models handed to Slint once and then updated in place, so a refresh only
/// touches the rows that changed instead of re-creating every row component.
#[derive(Default)]
struct TelemetryModels {
    sources: Rc<VecModel<TelemetrySourceItem>>,
    events: Rc<VecModel<TelemetryEventItem>>,
    endpoints: Rc<VecModel<TelemetryEndpointItem>>,
    processes: Rc<VecModel<TelemetryProcessItem>>,
    sockets: Rc<VecModel<TelemetrySocketItem>>,
    connections: Rc<VecModel<TelemetrySocketItem>>,
}

/// Makes `model` equal to `items`, writing only rows that differ.
fn sync_model<T: Clone + PartialEq + 'static>(model: &VecModel<T>, items: Vec<T>) {
    let kept = model.row_count().min(items.len());
    for row in (items.len()..model.row_count()).rev() {
        model.remove(row);
    }
    for (row, item) in items.into_iter().enumerate() {
        if row >= kept {
            model.push(item);
        } else if model.row_data(row).as_ref() != Some(&item) {
            model.set_row_data(row, item);
        }
    }
}

#[derive(Default)]
pub struct TelemetryUiState {
    pub selected_event_id: Option<u64>,
    pub secrets_revealed: bool,
    pub call_status: String,
    pub call_response: String,
    pub call_version: u64,
    pub endpoint_filter: String,
    models: TelemetryModels,
    last_events_sig: Option<(u64, usize, i32, i32, Lang)>,
    last_inspector_sig: Option<(u64, i32, bool, Lang)>,
    pub cached_inspector: (String, String, bool, String),
    cached_endpoints_total: i32,
    cached_endpoints_summary: String,
    last_catalog_sig: Option<(u64, String, u64, Lang)>,
    pub last_render_sig: Option<u64>,
}

impl TelemetryUiState {
    /// Empties the displayed event list (after the history was cleared).
    pub fn clear_event_rows(&mut self) {
        sync_model(&self.models.events, Vec::new());
    }

    pub fn invalidate_telemetry_cache(&mut self) {
        self.last_events_sig = None;
        self.last_inspector_sig = None;
        self.last_catalog_sig = None;
        self.last_render_sig = None;
    }
}

/// The payload as the inspector shows it: redacted as stored, or with secrets revealed.
fn inspected_payload<'a>(
    payload: &'a serde_json::Value,
    app_state: &AppState,
    reveal: bool,
) -> Cow<'a, serde_json::Value> {
    if reveal {
        Cow::Owned(app_state.reveal_secrets_in_value(payload))
    } else {
        Cow::Borrowed(payload)
    }
}

pub fn apply_telemetry_to_window(
    main_win: &MainWindow,
    app_state: &AppState,
    ui_state: &mut TelemetryUiState,
) {
    if !main_win.get_telemetry_enabled() {
        return;
    }

    let lang = app_state.lang();
    let src_filter = main_win.get_telemetry_source_filter();
    let kind_filter = main_win.get_telemetry_kind_filter();
    let paused = main_win.get_telemetry_paused();
    let inspector_mode = main_win.get_inspector_mode();

    // Cheap signature first: an unchanged Lab costs a few atomic reads and one hash.
    let (rate, total_len, latest_id, store_revision, stats) = {
        let store = app_state.store.read().unwrap();
        (
            store.current_rate(),
            store.len(),
            store.latest_event_id(),
            store.revision(),
            [
                SourceId::RiotClient,
                SourceId::LeagueClient,
                SourceId::LiveGame,
                SourceId::System,
            ]
            .map(|id| store.source_stats(id)),
        )
    };
    let catalog_revision = app_state.catalog.read().unwrap().revision();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    lang.as_code().hash(&mut hasher);
    ((rate * 10.0).round() as u64).hash(&mut hasher);
    store_revision.hash(&mut hasher);
    stats.hash(&mut hasher);
    app_state.discovery.revision().hash(&mut hasher);
    src_filter.hash(&mut hasher);
    kind_filter.hash(&mut hasher);
    paused.hash(&mut hasher);
    ui_state.selected_event_id.hash(&mut hasher);
    inspector_mode.hash(&mut hasher);
    ui_state.secrets_revealed.hash(&mut hasher);
    catalog_revision.hash(&mut hasher);
    ui_state.endpoint_filter.hash(&mut hasher);
    ui_state.call_version.hash(&mut hasher);
    let render_sig = hasher.finish();
    if ui_state.last_render_sig == Some(render_sig) {
        return;
    }
    ui_state.last_render_sig = Some(render_sig);

    let snap = app_state.get_snapshot();
    let raw_phase = snap
        .phase
        .as_ref()
        .map(|p| p.phase.as_str())
        .unwrap_or("None");
    let phase_label = match raw_phase {
        "Lobby" => lang.pick("Lobby", "Salon"),
        "Matchmaking" => lang.pick("Matchmaking", "File d'attente"),
        "ReadyCheck" => lang.pick("Ready Check", "Appel de partie"),
        "ChampSelect" => lang.pick("Champ Select", "Sélection"),
        "InProgress" => lang.pick("In Game", "En partie"),
        "EndOfGame" | "PreEndOfGame" => lang.pick("End of Game", "Fin de partie"),
        "WaitingForStats" => lang.pick("Waiting for Stats", "Attente des stats"),
        "Reconnect" => lang.pick("Reconnect", "Reconnexion"),
        "None" => lang.pick("None", "Aucune"),
        other => other,
    };

    let sources: Vec<TelemetrySourceItem> = snap
        .sources
        .iter()
        .map(|s| {
            let name = match s.id {
                SourceId::RiotClient => lang.pick("Riot Client", "Client Riot"),
                SourceId::LeagueClient => lang.pick("League Client (LCU)", "Client League (LCU)"),
                SourceId::LiveGame => lang.pick("Live Game :2999", "Partie en direct :2999"),
                SourceId::System => lang.pick("System Monitor", "Moniteur système"),
            };
            let live = matches!(s.status, SourceStatus::Live | SourceStatus::Discovered);
            let status_str = match s.status {
                SourceStatus::Live => lang.pick("LIVE", "DIRECT"),
                SourceStatus::Discovered => lang.pick("READY", "PRÊT"),
                SourceStatus::Connecting => lang.pick("CONNECTING", "CONNEXION"),
                SourceStatus::Offline => lang.pick("OFFLINE", "HORS LIGNE"),
                SourceStatus::Degraded { .. } => lang.pick("DEGRADED", "DÉGRADÉ"),
            };
            let port_str = match s.port {
                Some(p) => format!(":{}", p),
                None => "local".to_string(),
            };
            let pid_str = match s.pid {
                Some(pid) => format!("PID {}", pid),
                None => String::new(),
            };
            let stats_str = if s.id == SourceId::System {
                if lang.is_fr() {
                    format!(
                        "{} proc. · {} ports",
                        snap.system.processes.len(),
                        snap.system.sockets.len()
                    )
                } else {
                    format!(
                        "{} procs · {} sockets",
                        snap.system.processes.len(),
                        snap.system.sockets.len()
                    )
                }
            } else {
                format!(
                    "{} {} · {}",
                    s.events_total,
                    lang.pick("evts", "évts"),
                    format_bytes(s.bytes_total, lang)
                )
            };
            TelemetrySourceItem {
                id: SharedString::from(s.id.to_string()),
                name: SharedString::from(name),
                status: SharedString::from(status_str),
                live,
                port: SharedString::from(port_str),
                pid: SharedString::from(pid_str),
                stats: SharedString::from(stats_str),
            }
        })
        .collect();

    let events_sig = (latest_id, total_len, src_filter, kind_filter, lang);
    if (!paused && ui_state.last_events_sig != Some(events_sig)) || ui_state.models.events.row_count() == 0
    {
        let filtered: Vec<TelemetryEventItem> = snap
            .recent_events
            .iter()
            .rev()
            .filter(|ev| match src_filter {
                1 => ev.source == SourceId::LeagueClient,
                2 => ev.source == SourceId::RiotClient,
                3 => ev.source == SourceId::LiveGame,
                _ => true,
            })
            .filter(|ev| match kind_filter {
                1 => ev.kind == EventKind::Update,
                2 => ev.kind == EventKind::Create,
                3 => ev.kind == EventKind::Delete,
                4 => matches!(ev.kind, EventKind::Poll | EventKind::Call),
                _ => true,
            })
            .take(120)
            .map(|ev| {
                let (short_src, src_idx) = source_short_label(ev.source, lang);
                let kind_str = match ev.kind {
                    EventKind::Create => lang.pick("CREATE", "CRÉAT."),
                    EventKind::Update => lang.pick("UPD", "MÀJ"),
                    EventKind::Delete => lang.pick("DEL", "SUPPR."),
                    EventKind::Poll => lang.pick("POLL", "SONDE"),
                    EventKind::Call => lang.pick("CALL", "APPEL"),
                };
                TelemetryEventItem {
                    id: ev.id as i32,
                    time: SharedString::from(format_ts_hms(ev.ts)),
                    source: SharedString::from(short_src),
                    source_idx: src_idx,
                    kind: SharedString::from(kind_str),
                    uri: SharedString::from(ev.uri.as_str()),
                    size: SharedString::from(format_bytes(ev.size as u64, lang)),
                    preview: SharedString::from(ev.preview.as_str()),
                    has_secrets: ev.has_secrets,
                }
            })
            .collect();

        if ui_state.selected_event_id.is_none() {
            if let Some(first) = filtered.first() {
                ui_state.selected_event_id = Some(first.id as u64);
            }
        }
        sync_model(&ui_state.models.events, filtered);
        ui_state.last_events_sig = Some(events_sig);
    }

    let desired_ins_sig = ui_state
        .selected_event_id
        .map(|id| (id, inspector_mode, ui_state.secrets_revealed, lang));
    if ui_state.last_inspector_sig != desired_ins_sig
        || (ui_state.selected_event_id.is_some() && ui_state.cached_inspector.0.is_empty())
    {
        ui_state.cached_inspector = if let Some(sel_id) = ui_state.selected_event_id {
            let stored_opt = app_state.store.read().unwrap().get_event(sel_id);
            if let Some(stored) = stored_opt {
                let (short_src, _) = source_short_label(stored.meta.source, lang);
                let title = format!("#{} · {} · {}", stored.meta.id, short_src, stored.meta.uri);
                let kind_lbl = match stored.meta.kind {
                    EventKind::Create => lang.pick("Create", "Création"),
                    EventKind::Update => lang.pick("Update", "Mise à jour"),
                    EventKind::Delete => lang.pick("Delete", "Suppression"),
                    EventKind::Poll => lang.pick("Poll", "Sondage"),
                    EventKind::Call => lang.pick("Call", "Appel"),
                };
                let meta = if lang.is_fr() {
                    format!(
                        "Type : {} · {} · Heure : {}",
                        kind_lbl,
                        format_bytes(stored.meta.size as u64, lang),
                        format_ts_hms(stored.meta.ts)
                    )
                } else {
                    format!(
                        "Kind: {} · {} · Time: {}",
                        kind_lbl,
                        format_bytes(stored.meta.size as u64, lang),
                        format_ts_hms(stored.meta.ts)
                    )
                };
                let reveal = ui_state.secrets_revealed;
                let curr_val = inspected_payload(&stored.payload, app_state, reveal);
                let prev_val = stored
                    .previous_payload
                    .as_deref()
                    .map(|p| inspected_payload(p, app_state, reveal));
                let body = match inspector_mode {
                    1 => compute_json_diff_localized(prev_val.as_deref(), &curr_val, lang),
                    2 => match prev_val {
                        Some(ref pv) => {
                            serde_json::to_string_pretty(pv).unwrap_or_else(|_| "{}".to_string())
                        }
                        None => lang
                            .pick(
                                "(No previous state recorded for this URI)",
                                "(Aucun état précédent enregistré pour cette URI)",
                            )
                            .to_string(),
                    },
                    _ => serde_json::to_string_pretty(&curr_val)
                        .unwrap_or_else(|_| "{}".to_string()),
                };
                (title, meta, stored.meta.has_secrets, body)
            } else {
                (
                    if lang.is_fr() {
                        format!("Événement #{} éjecté du tampon", sel_id)
                    } else {
                        format!("Event #{} evicted from buffer", sel_id)
                    },
                    String::new(),
                    false,
                    String::new(),
                )
            }
        } else {
            (String::new(), String::new(), false, String::new())
        };
        ui_state.last_inspector_sig = desired_ins_sig;
    }
    let (ref inspector_title, ref inspector_meta, inspector_has_secrets, ref inspector_payload) =
        ui_state.cached_inspector;

    let catalog_sig = (
        catalog_revision,
        ui_state.endpoint_filter.clone(),
        ui_state.call_version,
        lang,
    );
    if ui_state.last_catalog_sig.as_ref() != Some(&catalog_sig) {
        let filter_lc = ui_state.endpoint_filter.trim().to_ascii_lowercase();
        let guard = app_state.catalog.read().unwrap();
        let all = guard.endpoints();
        let mut matched: Vec<_> = all
            .iter()
            .filter(|ep| {
                if filter_lc.is_empty() {
                    return true;
                }
                let (short_src, _) = source_short_label(ep.source, lang);
                ep.path_template.to_ascii_lowercase().contains(&filter_lc)
                    || ep.namespace.to_ascii_lowercase().contains(&filter_lc)
                    || short_src.to_ascii_lowercase().contains(&filter_lc)
                    || ep
                        .description
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&filter_lc)
            })
            .collect();
        // Prioritize endpoints with observed traffic, then alphabetical path
        matched.sort_by(|a, b| {
            (b.event_count > 0)
                .cmp(&(a.event_count > 0))
                .then_with(|| b.event_count.cmp(&a.event_count))
                .then_with(|| a.path_template.cmp(&b.path_template))
        });

        let matching_total = matched.len();
        let items: Vec<TelemetryEndpointItem> = matched
            .into_iter()
            .take(MAX_DISPLAYED_ENDPOINTS)
            .map(|ep| {
                let (short_src, _) = source_short_label(ep.source, lang);
                let call_src_idx = match ep.source {
                    SourceId::LeagueClient => 0,
                    SourceId::RiotClient => 1,
                    SourceId::LiveGame => 2,
                    SourceId::System => 0,
                };
                let origin_str = format!("{:?}", ep.origin);
                let stats_str = format!(
                    "{} {} · {}",
                    ep.event_count,
                    lang.pick("calls", "appels"),
                    format_bytes(ep.bytes_total, lang)
                );
                TelemetryEndpointItem {
                    source: SharedString::from(short_src),
                    source_idx: call_src_idx,
                    method: SharedString::from(ep.method.as_str()),
                    path: SharedString::from(ep.path_template.as_str()),
                    namespace: SharedString::from(ep.namespace.as_str()),
                    origin: SharedString::from(origin_str),
                    stats: SharedString::from(stats_str),
                    description: SharedString::from(ep.description.as_deref().unwrap_or("")),
                }
            })
            .collect();

        let shown = items.len();
        sync_model(&ui_state.models.endpoints, items);
        ui_state.cached_endpoints_summary = if matching_total > shown {
            if lang.is_fr() {
                format!(
                    "Affichage de {} sur {} ({} au total)",
                    shown,
                    matching_total,
                    all.len()
                )
            } else {
                format!(
                    "Showing {} of {} ({} total)",
                    shown,
                    matching_total,
                    all.len()
                )
            }
        } else if lang.is_fr() {
            format!("{} sur {} endpoints", matching_total, all.len())
        } else {
            format!("{} of {} endpoints", matching_total, all.len())
        };
        ui_state.cached_endpoints_total = all.len() as i32;
        ui_state.last_catalog_sig = Some(catalog_sig);
    }

    let processes: Vec<TelemetryProcessItem> = snap
        .system
        .processes
        .iter()
        .map(|p| TelemetryProcessItem {
            pid: SharedString::from(p.pid.to_string()),
            name: SharedString::from(p.name.as_str()),
            source: SharedString::from(
                p.source
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "riot".to_string()),
            ),
            cpu: SharedString::from(format!("{:.1}% CPU", p.cpu)),
            mem: SharedString::from(format_bytes(p.mem_bytes, lang)),
            exe: SharedString::from(p.exe.as_deref().unwrap_or("")),
        })
        .collect();

    let sockets: Vec<TelemetrySocketItem> = snap
        .system
        .sockets
        .iter()
        .map(|s| TelemetrySocketItem {
            proto: SharedString::from(s.proto.as_str()),
            address: SharedString::from(format!("{}:{}", s.local, s.port)),
            pid: SharedString::from(s.pid.to_string()),
            state: SharedString::from(lang.pick("LISTEN", "ÉCOUTE")),
        })
        .collect();

    let connections: Vec<TelemetrySocketItem> = snap
        .system
        .connections
        .iter()
        .map(|c| TelemetrySocketItem {
            proto: SharedString::from("TCP"),
            address: SharedString::from(format!("{} -> {}", c.local, c.remote)),
            pid: SharedString::from(c.pid.to_string()),
            state: SharedString::from(c.state.as_str()),
        })
        .collect();

    let models = &ui_state.models;
    sync_model(&models.sources, sources);
    sync_model(&models.processes, processes);
    sync_model(&models.sockets, sockets);
    sync_model(&models.connections, connections);

    main_win.set_telemetry(TelemetryView {
        rate: SharedString::from(if lang.is_fr() {
            format!("{:.1} évt/s", rate)
        } else {
            format!("{:.1} evt/s", rate)
        }),
        total_events: SharedString::from(if lang.is_fr() {
            format!("{} en mémoire", total_len)
        } else {
            format!("{} stored", total_len)
        }),
        phase: SharedString::from(phase_label),
        sources: ModelRc::from(models.sources.clone()),
        events: ModelRc::from(models.events.clone()),
        selected_event_id: ui_state.selected_event_id.unwrap_or(0) as i32,
        inspector_title: SharedString::from(inspector_title.as_str()),
        inspector_meta: SharedString::from(inspector_meta.as_str()),
        inspector_has_secrets,
        secrets_revealed: ui_state.secrets_revealed,
        inspector_payload: SharedString::from(inspector_payload.as_str()),
        endpoints: ModelRc::from(models.endpoints.clone()),
        endpoints_total: ui_state.cached_endpoints_total,
        endpoints_summary: SharedString::from(ui_state.cached_endpoints_summary.as_str()),
        call_status: SharedString::from(ui_state.call_status.as_str()),
        call_response: SharedString::from(ui_state.call_response.as_str()),
        processes: ModelRc::from(models.processes.clone()),
        sockets: ModelRc::from(models.sockets.clone()),
        connections: ModelRc::from(models.connections.clone()),
    });
}
