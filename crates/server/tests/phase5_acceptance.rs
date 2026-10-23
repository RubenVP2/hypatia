use std::sync::{Arc, RwLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use hypatia_server::catalog::{discover_endpoints, CatalogStore};
use hypatia_server::discovery::DiscoveryManager;
use hypatia_server::redaction::SecretVault;
use hypatia_server::riot_http::ReadOnlyRiotClient;
use hypatia_server::security::SecurityConfig;
use hypatia_server::store::EventStore;
use hypatia_server::{create_router, AppState};
use hypatia_shared::{EndpointOrigin, EventKind, SourceId};
use tokio::sync::broadcast;
use tower::ServiceExt;

fn setup_test_server(allowed_port: u16) -> (AppState, SecretVault, Arc<RwLock<EventStore>>, Arc<RwLock<CatalogStore>>) {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let (store, _rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store));
    let (ws_broadcast_tx, _ws_rx) = broadcast::channel(128);
    let catalog = Arc::new(RwLock::new(CatalogStore::new()));
    let riot_client = ReadOnlyRiotClient::new();
    let security_config = SecurityConfig::new(vec![allowed_port], true);

    let state = AppState {
        discovery,
        vault: vault.clone(),
        store: store.clone(),
        catalog: catalog.clone(),
        riot_client,
        ws_broadcast_tx,
        security_config,
        is_demo: false,
    };

    (state, vault, store, catalog)
}

#[tokio::test]
async fn test_security_hardening_acceptance() {
    let (state, _, _, _) = setup_test_server(7878);
    let app = create_router(state);

    // 1. Acceptance test: POST /api/call/LeagueClient?path=/x MUST return 405 Method Not Allowed
    let req_post_call = Request::builder()
        .method("POST")
        .uri("/api/call/LeagueClient?path=/x")
        .header("Host", "127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_post_call).await.unwrap();
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED, "POST /api/call must return 405");

    // 2. Acceptance test: Host: evil.test MUST return 403 Forbidden
    let req_evil_host = Request::builder()
        .method("GET")
        .uri("/api/health")
        .header("Host", "evil.test")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_evil_host).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "Host: evil.test must return 403");

    let req_evil_host_port = Request::builder()
        .method("GET")
        .uri("/api/health")
        .header("Host", "evil.test:7878")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_evil_host_port).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "Host: evil.test:7878 must return 403");

    // 3. Acceptance test: Origin: http://evil.test MUST return 403 Forbidden
    let req_evil_origin_ws = Request::builder()
        .method("GET")
        .uri("/ws")
        .header("Host", "127.0.0.1:7878")
        .header("Origin", "http://evil.test")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_evil_origin_ws).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "Origin: http://evil.test on /ws must return 403");

    let req_evil_origin_post = Request::builder()
        .method("POST")
        .uri("/api/secrets/1/reveal")
        .header("Host", "127.0.0.1:7878")
        .header("Origin", "http://evil.test")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_evil_origin_post).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "Origin: http://evil.test on POST must return 403");

    // 4. Allowed Host and Origin MUST succeed
    let req_valid = Request::builder()
        .method("GET")
        .uri("/api/health")
        .header("Host", "127.0.0.1:7878")
        .header("Origin", "http://127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_valid).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "Valid Host and Origin must return 200 OK");
}

#[tokio::test]
async fn test_api_catalog_routes() {
    let (state, _, _, catalog) = setup_test_server(7878);

    // Pre-populate catalog
    {
        let mut cat = catalog.write().unwrap();
        cat.record_observed(SourceId::LeagueClient, "GET", "/lol-gameflow/v1/gameflow-phase", 42);
        cat.record_observed(SourceId::RiotClient, "GET", "/riotclient/region-locale", 100);
    }

    let app = create_router(state);

    // GET /api/catalog
    let req = Request::builder()
        .method("GET")
        .uri("/api/catalog")
        .header("Host", "127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let all: Vec<hypatia_shared::EndpointInfo> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(all.len(), 2);

    // GET /api/catalog/LeagueClient
    let req_lcu = Request::builder()
        .method("GET")
        .uri("/api/catalog/LeagueClient")
        .header("Host", "127.0.0.1:7878")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req_lcu).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let lcu_eps: Vec<hypatia_shared::EndpointInfo> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(lcu_eps.len(), 1);
    assert_eq!(lcu_eps[0].path_template, "/lol-gameflow/v1/gameflow-phase");
}

#[tokio::test]
async fn test_live_catalog_discovery_acceptance() {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let client = ReadOnlyRiotClient::new();

    // Check if LeagueClient source is discovered
    let lcu_source = discovery.get_source(SourceId::LeagueClient);
    if let Some(src) = lcu_source {
        if let (Some(port), Some(cred)) = (src.port, src.credential) {
            if let Some(password) = vault.reveal(cred.id) {
                println!("Testing live catalog discovery on port {}...", port);
                let endpoints = discover_endpoints(&client, SourceId::LeagueClient, port, Some(&password)).await;
                println!("Discovered {} endpoints from live League Client!", endpoints.len());

                // Acceptance criterion: catalog contains more than 500 endpoints
                assert!(
                    endpoints.len() > 500,
                    "Catalogue LCU must contain > 500 endpoints, found {}",
                    endpoints.len()
                );

                // Verify some common endpoints exist
                assert!(
                    endpoints.iter().any(|e| e.path_template.contains("gameflow")),
                    "Should contain gameflow endpoints"
                );
            }
        }
    } else {
        println!("LeagueClient not running, skipping live discovery check");
    }
}

