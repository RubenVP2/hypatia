use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use hypatia_shared::{EventMeta, ServerMsg, SourceState};
use tokio::sync::{broadcast, mpsc};

use crate::discovery::DiscoveryManager;
use crate::store::EventStore;

/// Spawns the background batcher task running every 100 ms:
/// - Aggregates newly arrived events since the previous tick
/// - Broadcasts `ServerMsg::Batch` if events are present, or sources changed, or 1s elapsed
/// - Broadcasts `ServerMsg::Phase` whenever the game phase changes
/// - Broadcasts `ServerMsg::System` whenever a new system snapshot is available
pub fn spawn_batcher(
    mut event_rx: mpsc::UnboundedReceiver<EventMeta>,
    store: Arc<RwLock<EventStore>>,
    discovery: Arc<DiscoveryManager>,
    ws_broadcast_tx: broadcast::Sender<ServerMsg>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        let mut phase_rx = store.read().unwrap().subscribe_phase();
        let mut snapshot_rx = discovery.subscribe_snapshot();

        let mut last_batch_instant = Instant::now();
        let mut last_sources: Vec<SourceState> = Vec::new();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut events = Vec::new();
                    while let Ok(evt) = event_rx.try_recv() {
                        events.push(evt);
                    }

                    let mut current_sources = discovery.get_sources();
                    {
                        let store_guard = store.read().unwrap();
                        for s in &mut current_sources {
                            let (events_total, bytes_total) = store_guard.source_stats(s.id);
                            s.events_total = events_total;
                            s.bytes_total = bytes_total;
                        }
                    }

                    let sources_changed = current_sources != last_sources;
                    let heartbeat_elapsed = last_batch_instant.elapsed() >= Duration::from_millis(1000);

                    if !events.is_empty() || sources_changed || heartbeat_elapsed {
                        let rate = store.read().unwrap().current_rate();
                        let batch_msg = ServerMsg::Batch {
                            events,
                            sources: current_sources.clone(),
                            rate,
                        };
                        let _ = ws_broadcast_tx.send(batch_msg);
                        last_sources = current_sources;
                        last_batch_instant = Instant::now();
                    }
                }
                Ok(new_phase) = phase_rx.recv() => {
                    let _ = ws_broadcast_tx.send(ServerMsg::Phase(new_phase));
                }
                Ok(()) = snapshot_rx.changed() => {
                    let snap = snapshot_rx.borrow_and_update().clone();
                    let _ = ws_broadcast_tx.send(ServerMsg::System(snap));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_shared::{EventKind, SourceId};
    use serde_json::json;
    use crate::redaction::SecretVault;

    #[tokio::test]
    async fn test_batcher_broadcasts_events() {
        let vault = SecretVault::new();
        let (store, event_rx) = EventStore::new();
        let store = Arc::new(RwLock::new(store));
        let (discovery, _) = DiscoveryManager::new(vault.clone());

        let (ws_tx, mut ws_rx) = broadcast::channel(100);

        let _handle = spawn_batcher(event_rx, store.clone(), discovery, ws_tx);

        // Insert event into store
        {
            let mut guard = store.write().unwrap();
            guard.insert_event(
                SourceId::LeagueClient,
                EventKind::Create,
                "/lol-champ-select/v1/session".to_string(),
                json!({ "myTeam": [] }),
                &vault,
            );
        }

        // Wait for batcher to emit a Batch
        let timeout = tokio::time::sleep(Duration::from_millis(300));
        tokio::pin!(timeout);

        let mut received_batch = false;
        loop {
            tokio::select! {
                msg = ws_rx.recv() => {
                    match msg {
                        Ok(ServerMsg::Batch { events, .. }) => {
                            if !events.is_empty() {
                                assert_eq!(events[0].uri, "/lol-champ-select/v1/session");
                                assert_eq!(events[0].source, SourceId::LeagueClient);
                                received_batch = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                _ = &mut timeout => {
                    break;
                }
            }
        }

        assert!(received_batch, "Expected to receive Batch with events");
    }
}
