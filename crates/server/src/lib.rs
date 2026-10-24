pub mod assets;
pub mod batcher;
pub mod catalog;
pub mod connectors;
pub mod discovery;
pub mod fixtures;
pub mod redaction;
pub mod riot_http;
pub mod security;
pub mod store;

use std::sync::{Arc, RwLock};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use hypatia_shared::{
    ClientMsg, EndpointInfo, HealthResponse, ServerMsg, Snapshot, SourceId, SourceState,
    SystemSnapshot, UriState,
};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::catalog::CatalogStore;
use crate::discovery::DiscoveryManager;
use crate::redaction::SecretVault;
use crate::riot_http::ReadOnlyRiotClient;
use crate::security::{security_middleware, SecurityConfig};
use crate::store::{EventStore, StoredEvent};

#[derive(Clone)]
pub struct AppState {
    pub discovery: Arc<DiscoveryManager>,
    pub vault: SecretVault,
    pub store: Arc<RwLock<EventStore>>,
    pub catalog: Arc<RwLock<CatalogStore>>,
    pub riot_client: ReadOnlyRiotClient,
    pub ws_broadcast_tx: broadcast::Sender<ServerMsg>,
    pub security_config: SecurityConfig,
    pub is_demo: bool,
}

impl AppState {
    pub fn new(
        discovery: Arc<DiscoveryManager>,
        vault: SecretVault,
        store: Arc<RwLock<EventStore>>,
        catalog: Arc<RwLock<CatalogStore>>,
        riot_client: ReadOnlyRiotClient,
        ws_broadcast_tx: broadcast::Sender<ServerMsg>,
        security_config: SecurityConfig,
        is_demo: bool,
    ) -> Self {
        Self {
            discovery,
            vault,
            store,
            catalog,
            riot_client,
            ws_broadcast_tx,
            security_config,
            is_demo,
        }
    }

    /// Assembles a complete current system Snapshot.
    pub fn get_snapshot(&self) -> Snapshot {
        let mut sources = self.discovery.get_sources();
        {
            let guard = self.store.read().unwrap();
            for s in &mut sources {
                let (events_total, bytes_total) = guard.source_stats(s.id);
                s.events_total = events_total;
                s.bytes_total = bytes_total;
            }
        }
        let guard = self.store.read().unwrap();
        Snapshot {
            sources,
            phase: guard.phase(),
            recent_events: guard.get_recent_meta(500),
            system: self.discovery.get_system_snapshot(),
        }
    }
}

pub async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        demo: state.is_demo,
    })
}

pub async fn system_handler(State(state): State<AppState>) -> Json<SystemSnapshot> {
    Json(state.discovery.get_system_snapshot())
}

pub async fn sources_handler(State(state): State<AppState>) -> Json<Vec<SourceState>> {
    let mut sources = state.discovery.get_sources();
    let guard = state.store.read().unwrap();
    for s in &mut sources {
        let (events_total, bytes_total) = guard.source_stats(s.id);
        s.events_total = events_total;
        s.bytes_total = bytes_total;
    }
    Json(sources)
}

pub async fn snapshot_handler(State(state): State<AppState>) -> Json<Snapshot> {
    Json(state.get_snapshot())
}

pub async fn event_detail_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<StoredEvent>, (StatusCode, Json<serde_json::Value>)> {
    let guard = state.store.read().unwrap();
    match guard.get_event(id) {
        Some(stored) => Ok(Json(stored)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Event {} not found or evicted", id) })),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct StateParams {
    pub uri: Option<String>,
}

pub async fn uri_state_handler(
    State(state): State<AppState>,
    Path(source_str): Path<String>,
    Query(params): Query<StateParams>,
) -> Result<Json<UriState>, (StatusCode, Json<serde_json::Value>)> {
    let source_id = match source_str.parse::<SourceId>() {
        Ok(id) => id,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid source id: {}", source_str) })),
            ));
        }
    };

    let uri = match params.uri {
        Some(u) if !u.is_empty() => u,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing or empty 'uri' query parameter" })),
            ));
        }
    };

    let guard = state.store.read().unwrap();
    match guard.get_state(source_id, &uri) {
        Some(uri_state) => Ok(Json(uri_state)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("State for URI '{}' not found", uri) })),
        )),
    }
}

pub async fn catalog_all_handler(State(state): State<AppState>) -> Json<Vec<EndpointInfo>> {
    let endpoints = state.catalog.read().unwrap().get_all();
    Json(endpoints)
}

pub async fn catalog_source_handler(
    State(state): State<AppState>,
    Path(source_str): Path<String>,
) -> Result<Json<Vec<EndpointInfo>>, (StatusCode, Json<serde_json::Value>)> {
    let source_id = match source_str.parse::<SourceId>() {
        Ok(id) => id,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid source id: {}", source_str) })),
            ));
        }
    };

    let endpoints = state.catalog.read().unwrap().get_by_source(source_id);
    Ok(Json(endpoints))
}

#[derive(Debug, Deserialize)]
pub struct CallParams {
    pub path: String,
}

