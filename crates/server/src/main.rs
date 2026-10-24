use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use base64::prelude::*;
use clap::Parser;
use hypatia_server::batcher::spawn_batcher;
use hypatia_server::catalog::CatalogStore;
use hypatia_server::connectors::{
    spawn_live_client_poller, spawn_replay_poller, spawn_wamp_connector_full,
};
use hypatia_server::discovery::DiscoveryManager;
use hypatia_server::fixtures::spawn_demo_generator;
use hypatia_server::redaction::SecretVault;
use hypatia_server::riot_http::ReadOnlyRiotClient;
use hypatia_server::security::SecurityConfig;
use hypatia_server::store::EventStore;
use hypatia_server::{create_router, AppState};
use hypatia_shared::SourceId;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "hypatia-server", about = "Riot Local Observatory Server")]
struct Cli {
    #[arg(short, long, default_value_t = 7878)]
    port: u16,

    #[arg(long)]
    no_open: bool,

    #[arg(long)]
    dev: bool,

    #[arg(long)]
    demo: bool,

    #[arg(long, default_value_t = 10.0)]
    demo_rate: f32,
}

async fn test_riot_client_connection(port: u16, password: &str) -> Result<()> {
    info!(
        "Attempting connection to Riot Client on port {}...",
        port
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("Failed to create reqwest client")?;

    let auth_raw = format!("riot:{}", password);
    let auth_header = format!("Basic {}", BASE64_STANDARD.encode(auth_raw.as_bytes()));
    let url = format!("https://127.0.0.1:{}/riotclient/region-locale", port);

    let resp = client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await
        .context("Failed to send request to Riot Client")?;

    let status = resp.status();
    info!("HTTP status received from Riot Client /riotclient/region-locale: {}", status);

    if status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        info!("Riot Client response: {}", body_text);
    } else {
        warn!("Riot Client returned non-200 status: {}", status);
    }

    Ok(())
}

async fn bind_listener(preferred_port: u16) -> Result<(tokio::net::TcpListener, SocketAddr)> {
    let addr = SocketAddr::from(([127, 0, 0, 1], preferred_port));
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => Ok((listener, addr)),
        Err(e) => {
            warn!(
                "Unable to bind port {} ({:?}) - possible Windows Hyper-V port exclusion",
                preferred_port, e
            );
            if preferred_port == 7878 {
                let fallback_port = 8334;
                let fallback_addr = SocketAddr::from(([127, 0, 0, 1], fallback_port));
                info!("Attempting fallback to http://{}...", fallback_addr);
                let listener = tokio::net::TcpListener::bind(fallback_addr)
                    .await
                    .context("Failed to bind fallback port")?;
                Ok((listener, fallback_addr))
            } else {
                Err(e).context("Failed to bind port")
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hypatia_server=debug,tower_http=debug,info".into()),
        )
        .init();

    info!("Starting hypatia-server...");

    let vault = SecretVault::new();
    let (discovery, _snapshot_rx) = if cli.demo {
        info!(
            "Demo mode activated (synthetic data, rate: {:.1} evt/s)",
            cli.demo_rate
        );
        DiscoveryManager::new_demo(vault.clone())
    } else {
        let (disc, rx) = DiscoveryManager::new(vault.clone());
        disc.spawn_loop(Duration::from_millis(1000));
        (disc, rx)
    };

    // Initialize store and WebSocket streams
    let (store_inner, event_rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store_inner));
    let (ws_broadcast_tx, _ws_broadcast_rx) = broadcast::channel(1024);

    // Initialize Riot HTTP client and catalog
    let riot_client = ReadOnlyRiotClient::new();
    let catalog = Arc::new(RwLock::new(CatalogStore::new()));

    // Start batcher (100 ms aggregation)
    spawn_batcher(
        event_rx,
        store.clone(),
        discovery.clone(),
        ws_broadcast_tx.clone(),
    );

    if cli.demo {
        // In demo mode, inject realistic synthetic stream
        spawn_demo_generator(store.clone(), catalog.clone(), vault.clone(), cli.demo_rate);
    } else {
        // Start WAMP connectors for RiotClient and LeagueClient with catalog integration
        spawn_wamp_connector_full(
            SourceId::RiotClient,
            discovery.clone(),
            store.clone(),
            vault.clone(),
            Some(catalog.clone()),
            Some(riot_client.clone()),
            Some(ws_broadcast_tx.clone()),
        );
        spawn_wamp_connector_full(
            SourceId::LeagueClient,
            discovery.clone(),
            store.clone(),
            vault.clone(),
            Some(catalog.clone()),
            Some(riot_client.clone()),
            Some(ws_broadcast_tx.clone()),
        );

        // Start Live Client Data and Replay connectors
        spawn_live_client_poller(
            discovery.clone(),
            store.clone(),
            catalog.clone(),
            vault.clone(),
            riot_client.clone(),
            ws_broadcast_tx.clone(),
        );
        spawn_replay_poller(
            discovery.clone(),
            store.clone(),
            catalog.clone(),
            vault.clone(),
            riot_client.clone(),
            ws_broadcast_tx.clone(),
        );

        // Test Riot Client connection on startup if available
        if let Some(rc_source) = discovery.get_source(SourceId::RiotClient) {
            if let (Some(port), Some(cred)) = (rc_source.port, rc_source.credential) {
                if let Some(pwd) = vault.reveal(cred.id) {
                    if let Err(e) = test_riot_client_connection(port, &pwd).await {
                        error!("Error during test request to Riot Client: {:?}", e);
                    }
                }
            }
        }
    }

    let (listener, active_addr) = bind_listener(cli.port).await?;
    info!("hypatia-server listening on http://{}", active_addr);

    let mut allowed_ports = vec![active_addr.port()];
    if cli.port != active_addr.port() {
        allowed_ports.push(cli.port);
    }
    let security_config = SecurityConfig::new(allowed_ports, cli.dev);

    let state = AppState {
        discovery,
        vault,
        store,
        catalog,
        riot_client,
        ws_broadcast_tx,
        security_config,
        is_demo: cli.demo,
    };

    let app = create_router(state);

    if !cli.no_open {
        let open_url = format!("http://127.0.0.1:{}", active_addr.port());
        info!("Automatically opening browser at {}", open_url);
        tokio::task::spawn_blocking(move || {
            if let Err(e) = webbrowser::open(&open_url) {
                warn!("Unable to automatically open browser: {:?}", e);
            }
        });
    }

    axum::serve(listener, app).await?;

    Ok(())
}
