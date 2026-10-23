use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use hypatia_server::catalog::CatalogStore;
use hypatia_server::discovery::DiscoveryManager;
use hypatia_server::fixtures::spawn_demo_generator;
use hypatia_server::redaction::SecretVault;
use hypatia_server::riot_http::ReadOnlyRiotClient;
use hypatia_server::security::SecurityConfig;
use hypatia_server::store::EventStore;
use hypatia_server::{create_router, AppState};
use hypatia_shared::{HealthResponse, ServerMsg, SourceId};
use tokio::sync::broadcast;
use tower::ServiceExt;

fn setup_demo_app() -> (AppState, axum::Router) {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new_demo(vault.clone());
    let (store, event_rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store));
    let catalog = Arc::new(RwLock::new(CatalogStore::new()));
    let (ws_broadcast_tx, _) = broadcast::channel(128);
    let riot_client = ReadOnlyRiotClient::new();
    let security_config = SecurityConfig::new(vec![7878], true);

    hypatia_server::batcher::spawn_batcher(
        event_rx,
        store.clone(),
        discovery.clone(),
        ws_broadcast_tx.clone(),
    );

    spawn_demo_generator(store.clone(), catalog.clone(), vault.clone(), 50.0);

    let state = AppState {
        discovery,
        vault,
        store,
        catalog,
        riot_client,
        ws_broadcast_tx,
        security_config,
        is_demo: true,
    };

    let router = create_router(state.clone());
    (state, router)
}

#[tokio::test]
async fn test_static_assets_serving_and_spa_fallback() {
    let (_state, app) = setup_demo_app();

    // 1. GET / returns index.html with text/html content-type
    let req = Request::builder()
        .uri("/")
        .header(header::HOST, "127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ctype = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ctype.starts_with("text/html"));
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("<!DOCTYPE html>") || html.contains("<html"));

    // 2. SPA fallback: GET /live returns index.html
    let req = Request::builder()
        .uri("/live")
        .header(header::HOST, "127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ctype = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ctype.starts_with("text/html"));

    // 3. API 404: GET /api/nonexistent should return 404 NOT FOUND, NOT index.html
    let req = Request::builder()
        .uri("/api/nonexistent_endpoint")
        .header(header::HOST, "127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_demo_mode_health_and_websocket_stream() {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new_demo(vault.clone());
    let (store, event_rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store));
    let catalog = Arc::new(RwLock::new(CatalogStore::new()));
    let (ws_broadcast_tx, _) = broadcast::channel(128);
    let riot_client = ReadOnlyRiotClient::new();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    let security_config = SecurityConfig::new(vec![server_addr.port()], true);

    hypatia_server::batcher::spawn_batcher(
        event_rx,
        store.clone(),
        discovery.clone(),
        ws_broadcast_tx.clone(),
    );

    spawn_demo_generator(store.clone(), catalog.clone(), vault.clone(), 100.0);

    let state = AppState {
        discovery,
        vault,
        store,
        catalog,
        riot_client,
        ws_broadcast_tx,
        security_config,
        is_demo: true,
    };

    let app = create_router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // 1. Check /api/health returns demo = true
    let client = reqwest::Client::new();
    let health_resp: HealthResponse = client
        .get(format!("http://{}/api/health", server_addr))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health_resp.status, "ok");
    assert!(health_resp.demo, "HealthResponse must indicate demo mode");

    // 2. Connect to /ws and receive Hello with is_demo = true
    let ws_url = format!("ws://{}/ws", server_addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    let first_msg = ws_stream.next().await.unwrap().unwrap();
    let text = first_msg.into_text().unwrap();
    let server_msg: ServerMsg = serde_json::from_str(&text).unwrap();

    match server_msg {
        ServerMsg::Hello {
            version,
            snapshot,
            is_demo,
        } => {
            assert_eq!(version, "0.1.0");
            assert!(is_demo, "Hello message must have is_demo = true in demo mode");
            assert_eq!(snapshot.sources.len(), 4);
            assert!(snapshot.sources.iter().any(|s| s.id == SourceId::LeagueClient));
            assert!(snapshot.sources.iter().any(|s| s.id == SourceId::RiotClient));
        }
        other => panic!("Expected Hello, got {:?}", other),
    }

    // 3. Receive at least one Batch of synthetic events
    let mut got_batch = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(Ok(msg))) =
            tokio::time::timeout(Duration::from_millis(500), ws_stream.next()).await
        {
            if let Ok(text) = msg.into_text() {
                if let Ok(ServerMsg::Batch { events, rate, .. }) =
                    serde_json::from_str::<ServerMsg>(&text)
                {
                    if !events.is_empty() {
                        got_batch = true;
                        println!(
                            "Received demo batch with {} events, rate: {:.1} evt/s",
                            events.len(),
                            rate
                        );
                        break;
                    }
                }
            }
        }
    }

    assert!(
        got_batch,
        "Should have received at least one non-empty batch of synthetic events"
    );
}
