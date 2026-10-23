use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use hypatia_shared::{EventKind, EventMeta, GamePhase, SourceId, UriState};
pub use hypatia_shared::StoredEvent;
use tokio::sync::{broadcast, mpsc};

use crate::redaction::{generate_preview, redact_value, SecretVault};

pub const DEFAULT_MAX_EVENTS: usize = 10_000;



/// Circular buffer for counting events per second over the last 60 seconds.
#[derive(Debug, Clone)]
pub struct SecondRateCounter {
    buckets: [(i64, u32); 60],
}

impl Default for SecondRateCounter {
    fn default() -> Self {
        Self {
            buckets: [(0, 0); 60],
        }
    }
}

impl SecondRateCounter {
    pub fn record(&mut self, sec: i64) {
        let idx = sec.rem_euclid(60) as usize;
        if self.buckets[idx].0 == sec {
            self.buckets[idx].1 += 1;
        } else {
            self.buckets[idx] = (sec, 1);
        }
    }

    /// Returns the rate (events/second) for the given second.
    pub fn current_rate(&self, now_sec: i64) -> f32 {
        let curr_idx = now_sec.rem_euclid(60) as usize;
        let prev_idx = (now_sec - 1).rem_euclid(60) as usize;

        let curr_count = if self.buckets[curr_idx].0 == now_sec {
            self.buckets[curr_idx].1
        } else {
            0
        };
        let prev_count = if self.buckets[prev_idx].0 == (now_sec - 1) {
            self.buckets[prev_idx].1
        } else {
            0
        };

        curr_count.max(prev_count) as f32
    }

    /// Returns the 60 data points representing [now_sec - 59, ..., now_sec].
    pub fn history_60s(&self, now_sec: i64) -> Vec<u32> {
        let mut hist = Vec::with_capacity(60);
        for sec in (now_sec - 59)..=now_sec {
            let idx = sec.rem_euclid(60) as usize;
            if self.buckets[idx].0 == sec {
                hist.push(self.buckets[idx].1);
            } else {
                hist.push(0);
            }
        }
        hist
    }
}

/// Central in-memory store for all Riot events, URI states, rate counters, and phase tracking.
pub struct EventStore {
    events: VecDeque<StoredEvent>,
    max_events: usize,
    by_uri: HashMap<(SourceId, String), UriState>,
    rate_counter: SecondRateCounter,
    phase: Option<GamePhase>,
    source_stats: HashMap<SourceId, (u64, u64)>, // (events_total, bytes_total)
    next_event_id: u64,
    batcher_tx: mpsc::UnboundedSender<EventMeta>,
    phase_tx: broadcast::Sender<GamePhase>,
}

impl Default for EventStore {
    fn default() -> Self {
        let (batcher_tx, _batcher_rx) = mpsc::unbounded_channel();
        let (phase_tx, _phase_rx) = broadcast::channel(32);
        Self::with_channels(DEFAULT_MAX_EVENTS, batcher_tx, phase_tx)
    }
}

