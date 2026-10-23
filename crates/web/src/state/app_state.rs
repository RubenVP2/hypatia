use leptos::prelude::*;
use crate::state::endpoints::EndpointStore;
use crate::state::events::EventStore;
use crate::state::phase::PhaseStore;
use crate::state::sources::SourceStore;
use crate::state::system::SystemStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    Live,
    Endpoints,
    System,
    About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Data,
    Diff,
    Raw,
}

#[derive(Clone)]
pub struct AppState {
    pub active_tab: RwSignal<NavTab>,
    pub is_connected: RwSignal<bool>,
    pub is_demo: RwSignal<bool>,
    pub secrets_hidden: RwSignal<bool>,
    pub event_store: EventStore,
    pub source_store: SourceStore,
    pub phase_store: PhaseStore,
    pub system_store: SystemStore,
    pub endpoint_store: EndpointStore,
    pub command_palette_open: RwSignal<bool>,
    pub inspector_tab: RwSignal<InspectorTab>,
    pub left_rail_width: RwSignal<i32>,
    pub inspector_width: RwSignal<i32>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_tab: RwSignal::new(NavTab::Live),
            is_connected: RwSignal::new(false),
            is_demo: RwSignal::new(false),
            secrets_hidden: RwSignal::new(true),
            event_store: EventStore::new(),
            source_store: SourceStore::new(),
            phase_store: PhaseStore::new(),
            system_store: SystemStore::new(),
            endpoint_store: EndpointStore::new(),
            command_palette_open: RwSignal::new(false),
            inspector_tab: RwSignal::new(InspectorTab::Data),
            left_rail_width: RwSignal::new(280),
            inspector_width: RwSignal::new(440),
        }
    }
}