#[tokio::test]
async fn test_live_call_and_store_redaction_acceptance() {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());

    // If LeagueClient is live, test GET /api/call/LeagueClient?path=/lol-gameflow/v1/gameflow-phase
    let lcu_source = discovery.get_source(SourceId::LeagueClient);
    if let Some(src) = lcu_source {
        if src.port.is_some() && src.credential.is_some() {
            let (store, _rx) = EventStore::new();
            let store = Arc::new(RwLock::new(store));
            let catalog = Arc::new(RwLock::new(CatalogStore::new()));
            let (ws_broadcast_tx, _ws_rx) = broadcast::channel(128);
            let riot_client = ReadOnlyRiotClient::new();
            let security_config = SecurityConfig::new(vec![7878], true);

            let state = AppState {
                discovery,
                vault,
                store: store.clone(),
                catalog: catalog.clone(),
                riot_client,
                ws_broadcast_tx,
                security_config,
                is_demo: false,
            };

            let app = create_router(state);

            let req = Request::builder()
                .method("GET")
                .uri("/api/call/LeagueClient?path=/lol-gameflow/v1/gameflow-phase")
                .header("Host", "127.0.0.1:7878")
                .body(Body::empty())
                .unwrap();

            let res = app.oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);

            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            let json_resp: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

            println!("Live /api/call response: {:?}", json_resp);
            assert_eq!(json_resp["status"], 200);
            assert!(json_resp.get("data").is_some());

            // Invariant 4: Event recorded in EventStore as EventKind::Call
            let store_guard = store.read().unwrap();
            assert_eq!(store_guard.len(), 1, "Should have recorded 1 event in store");
            let event = store_guard.get_event(1).unwrap();
            assert_eq!(event.meta.kind, EventKind::Call);
            assert_eq!(event.meta.source, SourceId::LeagueClient);
            assert_eq!(event.meta.uri, "/lol-gameflow/v1/gameflow-phase");

            // Verify catalog observed entry was recorded
            let cat_guard = catalog.read().unwrap();
            let eps = cat_guard.get_by_source(SourceId::LeagueClient);
            assert_eq!(eps.len(), 1);
            assert_eq!(eps[0].path_template, "/lol-gameflow/v1/gameflow-phase");
            assert_eq!(eps[0].origin, EndpointOrigin::Observed);
            assert_eq!(eps[0].event_count, 1);
        }
    }
}

#[tokio::test]
async fn test_live_client_data_and_replay_acceptance() {
    let client = ReadOnlyRiotClient::new();

    // 1. Direct probe of Live Client Data port 2999
    match client.get(2999, None, "/liveclientdata/allgamedata").await {
        Ok((status, _, body)) => {
            println!("LiveClientData /allgamedata HTTP code: {}", status);
            assert_eq!(status, StatusCode::OK);
            assert!(body.get("activePlayer").is_some(), "allgamedata should have activePlayer");
            let champion_name = body["activePlayer"]["championName"]
                .as_str()
                .or_else(|| body["activePlayer"]["summonerName"].as_str())
                .unwrap_or("Unknown");
            println!("Active player champion: {}", champion_name);
        }
        Err(e) => {
            println!("Game not currently in live match, skipping 2999 active check: {:?}", e);
        }
    }

    // 2. Direct probe of Replay API port 2999
    match client.get(2999, None, "/replay/game").await {
        Ok((status, _, body)) => {
            println!("Replay API /replay/game status: {}", status);
            // In typical match, replay is disabled (404) unless EnableReplayApi=1
            if status == StatusCode::NOT_FOUND {
                println!("Replay API disabled as expected (requires EnableReplayApi=1 in game.cfg)");
            } else if status == StatusCode::OK {
                println!("Replay API active! Body: {:?}", body);
            }
        }
        Err(e) => {
            println!("Replay API connection error: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_live_server_network_hardening_with_client() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let port = addr.port();

    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let (store, _rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store));
    let catalog = Arc::new(RwLock::new(CatalogStore::new()));
    let (ws_broadcast_tx, _ws_rx) = broadcast::channel(128);
    let riot_client = ReadOnlyRiotClient::new();
    let security_config = SecurityConfig::new(vec![port], true);

    let state = AppState {
        discovery,
        vault,
        store,
        catalog,
        riot_client,
        ws_broadcast_tx,
        security_config,
        is_demo: false,
    };

    let app = create_router(state);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::builder().build().unwrap();

    // 1. POST on /api/call -> 405 Method Not Allowed
    let resp = client
        .post(format!("http://127.0.0.1:{}/api/call/LeagueClient?path=/x", port))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 405, "POST /api/call must return 405 Method Not Allowed");

    // 2. Host: evil.test -> 403 Forbidden
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/health", port))
        .header("Host", "evil.test")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "Host: evil.test must return 403 Forbidden");

    // 3. Origin: http://evil.test on /ws -> 403 Forbidden
    let resp = client
        .get(format!("http://127.0.0.1:{}/ws", port))
        .header("Origin", "http://evil.test")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "Origin: http://evil.test on /ws must return 403 Forbidden");

    // 4. GET /api/health with valid headers -> 200 OK
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/health", port))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "GET /api/health with valid headers must return 200 OK");
}


