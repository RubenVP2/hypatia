use std::sync::{Arc, RwLock};
use std::time::Duration;

use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use hypatia_shared::{EventKind, SourceId};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::discovery::DiscoveryManager;
use crate::redaction::SecretVault;
use crate::store::EventStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WampTarget {
    pub port: u16,
    pub password: String,
}

/// Retrieves the connection target (port and revealed password) for a source from DiscoveryManager.
pub fn get_wamp_target(
    source_id: SourceId,
    discovery: &DiscoveryManager,
    vault: &SecretVault,
) -> Option<WampTarget> {
    let source = discovery.get_source(source_id)?;
    let port = source.port?;
    let cred = source.credential?;
    let password = vault.reveal(cred.id)?;
    Some(WampTarget { port, password })
}

/// Parses and processes an incoming WAMP message string from Riot/League Client.
pub fn handle_wamp_message(
    source: SourceId,
    text: &str,
    store: &Arc<RwLock<EventStore>>,
    vault: &SecretVault,
) {
    if text.trim().is_empty() {
        return;
    }

    let parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            tracing::trace!("{}: WAMP text non JSON: {:?} in '{}'", source, e, text);
            return;
        }
    };

    let array = match parsed.as_array() {
        Some(a) => a,
        None => {
            tracing::trace!("{}: WAMP message is not an array", source);
            return;
        }
    };

    if array.len() < 3 {
        tracing::trace!("{}: WAMP array too short: {:?}", source, array);
        return;
    }

    // Opcode must be 8 (EVENT)
    if array[0].as_i64() != Some(8) {
        tracing::trace!("{}: WAMP opcode is not 8: {:?}", source, array[0]);
        return;
    }

    let event_name = array[1].as_str().unwrap_or_default();
    if event_name != "OnJsonApiEvent" {
        tracing::trace!("{}: WAMP event is not OnJsonApiEvent: {}", source, event_name);
        return;
    }

    let payload_obj = &array[2];
    let uri = match payload_obj.get("uri").and_then(|u| u.as_str()) {
        Some(u) => u.to_string(),
        None => {
            tracing::trace!("{}: WAMP payload missing uri", source);
            return;
        }
    };

    let event_type = payload_obj
        .get("eventType")
        .and_then(|t| t.as_str())
        .unwrap_or("Update");

    let kind = match event_type.to_ascii_lowercase().as_str() {
        "create" => EventKind::Create,
        "delete" => EventKind::Delete,
        _ => EventKind::Update,
    };

    let data = payload_obj
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let mut guard = store.write().unwrap();
    guard.insert_event(source, kind, uri, data, vault);
}

/// Initial bootstrap on live connection: fetches `/lol-gameflow/v1/gameflow-phase` to seed phase.
async fn bootstrap_lcu_phase(
    port: u16,
    password: &str,
    store: &Arc<RwLock<EventStore>>,
    vault: &SecretVault,
) {
    let client = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("LCU bootstrap client build error: {:?}", e);
            return;
        }
    };

    let auth = format!("Basic {}", BASE64_STANDARD.encode(format!("riot:{}", password)));
    let url = format!("https://127.0.0.1:{}/lol-gameflow/v1/gameflow-phase", port);

    match client.get(&url).header("Authorization", auth).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json_val) = resp.json::<serde_json::Value>().await {
                tracing::info!("LCU phase initialisée via bootstrap: {:?}", json_val);
                let mut guard = store.write().unwrap();
                guard.insert_event(
                    SourceId::LeagueClient,
                    EventKind::Update,
                    "/lol-gameflow/v1/gameflow-phase".to_string(),
                    json_val,
                    vault,
                );
            }
        }
        Ok(resp) => {
            tracing::debug!("LCU bootstrap gameflow-phase returned {}", resp.status());
        }
        Err(e) => {
            tracing::debug!("LCU bootstrap gameflow-phase request failed: {:?}", e);
        }
    }
}

