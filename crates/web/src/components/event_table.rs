use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};
use hypatia_shared::{EventKind, SourceId};
use crate::state::AppState;

const ROW_HEIGHT: f64 = 28.0;
const OVERSCAN: usize = 12;

#[component]
pub fn EventTable() -> impl IntoView {
    let state = expect_context::<AppState>();
    let store = state.event_store.clone();

    let scroll_top = RwSignal::new(0.0f64);
    let viewport_height = RwSignal::new(500.0f64);

    let search_ref = NodeRef::<leptos::html::Input>::new();

    // Memo réactif des événements filtrés (dépend de version et des filtres)
    let store_for_filter = store.clone();
    let filtered_events = Memo::new(move |_| {
        let _ = store_for_filter.version.get();
        let _ = store_for_filter.is_paused.get();
        let _ = store_for_filter.filter_source.get();
        let _ = store_for_filter.filter_kind.get();
        let _ = store_for_filter.filter_query.get();
        store_for_filter.get_filtered()
    });

    // Calcul de la tranche virtualisée
    let visible_slice = move || {
        let events = filtered_events.get();
        let total = events.len();
        if total == 0 {
            return (0usize, 0.0f64, total * (ROW_HEIGHT as usize), Vec::new());
        }

        let st = scroll_top.get();
        let vh = viewport_height.get();

        let start_raw = (st / ROW_HEIGHT).floor() as usize;
        let start = start_raw.saturating_sub(OVERSCAN);

        let visible_count = (vh / ROW_HEIGHT).ceil() as usize + (OVERSCAN * 2);
        let end = (start + visible_count).min(total);

        let top_offset = (start as f64) * ROW_HEIGHT;
        let total_height = (total as f64) * ROW_HEIGHT;

        let slice = events[start..end].to_vec();
        (start, top_offset, total_height as usize, slice)
    };

    let on_scroll = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(elem) = target.dyn_into::<HtmlElement>() {
                scroll_top.set(elem.scroll_top() as f64);
                let client_h = elem.client_height() as f64;
                if client_h > 0.0 {
                    viewport_height.set(client_h);
                }
            }
        }
    };

    // Gestion du clavier pour la navigation (j, k, flèches)
    let store_for_nav = store.clone();
    let on_table_keydown = move |ev: KeyboardEvent| {
        let key = ev.key();
        let events = filtered_events.get();
        if events.is_empty() {
            return;
        }

        let curr_id = store_for_nav.selected_id.get_untracked();
        let curr_idx = curr_id.and_then(|id| events.iter().position(|e| e.id == id));

        match key.as_str() {
            "ArrowDown" | "j" => {
                ev.prevent_default();
                let next_idx = match curr_idx {
                    Some(i) => (i + 1).min(events.len() - 1),
                    None => 0,
                };
                let id = events[next_idx].id;
                store_for_nav.select_event(id);
            }
            "ArrowUp" | "k" => {
                ev.prevent_default();
                let prev_idx = match curr_idx {
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                let id = events[prev_idx].id;
                store_for_nav.select_event(id);
            }
            _ => {}
        }
    };

    let store_for_clear = store.clone();
    let store_for_pause = store.clone();
    let store_for_input = store.clone();
    let store_for_chips = store.clone();

    view! {
        <div
            class="event-table-container"
            tabindex="0"
            role="region"
            aria-label="Tableau des événements en direct"
            on:keydown=on_table_keydown
        >
            <div class="table-toolbar">
                <div class="toolbar-filters">
                    <button
                        class=move || {
                            format!(
                                "filter-chip {}",
                                if store_for_chips.filter_source.get().is_none() { "active" } else { "" }
                            )
                        }
                        on:click={
                            let s = store_for_chips.clone();
                            move |_| s.filter_source.set(None)
                        }
                    >
                        "Toutes"
                    </button>
                    <button
                        class=move || {
                            format!(
                                "filter-chip {}",
                                if store_for_chips.filter_source.get() == Some(SourceId::RiotClient) { "active" } else { "" }
                            )
                        }
                        on:click={
                            let s = store_for_chips.clone();
                            move |_| s.filter_source.set(Some(SourceId::RiotClient))
                        }
                    >
                        "RC"
                    </button>
                    <button
                        class=move || {
                            format!(
                                "filter-chip {}",
                                if store_for_chips.filter_source.get() == Some(SourceId::LeagueClient) { "active" } else { "" }
                            )
                        }
                        on:click={
                            let s = store_for_chips.clone();
                            move |_| s.filter_source.set(Some(SourceId::LeagueClient))
                        }
                    >
                        "LCU"
                    </button>
                    <button
                        class=move || {
                            format!(
                                "filter-chip {}",
                                if store_for_chips.filter_source.get() == Some(SourceId::LiveGame) { "active" } else { "" }
                            )
                        }
                        on:click={
                            let s = store_for_chips.clone();
                            move |_| s.filter_source.set(Some(SourceId::LiveGame))
                        }
                    >
                        "Jeu"
                    </button>

                    <input
                        node_ref=search_ref
                        type="text"
                        class="filter-search-box"
                        placeholder="Filtrer URI... (/)"
                        prop:value=move || store_for_input.filter_query.get()
                        on:input=move |ev| {
                            store_for_input.filter_query.set(event_target_value(&ev));
                        }
                    />
                </div>

                <div class="toolbar-actions">
                    <button
                        class=move || {
                            format!(
                                "pause-btn {}",
                                if store_for_pause.is_paused.get() { "paused" } else { "" }
                            )
                        }
                        on:click=move |_| store_for_pause.toggle_pause()
                        title="Mettre en pause le flux visuel (Espace)"
                    >
                        <span>
                            {move || if store_for_pause.is_paused.get() { "▶ Reprendre" } else { "❚❚ Pause" }}
                        </span>
                        {move || {
                            let unseen = store_for_pause.unseen_count.get();
                            if unseen > 0 {
                                view! {
                                    <span class="unseen-badge">
                                        {format!("+{unseen}")}
                                    </span>
                                }.into_any()
                            } else {
                                ().into_any()
                            }
                        }}
                    </button>

                    <button
                        class="clear-btn"
                        on:click=move |_| store_for_clear.clear()
                        title="Vider la liste des événements"
                    >
                        "Effacer"
                    </button>
                </div>
            </div>

            <div class="table-header" role="row">
                <span class="col-ts">"Horodatage"</span>
                <span class="col-src">"Source"</span>
                <span class="col-kind">"Type"</span>
                <span class="col-uri">"URI"</span>
                <span class="col-size">"Taille"</span>
                <span class="col-lock">""</span>
            </div>

            <div
                class="table-body-viewport"
                on:scroll=on_scroll
                role="grid"
                aria-rowcount=move || filtered_events.get().len() as i32
            >
                {move || {
                    let (_start, top_offset, total_height, slice) = visible_slice();

                    if filtered_events.get().is_empty() {
                        view! {
                            <div class="empty-state">
                                <div class="empty-state-icon">"○"</div>
                                <div class="empty-state-text">
                                    "En attente d'événements Riot en direct..."
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div
                                class="table-scroll-phantom"
                                style=format!("height: {}px;", total_height)
                            >
                                <div
                                    class="table-visible-rows"
                                    style=format!("transform: translateY({}px);", top_offset)
                                >
                                    {slice.into_iter().map(|ev| {
                                        let id = ev.id;
                                        let is_selected = move || store.selected_id.get() == Some(id);

                                        let ts_str = format_ts(ev.ts);
                                        let (tag_cls, tag_txt) = match ev.source {
                                            SourceId::RiotClient => ("tag-rc", "RC"),
                                            SourceId::LeagueClient => ("tag-lcu", "LCU"),
                                            SourceId::LiveGame => ("tag-jeu", "JEU"),
                                            SourceId::System => ("tag-sys", "SYS"),
                                        };

                                        let (kind_cls, kind_txt) = match ev.kind {
                                            EventKind::Create => ("kind-badge-c", "C"),
                                            EventKind::Update => ("kind-badge-u", "U"),
                                            EventKind::Delete => ("kind-badge-d", "D"),
                                            EventKind::Poll => ("kind-badge-p", "P"),
                                            EventKind::Call => ("kind-badge-g", "G"),
                                        };

                                        let size_str = format_bytes(ev.size);
                                        let store_click = store.clone();
                                        let uri_label = ev.uri.clone();
                                        let uri_title = ev.uri.clone();

                                        view! {
                                            <div
                                                class=move || {
                                                    format!(
                                                        "event-row {}",
                                                        if is_selected() { "selected" } else { "" }
                                                    )
                                                }
                                                role="row"
                                                on:click=move |_| store_click.select_event(id)
                                            >
                                                <span class="col-ts tabular-nums">{ts_str}</span>
                                                <span class="col-src">
                                                    <span class=format!("src-tag chamfer-badge {}", tag_cls)>
                                                        {tag_txt}
                                                    </span>
                                                </span>
                                                <span class=format!("col-kind {}", kind_cls)>
                                                    {kind_txt}
                                                </span>
                                                <span class="col-uri" title=uri_title>
                                                    {uri_label}
                                                </span>
                                                <span class="col-size tabular-nums">{size_str}</span>
                                                <span class="col-lock">
                                                    {if ev.has_secrets { "🔒" } else { "" }}
                                                </span>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

fn format_ts(epoch_ms: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(epoch_ms as f64));
    let h = date.get_hours();
    let m = date.get_minutes();
    let s = date.get_seconds();
    let ms = date.get_milliseconds();
    format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms)
}

fn format_bytes(bytes: u32) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} Mo", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} Ko", bytes as f64 / 1024.0)
    } else {
        format!("{} o", bytes)
    }
}
