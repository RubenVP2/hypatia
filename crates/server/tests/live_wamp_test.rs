use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::http::StatusCode;
use futures_util::StreamExt;
use hypatia_server::batcher::spawn_batcher;
use hypatia_server::connectors::spawn_wamp_connector;
use hypatia_server::discovery::DiscoveryManager;
use hypatia_server::redaction::SecretVault;
use hypatia_server::store::{EventStore, StoredEvent};
use hypatia_server::{create_router, AppState};
use hypatia_shared::{ServerMsg, Snapshot, SourceId, UriState};
use tokio::sync::broadcast;

#[tokio::test]
async fn test_live_wamp_capture_and_routes() {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());

    // Start discovery background loop
    discovery.spawn_loop(Duration::from_millis(500));

    // Initialize EventStore, batcher, and broadcast channel
    let (store_inner, event_rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store_inner));
    let (ws_broadcast_tx, _) = broadcast::channel(1024);

    spawn_batcher(
        event_rx,
        store.clone(),
        discovery.clone(),
        ws_broadcast_tx.clone(),
    );

    // Spawn WAMP connectors for both RiotClient and LeagueClient
    spawn_wamp_connector(
        SourceId::RiotClient,
        discovery.clone(),
        store.clone(),
        vault.clone(),
    );
    spawn_wamp_connector(
        SourceId::LeagueClient,
        discovery.clone(),
        store.clone(),
        vault.clone(),
    );

    // Bind server on ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let catalog = Arc::new(RwLock::new(hypatia_server::catalog::CatalogStore::new()));
    let riot_client = hypatia_server::riot_http::ReadOnlyRiotClient::new();
    let security_config = hypatia_server::security::SecurityConfig::new(vec![server_addr.port()], true);

    let state = AppState {
        discovery: discovery.clone(),
        vault: vault.clone(),
        store: store.clone(),
        catalog,
        riot_client,
        ws_broadcast_tx,
        security_config,
    };

    let app = create_router(state);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    println!("Live test server listening on http://{}", server_addr);

    // 1. Connect to /ws
    let ws_url = format!("ws://{}/ws", server_addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    // 2. Receive Hello message
    let first_msg = ws_stream.next().await.unwrap().unwrap();
    let text = first_msg.into_text().unwrap();
    let hello_msg: ServerMsg = serde_json::from_str(&text).unwrap();

    match hello_msg {
        ServerMsg::Hello { version, snapshot } => {
            println!("Received Hello version: {}", version);
            println!("Initial sources in Hello: {:?}", snapshot.sources.len());
            assert_eq!(version, "0.1.0");
        }
        other => panic!("Expected Hello, got {:?}", other),
    }

    // 3. Listen for Batches from real LeagueClient / RiotClient
    let timeout = tokio::time::sleep(Duration::from_secs(8));
    tokio::pin!(timeout);

    let mut captured_events = Vec::new();

    println!("Listening for live WAMP events from League of Legends / Riot Client...");
    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) = msg {
                    if let Ok(server_msg) = serde_json::from_str::<ServerMsg>(&t) {
                        match server_msg {
                            ServerMsg::Batch { events, rate, .. } => {
                                if !events.is_empty() {
                                    println!("Received live Batch: {} events, rate: {:.1} evt/s", events.len(), rate);
                                    for e in &events {
                                        println!("  -> [{}] {} {} (preview: {})", e.source, e.kind as u8, e.uri, e.preview);
                                        captured_events.push(e.clone());
                                    }
                                    if captured_events.len() >= 3 {
                                        break;
                                    }
                                }
                            }
                            ServerMsg::Phase(phase) => {
                                println!("Live game phase update: {}", phase.phase);
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ = &mut timeout => {
                println!("Timeout waiting for additional live events. Total captured: {}", captured_events.len());
                break;
            }
        }
    }

    // Check store contents
    let store_count = store.read().unwrap().len();
    println!("Total events in store: {}", store_count);

    let client = reqwest::Client::new();

    // 4. Test GET /api/snapshot
    let snap_url = format!("http://{}/api/snapshot", server_addr);
    let snap_resp = client.get(&snap_url).send().await.unwrap();
    assert_eq!(snap_resp.status(), StatusCode::OK);
    let snapshot: Snapshot = snap_resp.json().await.unwrap();
    println!("GET /api/snapshot returned {} sources, recent events: {}", snapshot.sources.len(), snapshot.recent_events.len());

    // 5. If we captured events, test GET /api/events/{id} and GET /api/state/{source}?uri=...
    if let Some(sample_event) = captured_events.first() {
        let event_url = format!("http://{}/api/events/{}", server_addr, sample_event.id);
        let event_resp = client.get(&event_url).send().await.unwrap();
        assert_eq!(event_resp.status(), StatusCode::OK);
        let event_text = event_resp.text().await.unwrap();

        // Strict invariant check: no raw JWTs or cleartext secrets
        assert!(
            !event_text.contains("eyJhbGciOi"),
            "Raw unmasked JWT detected in event payload!"
        );

        let stored: StoredEvent = serde_json::from_str(&event_text).unwrap();
        assert_eq!(stored.meta.id, sample_event.id);
        println!("GET /api/events/{} payload successfully fetched and verified (size: {} bytes)", sample_event.id, stored.meta.size);

        // Test GET /api/state/{source}?uri=...
        let state_url = format!(
            "http://{}/api/state/{}?uri={}",
            server_addr, sample_event.source, sample_event.uri
        );
        let state_resp = client.get(&state_url).send().await.unwrap();
        assert_eq!(state_resp.status(), StatusCode::OK);
        let uri_state: UriState = state_resp.json().await.unwrap();
        assert!(uri_state.count >= 1);
        println!("GET /api/state/{} for '{}' confirmed (count: {})", sample_event.source, sample_event.uri, uri_state.count);
    } else {
        println!("Note: No live events were pushed within the test window, verifying route fallbacks");
        let non_existent = format!("http://{}/api/events/99999", server_addr);
        let resp_404 = client.get(&non_existent).send().await.unwrap();
        assert_eq!(resp_404.status(), StatusCode::NOT_FOUND);
    }
}
