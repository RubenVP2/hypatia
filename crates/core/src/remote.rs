//! Opt-in LAN Mobile Remote server, QR code generator, and pairing token guard.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hypatia_shared::{RemoteCommand, SourceId};
use qrcode::QrCode;
use rand::Rng;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{broadcast, watch};
use tracing::{info, warn};

use crate::lcu_actions::{
    extract_remote_state, AlertTracker, LcuActionClient, RemoteActionError,
};
use crate::AppState;

const REMOTE_UI_HTML: &str = include_str!("remote_ui.html");

const FONT_OVERPASS_REGULAR: &[u8] =
    include_bytes!("../../desktop/ui/fonts/Overpass-Regular.ttf");
const FONT_OVERPASS_SEMIBOLD: &[u8] =
    include_bytes!("../../desktop/ui/fonts/Overpass-SemiBold.ttf");
const FONT_OVERPASS_EXTRABOLD: &[u8] =
    include_bytes!("../../desktop/ui/fonts/Overpass-ExtraBold.ttf");
const FONT_BARLOW_CONDENSED_BOLD: &[u8] =
    include_bytes!("../../desktop/ui/fonts/BarlowCondensed-Bold.ttf");
const FONT_BARLOW_CONDENSED_EXTRABOLD: &[u8] =
    include_bytes!("../../desktop/ui/fonts/BarlowCondensed-ExtraBold.ttf");
const FONT_OVERPASS_MONO: &[u8] = include_bytes!("../../desktop/ui/fonts/OverpassMono.ttf");

const SERVICE_WORKER_JS: &str = r#"
self.addEventListener('install', (event) => {
  self.skipWaiting();
});
self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});
self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clientList) => {
      for (const client of clientList) {
        if ('focus' in client) return client.focus();
      }
      if (self.clients.openWindow) return self.clients.openWindow('/');
    })
  );
});
"#;

/// Rendered QR code in both RGBA8 pixel buffer (for Slint UI) and Unicode text (for CLI).
#[derive(Debug, Clone)]
pub struct QrCodeVisual {
    pub url: String,
    pub token: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub terminal: String,
}

/// Discovers the primary LAN IPv4 address of this machine without sending network traffic.
pub fn detect_lan_ip() -> IpAddr {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = sock.local_addr() {
                if !local.ip().is_loopback() && !local.ip().is_unspecified() {
                    return local.ip();
                }
            }
        }
    }
    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
}

