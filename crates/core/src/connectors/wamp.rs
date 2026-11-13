use std::borrow::Cow;
use std::time::Duration;

use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use hypatia_shared::{EventKind, SourceId};
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use super::ConnectorContext;
use crate::catalog::spawn_catalog_discovery;
use crate::store::projected_uris;

const ALL_EVENTS_TOPIC: &str = "OnJsonApiEvent";
const WAMP_SUBSCRIBE: u8 = 5;
const WAMP_UNSUBSCRIBE: u8 = 6;
const WAMP_EVENT: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WampTarget {
    pub port: u16,
    pub password: String,
}

fn wamp_target(source: SourceId, ctx: &ConnectorContext) -> Option<WampTarget> {
    if ctx.discovery.is_demo_override()
        || (source == SourceId::RiotClient && !ctx.flags.riot_client())
    {
        return None;
    }
    let (port, password) = ctx.discovery.credentials(source)?;
    Some(WampTarget { port, password })
}

/// WAMP topic carrying events for one resource, e.g.
/// `/lol-gameflow/v1/gameflow-phase` -> `OnJsonApiEvent_lol-gameflow_v1_gameflow-phase`.
pub fn topic_for_uri(uri: &str) -> String {
    format!("{ALL_EVENTS_TOPIC}{}", uri.replace('/', "_"))
}

/// `[8, "<topic>", {"uri": ..., "eventType": ..., "data": ...}]`
#[derive(Deserialize)]
struct EventFrame<'a>(
    u8,
    #[serde(borrow)] Cow<'a, str>,
    #[serde(borrow)] JsonApiEvent<'a>,
);

#[derive(Deserialize)]
struct JsonApiEvent<'a> {
    #[serde(borrow)]
    uri: Cow<'a, str>,
    #[serde(borrow, default, rename = "eventType")]
    event_type: Option<Cow<'a, str>>,
    /// Left unparsed until we know the event is worth keeping.
    #[serde(borrow, default)]
    data: Option<&'a RawValue>,
}

/// Processes one WAMP text frame from the Riot / League client.
///
/// The envelope is parsed without materialising the payload; the payload itself is only parsed
/// when the store retains it (a projected resource, or any resource while capturing).
pub fn handle_wamp_message(source: SourceId, text: &str, ctx: &ConnectorContext) {
    let Ok(EventFrame(opcode, topic, event)) = serde_json::from_str::<EventFrame>(text) else {
        return;
    };
    if opcode != WAMP_EVENT || !topic.starts_with(ALL_EVENTS_TOPIC) {
        return;
    }
    let raw = event.data.map_or("null", RawValue::get);
    tracing::trace!("{source}: {topic} {} ({} bytes)", event.uri, raw.len());

    {
        let mut store = ctx.store.write().unwrap();
        if !store.wants(source, &event.uri) {
            store.record_skipped(source, raw.len() as u64);
            return;
        }
    }

    let kind = match event.event_type.as_deref() {
        Some(t) if t.eq_ignore_ascii_case("create") => EventKind::Create,
        Some(t) if t.eq_ignore_ascii_case("delete") => EventKind::Delete,
        _ => EventKind::Update,
    };
    let data: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let uri = event.uri.into_owned();
    let meta = ctx
        .store
        .write()
        .unwrap()
        .insert_event(source, kind, uri, data, &ctx.vault);
    ctx.observe(source, &meta.uri, meta.size as u64);
}

/// Seeds the store with read-only GET snapshots of every projected resource.
async fn bootstrap_source_state(source: SourceId, target: &WampTarget, ctx: &ConnectorContext) {
    for path in projected_uris(source) {
        match ctx.client.get(target.port, Some(&target.password), path).await {
            Ok((status, _, body)) if status.is_success() => {
                tracing::debug!("{source}: bootstrapped {path}");
                ctx.store.write().unwrap().insert_event(
                    source,
                    EventKind::Update,
                    (*path).to_string(),
                    body,
                    &ctx.vault,
                );
            }
            Ok((status, _, _)) => tracing::debug!("{source}: bootstrap {path} returned {status}"),
            Err(e) => tracing::debug!("{source}: bootstrap {path} failed: {e:?}"),
        }
    }
}

type WampStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn send_wamp(ws: &mut WampStream, opcode: u8, topic: &str) -> anyhow::Result<()> {
    let frame = serde_json::json!([opcode, topic]).to_string();
    ws.send(Message::Text(frame.into())).await?;
    Ok(())
}

