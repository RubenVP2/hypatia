use leptos::prelude::*;
use crate::state::events::EventStore;
use crate::state::phase::PhaseStore;
use crate::state::sources::SourceStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    Live,
    Endpoints,
    System,
    About,
}

#[derive(Clone)]
pub struct AppState {
    pub active_tab: RwSignal<NavTab>,
    pub is_connected: RwSignal<bool>,
    pub secrets_hidden: RwSignal<bool>,
    pub event_store: EventStore,
    pub source_store: SourceStore,
    pub phase_store: PhaseStore,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_tab: RwSignal::new(NavTab::Live),
            is_connected: RwSignal::new(false),
            secrets_hidden: RwSignal::new(true),
            event_store: EventStore::new(),
            source_store: SourceStore::new(),
            phase_store: PhaseStore::new(),
        }
    }
}