/// Generates a random 128-bit hex pairing token.
pub fn generate_pairing_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 16] = rng.random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Renders a pairing URL into an RGBA8 image buffer and a compact Unicode string.
pub fn render_qr_code(url: &str, token: &str) -> Result<QrCodeVisual> {
    let code = QrCode::new(url.as_bytes()).context("Failed to encode QR code")?;
    let modules = code.to_colors();
    let n = code.width();
    let quiet = 3usize;
    let total = n + quiet * 2;
    let scale = 6usize;
    let img_size = (total * scale) as u32;
    let mut rgba = vec![255u8; (img_size * img_size * 4) as usize];

    for my in 0..total {
        for mx in 0..total {
            let is_dark = if mx >= quiet && mx < quiet + n && my >= quiet && my < quiet + n {
                modules[(my - quiet) * n + (mx - quiet)] == qrcode::Color::Dark
            } else {
                false
            };
            let (r, g, b) = if is_dark { (20, 22, 27) } else { (243, 241, 234) };
            for dy in 0..scale {
                for dx in 0..scale {
                    let px = mx * scale + dx;
                    let py = my * scale + dy;
                    let idx = (py * (img_size as usize) + px) * 4;
                    rgba[idx] = r;
                    rgba[idx + 1] = g;
                    rgba[idx + 2] = b;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }

    // Compact 2-rows-per-line terminal representation
    let mut terminal = String::new();
    let mut y = 0;
    while y < total {
        for x in 0..total {
            let top = if x >= quiet && x < quiet + n && y >= quiet && y < quiet + n {
                modules[(y - quiet) * n + (x - quiet)] == qrcode::Color::Dark
            } else {
                false
            };
            let bot = if y + 1 < total && x >= quiet && x < quiet + n && (y + 1) >= quiet && (y + 1) < quiet + n {
                modules[(y + 1 - quiet) * n + (x - quiet)] == qrcode::Color::Dark
            } else {
                false
            };
            let ch = match (top, bot) {
                (true, true) => ' ',
                (true, false) => '▄',
                (false, true) => '▀',
                (false, false) => '█',
            };
            terminal.push(ch);
        }
        terminal.push('\n');
        y += 2;
    }

    Ok(QrCodeVisual {
        url: url.to_string(),
        token: token.to_string(),
        width: img_size,
        height: img_size,
        rgba,
        terminal,
    })
}

#[derive(Clone)]
struct RemoteRouterState {
    app: AppState,
    action_client: LcuActionClient,
    token: Arc<RwLock<String>>,
    alerts: Arc<RwLock<AlertTracker>>,
    state_tx: broadcast::Sender<String>,
    allowed_hosts: Vec<String>,
    port: u16,
    active_clients: Arc<AtomicUsize>,
}

#[derive(Deserialize, Default)]
struct TokenQuery {
    token: Option<String>,
}

/// Handle to control a running Mobile Remote server from the desktop UI or CLI.
pub struct RemoteServerHandle {
    pub lan_ip: IpAddr,
    pub port: u16,
    token: Arc<RwLock<String>>,
    active_clients: Arc<AtomicUsize>,
    shutdown_tx: Option<watch::Sender<bool>>,
}

impl RemoteServerHandle {
    pub fn connected_clients(&self) -> usize {
        self.active_clients.load(Ordering::Relaxed)
    }

    pub fn qr_visual(&self) -> Result<QrCodeVisual> {
        let tok = self.token.read().unwrap().clone();
        let url = format!("http://{}:{}/?token={}", self.lan_ip, self.port, tok);
        render_qr_code(&url, &tok)
    }

    /// Rotates the pairing token immediately, invalidating any previously paired phone.
    pub fn rotate_token(&self) -> Result<QrCodeVisual> {
        let new_tok = generate_pairing_token();
        *self.token.write().unwrap() = new_tok.clone();
        let url = format!("http://{}:{}/?token={}", self.lan_ip, self.port, new_tok);
        render_qr_code(&url, &new_tok)
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
    }
}

impl Drop for RemoteServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn check_host_and_origin(headers: &HeaderMap, allowed_hosts: &[String], port: u16) -> bool {
    if let Some(host_str) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) {
        let host_only = host_str
            .split(':')
            .next()
            .unwrap_or(host_str)
            .trim()
            .to_ascii_lowercase();
        let host_ok = allowed_hosts
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&host_only));
        if !host_ok {
            warn!("Remote rejected Host header '{}'", host_str);
            return false;
        }
    } else {
        return false;
    }

    if let Some(origin_str) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        let stripped = origin_str
            .strip_prefix("http://")
            .or_else(|| origin_str.strip_prefix("https://"))
            .unwrap_or("");
        let authority = stripped.trim_end_matches('/');
        let valid = allowed_hosts.iter().any(|h| {
            authority.eq_ignore_ascii_case(h)
                || authority.eq_ignore_ascii_case(&format!("{h}:{port}"))
        });
        if !valid {
            warn!("Remote rejected Origin header '{}'", origin_str);
            return false;
        }
    }

    true
}

fn check_token(headers: &HeaderMap, query_token: Option<&str>, expected: &str) -> bool {
    if let Some(qt) = query_token {
        if qt == expected {
            return true;
        }
    }
    if let Some(ht) = headers
        .get("x-hypatia-token")
        .and_then(|v| v.to_str().ok())
    {
        if ht.trim() == expected {
            return true;
        }
    }
    if let Some(cookie_hdr) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for part in cookie_hdr.split(';') {
            if let Some((k, v)) = part.trim().split_once('=') {
                if k.trim() == "hypatia_token" && v.trim() == expected {
                    return true;
                }
            }
        }
    }
    false
}

