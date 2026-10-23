use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use hypatia_server::discovery::DiscoveryManager;
use hypatia_server::redaction::SecretVault;
use hypatia_server::{create_router, AppState};
use hypatia_shared::{HealthResponse, SourceId, SourceState, SystemSnapshot};
use tower::ServiceExt;

#[tokio::test]
async fn test_api_health() {
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let state = AppState { discovery, vault };
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
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let state = AppState { discovery, vault };
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
    let vault = SecretVault::new();
    let (discovery, _) = DiscoveryManager::new(vault.clone());
    let state = AppState { discovery, vault };
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
    let raw_json = String::from_utf8(bytes.to_vec()).unwrap();

    // Verify raw credentials never leak raw passwords in the JSON output
    assert!(
        !raw_json.contains("npfPQc0M"),
        "Raw secret leaked in /api/sources JSON!"
    );

    let sources: Vec<SourceState> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sources.len(), 4);
    assert_eq!(sources[0].id, SourceId::RiotClient);
    assert_eq!(sources[1].id, SourceId::LeagueClient);
    assert_eq!(sources[2].id, SourceId::LiveGame);
    assert_eq!(sources[3].id, SourceId::System);
}
