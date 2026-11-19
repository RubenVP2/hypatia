//! Telemetry Lab view model: event stream, inspector, endpoint catalog and system footprint.

use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use hypatia_core::{compute_json_diff_localized, AppState};
use hypatia_shared::{
    EndpointInfo, EventKind, EventMeta, Lang, Snapshot, SourceId, SourceStatus, StoredEvent,
    SystemSnapshot,
};
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
    reveal: bool,
    reveal_fn: &impl Fn(&serde_json::Value) -> serde_json::Value,
) -> Cow<'a, serde_json::Value> {
    if reveal {
        Cow::Owned(reveal_fn(payload))
    } else {
        Cow::Borrowed(payload)
    }
}

/// Localized phase label shown in the Telemetry Observatory status strip.
pub fn phase_display_label(raw_phase: &str, lang: Lang) -> &str {
    match raw_phase {
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
    }
}

/// Projects source cards for the Telemetry Observatory.
pub fn project_telemetry_sources(snap: &Snapshot, lang: Lang) -> Vec<TelemetrySourceItem> {
    snap.sources
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
                Some(p) => format!(":{p}"),
                None => "local".to_string(),
            };
            let pid_str = match s.pid {
                Some(pid) => format!("PID {pid}"),
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
        .collect()
}

/// Filters and formats recent event metadata rows for the Telemetry Observatory stream.
pub fn project_telemetry_events(
    recent_events: &[EventMeta],
    src_filter: i32,
    kind_filter: i32,
    lang: Lang,
) -> Vec<TelemetryEventItem> {
    recent_events
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
        .collect()
}

/// Formats the inspector title, metadata subtitle, secret badge, and body (`current`/`diff`/`previous`).
pub fn project_telemetry_inspector(
    stored: Option<&StoredEvent>,
    inspector_mode: i32,
    reveal: bool,
    sel_id: u64,
    lang: Lang,
    reveal_fn: impl Fn(&serde_json::Value) -> serde_json::Value,
) -> (String, String, bool, String) {
    let Some(stored) = stored else {
        return (
            if lang.is_fr() {
                format!("Événement #{sel_id} éjecté du tampon")
            } else {
                format!("Event #{sel_id} evicted from buffer")
            },
            String::new(),
            false,
            String::new(),
        );
    };
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
            "Type : {kind_lbl} · {} · Heure : {}",
            format_bytes(stored.meta.size as u64, lang),
            format_ts_hms(stored.meta.ts)
        )
    } else {
        format!(
            "Kind: {kind_lbl} · {} · Time: {}",
            format_bytes(stored.meta.size as u64, lang),
            format_ts_hms(stored.meta.ts)
        )
    };
    let curr_val = inspected_payload(&stored.payload, reveal, &reveal_fn);
    let prev_val = stored
        .previous_payload
        .as_deref()
        .map(|p| inspected_payload(p, reveal, &reveal_fn));
    let body = match inspector_mode {
        1 => compute_json_diff_localized(prev_val.as_deref(), &curr_val, lang),
        2 => match prev_val {
            Some(ref pv) => serde_json::to_string_pretty(pv).unwrap_or_else(|_| "{}".to_string()),
            None => lang
                .pick(
                    "(No previous state recorded for this URI)",
                    "(Aucun état précédent enregistré pour cette URI)",
                )
                .to_string(),
        },
        _ => serde_json::to_string_pretty(&curr_val).unwrap_or_else(|_| "{}".to_string()),
    };
    (title, meta, stored.meta.has_secrets, body)
}

/// Filters, prioritizes observed endpoints, and formats catalog summary text.
pub fn project_telemetry_endpoints(
    all: &[EndpointInfo],
    filter: &str,
    lang: Lang,
) -> (Vec<TelemetryEndpointItem>, String, i32) {
    let filter_lc = filter.trim().to_ascii_lowercase();
    let mut matched: Vec<&EndpointInfo> = all
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
    let summary = if matching_total > shown {
        if lang.is_fr() {
            format!(
                "Affichage de {shown} sur {matching_total} ({} au total)",
                all.len()
            )
        } else {
            format!("Showing {shown} of {matching_total} ({} total)", all.len())
        }
    } else if lang.is_fr() {
        format!("{matching_total} sur {} endpoints", all.len())
    } else {
        format!("{matching_total} of {} endpoints", all.len())
    };
    (items, summary, all.len() as i32)
}