/// Connects to a WAMP server and processes messages until disconnection or target change.
async fn connect_and_listen(
    source_id: SourceId,
    target: &WampTarget,
    discovery: &DiscoveryManager,
    store: &Arc<RwLock<EventStore>>,
    vault: &SecretVault,
) -> anyhow::Result<()> {
    let mut builder = native_tls::TlsConnector::builder();
    builder.danger_accept_invalid_certs(true);
    let tls = builder.build()?;
    let connector = tokio_tungstenite::Connector::NativeTls(tls);

    let url = format!("wss://127.0.0.1:{}/", target.port);
    let mut req = url.into_client_request()?;
    let auth = format!("Basic {}", BASE64_STANDARD.encode(format!("riot:{}", target.password)));
    req.headers_mut().insert("Authorization", auth.parse()?);
    req.headers_mut().insert("Sec-WebSocket-Protocol", "wamp".parse()?);

    let (mut ws_stream, _) = tokio_tungstenite::connect_async_tls_with_config(
        req,
        None,
        false,
        Some(connector),
    )
    .await?;

    tracing::info!("{}: Connecté au WebSocket WAMP sur port {}", source_id, target.port);

    // Bootstrap LCU phase if connecting to LeagueClient
    if source_id == SourceId::LeagueClient {
        bootstrap_lcu_phase(target.port, &target.password, store, vault).await;
    }

    // Subscribe to all json api events: [5, "OnJsonApiEvent"]
    let sub = serde_json::json!([5, "OnJsonApiEvent"]).to_string();
    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(sub.into()))
        .await?;

    loop {
        // Periodically verify target credentials haven't changed in discovery
        let current_target = get_wamp_target(source_id, discovery, vault);
        if current_target.as_ref() != Some(target) {
            tracing::info!("{}: Cible WAMP modifiée ou disparue, déconnexion", source_id);
            break;
        }

        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        handle_wamp_message(source_id, &text, store, vault);
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(payload))) => {
                        let _ = ws_stream.send(tokio_tungstenite::tungstenite::Message::Pong(payload)).await;
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                        tracing::info!("{}: Connexion WAMP fermée par le serveur", source_id);
                        break;
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("Erreur WebSocket: {:?}", e));
                    }
                    None => {
                        return Err(anyhow::anyhow!("Flux WebSocket terminé de manière inattendue"));
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(1000)) => {
                // tick to re-verify credentials
            }
        }
    }

    Ok(())
}

/// Spawns a background task for connecting to a source's WAMP WebSocket with exponential backoff.
pub fn spawn_wamp_connector(
    source_id: SourceId,
    discovery: Arc<DiscoveryManager>,
    store: Arc<RwLock<EventStore>>,
    vault: SecretVault,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = Duration::from_millis(500);
        let mut last_target: Option<WampTarget> = None;

        loop {
            let target = get_wamp_target(source_id, &discovery, &vault);

            // If target credentials changed, reset backoff
            if target.is_some() && target != last_target {
                backoff = Duration::from_millis(500);
                last_target = target.clone();
            }

            let current_target = match target {
                Some(t) => t,
                None => {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    continue;
                }
            };

            match connect_and_listen(source_id, &current_target, &discovery, &store, &vault).await {
                Ok(()) => {
                    backoff = Duration::from_millis(500);
                }
                Err(e) => {
                    tracing::debug!(
                        "{}: Déconnexion / échec WAMP (port {}): {:?}. Prochain essai dans {:?}",
                        source_id,
                        current_target.port,
                        e,
                        backoff
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
    use super::*;
    use serde_json::json;

    #[test]
    fn test_handle_wamp_message_valid() {
        let vault = SecretVault::new();
        let (store, _rx) = EventStore::new();
        let store = Arc::new(RwLock::new(store));

        let wamp_msg = json!([
            8,
            "OnJsonApiEvent",
            {
                "data": { "presence": "chat", "token": "secret_12345" },
                "eventType": "Update",
                "uri": "/chat/v4/presences"
            }
        ])
        .to_string();

        handle_wamp_message(SourceId::RiotClient, &wamp_msg, &store, &vault);

        let guard = store.read().unwrap();
        assert_eq!(guard.len(), 1);

        let evt = guard.get_event(1).expect("event 1 should exist");
        assert_eq!(evt.meta.source, SourceId::RiotClient);
        assert_eq!(evt.meta.uri, "/chat/v4/presences");
        assert_eq!(evt.meta.kind, EventKind::Update);
        assert!(evt.meta.has_secrets);

        // Verify token in payload was masked
        assert!(evt.payload["token"].get("$secret").is_some());
    }

    #[test]
    fn test_handle_wamp_message_opcodes() {
        let vault = SecretVault::new();
        let (store, _rx) = EventStore::new();
        let store = Arc::new(RwLock::new(store));

        // Non-8 opcode should be ignored
        let subscribe_msg = json!([5, "OnJsonApiEvent"]).to_string();
        handle_wamp_message(SourceId::LeagueClient, &subscribe_msg, &store, &vault);
        assert_eq!(store.read().unwrap().len(), 0);

        // Different event name should be ignored
        let other_msg = json!([8, "OtherEvent", { "uri": "/test" }]).to_string();
        handle_wamp_message(SourceId::LeagueClient, &other_msg, &store, &vault);
        assert_eq!(store.read().unwrap().len(), 0);

        // Create event
        let create_msg = json!([
            8,
            "OnJsonApiEvent",
            {
                "data": { "id": 123 },
                "eventType": "Create",
                "uri": "/lol-lobby/v2/lobby"
            }
        ])
        .to_string();
        handle_wamp_message(SourceId::LeagueClient, &create_msg, &store, &vault);
        assert_eq!(store.read().unwrap().len(), 1);
        let evt = store.read().unwrap().get_event(1).unwrap();
        assert_eq!(evt.meta.kind, EventKind::Create);
    }
}