/// Starts the opt-in LAN Mobile Remote server on `0.0.0.0:{port}` (or an OS-assigned port if `0`).
pub async fn start_remote_server(app: AppState, preferred_port: u16) -> Result<RemoteServerHandle> {
    let lan_ip = detect_lan_ip();
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], preferred_port));
    let listener = match tokio::net::TcpListener::bind(bind_addr).await {
        Ok(l) => l,
        Err(_) => {
            tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
                .await
                .context("Failed to bind Mobile Remote listener")?
        }
    };
    let actual_port = listener.local_addr()?.port();
    let token = Arc::new(RwLock::new(generate_pairing_token()));
    let alerts = Arc::new(RwLock::new(AlertTracker::default()));
    let active_clients = Arc::new(AtomicUsize::new(0));
    let (state_tx, _) = broadcast::channel::<String>(64);
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    let allowed_hosts = vec![
        lan_ip.to_string(),
        "127.0.0.1".to_string(),
        "localhost".to_string(),
    ];

    let router_state = RemoteRouterState {
        app: app.clone(),
        action_client: LcuActionClient::new(),
        token: token.clone(),
        alerts: alerts.clone(),
        state_tx: state_tx.clone(),
        allowed_hosts,
        port: actual_port,
        active_clients: active_clients.clone(),
    };

    // Pushes a fresh snapshot to paired phones whenever observable state changes. With no
    // phone connected this task does nothing: no projection, no serialization, no LCU calls.
    let bg_state = router_state.clone();
    let mut shutdown_bg = shutdown_rx.clone();
    tokio::spawn(async move {
        let mut changes = bg_state.app.signal.subscribe();
        let mut aux_sync = tokio::time::interval(Duration::from_millis(1000));
        aux_sync.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_sent = String::new();
        loop {
            let has_clients = bg_state.active_clients.load(Ordering::Relaxed) > 0;
            tokio::select! {
                _ = shutdown_bg.changed() => break,
                _ = aux_sync.tick(), if has_clients => {
                    if !bg_state.app.is_demo_active() {
                        sync_live_lcu_aux(&bg_state).await;
                    }
                }
                changed = changes.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    if has_clients {
                        let payload = current_snapshot_json(&bg_state);
                        if payload != last_sent {
                            let _ = bg_state.state_tx.send(payload.clone());
                            last_sent = payload;
                        }
                    }
                    // Coalesce bursts of changes into at most ~10 pushes per second.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    });

    let router = Router::new()
        .route("/", get(handle_index))
        .route("/sw.js", get(handle_sw))
        .route("/fonts/{name}", get(handle_font))
        .route("/assets/{category}/{id}", get(handle_asset))
        .route("/api/state", get(handle_get_state))
        .route("/api/action", post(handle_post_action))
        .route("/ws", get(handle_ws))
        .with_state(router_state);

    tokio::spawn(async move {
        info!("Mobile Remote listening on http://{}:{}", lan_ip, actual_port);
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await;
        info!("Mobile Remote server stopped");
    });

    Ok(RemoteServerHandle {
        lan_ip,
        port: actual_port,
        token,
        active_clients,
        shutdown_tx: Some(shutdown_tx),
    })
}

async fn sync_live_lcu_aux(st: &RemoteRouterState) {
    st.app
        .assets
        .ensure_metadata_synced(&st.app.discovery, &st.app.riot_client)
        .await;

    let Some(lcu) = st.app.discovery.lcu_args() else {
        return;
    };
    let Some(port) = lcu.app_port else {
        return;
    };
    let token_ref = lcu.remoting_auth_token.as_deref();

    let (phase, need_perks, need_pickable, need_skins) = {
        let guard = st.app.store.read().unwrap();
        let p = guard.phase().map(|gp| gp.phase).unwrap_or_default();
        let np = guard
            .get_state(SourceId::LeagueClient, "/lol-perks/v1/pages")
            .is_none();
        let npick = guard
            .get_state(
                SourceId::LeagueClient,
                "/lol-champ-select/v1/pickable-champion-ids",
            )
            .is_none();
        let nskins = guard
            .get_state(
                SourceId::LeagueClient,
                "/lol-champ-select/v1/skin-carousel-skins",
            )
            .is_none();
        (p, np, npick, nskins)
    };

    let mut uris = Vec::new();
    if need_perks {
        uris.push("/lol-perks/v1/pages");
    }
    if phase == "ChampSelect" {
        if need_pickable {
            uris.push("/lol-champ-select/v1/pickable-champion-ids");
            uris.push("/lol-champ-select/v1/bannable-champion-ids");
        }
        if need_skins {
            uris.push("/lol-champ-select/v1/skin-carousel-skins");
        }
    }

    for uri in uris {
        if let Ok((status, _, body)) = st.app.riot_client.get(port, token_ref, uri).await {
            if status.is_success() {
                st.app.insert_lcu_update(uri, body);
            }
        }
    }
}

fn current_snapshot_json(st: &RemoteRouterState) -> String {
    let mut alert_guard = st.alerts.write().unwrap();
    let snap = extract_remote_state(&st.app, &mut alert_guard);
    serde_json::to_string(&snap).unwrap_or_else(|_| "{}".to_string())
}

fn request_lang(headers: &HeaderMap, app: &AppState) -> hypatia_shared::Lang {
    if let Some(raw) = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
    {
        for item in raw.split(',') {
            let tag = item.split(';').next().unwrap_or("").trim();
            if let Some(lang) = hypatia_shared::Lang::from_code(tag) {
                return lang;
            }
        }
    }
    app.lang()
}

async fn handle_index(
    State(st): State<RemoteRouterState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, "Forbidden host").into_response();
    }
    let expected = st.token.read().unwrap().clone();
    if !check_token(&headers, q.token.as_deref(), &expected) {
        let lang = request_lang(&headers, &st.app);
        let html = format!(
            "<!doctype html><html lang='{}'><body style='background:#14161b;color:#eceef2;font-family:sans-serif;padding:32px;text-align:center'>\
            <h2>{}</h2>\
            <p>{}</p>\
            </body></html>",
            lang.as_code(),
            lang.pick(
                "Pairing token required or expired",
                "Jeton d'appairage requis ou expiré",
            ),
            lang.pick(
                "Scan the current QR code shown in Hypatia on your PC to connect.",
                "Scannez le QR code actuel affiché dans Hypatia sur votre PC pour vous connecter.",
            )
        );
        return (StatusCode::UNAUTHORIZED, Html(html)).into_response();
    }

    let cookie = format!(
        "hypatia_token={}; Path=/; HttpOnly; SameSite=Strict",
        expected
    );
    (
        StatusCode::OK,
        [
            (header::SET_COOKIE, cookie),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Html(REMOTE_UI_HTML),
    )
        .into_response()
}

async fn handle_sw(State(st): State<RemoteRouterState>, headers: HeaderMap) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        SERVICE_WORKER_JS,
    )
        .into_response()
}

