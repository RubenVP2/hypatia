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

/// Spawns the Replay API polling task.
///
/// Invariants:
/// - If `League of Legends.exe` is absent: 0 HTTP requests are made (0% CPU).
/// - If `League of Legends.exe` is running: polls `/replay/game` every 5 seconds.
/// - If Replay API returns an error or 404, documents the hint: `EnableReplayApi=1` in `game.cfg`.
pub fn spawn_replay_poller(
    discovery: Arc<DiscoveryManager>,
    store: Arc<RwLock<EventStore>>,
    catalog: Arc<RwLock<CatalogStore>>,
    vault: SecretVault,
    client: ReadOnlyRiotClient,
    ws_broadcast_tx: broadcast::Sender<ServerMsg>,
) {
    tokio::spawn(async move {
        info!("Starting Replay poller (port 2999)...");

        let mut logged_hint = false;

        loop {
            // Check if League of Legends.exe is running
            let is_game_running = discovery
                .get_source(SourceId::LiveGame)
                .map(|s| s.status == SourceStatus::Live)
                .unwrap_or(false);

            if !is_game_running {
                logged_hint = false;
                tokio::time::sleep(Duration::from_millis(2000)).await;
                continue;
            }

            // Probe /replay/game every 5 seconds
            match client.get(2999, None, "/replay/game").await {
                Ok((status, _, body)) if status.is_success() => {
                    let size = serde_json::to_vec(&body).map(|v| v.len() as u64).unwrap_or(0);

                    {
                        let mut store_guard = store.write().unwrap();
                        store_guard.insert_event(
                            SourceId::LiveGame,
                            EventKind::Poll,
                            "/replay/game".to_string(),
                            body,
                            &vault,
                        );
                    }

                    let changed = {
                        let mut cat_guard = catalog.write().unwrap();
                        cat_guard.record_observed(SourceId::LiveGame, "GET", "/replay/game", size)
                    };
                    if changed {
                        let _ = ws_broadcast_tx.send(ServerMsg::CatalogChanged {
                            source: SourceId::LiveGame,
                        });
                    }
                }
                Ok((status, _, _)) => {
                    if !logged_hint {
                        debug!(
                            "Replay API disabled (code {}) — enable with 'EnableReplayApi=1' in the [General] section of game.cfg",
                            status
                        );
                        logged_hint = true;
                    } else {
                        trace!("Replay API status: {}", status);
                    }
                }
                Err(e) => {
                    trace!("Replay API poll error: {:?}", e);
                }
            }

            tokio::time::sleep(Duration::from_millis(5000)).await;
        }
    });
}