/// Projects process, listening socket, and connection tables for the Telemetry Observatory.
pub fn project_telemetry_system(
    system: &SystemSnapshot,
    lang: Lang,
) -> (
    Vec<TelemetryProcessItem>,
    Vec<TelemetrySocketItem>,
    Vec<TelemetrySocketItem>,
) {
    let processes = system
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

    let sockets = system
        .sockets
        .iter()
        .map(|s| TelemetrySocketItem {
            proto: SharedString::from(s.proto.as_str()),
            address: SharedString::from(format!("{}:{}", s.local, s.port)),
            pid: SharedString::from(s.pid.to_string()),
            state: SharedString::from(lang.pick("LISTEN", "ÉCOUTE")),
        })
        .collect();

    let connections = system
        .connections
        .iter()
        .map(|c| TelemetrySocketItem {
            proto: SharedString::from("TCP"),
            address: SharedString::from(format!("{} -> {}", c.local, c.remote)),
            pid: SharedString::from(c.pid.to_string()),
            state: SharedString::from(c.state.as_str()),
        })
        .collect();

    (processes, sockets, connections)
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
    let phase_label = phase_display_label(raw_phase, lang);
    let sources = project_telemetry_sources(&snap, lang);

    let events_sig = (latest_id, total_len, src_filter, kind_filter, lang);
    if (!paused && ui_state.last_events_sig != Some(events_sig))
        || ui_state.models.events.row_count() == 0
    {
        let filtered = project_telemetry_events(&snap.recent_events, src_filter, kind_filter, lang);
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
            project_telemetry_inspector(
                stored_opt.as_ref(),
                inspector_mode,
                ui_state.secrets_revealed,
                sel_id,
                lang,
                |v| app_state.reveal_secrets_in_value(v),
            )
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
        let guard = app_state.catalog.read().unwrap();
        let all = guard.endpoints();
        let (items, summary, total) =
            project_telemetry_endpoints(all, &ui_state.endpoint_filter, lang);
        sync_model(&ui_state.models.endpoints, items);
        ui_state.cached_endpoints_summary = summary;
        ui_state.cached_endpoints_total = total;
        ui_state.last_catalog_sig = Some(catalog_sig);
    }

    let (processes, sockets, connections) = project_telemetry_system(&snap.system, lang);

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

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_shared::{
        EndpointInfo, EndpointOrigin, EventMeta, ListenSocket, ProcessInfo, RemoteConn, Snapshot,
        SystemSnapshot,
    };
    use serde_json::json;

    #[test]
    fn projects_telemetry_observatory_views_without_window_handles() {
        assert_eq!(phase_display_label("ChampSelect", Lang::En), "Champ Select");
        assert_eq!(phase_display_label("ChampSelect", Lang::Fr), "Sélection");

        let events = vec![
            EventMeta {
                id: 1,
                ts: 1_000,
                source: SourceId::LeagueClient,
                kind: EventKind::Update,
                uri: "/lol-lobby/v2/lobby".to_string(),
                size: 120,
                preview: "{}".to_string(),
                has_secrets: false,
            },
            EventMeta {
                id: 2,
                ts: 2_000,
                source: SourceId::LiveGame,
                kind: EventKind::Poll,
                uri: "/liveclientdata/allgamedata".to_string(),
                size: 2048,
                preview: "{}".to_string(),
                has_secrets: false,
            },
        ];
        let lcu_only = project_telemetry_events(&events, 1, 0, Lang::Fr);
        assert_eq!(lcu_only.len(), 1);
        assert_eq!(lcu_only[0].id, 1);
        assert_eq!(lcu_only[0].kind.as_str(), "MÀJ");

        let endpoints = vec![
            EndpointInfo {
                source: SourceId::LeagueClient,
                method: "GET".to_string(),
                path_template: "/lol-zeta/v1/x".to_string(),
                namespace: "lol-zeta".to_string(),
                description: None,
                origin: EndpointOrigin::Declared,
                last_seen: None,
                event_count: 0,
                bytes_total: 0,
            },
            EndpointInfo {
                source: SourceId::LeagueClient,
                method: "GET".to_string(),
                path_template: "/lol-alpha/v1/live".to_string(),
                namespace: "lol-alpha".to_string(),
                description: Some("Match state".to_string()),
                origin: EndpointOrigin::Observed,
                last_seen: Some(1_000),
                event_count: 5,
                bytes_total: 1024,
            },
        ];
        let (items, summary, total) = project_telemetry_endpoints(&endpoints, "alpha", Lang::En);
        assert_eq!(total, 2);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path.as_str(), "/lol-alpha/v1/live");
        assert!(summary.contains("1 of 2"));

        let snap = Snapshot {
            sources: vec![],
            phase: None,
            recent_events: vec![],
            system: SystemSnapshot {
                ts: 0,
                processes: vec![ProcessInfo {
                    pid: 42,
                    name: "LeagueClientUx.exe".to_string(),
                    exe: None,
                    started: 0,
                    cpu: 1.5,
                    mem_bytes: 2048,
                    source: Some(SourceId::LeagueClient),
                }],
                sockets: vec![ListenSocket {
                    proto: "TCP".to_string(),
                    local: "127.0.0.1".to_string(),
                    port: 1234,
                    pid: 42,
                }],
                connections: vec![RemoteConn {
                    local: "127.0.0.1:1".to_string(),
                    remote: "127.0.0.1:2".to_string(),
                    state: "ESTABLISHED".to_string(),
                    pid: 42,
                }],
            },
        };
        let (procs, socks, conns) = project_telemetry_system(&snap.system, Lang::Fr);
        assert_eq!(procs.len(), 1);
        assert_eq!(socks[0].state.as_str(), "ÉCOUTE");
        assert_eq!(conns.len(), 1);

        let stored = hypatia_shared::StoredEvent {
            meta: events[0].clone(),
            payload: std::sync::Arc::new(json!({"a": 2})),
            previous_payload: Some(std::sync::Arc::new(json!({"a": 1}))),
        };
        let (title, _meta, has_sec, body) =
            project_telemetry_inspector(Some(&stored), 1, false, 1, Lang::En, |v| v.clone());
        assert!(!has_sec);
        assert!(title.contains("/lol-lobby/v2/lobby"));
        assert!(body.contains("-   \"a\": 1"));
    }
}