async fn handle_font(
    State(st): State<RemoteRouterState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    let bytes: &[u8] = match name.as_str() {
        "Overpass-Regular.ttf" => FONT_OVERPASS_REGULAR,
        "Overpass-SemiBold.ttf" => FONT_OVERPASS_SEMIBOLD,
        "Overpass-ExtraBold.ttf" => FONT_OVERPASS_EXTRABOLD,
        "BarlowCondensed-Bold.ttf" => FONT_BARLOW_CONDENSED_BOLD,
        "BarlowCondensed-ExtraBold.ttf" => FONT_BARLOW_CONDENSED_EXTRABOLD,
        "OverpassMono.ttf" => FONT_OVERPASS_MONO,
        _ => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "font/ttf"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

async fn handle_asset(
    State(st): State<RemoteRouterState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
    Path((category, id_part)): Path<(String, String)>,
) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    let expected = st.token.read().unwrap().clone();
    if !check_token(&headers, q.token.as_deref(), &expected) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let id_str = id_part.split('.').next().unwrap_or("");
    let Ok(id) = id_str.parse::<i64>() else {
        return (StatusCode::BAD_REQUEST, "Invalid asset id").into_response();
    };

    let Some((mime, bytes)) = st.app.fetch_asset_bytes(&category, id).await else {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

async fn handle_get_state(
    State(st): State<RemoteRouterState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "Forbidden"}))).into_response();
    }
    let expected = st.token.read().unwrap().clone();
    if !check_token(&headers, q.token.as_deref(), &expected) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": st.app.lang().pick(
                "Unauthorized — scan QR code again",
                "Non autorisé — scannez à nouveau le QR code",
            )})),
        )
            .into_response();
    }

    let mut alert_guard = st.alerts.write().unwrap();
    let snap = extract_remote_state(&st.app, &mut alert_guard);
    (StatusCode::OK, Json(snap)).into_response()
}