pub async fn call_handler(
    State(state): State<AppState>,
    Path(source_str): Path<String>,
    Query(params): Query<CallParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let source_id = match source_str.parse::<SourceId>() {
        Ok(id) => id,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("Invalid source id: {}", source_str) })),
            ));
        }
    };

    if source_id == SourceId::System {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Cannot make API call to System source" })),
        ));
    }

    if !params.path.starts_with('/') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Query parameter 'path' must start with '/'" })),
        ));
    }

    // Determine target port and auth credentials
    let (port, auth_password) = match source_id {
        SourceId::RiotClient => {
            let src = state.discovery.get_source(SourceId::RiotClient);
            let port = src.as_ref().and_then(|s| s.port);
            let cred = src.as_ref().and_then(|s| s.credential.clone());
            let pwd = cred.and_then(|c| state.vault.reveal(c.id));
            match (port, pwd) {
                (Some(p), Some(pwd)) => (p, Some(pwd)),
                _ => {
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(serde_json::json!({ "error": "RiotClient is not available or credentials missing" })),
                    ));
                }
            }
        }
        SourceId::LeagueClient => {
            let src = state.discovery.get_source(SourceId::LeagueClient);
            let port = src.as_ref().and_then(|s| s.port);
            let cred = src.as_ref().and_then(|s| s.credential.clone());
            let pwd = cred.and_then(|c| state.vault.reveal(c.id));
            match (port, pwd) {
                (Some(p), Some(pwd)) => (p, Some(pwd)),
                _ => {
                    return Err((
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(serde_json::json!({ "error": "LeagueClient is not available or credentials missing" })),
                    ));
                }
            }
        }
        SourceId::LiveGame => (2999, None),
        SourceId::System => unreachable!(),
    };

    // Execute read-only GET request
    let (status, _headers, mut body) = match state
        .riot_client
        .get(port, auth_password.as_deref(), &params.path)
        .await
    {
        Ok(res) => res,
        Err(e) => {
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("GET request failed: {:?}", e) })),
            ));
        }
    };

    // Invariant 4: Must pass through redact_value and be recorded as EventKind::Call
    let meta = {
        let mut guard = state.store.write().unwrap();
        guard.insert_event(
            source_id,
            hypatia_shared::EventKind::Call,
            params.path.clone(),
            body.clone(),
            &state.vault,
        )
    };

    // Update Catalog observed entries
    let changed = {
        let mut cat = state.catalog.write().unwrap();
        cat.record_observed(source_id, "GET", &params.path, meta.size as u64)
    };
    if changed {
        let _ = state
            .ws_broadcast_tx
            .send(ServerMsg::CatalogChanged { source: source_id });
    }

    // Retrieve the redacted body from the event store
    let guard = state.store.read().unwrap();
    let redacted_data = guard
        .get_event(meta.id)
        .map(|e| e.payload)
        .unwrap_or_else(|| {
            crate::redaction::redact_value(&mut body, &state.vault);
            body
        });

    Ok(Json(serde_json::json!({
        "status": status.as_u16(),
        "data": redacted_data,
    })))
}

pub async fn secret_reveal_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.vault.reveal(id) {
        Some(secret) => {
            tracing::info!("Secret {} revealed", id);
            Ok(Json(serde_json::json!({
                "id": id,
                "value": secret,
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Secret {} not found or expired", id) })),
        )),
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, state))
}

async fn handle_ws_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Send Hello message immediately on connection
    let snapshot = state.get_snapshot();
    let hello = ServerMsg::Hello {
        version: "0.1.0".to_string(),
        snapshot,
        is_demo: state.is_demo,
    };

    if let Ok(hello_json) = serde_json::to_string(&hello) {
        if sender.send(Message::Text(hello_json.into())).await.is_err() {
            return;
        }
    }

    let mut broadcast_rx = state.ws_broadcast_tx.subscribe();
    let mut paused = false;

    loop {
        tokio::select! {
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(msg) = serde_json::from_str::<ClientMsg>(&text) {
                            match msg {
                                ClientMsg::Pause(p) => {
                                    paused = p;
                                }
                                ClientMsg::Ping => {
                                    let _ = sender.send(Message::Pong(vec![].into())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = sender.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::debug!("WebSocket client error: {:?}", e);
                        break;
                    }
                    _ => {}
                }
            }
            server_msg = broadcast_rx.recv() => {
                match server_msg {
                    Ok(msg) => {
                        if paused && matches!(msg, ServerMsg::Batch { .. }) {
                            // Don't forward event batches while client is paused
                            continue;
                        }
                        if let Ok(json) = serde_json::to_string(&msg) {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("WebSocket client lagged, skipped {} messages; sending Hello resync", skipped);
                        let snapshot = state.get_snapshot();
                        let hello = ServerMsg::Hello {
                            version: "0.1.0".to_string(),
                            snapshot,
                            is_demo: state.is_demo,
                        };
                        if let Ok(json) = serde_json::to_string(&hello) {
                            if sender.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        }
    }
}

pub fn create_router(state: AppState) -> Router {
    let sec_config = state.security_config.clone();

    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/system", get(system_handler))
        .route("/api/sources", get(sources_handler))
        .route("/api/snapshot", get(snapshot_handler))
        .route("/api/events/{id}", get(event_detail_handler))
        .route("/api/state/{source}", get(uri_state_handler))
        .route("/api/catalog", get(catalog_all_handler))
        .route("/api/catalog/{source}", get(catalog_source_handler))
        .route("/api/call/{source}", get(call_handler))
        .route("/api/secrets/{id}/reveal", post(secret_reveal_handler))
        .route("/ws", get(ws_handler))
        .route("/", get(assets::static_handler))
        .fallback(assets::static_handler)
        .layer(axum::middleware::from_fn_with_state(
            sec_config,
            security_middleware,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}
