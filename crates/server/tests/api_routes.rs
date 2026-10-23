use std::sync::{Arc, RwLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use hypatia_server::discovery::DiscoveryManager;
use hypatia_server::redaction::SecretVault;
use hypatia_server::store::{EventStore, StoredEvent};
use hypatia_server::{create_router, AppState};
use hypatia_shared::{
    ClientMsg, EventKind, HealthResponse, ServerMsg, Snapshot, SourceId, SourceState,
    SystemSnapshot, UriState,
};
use serde_json::json;
use tokio::sync::broadcast;
use tower::ServiceExt;

fn setup_test_state() -> (AppState, SecretVault, Arc<RwLock<EventStore>>) {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let (store, _rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store));
    let (ws_broadcast_tx, _ws_rx) = broadcast::channel(128);

    let state = AppState {
        discovery,
        vault: vault.clone(),
        store: store.clone(),
        ws_broadcast_tx,
    };

    (state, vault, store)
}

#[tokio::test]
async fn test_api_health() {
    let (state, _, _) = setup_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let health: HealthResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(health.status, "ok");
}

#[tokio::test]
async fn test_api_system() {
    let (state, _, _) = setup_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/system")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let snapshot: SystemSnapshot = serde_json::from_slice(&bytes).unwrap();
    assert!(snapshot.ts > 0);
}

#[tokio::test]
async fn test_api_sources() {
    let (state, _, _) = setup_test_state();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/sources")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let sources: Vec<SourceState> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sources.len(), 4);
    assert_eq!(sources[0].id, SourceId::RiotClient);
    assert_eq!(sources[1].id, SourceId::LeagueClient);
    assert_eq!(sources[2].id, SourceId::LiveGame);
    assert_eq!(sources[3].id, SourceId::System);
}

#[tokio::test]
async fn test_api_snapshot() {
    let (state, vault, store) = setup_test_state();

    // Insert an event and phase
    {
        let mut guard = store.write().unwrap();
        guard.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-gameflow/v1/gameflow-phase".to_string(),
            json!("Lobby"),
            &vault,
        );
    }

    let app = create_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/snapshot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let snapshot: Snapshot = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(snapshot.sources.len(), 4);
    assert!(snapshot.phase.is_some());
    assert_eq!(snapshot.phase.unwrap().phase, "Lobby");
    assert_eq!(snapshot.recent_events.len(), 1);
    assert_eq!(
        snapshot.recent_events[0].uri,
        "/lol-gameflow/v1/gameflow-phase"
    );
}

