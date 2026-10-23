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
}

async fn test_riot_client_connection(port: u16, password: &str) -> Result<()> {
    info!(
        "Tentative de connexion au Riot Client sur port {}...",
        port
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("Échec de création du client reqwest")?;

    let auth_raw = format!("riot:{}", password);
    let auth_header = format!("Basic {}", BASE64_STANDARD.encode(auth_raw.as_bytes()));
    let url = format!("https://127.0.0.1:{}/riotclient/region-locale", port);

    let resp = client
        .get(&url)
        .header("Authorization", auth_header)
        .send()
        .await
        .context("Échec de la requête vers Riot Client")?;

    let status = resp.status();
    info!("Code HTTP obtenu depuis Riot Client /riotclient/region-locale : {}", status);

    if status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        info!("Réponse Riot Client : {}", body_text);
    } else {
        warn!("Riot Client a renvoyé un statut non-200 : {}", status);
    }

    Ok(())
}

async fn bind_listener(preferred_port: u16) -> Result<(tokio::net::TcpListener, SocketAddr)> {
    let addr = SocketAddr::from(([127, 0, 0, 1], preferred_port));
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => Ok((listener, addr)),
        Err(e) => {
            warn!(
                "Impossible de lier le port {} ({:?}) - possible exclusion de plage Windows Hyper-V",
                preferred_port, e
            );
            if preferred_port == 7878 {
                let fallback_port = 8334;
                let fallback_addr = SocketAddr::from(([127, 0, 0, 1], fallback_port));
                info!("Tentative de repli sur http://{}...", fallback_addr);
                let listener = tokio::net::TcpListener::bind(fallback_addr)
                    .await
                    .context("Échec du port de repli")?;
                Ok((listener, fallback_addr))
            } else {
                Err(e).context("Échec de liaison du port")
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

    info!("Démarrage de hypatia-server...");

    let vault = SecretVault::new();
    let (discovery, _snapshot_rx) = DiscoveryManager::new(vault.clone());

    // Démarrage de la boucle de découverte en tâche de fond (intervalle 1 seconde)
    discovery.spawn_loop(Duration::from_millis(1000));

    // Initialisation du store et des flux WebSocket
    let (store_inner, event_rx) = EventStore::new();
    let store = Arc::new(RwLock::new(store_inner));
    let (ws_broadcast_tx, _ws_broadcast_rx) = broadcast::channel(1024);

    // Initialisation du client HTTP Riot et du catalogue
    let riot_client = ReadOnlyRiotClient::new();
    let catalog = Arc::new(RwLock::new(CatalogStore::new()));

    // Démarrage du batcher (agrégation 100 ms)
    spawn_batcher(
        event_rx,
        store.clone(),
        discovery.clone(),
        ws_broadcast_tx.clone(),
    );

    // Démarrage des connecteurs WAMP pour RiotClient et LeagueClient avec intégration catalogue
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

    // Démarrage des connecteurs Live Client Data et Replay
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

    // Test de connexion Riot Client au démarrage si disponible
    if let Some(rc_source) = discovery.get_source(SourceId::RiotClient) {
        if let (Some(port), Some(cred)) = (rc_source.port, rc_source.credential) {
            if let Some(pwd) = vault.reveal(cred.id) {
                if let Err(e) = test_riot_client_connection(port, &pwd).await {
                    error!("Erreur lors de la requête de test vers Riot Client : {:?}", e);
                }
            }
        }
    }

    let (listener, active_addr) = bind_listener(cli.port).await?;
    info!("hypatia-server écoute sur http://{}", active_addr);

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
    };

    let app = create_router(state);

    axum::serve(listener, app).await?;

    Ok(())
}
