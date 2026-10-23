use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::{routing::get, Json, Router};
use base64::prelude::*;
use clap::Parser;
use hypatia_shared::{HealthResponse, RiotClientLockfile};
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

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

fn locate_riot_client_lockfile() -> Option<PathBuf> {
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local_appdata)
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile");
        if p.exists() {
            return Some(p);
        }
    }

    // Also support fallback or USERPROFILE
    if let Ok(home) = env::var("USERPROFILE") {
        let p = PathBuf::from(home)
            .join("AppData")
            .join("Local")
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile");
        if p.exists() {
            return Some(p);
        }
    }

    // WSL path check if run under Linux
    let wsl_p = PathBuf::from("/mnt/c/Users/User/AppData/Local/Riot Games/Riot Client/Config/lockfile");
    if wsl_p.exists() {
        return Some(wsl_p);
    }

    None
}

async fn test_riot_client_connection(lockfile: &RiotClientLockfile) -> Result<()> {
    info!(
        "Tentative de connexion au Riot Client sur port {} (protocole: {})...",
        lockfile.port, lockfile.protocol
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .context("Échec de création du client reqwest")?;

    let auth_raw = format!("riot:{}", lockfile.password);
    let auth_header = format!("Basic {}", BASE64_STANDARD.encode(auth_raw.as_bytes()));
    let url = format!(
        "https://127.0.0.1:{}/riotclient/region-locale",
        lockfile.port
    );

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

    // Vérification du lockfile Riot Client
    if let Some(lockfile_path) = locate_riot_client_lockfile() {
        info!("Lockfile Riot Client trouvé : {:?}", lockfile_path);
        match std::fs::read_to_string(&lockfile_path) {
            Ok(content) => {
                if let Some(parsed) = RiotClientLockfile::parse(&content) {
                    info!(
                        "Lockfile analysé avec succès : process={}, PID={}, port={}",
                        parsed.name, parsed.pid, parsed.port
                    );
                    if let Err(e) = test_riot_client_connection(&parsed).await {
                        error!("Erreur lors de la requête de test vers Riot Client : {:?}", e);
                    }
                } else {
                    warn!("Impossible d'analyser le contenu du lockfile : {}", content);
                }
            }
            Err(e) => {
                warn!("Impossible de lire le lockfile {:?} : {:?}", lockfile_path, e);
            }
        }
    } else {
        warn!("Aucun lockfile Riot Client trouvé dans LOCALAPPDATA.");
    }

    let app = Router::new()
        .route("/api/health", get(health_handler))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let (listener, active_addr) = bind_listener(cli.port).await?;
    info!("hypatia-server écoute sur http://{}", active_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
