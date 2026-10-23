pub mod discovery;
pub mod redaction;

use std::sync::Arc;
use axum::{extract::State, routing::get, Json, Router};
use discovery::DiscoveryManager;
use redaction::SecretVault;
use hypatia_shared::{HealthResponse, SourceState, SystemSnapshot};

#[derive(Clone)]
pub struct AppState {
    pub discovery: Arc<DiscoveryManager>,
    pub vault: SecretVault,
}

pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

pub async fn system_handler(State(state): State<AppState>) -> Json<SystemSnapshot> {
    Json(state.discovery.get_system_snapshot())
}

pub async fn sources_handler(State(state): State<AppState>) -> Json<Vec<SourceState>> {
    Json(state.discovery.get_sources())
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/system", get(system_handler))
        .route("/api/sources", get(sources_handler))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
}