impl EventStore {
    /// Creates a new `EventStore` with default capacity and returns the store along with the batcher event receiver.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<EventMeta>) {
        let (batcher_tx, batcher_rx) = mpsc::unbounded_channel();
        let (phase_tx, _phase_rx) = broadcast::channel(32);
        (
            Self::with_channels(DEFAULT_MAX_EVENTS, batcher_tx, phase_tx),
            batcher_rx,
        )
    }

    /// Creates a store with custom capacity and channels.
    pub fn with_channels(
        max_events: usize,
        batcher_tx: mpsc::UnboundedSender<EventMeta>,
        phase_tx: broadcast::Sender<GamePhase>,
    ) -> Self {
        Self {
            events: VecDeque::with_capacity(max_events.min(1024)),
            max_events,
            by_uri: HashMap::new(),
            rate_counter: SecondRateCounter::default(),
            phase: None,
            source_stats: HashMap::new(),
            next_event_id: 1,
            batcher_tx,
            phase_tx,
        }
    }

    /// Inserts a new event into the store:
    /// - Retrieves previous payload for the same (source, URI)
    /// - Redacts sensitive data in `payload` using `vault`
    /// - Generates a single-line preview (<= 120 chars)
    /// - Stores state in `by_uri` and in the ring buffer
    /// - Updates current game phase if URI is `/lol-gameflow/v1/gameflow-phase`
    /// - Notifies the batcher channel
    pub fn insert_event(
        &mut self,
        source: SourceId,
        kind: EventKind,
        uri: String,
        mut payload: serde_json::Value,
        vault: &SecretVault,
    ) -> EventMeta {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let now_sec = now_ms / 1000;

        // 1. Retrieve previous payload for this URI if available
        let key = (source, uri.clone());
        let previous_payload = self.by_uri.get(&key).map(|s| s.current.clone());

        // 2. Redact payload
        let has_secrets = redact_value(&mut payload, vault);

        // 3. Generate preview
        let preview = generate_preview(&payload, 120);

        // 4. Calculate size
        let size = serde_json::to_vec(&payload)
            .map(|v| v.len() as u32)
            .unwrap_or(0);

        // 5. Update rate counter
        self.rate_counter.record(now_sec);

        // 6. Assign unique event id
        let id = self.next_event_id;
        self.next_event_id = self.next_event_id.wrapping_add(1);
        if self.next_event_id == 0 {
            self.next_event_id = 1;
        }

        let meta = EventMeta {
            id,
            ts: now_ms,
            source,
            kind,
            uri: uri.clone(),
            size,
            preview,
            has_secrets,
        };

        // 7. Update by_uri state
        let uri_entry = self.by_uri.entry(key).or_insert_with(|| UriState {
            current: payload.clone(),
            previous: None,
            kind,
            count: 0,
            last_ts: now_ms,
            bytes: 0,
        });
        uri_entry.previous = previous_payload.clone();
        uri_entry.current = payload.clone();
        uri_entry.kind = kind;
        uri_entry.count += 1;
        uri_entry.last_ts = now_ms;
        uri_entry.bytes = size as u64;

        // 8. Update source stats
        let stats = self.source_stats.entry(source).or_insert((0, 0));
        stats.0 += 1;
        stats.1 += size as u64;

        // 9. Update game phase if URI is gameflow-phase
        if uri == "/lol-gameflow/v1/gameflow-phase" {
            let phase_name = match &payload {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Object(map) if map.get("phase").and_then(|v| v.as_str()).is_some() => {
                    map.get("phase").unwrap().as_str().unwrap().to_string()
                }
                other => other.to_string().trim_matches('"').to_string(),
            };

            if !phase_name.is_empty() {
                let phase_changed = match &self.phase {
                    Some(current) => current.phase != phase_name,
                    None => true,
                };

                if phase_changed {
                    let new_phase = GamePhase {
                        phase: phase_name,
                        since: now_ms,
                    };
                    self.phase = Some(new_phase.clone());
                    let _ = self.phase_tx.send(new_phase);
                }
            }
        }

        // 10. Store in ring buffer with FIFO eviction
        if self.events.len() >= self.max_events {
            self.events.pop_front();
        }
        self.events.push_back(StoredEvent {
            meta: meta.clone(),
            payload,
            previous_payload,
        });

        // 11. Notify batcher
        let _ = self.batcher_tx.send(meta.clone());

        meta
    }

    /// Retrieves a stored event by ID via binary search.
    pub fn get_event(&self, id: u64) -> Option<StoredEvent> {
        self.events
            .binary_search_by_key(&id, |e| e.meta.id)
            .ok()
            .map(|idx| self.events[idx].clone())
    }

    /// Retrieves the URI state for a specific source and URI.
    pub fn get_state(&self, source: SourceId, uri: &str) -> Option<UriState> {
        self.by_uri.get(&(source, uri.to_string())).cloned()
    }

    /// Retrieves up to `limit` most recent `EventMeta` in chronological order.
    pub fn get_recent_meta(&self, limit: usize) -> Vec<EventMeta> {
        let start = self.events.len().saturating_sub(limit);
        self.events.iter().skip(start).map(|e| e.meta.clone()).collect()
    }

    /// Returns the current game phase, if known.
    pub fn phase(&self) -> Option<GamePhase> {
        self.phase.clone()
    }

    /// Sets or overrides the current game phase.
    pub fn set_phase(&mut self, phase: GamePhase) {
        self.phase = Some(phase.clone());
        let _ = self.phase_tx.send(phase);
    }

    /// Returns the current event rate (events/second).
    pub fn current_rate(&self) -> f32 {
        let now_sec = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 1000) as i64;
        self.rate_counter.current_rate(now_sec)
    }

    /// Returns the 60 data points representing rate over the last 60 seconds.
    pub fn rate_history(&self) -> Vec<u32> {
        let now_sec = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 1000) as i64;
        self.rate_counter.history_60s(now_sec)
    }

    /// Returns `(events_total, bytes_total)` for a source.
    pub fn source_stats(&self, source: SourceId) -> (u64, u64) {
        self.source_stats.get(&source).copied().unwrap_or((0, 0))
    }

    /// Subscribes to game phase changes.
    pub fn subscribe_phase(&self) -> broadcast::Receiver<GamePhase> {
        self.phase_tx.subscribe()
    }

    /// Returns the number of events currently stored.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Returns true if no events are stored.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_insert_event_and_retrieve() {
        let vault = SecretVault::new();
        let (mut store, mut rx) = EventStore::new();

        let payload1 = json!({ "name": "Summoner1", "level": 30 });
        let meta1 = store.insert_event(
            SourceId::LeagueClient,
            EventKind::Create,
            "/lol-summoner/v1/current-summoner".to_string(),
            payload1.clone(),
            &vault,
        );

        assert_eq!(meta1.id, 1);
        assert_eq!(meta1.source, SourceId::LeagueClient);
        assert_eq!(meta1.kind, EventKind::Create);
        assert!(!meta1.has_secrets);

        // Check batcher receiver received the event
        let recv_meta = rx.try_recv().expect("expected event on batcher channel");
        assert_eq!(recv_meta, meta1);

        // Check get_event
        let stored1 = store.get_event(1).expect("event 1 should exist");
        assert_eq!(stored1.meta, meta1);
        assert_eq!(stored1.payload, payload1);
        assert_eq!(stored1.previous_payload, None);

        // Insert second event on same URI
        let payload2 = json!({ "name": "Summoner1", "level": 31 });
        let meta2 = store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-summoner/v1/current-summoner".to_string(),
            payload2.clone(),
            &vault,
        );

        assert_eq!(meta2.id, 2);
        let stored2 = store.get_event(2).expect("event 2 should exist");
        assert_eq!(stored2.previous_payload, Some(payload1.clone()));
        assert_eq!(stored2.payload, payload2);

        // Check by_uri state
        let uri_state = store
            .get_state(SourceId::LeagueClient, "/lol-summoner/v1/current-summoner")
            .expect("uri state should exist");
        assert_eq!(uri_state.count, 2);
        assert_eq!(uri_state.previous, Some(payload1));
        assert_eq!(uri_state.current, payload2);
    }

    #[test]
    fn test_insert_with_secrets_redacted() {
        let vault = SecretVault::new();
        let (mut store, _rx) = EventStore::new();

        let payload = json!({
            "token": "sensitive_session_token_12345",
            "account": { "username": "player1" }
        });

        let meta = store.insert_event(
            SourceId::RiotClient,
            EventKind::Update,
            "/rso-auth/v1/authorization".to_string(),
            payload,
            &vault,
        );

        assert!(meta.has_secrets);
        let stored = store.get_event(meta.id).unwrap();
        assert!(stored.payload.get("token").unwrap().get("$secret").is_some());
    }

    #[test]
    fn test_phase_automatic_update() {
        let vault = SecretVault::new();
        let (mut store, _rx) = EventStore::new();
        let mut phase_rx = store.subscribe_phase();

        assert_eq!(store.phase(), None);

        // Insert phase string
        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-gameflow/v1/gameflow-phase".to_string(),
            json!("Lobby"),
            &vault,
        );

        let phase = store.phase().expect("phase should be set");
        assert_eq!(phase.phase, "Lobby");

        let emitted = phase_rx.try_recv().expect("phase event should be emitted");
        assert_eq!(emitted.phase, "Lobby");

        // Insert phase object
        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-gameflow/v1/gameflow-phase".to_string(),
            json!({ "phase": "ChampSelect" }),
            &vault,
        );

        let phase2 = store.phase().expect("phase should be updated");
        assert_eq!(phase2.phase, "ChampSelect");

        let emitted2 = phase_rx.try_recv().expect("phase event should be emitted");
        assert_eq!(emitted2.phase, "ChampSelect");
    }

    #[test]
    fn test_ring_buffer_fifo_eviction() {
        let vault = SecretVault::new();
        let (batcher_tx, _batcher_rx) = mpsc::unbounded_channel();
        let (phase_tx, _phase_rx) = broadcast::channel(16);
        let mut store = EventStore::with_channels(5, batcher_tx, phase_tx);

        for i in 1..=8 {
            store.insert_event(
                SourceId::LeagueClient,
                EventKind::Update,
                format!("/uri/{}", i),
                json!({ "i": i }),
                &vault,
            );
        }

        assert_eq!(store.len(), 5);
        // Events 1, 2, 3 should have been evicted
        assert!(store.get_event(1).is_none());
        assert!(store.get_event(2).is_none());
        assert!(store.get_event(3).is_none());
        // Events 4, 5, 6, 7, 8 should exist
        assert!(store.get_event(4).is_some());
        assert!(store.get_event(8).is_some());

        let recent = store.get_recent_meta(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].id, 6);
        assert_eq!(recent[1].id, 7);
        assert_eq!(recent[2].id, 8);
    }
}
