use leptos::prelude::*;
use hypatia_shared::{SourceId, SourceStatus};
use crate::state::AppState;

#[component]
pub fn SourceChip(id: SourceId) -> impl IntoView {
    let state = expect_context::<AppState>();

    let source_state = Memo::new(move |_| state.source_store.get_state(id));

    let (label, color_class) = match id {
        SourceId::RiotClient => ("Riot Client", "src-riot"),
        SourceId::LeagueClient => ("League Client", "src-lcu"),
        SourceId::LiveGame => ("Partie", "src-live"),
        SourceId::System => ("Système", "src-system"),
    };

    let status_class = move || {
        match source_state.get().map(|s| s.status) {
            Some(SourceStatus::Live) => "live",
            Some(SourceStatus::Connecting) => "connecting",
            Some(SourceStatus::Degraded { .. }) => "connecting",
            _ => "offline",
        }
    };

    let port_text = move || {
        source_state.get().and_then(|s| s.port).map(|p| format!(":{p}")).unwrap_or_default()
    };

    let is_selected = move || {
        state.event_store.filter_source.get() == Some(id)
    };

    let toggle_filter = move |_| {
        let current = state.event_store.filter_source.get();
        if current == Some(id) {
            state.event_store.filter_source.set(None);
        } else {
            state.event_store.filter_source.set(Some(id));
        }
    };

    view! {
        <div
            class=move || {
                format!(
                    "source-chip chamfer-badge {} {}",
                    color_class,
                    if is_selected() { "selected" } else { "" }
                )
            }
            on:click=toggle_filter
            title=format!("Filtrer les événements de {label}")
        >
            <div class=move || format!("source-chip-dot {}", status_class())></div>
            <span>{label}</span>
            <span class="source-chip-port">{port_text}</span>
        </div>
    }
}