async fn handle_post_action(
    State(st): State<RemoteRouterState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
    Json(cmd): Json<RemoteCommand>,
) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, Json(json!({"error": "Forbidden"}))).into_response();
    }
    let lang = st.app.lang();
    let expected = st.token.read().unwrap().clone();
    if !check_token(&headers, q.token.as_deref(), &expected) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": lang.pick(
                "Session expired — scan QR code again",
                "Session expirée — scannez à nouveau le QR code",
            )})),
        )
            .into_response();
    }

    if let Err(err) = st.app.execute_remote_command(&st.action_client, &cmd).await {
        let status = match err {
            RemoteActionError::BadRequest(_) => StatusCode::BAD_REQUEST,
            RemoteActionError::ClientUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            RemoteActionError::GatewayError(_) => StatusCode::BAD_GATEWAY,
        };
        return (status, Json(json!({ "error": err.to_string() }))).into_response();
    }

    let mut alert_guard = st.alerts.write().unwrap();
    let snap = extract_remote_state(&st.app, &mut alert_guard);
    drop(alert_guard);

    if let Ok(serialized) = serde_json::to_string(&snap) {
        let _ = st.state_tx.send(serialized);
    }

    (StatusCode::OK, Json(json!({ "ok": true, "state": snap }))).into_response()
}

async fn handle_ws(
    State(st): State<RemoteRouterState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if !check_host_and_origin(&headers, &st.allowed_hosts, st.port) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    let expected = st.token.read().unwrap().clone();
    if !check_token(&headers, q.token.as_deref(), &expected) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    ws.on_upgrade(move |socket| ws_session(socket, st))
}

/// Counts a paired phone for as long as its WebSocket session lives.
struct ClientGuard {
    count: Arc<AtomicUsize>,
    signal: crate::signal::ChangeSignal,
}

impl ClientGuard {
    fn new(st: &RemoteRouterState) -> Self {
        st.active_clients.fetch_add(1, Ordering::Relaxed);
        st.app.signal.bump();
        Self {
            count: st.active_clients.clone(),
            signal: st.app.signal.clone(),
        }
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::Relaxed);
        self.signal.bump();
    }
}