#[tokio::test]
async fn test_api_event_detail() {
    let (state, vault, store) = setup_test_state();

    // Insert two events on same URI
    let meta1 = {
        let mut guard = store.write().unwrap();
        guard.insert_event(
            SourceId::LeagueClient,
            EventKind::Create,
            "/lol-summoner/v1/current-summoner".to_string(),
            json!({ "name": "TestPlayer", "token": "secret_abc" }),
            &vault,
        )
    };

    let meta2 = {
        let mut guard = store.write().unwrap();
        guard.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-summoner/v1/current-summoner".to_string(),
            json!({ "name": "TestPlayer", "level": 32 }),
            &vault,
        )
    };

    let app = create_router(state.clone());

    // 1. Check event 1 (has secret redacted and no previous payload)
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/events/{}", meta1.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp1.status(), StatusCode::OK);
    let bytes1 = resp1.into_body().collect().await.unwrap().to_bytes();
    let raw1 = String::from_utf8(bytes1.to_vec()).unwrap();
    assert!(
        !raw1.contains("secret_abc"),
        "Raw secret leaked in event payload!"
    );

    let stored1: StoredEvent = serde_json::from_slice(&bytes1).unwrap();
    assert_eq!(stored1.meta.id, meta1.id);
    assert!(stored1.meta.has_secrets);
    assert_eq!(stored1.previous_payload, None);

    // 2. Check event 2 (has previous payload)
    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/events/{}", meta2.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::OK);
    let bytes2 = resp2.into_body().collect().await.unwrap().to_bytes();
    let stored2: StoredEvent = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(stored2.meta.id, meta2.id);
    assert!(stored2.previous_payload.is_some());

    // 3. Check 404 for non-existent event
    let resp_404 = app
        .oneshot(
            Request::builder()
                .uri("/api/events/999999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_404.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_api_uri_state() {
    let (state, vault, store) = setup_test_state();

    {
        let mut guard = store.write().unwrap();
        guard.insert_event(
            SourceId::RiotClient,
            EventKind::Update,
            "/chat/v4/presences".to_string(),
            json!({ "status": "online" }),
            &vault,
        );
    }

    let app = create_router(state);

    // Valid URI state request
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/state/RiotClient?uri=/chat/v4/presences")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let uri_state: UriState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(uri_state.count, 1);
    assert_eq!(uri_state.current, json!({ "status": "online" }));

    // Unknown URI returns 404
    let resp_404 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/state/RiotClient?uri=/unknown/uri")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_404.status(), StatusCode::NOT_FOUND);

    // Unknown source returns 400
    let resp_bad_src = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/state/InvalidSource?uri=/chat/v4/presences")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_bad_src.status(), StatusCode::BAD_REQUEST);

    // Missing uri param returns 400
    let resp_missing_uri = app
        .oneshot(
            Request::builder()
                .uri("/api/state/RiotClient")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_missing_uri.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_api_secret_reveal() {
    let (state, vault, _) = setup_test_state();

    let secret_ref = vault.store("super_secret_token_xyz".to_string(), "token".to_string());
    let app = create_router(state);

    // Reveal stored secret
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/secrets/{}/reveal", secret_ref.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_resp: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json_resp["id"], secret_ref.id);
    assert_eq!(json_resp["value"], "super_secret_token_xyz");

    // Reveal unknown secret returns 404
    let resp_404 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/secrets/999999/reveal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_404.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_ws_connection_and_protocol() {
    let (state, vault, store) = setup_test_state();

    // Bind server on ephemeral port to test live WebSocket connection
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Connect WebSocket client
    let url = format!("ws://{}/ws", addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    // 1. Verify First message is ServerMsg::Hello
    let first_msg = ws_stream.next().await.unwrap().unwrap();
    let text = first_msg.into_text().unwrap();
    let server_msg: ServerMsg = serde_json::from_str(&text).unwrap();

    match server_msg {
        ServerMsg::Hello { version, snapshot } => {
            assert_eq!(version, "0.1.0");
            assert_eq!(snapshot.sources.len(), 4);
        }
        other => panic!("Expected Hello message, got: {:?}", other),
    }

    // 2. Broadcast a Batch message and verify receipt
    let meta = {
        let mut guard = store.write().unwrap();
        guard.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-chat/v1/me".to_string(),
            json!({ "statusMessage": "Testing WS" }),
            &vault,
        )
    };

    let batch = ServerMsg::Batch {
        events: vec![meta],
        sources: state.discovery.get_sources(),
        rate: 1.0,
    };
    state.ws_broadcast_tx.send(batch.clone()).unwrap();

    let second_msg = ws_stream.next().await.unwrap().unwrap();
    let text2 = second_msg.into_text().unwrap();
    let server_msg2: ServerMsg = serde_json::from_str(&text2).unwrap();
    assert_eq!(server_msg2, batch);

    // 3. Test sending ClientMsg::Ping
    let ping = ClientMsg::Ping;
    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(&ping).unwrap().into(),
        ))
        .await
        .unwrap();

    let pong_resp = ws_stream.next().await.unwrap().unwrap();
    assert!(pong_resp.is_pong(), "Expected Pong message in response to Ping");

    // 4. Test ClientMsg::Pause(true) - batches should be skipped
    let pause = ClientMsg::Pause(true);
    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(&pause).unwrap().into(),
        ))
        .await
        .unwrap();

    // Small delay to ensure pause is processed
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Send another batch
    state.ws_broadcast_tx.send(batch).unwrap();

    // Also send a Phase message (phase is NOT a batch, so it should NOT be skipped)
    let phase_msg = ServerMsg::Phase(hypatia_shared::GamePhase {
        phase: "InProgress".to_string(),
        since: 1700000000,
    });
    state.ws_broadcast_tx.send(phase_msg.clone()).unwrap();

    // The next received message must be the Phase message, not the Batch!
    let third_msg = ws_stream.next().await.unwrap().unwrap();
    let text3 = third_msg.into_text().unwrap();
    let server_msg3: ServerMsg = serde_json::from_str(&text3).unwrap();
    assert_eq!(server_msg3, phase_msg);
}
