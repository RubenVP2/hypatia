use futures::StreamExt;
use gloo_net::websocket::{futures::WebSocket, Message};
use gloo_timers::future::TimeoutFuture;
use hypatia_shared::ServerMsg;
use leptos::prelude::*;
use crate::state::AppState;

pub fn start_websocket(state: AppState) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut backoff_ms = 500u32;

        loop {
            let ws_url = get_ws_url();
            match WebSocket::open(&ws_url) {
                Ok(ws) => {
                    state.is_connected.set(true);
                    backoff_ms = 500;

                    let (_write, mut read) = ws.split();

                    while let Some(msg_res) = read.next().await {
                        match msg_res {
                            Ok(Message::Text(text)) => {
                                handle_server_msg(&text, &state);
                            }
                            Ok(Message::Bytes(_)) => {}
                            Err(_) => {
                                break;
                            }
                        }
                    }

                    state.is_connected.set(false);
                }
                Err(_) => {
                    state.is_connected.set(false);
                }
            }

            // Attente avant reconnexion avec backoff plafonné à 5 s
            TimeoutFuture::new(backoff_ms).await;
            backoff_ms = (backoff_ms * 2).min(5000);
        }
    });
}

fn handle_server_msg(text: &str, state: &AppState) {
    if let Ok(msg) = serde_json::from_str::<ServerMsg>(text) {
        match msg {
            ServerMsg::Hello { snapshot, is_demo, .. } => {
                state.is_demo.set(is_demo);
                state.source_store.update_sources(snapshot.sources);
                if let Some(phase) = snapshot.phase {
                    state.phase_store.set_phase(phase);
                }
                state.event_store.init_recent(snapshot.recent_events);
                state.system_store.set_snapshot(snapshot.system);
                state.is_connected.set(true);
            }
            ServerMsg::Batch { events, sources, rate } => {
                state.event_store.push_batch(events, rate);
                state.source_store.update_sources(sources);
            }
            ServerMsg::Phase(game_phase) => {
                state.phase_store.set_phase(game_phase);
            }
            ServerMsg::System(sys) => {
                state.system_store.set_snapshot(sys);
            }
            ServerMsg::CatalogChanged { .. } => {}
        }
    }
}

fn get_ws_url() -> String {
    let window = web_sys::window();
    let host = window
        .as_ref()
        .and_then(|w| w.location().host().ok())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "127.0.0.1:7878".to_string());

    let proto = window
        .as_ref()
        .and_then(|w| w.location().protocol().ok())
        .map(|p| if p == "https:" { "wss:" } else { "ws:" })
        .unwrap_or("ws:");

    format!("{proto}//{host}/ws")
}
