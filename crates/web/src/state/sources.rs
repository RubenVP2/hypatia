use leptos::prelude::*;
use hypatia_shared::{SourceId, SourceState, SourceStatus};

#[derive(Clone)]
pub struct SourceStore {
    pub sources: RwSignal<Vec<SourceState>>,
}

impl SourceStore {
    pub fn new() -> Self {
        // Initialize with 4 sources in Offline state by default
        let defaults = vec![
            SourceState {
                id: SourceId::RiotClient,
                status: SourceStatus::Offline,
                pid: None,
                port: None,
                protocol: None,
                since: 0,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            },
            SourceState {
                id: SourceId::LeagueClient,
                status: SourceStatus::Offline,
                pid: None,
                port: None,
                protocol: None,
                since: 0,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            },
            SourceState {
                id: SourceId::LiveGame,
                status: SourceStatus::Offline,
                pid: None,
                port: None,
                protocol: None,
                since: 0,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            },
            SourceState {
                id: SourceId::System,
                status: SourceStatus::Live,
                pid: None,
                port: None,
                protocol: None,
                since: 0,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            },
        ];

        Self {
            sources: RwSignal::new(defaults),
        }
    }

    pub fn update_sources(&self, new_sources: Vec<SourceState>) {
        self.sources.update(|current| {
            for new_s in new_sources {
                if let Some(pos) = current.iter().position(|s| s.id == new_s.id) {
                    current[pos] = new_s;
                } else {
                    current.push(new_s);
                }
            }
        });
    }

    pub fn get_state(&self, id: SourceId) -> Option<SourceState> {
        self.sources.get().into_iter().find(|s| s.id == id)
    }
}
