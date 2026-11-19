use std::collections::HashSet;

use hypatia_shared::{EventKind, SourceId};
use serde_json::Value;
use tracing::{debug, info, trace};

use super::ConnectorContext;

const ALLGAMEDATA: &str = "/liveclientdata/allgamedata";
const EVENTDATA: &str = "/liveclientdata/eventdata";

/// Spawns the Live Client Data polling task.
///
/// Invariants:
/// - No request is made unless `League of Legends.exe` runs and the live-game feature is on.
/// - While in game, a single `GET /liveclientdata/allgamedata` per poll interval (`1s`, `2s`, or `4s`)
///   feeds the live board; the game event list is read from that same payload.
/// - While the Telemetry Lab is open, each new game event is also recorded individually.
pub fn spawn_live_client_poller(ctx: ConnectorContext) {
    tokio::spawn(async move {
        let mut seen_event_ids: HashSet<i64> = HashSet::new();
        let mut was_running = false;

        loop {
            let period = ctx.flags.live_poll_interval();
            let running = ctx.flags.live_game() && ctx.discovery.is_live_real(SourceId::LiveGame);
            if running != was_running {
                info!(
                    "Live Client Data polling {}",
                    if running { "started" } else { "stopped" }
                );
                seen_event_ids.clear();
                was_running = running;
            }
            if !running {
                tokio::time::sleep(ctx.flags.live_idle_interval()).await;
                continue;
            }

            match ctx.client.get(2999, None, ALLGAMEDATA).await {
                Ok((status, _, body)) if status.is_success() => {
                    let new_events = take_new_events(&body, &mut seen_event_ids);
                    let capture = ctx.flags.telemetry();
                    let (poll_bytes, event_bytes) = {
                        let mut store = ctx.store.write().unwrap();
                        let meta = store.insert_event(
                            SourceId::LiveGame,
                            EventKind::Poll,
                            ALLGAMEDATA.to_string(),
                            body,
                            &ctx.vault,
                        );
                        let mut event_bytes = Vec::new();
                        if capture {
                            for ev in new_events {
                                let m = store.insert_event(
                                    SourceId::LiveGame,
                                    EventKind::Create,
                                    EVENTDATA.to_string(),
                                    ev,
                                    &ctx.vault,
                                );
                                event_bytes.push(m.size as u64);
                            }
                        }
                        (meta.size as u64, event_bytes)
                    };
                    ctx.observe(SourceId::LiveGame, ALLGAMEDATA, poll_bytes);
                    for bytes in event_bytes {
                        ctx.observe(SourceId::LiveGame, EVENTDATA, bytes);
                    }
                }
                Ok((status, _, _)) => trace!("Live Client allgamedata non-success: {status}"),
                Err(e) => debug!("Live Client allgamedata connection error: {e:?}"),
            }

            tokio::time::sleep(period).await;
        }
    });
}

/// Returns the game events of `allgamedata` not seen before, remembering their IDs.
fn take_new_events(allgamedata: &Value, seen: &mut HashSet<i64>) -> Vec<Value> {
    allgamedata
        .get("events")
        .and_then(|e| e.get("Events"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|ev| {
            ev.get("EventID")
                .and_then(Value::as_i64)
                .is_some_and(|id| seen.insert(id))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_unseen_game_events_are_reported() {
        let mut seen = HashSet::new();
        let first = json!({"events": {"Events": [{"EventID": 0, "EventName": "GameStart"}]}});
        assert_eq!(take_new_events(&first, &mut seen).len(), 1);

        let second = json!({"events": {"Events": [
            {"EventID": 0, "EventName": "GameStart"},
            {"EventID": 1, "EventName": "MinionsSpawning"},
            {"EventName": "NoId"}
        ]}});
        let fresh = take_new_events(&second, &mut seen);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0]["EventID"], 1);
        assert!(take_new_events(&json!({}), &mut seen).is_empty());
    }
}
