use std::time::Duration;

use hypatia_shared::{EventKind, SourceId};
use tracing::{debug, trace};

use super::ConnectorContext;

const REPLAY_GAME: &str = "/replay/game";

/// Spawns the Replay API probe (Telemetry Lab only, opt-in).
///
/// Invariants:
/// - No request is made unless the game runs, the Telemetry Lab is open and the Replay API
///   setting is enabled.
/// - Otherwise `/replay/game` is probed every 5 seconds; a non-success answer usually means
///   `EnableReplayApi=1` is missing from the `[General]` section of `game.cfg`.
pub fn spawn_replay_poller(ctx: ConnectorContext) {
    tokio::spawn(async move {
        let mut logged_hint = false;

        loop {
            if !(ctx.flags.replay_api() && ctx.discovery.is_live_real(SourceId::LiveGame)) {
                logged_hint = false;
                tokio::time::sleep(Duration::from_millis(2000)).await;
                continue;
            }

            match ctx.client.get(2999, None, REPLAY_GAME).await {
                Ok((status, _, body)) if status.is_success() => {
                    let meta = ctx.store.write().unwrap().insert_event(
                        SourceId::LiveGame,
                        EventKind::Poll,
                        REPLAY_GAME.to_string(),
                        body,
                        &ctx.vault,
                    );
                    ctx.observe(SourceId::LiveGame, REPLAY_GAME, meta.size as u64);
                }
                Ok((status, _, _)) if !logged_hint => {
                    debug!(
                        "Replay API disabled (code {status}) — enable with 'EnableReplayApi=1' in the [General] section of game.cfg"
                    );
                    logged_hint = true;
                }
                Ok((status, _, _)) => trace!("Replay API status: {status}"),
                Err(e) => trace!("Replay API poll error: {e:?}"),
            }

            tokio::time::sleep(Duration::from_millis(5000)).await;
        }
    });
}
