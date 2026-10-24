use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use hypatia_shared::{EventKind, ServerMsg, SourceId, SourceStatus};
use tokio::sync::broadcast;
use tracing::{debug, info, trace};

use crate::catalog::CatalogStore;
use crate::discovery::DiscoveryManager;
use crate::redaction::SecretVault;
use crate::riot_http::ReadOnlyRiotClient;
use crate::store::EventStore;

/// Spawns the Live Client Data polling task.
///
/// Invariants:
/// - If `League of Legends.exe` is absent: state is `Offline`, 0 HTTP requests are made (0% CPU).
/// - If `League of Legends.exe` is running: polls `/liveclientdata/allgamedata` at 1 Hz and records `EventKind::Poll`.
/// - Polls `/liveclientdata/eventdata` and produces an `EventKind::Create` event for each new `EventID`.
pub fn spawn_live_client_poller(
    discovery: Arc<DiscoveryManager>,
    store: Arc<RwLock<EventStore>>,
    catalog: Arc<RwLock<CatalogStore>>,
    vault: SecretVault,
    client: ReadOnlyRiotClient,
    ws_broadcast_tx: broadcast::Sender<ServerMsg>,
) {
    tokio::spawn(async move {
        info!("Starting Live Client Data poller (port 2999)...");

        let mut seen_event_ids: HashSet<i64> = HashSet::new();
        let mut was_running = false;

        loop {
            // Check if League of Legends.exe is running
            let is_game_running = discovery
                .get_source(SourceId::LiveGame)
                .map(|s| s.status == SourceStatus::Live)
                .unwrap_or(false);

            if !is_game_running {
                if was_running {
                    info!("League of Legends.exe terminated: stopping Live Client Data polling (0% CPU)");
                    seen_event_ids.clear();
                    was_running = false;
                }
                tokio::time::sleep(Duration::from_millis(1000)).await;
                continue;
            }

            if !was_running {
                info!("League of Legends.exe detected: starting Live Client Data polling at 1 Hz");
                was_running = true;
            }

            // 1. Poll /liveclientdata/allgamedata at 1 Hz
            match client.get(2999, None, "/liveclientdata/allgamedata").await {
                Ok((status, _, body)) if status.is_success() => {
                    let size = serde_json::to_vec(&body).map(|v| v.len() as u64).unwrap_or(0);

                    {
                        let mut store_guard = store.write().unwrap();
                        store_guard.insert_event(
                            SourceId::LiveGame,
                            EventKind::Poll,
                            "/liveclientdata/allgamedata".to_string(),
                            body,
                            &vault,
                        );
                    }

                    let changed = {
                        let mut cat_guard = catalog.write().unwrap();
                        cat_guard.record_observed(
                            SourceId::LiveGame,
                            "GET",
                            "/liveclientdata/allgamedata",
                            size,
                        )
                    };
                    if changed {
                        let _ = ws_broadcast_tx
                            .send(ServerMsg::CatalogChanged { source: SourceId::LiveGame });
                    }
                }
                Ok((status, _, _)) => {
                    trace!("Live Client allgamedata non-success: {}", status);
                }
                Err(e) => {
                    debug!("Live Client allgamedata connection error: {:?}", e);
                }
            }

            // 2. Poll /liveclientdata/eventdata and produce Create event for new EventIDs
            match client.get(2999, None, "/liveclientdata/eventdata").await {
                Ok((status, _, body)) if status.is_success() => {
                    if let Some(events_arr) = body.get("Events").and_then(|e| e.as_array()) {
                        for ev in events_arr {
                            if let Some(event_id) = ev.get("EventID").and_then(|id| id.as_i64()) {
                                if seen_event_ids.insert(event_id) {
                                    let size = serde_json::to_vec(ev)
                                        .map(|v| v.len() as u64)
                                        .unwrap_or(0);

                                    {
                                        let mut store_guard = store.write().unwrap();
                                        store_guard.insert_event(
                                            SourceId::LiveGame,
                                            EventKind::Create,
                                            "/liveclientdata/eventdata".to_string(),
                                            ev.clone(),
                                            &vault,
                                        );
                                    }

                                    let changed = {
                                        let mut cat_guard = catalog.write().unwrap();
                                        cat_guard.record_observed(
                                            SourceId::LiveGame,
                                            "GET",
                                            "/liveclientdata/eventdata",
                                            size,
                                        )
                                    };
                                    if changed {
                                        let _ = ws_broadcast_tx.send(ServerMsg::CatalogChanged {
                                            source: SourceId::LiveGame,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                Ok((status, _, _)) => {
                    trace!("Live Client eventdata non-success: {}", status);
                }
                Err(e) => {
                    debug!("Live Client eventdata connection error: {:?}", e);
                }
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });
}
