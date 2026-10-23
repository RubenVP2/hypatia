use hypatia_shared::{EndpointInfo, SourceId};
use leptos::prelude::*;
use crate::net::CallResponse;

#[derive(Clone)]
pub struct EndpointStore {
    pub endpoints: RwSignal<Vec<EndpointInfo>>,
    pub selected_endpoint: RwSignal<Option<EndpointInfo>>,
    pub filter_query: RwSignal<String>,
    pub filter_source: RwSignal<Option<SourceId>>,
    pub is_loading: RwSignal<bool>,
    pub call_loading: RwSignal<bool>,
    pub call_result: RwSignal<Option<Result<CallResponse, String>>>,
}

impl EndpointStore {
    pub fn new() -> Self {
        Self {
            endpoints: RwSignal::new(Vec::new()),
            selected_endpoint: RwSignal::new(None),
            filter_query: RwSignal::new(String::new()),
            filter_source: RwSignal::new(None),
            is_loading: RwSignal::new(false),
            call_loading: RwSignal::new(false),
            call_result: RwSignal::new(None),
        }
    }

    pub fn set_endpoints(&self, list: Vec<EndpointInfo>) {
        self.endpoints.set(list);
        self.is_loading.set(false);
    }

    pub fn select_endpoint(&self, ep: EndpointInfo) {
        self.selected_endpoint.set(Some(ep));
        self.call_result.set(None);
    }

    pub fn select_by_uri(&self, uri: &str) {
        let list = self.endpoints.get_untracked();
        if let Some(ep) = list.into_iter().find(|e| e.path_template == uri) {
            self.select_endpoint(ep);
        } else {
            // Créer un endpoint temporaire observé si non trouvé dans le catalogue
            let trimmed = uri.trim_start_matches('/');
            let ns = trimmed.split('/').next().unwrap_or("api").to_string();
            let ep = EndpointInfo {
                source: SourceId::LeagueClient,
                method: "GET".to_string(),
                path_template: uri.to_string(),
                namespace: ns,
                description: None,
                origin: hypatia_shared::EndpointOrigin::Observed,
                last_seen: None,
                event_count: 0,
                bytes_total: 0,
            };
            self.select_endpoint(ep);
        }
    }
}