/// Subscribes either to the complete event stream (`full`) or only to the projected resources,
/// undoing the previous subscription set (`previous`) first.
async fn switch_subscription(
    ws: &mut WampStream,
    source: SourceId,
    previous: Option<bool>,
    full: bool,
) -> anyhow::Result<()> {
    let focused: Vec<String> = projected_uris(source).iter().map(|u| topic_for_uri(u)).collect();
    match previous {
        Some(true) => send_wamp(ws, WAMP_UNSUBSCRIBE, ALL_EVENTS_TOPIC).await?,
        Some(false) => {
            for topic in &focused {
                send_wamp(ws, WAMP_UNSUBSCRIBE, topic).await?;
            }
        }
        None => {}
    }
    if full {
        send_wamp(ws, WAMP_SUBSCRIBE, ALL_EVENTS_TOPIC).await?;
    } else {
        for topic in &focused {
            send_wamp(ws, WAMP_SUBSCRIBE, topic).await?;
        }
    }
    Ok(())
}

/// Connects to a WAMP server and processes messages until disconnection or target change.
async fn connect_and_listen(
    source: SourceId,
    target: &WampTarget,
    ctx: &ConnectorContext,
) -> anyhow::Result<()> {
    let mut builder = native_tls::TlsConnector::builder();
    builder.danger_accept_invalid_certs(true);
    let connector = tokio_tungstenite::Connector::NativeTls(builder.build()?);

    let mut req = format!("wss://127.0.0.1:{}/", target.port).into_client_request()?;
    let auth = format!(
        "Basic {}",
        BASE64_STANDARD.encode(format!("riot:{}", target.password))
    );
    req.headers_mut().insert("Authorization", auth.parse()?);
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", "wamp".parse()?);

    let (mut ws, _) =
        tokio_tungstenite::connect_async_tls_with_config(req, None, false, Some(connector)).await?;
    tracing::info!("{source}: connected to WAMP WebSocket on port {}", target.port);

    // Subscribe before bootstrapping so no update can slip between the two.
    let mut full_stream: Option<bool> = None;
    let mut housekeeping = tokio::time::interval(Duration::from_millis(1000));

    loop {
        tokio::select! {
            _ = housekeeping.tick() => {
                if wamp_target(source, ctx).as_ref() != Some(target) {
                    tracing::info!("{source}: WAMP target changed or disappeared, disconnecting");
                    return Ok(());
                }
                // Telemetry Lab open: full firehose. Closed: only what the projection reads.
                let want_full = ctx.flags.telemetry();
                if full_stream != Some(want_full) {
                    switch_subscription(&mut ws, source, full_stream, want_full).await?;
                    if full_stream.is_none() {
                        bootstrap_source_state(source, target, ctx).await;
                    }
                    full_stream = Some(want_full);
                }
                if ctx.flags.endpoint_catalog() {
                    spawn_catalog_discovery(
                        source,
                        target.port,
                        Some(target.password.clone()),
                        ctx.catalog.clone(),
                        ctx.client.clone(),
                        ctx.signal.clone(),
                    );
                }
            }
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => handle_wamp_message(source, &text, ctx),
                Some(Ok(Message::Ping(payload))) => {
                    let _ = ws.send(Message::Pong(payload)).await;
                }
                Some(Ok(Message::Close(_))) => {
                    tracing::info!("{source}: WAMP connection closed by server");
                    return Ok(());
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => anyhow::bail!("WebSocket error: {e:?}"),
                None => anyhow::bail!("WebSocket stream terminated unexpectedly"),
            }
        }
    }
}