async fn ws_session(mut socket: WebSocket, st: RemoteRouterState) {
    let _guard = ClientGuard::new(&st);

    let initial = current_snapshot_json(&st);
    if socket.send(Message::Text(initial.into())).await.is_err() {
        return;
    }

    let mut rx = st.state_tx.subscribe();
    loop {
        tokio::select! {
            Ok(msg) = rx.recv() => {
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qr_code_renders_rgba_and_terminal() {
        let tok = generate_pairing_token();
        assert_eq!(tok.len(), 32);
        let url = format!("http://192.168.1.42:7879/?token={tok}");
        let qr = render_qr_code(&url, &tok).unwrap();
        assert!(qr.width > 64);
        assert_eq!(qr.width, qr.height);
        assert_eq!(qr.rgba.len() as u32, qr.width * qr.height * 4);
        assert!(!qr.terminal.is_empty());
    }

    #[tokio::test]
    async fn test_remote_server_auth_and_demo_actions() {
        let app = AppState::start_with_lang(true, 2.0, hypatia_shared::Lang::En);
        let app_ref = app.clone();
        let handle = start_remote_server(app, 0).await.unwrap();
        let qr = handle.qr_visual().unwrap();
        let client = reqwest::Client::new();

        // 1. Unauthorized request without token fails with 401
        let unauth = client
            .get(format!("http://127.0.0.1:{}/api/state", handle.port))
            .send()
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

        // 2. Authorized request with token returns state and reflects live language switches
        let state_resp = client
            .get(format!("http://127.0.0.1:{}/api/state", handle.port))
            .header("X-Hypatia-Token", &qr.token)
            .send()
            .await
            .unwrap();
        assert_eq!(state_resp.status(), StatusCode::OK);
        let state_en: serde_json::Value = state_resp.json().await.unwrap();
        assert_eq!(state_en["lang"], "en");

        app_ref.set_lang(hypatia_shared::Lang::Fr);
        let state_fr: serde_json::Value = client
            .get(format!("http://127.0.0.1:{}/api/state", handle.port))
            .header("X-Hypatia-Token", &qr.token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(state_fr["lang"], "fr");

        // 3. Jump to ChampSelect and hover + lock a champion and request a role swap
        let set_phase = client
            .post(format!("http://127.0.0.1:{}/api/action", handle.port))
            .header("X-Hypatia-Token", &qr.token)
            .json(&RemoteCommand::DemoSetPhase {
                phase: "ChampSelect".to_string(),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(set_phase.status(), StatusCode::OK);

        let hover_resp = client
            .post(format!("http://127.0.0.1:{}/api/action", handle.port))
            .header("X-Hypatia-Token", &qr.token)
            .json(&RemoteCommand::HoverChampion {
                action_id: 10,
                champion_id: 222,
            })
            .send()
            .await
            .unwrap();
        assert_eq!(hover_resp.status(), StatusCode::OK);

        // 4. Rotating the token invalidates the old token
        let new_qr = handle.rotate_token().unwrap();
        assert_ne!(new_qr.token, qr.token);
        let old_tok_resp = client
            .get(format!("http://127.0.0.1:{}/api/state", handle.port))
            .header("X-Hypatia-Token", &qr.token)
            .send()
            .await
            .unwrap();
        assert_eq!(old_tok_resp.status(), StatusCode::UNAUTHORIZED);

        // 5. Font assets are served cleanly from /fonts/*
        let font_resp = client
            .get(format!(
                "http://127.0.0.1:{}/fonts/BarlowCondensed-Bold.ttf",
                handle.port
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(font_resp.status(), StatusCode::OK);
        assert!(font_resp.bytes().await.unwrap().len() > 10_000);

        // 6. Demo switch to InProgress populates live_game scoreboard & feed on Mobile Remote
        let live_phase = client
            .post(format!("http://127.0.0.1:{}/api/action", handle.port))
            .header("X-Hypatia-Token", &new_qr.token)
            .json(&RemoteCommand::DemoSetPhase {
                phase: "InProgress".to_string(),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(live_phase.status(), StatusCode::OK);
        let live_body: serde_json::Value = live_phase.json().await.unwrap();
        assert_eq!(live_body["state"]["phase"], "InProgress");
        assert!(live_body["state"]["live_game"]["clock"].is_string());
        assert_eq!(
            live_body["state"]["live_game"]["allies"]
                .as_array()
                .map(|a| a.len()),
            Some(5)
        );
    }

    #[tokio::test]
    async fn test_remote_assets_require_auth_and_serve_cached_bitmaps() {
        let dir = std::env::temp_dir().join(format!("hypatia_remote_assets_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("champion")).unwrap();
        let fake_png = b"\x89PNG\r\n\x1a\nTESTCHAMP".to_vec();
        std::fs::write(dir.join("champion").join("103.png"), &fake_png).unwrap();

        let mut app = AppState::start(true, 2.0);
        app.assets = Arc::new(crate::assets::GameAssetCache::with_dir(dir.clone()));

        let handle = start_remote_server(app, 0).await.unwrap();
        let qr = handle.qr_visual().unwrap();
        let client = reqwest::Client::new();

        // Missing auth -> 401
        let no_auth = client
            .get(format!("http://127.0.0.1:{}/assets/champion/103.png", handle.port))
            .send()
            .await
            .unwrap();
        assert_eq!(no_auth.status(), StatusCode::UNAUTHORIZED);

        // Authorized request for cached champion portrait -> 200 PNG
        let hit = client
            .get(format!(
                "http://127.0.0.1:{}/assets/champion/103.png?token={}",
                handle.port, qr.token
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(hit.status(), StatusCode::OK);
        assert_eq!(hit.headers().get(reqwest::header::CONTENT_TYPE).unwrap(), "image/png");
        assert_eq!(hit.bytes().await.unwrap().as_ref(), fake_png.as_slice());

        // Authorized request for unknown/uncached champion -> 404
        let miss = client
            .get(format!(
                "http://127.0.0.1:{}/assets/champion/999999.png?token={}",
                handle.port, qr.token
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(miss.status(), StatusCode::NOT_FOUND);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_remote_ui_html_has_no_external_cdn_links() {
        assert!(!REMOTE_UI_HTML.contains("ddragon."));
        assert!(!REMOTE_UI_HTML.contains("cdragon."));
        assert!(!REMOTE_UI_HTML.contains("https://"));
        assert!(!REMOTE_UI_HTML.contains("http://"));
        assert!(REMOTE_UI_HTML.contains("/assets/"));
    }
}
