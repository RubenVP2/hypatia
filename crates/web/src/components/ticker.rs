use leptos::prelude::*;
use hypatia_shared::{EventKind, SourceId};
use crate::state::AppState;

#[component]
pub fn Ticker() -> impl IntoView {
    let state = expect_context::<AppState>();
    let store = state.event_store.clone();

    let is_paused = store.is_paused;

    let store_for_ticker = store.clone();
    let ticker_events = move || {
        let _ = store_for_ticker.version.get();
        store_for_ticker.get_ticker_events()
    };

    view! {
        <footer class="app-ticker" aria-hidden="true">
            <div class="ticker-content">
                <div class="ticker-badge">
                    {move || if is_paused.get() {
                        view! {
                            <span class="live-dot" style="background-color: var(--n-6); animation: none;"></span>
                            <span>"PAUSED"</span>
                        }.into_any()
                    } else {
                        view! {
                            <span class="live-dot"></span>
                            <span>"LIVE"</span>
                        }.into_any()
                    }}
                </div>

                <span class="ticker-separator">"|"</span>

                <div class="ticker-items-track">
                    {move || {
                        let evts = ticker_events();
                        if evts.is_empty() {
                            view! {
                                <span class="ticker-item" style="color: var(--text-faint);">
                                    "Waiting for event stream..."
                                </span>
                            }.into_any()
                        } else {
                            evts.into_iter().map(|ev| {
                                let id = ev.id;
                                let (src_tag, _src_cls) = match ev.source {
                                    SourceId::RiotClient => ("RC", "tag-rc"),
                                    SourceId::LeagueClient => ("LCU", "tag-lcu"),
                                    SourceId::LiveGame => ("GAME", "tag-jeu"),
                                    SourceId::System => ("SYS", "tag-sys"),
                                };

                                let (kind_cls, kind_letter) = match ev.kind {
                                    EventKind::Create => ("kind-badge-c", "C"),
                                    EventKind::Update => ("kind-badge-u", "U"),
                                    EventKind::Delete => ("kind-badge-d", "D"),
                                    EventKind::Poll => ("kind-badge-p", "P"),
                                    EventKind::Call => ("kind-badge-g", "G"),
                                };

                                let uri_short = if ev.uri.len() > 36 {
                                    format!("{}…", &ev.uri[..35])
                                } else {
                                    ev.uri.clone()
                                };

                                let store_click = store.clone();

                                view! {
                                    <div
                                        class="ticker-item"
                                        on:click=move |_| store_click.select_event(id)
                                        title=format!("{}: {}", ev.uri, ev.preview)
                                    >
                                        <strong style="color: var(--text);">{src_tag}</strong>
                                        <span class=kind_cls>{kind_letter}</span>
                                        <span>{uri_short}</span>
                                        <span style="color: var(--line);">"•"</span>
                                    </div>
                                }
                            }).collect::<Vec<_>>().into_any()
                        }
                    }}
                </div>
            </div>
        </footer>
    }
}