/// Spawns the WAMP connector for `source`, reconnecting with exponential backoff.
pub fn spawn_wamp_connector(source: SourceId, ctx: ConnectorContext) -> tokio::task::JoinHandle<()> {
    const INITIAL_BACKOFF: Duration = Duration::from_millis(500);
    tokio::spawn(async move {
        let mut backoff = INITIAL_BACKOFF;
        let mut last_target: Option<WampTarget> = None;

        loop {
            let Some(target) = wamp_target(source, &ctx) else {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                continue;
            };
            if last_target.as_ref() != Some(&target) {
                backoff = INITIAL_BACKOFF;
                last_target = Some(target.clone());
            }

            match connect_and_listen(source, &target, &ctx).await {
                Ok(()) => backoff = INITIAL_BACKOFF,
                Err(e) => {
                    tracing::debug!(
                        "{source}: WAMP failure (port {}): {e:?}. Next retry in {backoff:?}",
                        target.port
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(10));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};

    use serde_json::json;

    use super::*;
    use crate::catalog::CatalogStore;
    use crate::discovery::DiscoveryManager;
    use crate::redaction::SecretVault;
    use crate::riot_http::ReadOnlyRiotClient;
    use crate::settings::{Flags, Settings};
    use crate::signal::ChangeSignal;
    use crate::store::EventStore;

    fn context(telemetry: bool) -> ConnectorContext {
        let signal = ChangeSignal::new();
        let vault = SecretVault::new();
        let flags = Arc::new(Flags::new(&Settings::default()));
        flags.set_telemetry(telemetry);
        let mut store = EventStore::new(signal.clone());
        store.set_capture(telemetry);
        ConnectorContext {
            discovery: DiscoveryManager::new_demo(vault.clone(), signal.clone()),
            store: Arc::new(RwLock::new(store)),
            catalog: Arc::new(RwLock::new(CatalogStore::new())),
            vault,
            client: ReadOnlyRiotClient::new(),
            flags,
            signal,
        }
    }

    fn frame(topic: &str, uri: &str, event_type: &str, data: Value) -> String {
        json!([8, topic, { "data": data, "eventType": event_type, "uri": uri }]).to_string()
    }

    #[test]
    fn captures_and_redacts_events_while_telemetry_is_open() {
        let ctx = context(true);
        let msg = frame(
            "OnJsonApiEvent",
            "/chat/v4/presences",
            "Update",
            json!({ "presence": "chat", "token": "secret_12345" }),
        );
        handle_wamp_message(SourceId::RiotClient, &msg, &ctx);

        let store = ctx.store.read().unwrap();
        let evt = store.get_event(1).expect("event 1 should exist");
        assert_eq!(evt.meta.source, SourceId::RiotClient);
        assert_eq!(evt.meta.uri, "/chat/v4/presences");
        assert_eq!(evt.meta.kind, EventKind::Update);
        assert!(evt.meta.has_secrets);
        assert!(evt.payload["token"].get("$secret").is_some());
        assert_eq!(ctx.catalog.read().unwrap().len(), 1);
    }

    #[test]
    fn ignores_frames_that_are_not_json_api_events() {
        let ctx = context(true);
        for msg in [
            json!([5, "OnJsonApiEvent"]).to_string(),
            json!([8, "OtherEvent", { "uri": "/test" }]).to_string(),
            "not json".to_string(),
            String::new(),
        ] {
            handle_wamp_message(SourceId::LeagueClient, &msg, &ctx);
        }
        assert!(ctx.store.read().unwrap().is_empty());

        let msg = frame("OnJsonApiEvent", "/lol-lobby/v2/lobby", "Create", json!({ "id": 123 }));
        handle_wamp_message(SourceId::LeagueClient, &msg, &ctx);
        let store = ctx.store.read().unwrap();
        assert_eq!(store.get_event(1).unwrap().meta.kind, EventKind::Create);
    }

    #[test]
    fn projection_only_mode_keeps_projected_resources_and_skips_the_rest() {
        let ctx = context(false);
        let topic = topic_for_uri("/lol-gameflow/v1/gameflow-phase");
        assert_eq!(topic, "OnJsonApiEvent_lol-gameflow_v1_gameflow-phase");

        let phase = frame(&topic, "/lol-gameflow/v1/gameflow-phase", "Update", json!("Lobby"));
        handle_wamp_message(SourceId::LeagueClient, &phase, &ctx);
        let chatter = frame("OnJsonApiEvent", "/lol-chat/v1/me", "Update", json!({ "a": 1 }));
        handle_wamp_message(SourceId::LeagueClient, &chatter, &ctx);

        let store = ctx.store.read().unwrap();
        assert_eq!(store.phase().unwrap().phase, "Lobby");
        assert!(store.get_state(SourceId::LeagueClient, "/lol-chat/v1/me").is_none());
        assert_eq!(store.source_stats(SourceId::LeagueClient).0, 2);
        assert!(store.is_empty());
        assert!(ctx.catalog.read().unwrap().is_empty());
    }
}
