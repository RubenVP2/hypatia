use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub use hypatia_shared::StoredEvent;
use hypatia_shared::{EventKind, EventMeta, GamePhase, SourceId, UriState};
use serde_json::Value;

use crate::redaction::{generate_preview, json_len, redact_value, SecretVault};
use crate::signal::ChangeSignal;

pub const DEFAULT_MAX_EVENTS: usize = 10_000;
/// Budget for the serialized size of payloads retained by the event history.
pub const DEFAULT_MAX_HISTORY_BYTES: u64 = 16 * 1024 * 1024;

const GAMEFLOW_PHASE_URI: &str = "/lol-gameflow/v1/gameflow-phase";

/// LCU resources the Match Projection and Mobile Remote read. Only these are retained (and only
/// these are subscribed to over WAMP) while the Telemetry Lab is closed.
pub const LCU_PROJECTED_URIS: &[&str] = &[
    GAMEFLOW_PHASE_URI,
    "/lol-summoner/v1/current-summoner",
    "/lol-ranked/v1/current-ranked-stats",
    "/lol-lobby/v2/lobby",
    "/lol-matchmaking/v1/search",
    "/lol-matchmaking/v1/ready-check",
    "/lol-champ-select/v1/session",
    "/lol-champ-select/v1/pickable-champion-ids",
    "/lol-champ-select/v1/bannable-champion-ids",
    "/lol-champ-select/v1/skin-carousel-skins",
    "/lol-perks/v1/pages",
    "/lol-end-of-game/v1/eog-stats-block",
];
pub const RIOT_CLIENT_PROJECTED_URIS: &[&str] = &["/chat/v4/presences"];
pub const LIVE_GAME_PROJECTED_URIS: &[&str] = &[
    "/liveclientdata/allgamedata",
    "/liveclientdata/activeplayer",
    "/liveclientdata/eventdata",
];

/// URIs of `source` that feed the Match Projection.
pub fn projected_uris(source: SourceId) -> &'static [&'static str] {
    match source {
        SourceId::LeagueClient => LCU_PROJECTED_URIS,
        SourceId::RiotClient => RIOT_CLIENT_PROJECTED_URIS,
        SourceId::LiveGame => LIVE_GAME_PROJECTED_URIS,
        SourceId::System => &[],
    }
}

fn is_projected(source: SourceId, uri: &str) -> bool {
    projected_uris(source).contains(&uri)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Circular buffer counting events per second over the last 60 seconds.
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
        let bucket = &mut self.buckets[sec.rem_euclid(60) as usize];
        if bucket.0 == sec {
            bucket.1 += 1;
        } else {
            *bucket = (sec, 1);
        }
    }

    fn count_at(&self, sec: i64) -> u32 {
        let bucket = self.buckets[sec.rem_euclid(60) as usize];
        if bucket.0 == sec {
            bucket.1
        } else {
            0
        }
    }

    /// Events/second: the busier of the current (partial) and previous (complete) second.
    pub fn current_rate(&self, now_sec: i64) -> f32 {
        self.count_at(now_sec).max(self.count_at(now_sec - 1)) as f32
    }
}

/// Central in-memory store: latest payload per URI, optional event history, rate and phase.
///
/// Two retention modes:
/// - **capturing** (Telemetry Lab open): every event is redacted, previewed and appended to a
///   history bounded by both event count and payload bytes.
/// - **projection only** (default): just the latest payload of the few projected URIs is kept;
///   everything else is counted and dropped, so memory stays flat however long a session runs.
pub struct EventStore {
    events: VecDeque<StoredEvent>,
    history_bytes: u64,
    max_events: usize,
    max_history_bytes: u64,
    capture: bool,
    by_uri: [HashMap<String, UriState>; SourceId::COUNT],
    rate_counter: SecondRateCounter,
    phase: Option<GamePhase>,
    source_stats: [(u64, u64); SourceId::COUNT],
    next_event_id: u64,
    revision: u64,
    state_revision: u64,
    signal: ChangeSignal,
}

impl Default for EventStore {
    /// A capturing store with its own private change signal.
    fn default() -> Self {
        Self::new(ChangeSignal::new())
    }
}

impl EventStore {
    pub fn new(signal: ChangeSignal) -> Self {
        Self::with_limits(DEFAULT_MAX_EVENTS, DEFAULT_MAX_HISTORY_BYTES, signal)
    }

    pub fn with_limits(max_events: usize, max_history_bytes: u64, signal: ChangeSignal) -> Self {
        Self {
            events: VecDeque::new(),
            history_bytes: 0,
            max_events,
            max_history_bytes,
            capture: true,
            by_uri: Default::default(),
            rate_counter: SecondRateCounter::default(),
            phase: None,
            source_stats: [(0, 0); SourceId::COUNT],
            next_event_id: 1,
            revision: 0,
            state_revision: 0,
            signal,
        }
    }

    /// Whether the full event history is being recorded.
    pub fn capture(&self) -> bool {
        self.capture
    }

    /// Switches retention mode. Leaving capture mode frees the history and every payload the
    /// Match Projection does not need.
    pub fn set_capture(&mut self, capture: bool) {
        if self.capture == capture {
            return;
        }
        self.capture = capture;
        if !capture {
            self.events = VecDeque::new();
            self.history_bytes = 0;
            for (idx, map) in self.by_uri.iter_mut().enumerate() {
                let keep = match idx {
                    i if i == SourceId::LeagueClient.index() => LCU_PROJECTED_URIS,
                    i if i == SourceId::RiotClient.index() => RIOT_CLIENT_PROJECTED_URIS,
                    i if i == SourceId::LiveGame.index() => LIVE_GAME_PROJECTED_URIS,
                    _ => &[],
                };
                map.retain(|uri, state| {
                    state.previous = None;
                    keep.contains(&uri.as_str())
                });
                map.shrink_to_fit();
            }
        }
        self.revision += 1;
        self.signal.bump();
    }

    /// True when an event on `uri` would be retained; lets connectors skip parsing otherwise.
    pub fn wants(&self, source: SourceId, uri: &str) -> bool {
        self.capture || is_projected(source, uri)
    }

    /// Accounts for an event that was dropped before parsing (see [`Self::wants`]).
    pub fn record_skipped(&mut self, source: SourceId, bytes: u64) {
        self.rate_counter.record(now_ms() / 1000);
        let stats = &mut self.source_stats[source.index()];
        stats.0 += 1;
        stats.1 += bytes;
    }

    /// Records an event: redacts secrets, updates the per-URI state and game phase, and (when
    /// capturing) appends it to the history. Subscribers of the change signal are woken only
    /// when something they can observe actually changed.
    pub fn insert_event(
        &mut self,
        source: SourceId,
        kind: EventKind,
        uri: String,
        mut payload: Value,
        vault: &SecretVault,
    ) -> EventMeta {
        let ts = now_ms();
        let has_secrets = redact_value(&mut payload, vault);
        let size = json_len(&payload);

        self.rate_counter.record(ts / 1000);
        let stats = &mut self.source_stats[source.index()];
        stats.0 += 1;
        stats.1 += size as u64;

        let id = self.next_event_id;
        self.next_event_id = self.next_event_id.checked_add(1).unwrap_or(1);

        let projected = is_projected(source, &uri);
        let mut meta = EventMeta {
            id,
            ts,
            source,
            kind,
            uri,
            size,
            preview: String::new(),
            has_secrets,
        };
        if !self.capture && !projected {
            return meta;
        }

        let mut state_changed = uri_is_phase(source, &meta.uri) && self.update_phase(&payload, ts);

        let payload = Arc::new(payload);
        let map = &mut self.by_uri[source.index()];
        let previous = match map.get_mut(meta.uri.as_str()) {
            Some(state) => {
                state_changed |= projected && *state.current != *payload;
                let previous = std::mem::replace(&mut state.current, Arc::clone(&payload));
                state.previous = self.capture.then(|| Arc::clone(&previous));
                state.kind = kind;
                state.count += 1;
                state.last_ts = ts;
                state.bytes = size as u64;
                Some(previous)
            }
            None => {
                state_changed |= projected;
                map.insert(
                    meta.uri.clone(),
                    UriState {
                        current: Arc::clone(&payload),
                        previous: None,
                        kind,
                        count: 1,
                        last_ts: ts,
                        bytes: size as u64,
                    },
                );
                None
            }
        };

        if self.capture {
            meta.preview = generate_preview(&payload, 120);
            self.history_bytes += size as u64;
            self.events.push_back(StoredEvent {
                meta: meta.clone(),
                payload,
                previous_payload: previous,
            });
            self.evict();
            self.revision += 1;
        }
        if state_changed {
            self.state_revision += 1;
        }
        if state_changed || self.capture {
            self.signal.bump();
        }
        meta
    }

    fn evict(&mut self) {
        while self.events.len() > self.max_events
            || (self.history_bytes > self.max_history_bytes && self.events.len() > 1)
        {
            if let Some(old) = self.events.pop_front() {
                self.history_bytes -= old.meta.size as u64;
            }
        }
    }

    fn update_phase(&mut self, payload: &Value, ts: i64) -> bool {
        let name = match payload {
            Value::String(s) => s.as_str(),
            other => other.get("phase").and_then(Value::as_str).unwrap_or(""),
        };
        if name.is_empty() || self.phase.as_ref().is_some_and(|p| p.phase == name) {
            return false;
        }
        self.phase = Some(GamePhase {
            phase: name.to_string(),
            since: ts,
        });
        true
    }

    /// Retrieves a stored event by ID (history is ordered by ID).
    pub fn get_event(&self, id: u64) -> Option<StoredEvent> {
        self.events
            .binary_search_by_key(&id, |e| e.meta.id)
            .ok()
            .map(|idx| self.events[idx].clone())
    }

    /// Per-URI state (latest payload, counters) for `source` + `uri`.
    pub fn get_state(&self, source: SourceId, uri: &str) -> Option<&UriState> {
        self.by_uri[source.index()].get(uri)
    }

    /// Latest redacted payload for `source` + `uri` (a cheap reference-count clone).
    pub fn current(&self, source: SourceId, uri: &str) -> Option<Arc<Value>> {
        self.get_state(source, uri).map(|s| Arc::clone(&s.current))
    }

    /// Up to `limit` most recent `EventMeta` in chronological order.
    pub fn get_recent_meta(&self, limit: usize) -> Vec<EventMeta> {
        let start = self.events.len().saturating_sub(limit);
        self.events.range(start..).map(|e| e.meta.clone()).collect()
    }

    pub fn latest_event_id(&self) -> u64 {
        self.events.back().map_or(0, |e| e.meta.id)
    }

    pub fn phase(&self) -> Option<GamePhase> {
        self.phase.clone()
    }

    /// Sets or overrides the current game phase.
    pub fn set_phase(&mut self, phase: GamePhase) {
        if self.phase.as_ref() != Some(&phase) {
            self.phase = Some(phase);
            self.state_revision += 1;
            self.signal.bump();
        }
    }

    /// Current event rate (events/second).
    pub fn current_rate(&self) -> f32 {
        self.rate_counter.current_rate(now_ms() / 1000)
    }

    /// `(events_total, bytes_total)` for a source.
    pub fn source_stats(&self, source: SourceId) -> (u64, u64) {
        self.source_stats[source.index()]
    }

    /// Bumped whenever the event history changes (insert while capturing, clear, mode switch).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Bumped only when a projected payload or the game phase actually changed.
    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Forgets everything observed so far: history, per-URI state, phase and counters. Used when
    /// switching between synthetic Demo data and the real clients so neither leaks into the other.
    pub fn reset_state(&mut self) {
        self.events.clear();
        self.history_bytes = 0;
        for map in &mut self.by_uri {
            map.clear();
        }
        self.phase = None;
        self.source_stats = [(0, 0); SourceId::COUNT];
        self.revision += 1;
        self.state_revision += 1;
        self.signal.bump();
    }

    /// Clears the event history (per-URI state is kept).
    pub fn clear_events(&mut self) {
        self.events.clear();
        self.history_bytes = 0;
        self.revision += 1;
        self.signal.bump();
    }
}

fn uri_is_phase(source: SourceId, uri: &str) -> bool {
    source == SourceId::LeagueClient && uri == GAMEFLOW_PHASE_URI
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn insert(store: &mut EventStore, uri: &str, payload: Value) -> EventMeta {
        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            uri.to_string(),
            payload,
            &SecretVault::new(),
        )
    }

    #[test]
    fn insert_shares_payloads_between_state_and_history() {
        let mut store = EventStore::default();
        let uri = "/lol-summoner/v1/current-summoner";
        let first = json!({ "name": "Summoner1", "level": 30 });
        let second = json!({ "name": "Summoner1", "level": 31 });

        let meta1 = insert(&mut store, uri, first.clone());
        assert_eq!((meta1.id, meta1.has_secrets), (1, false));
        let stored1 = store.get_event(1).expect("event 1 should exist");
        assert_eq!(*stored1.payload, first);
        assert!(stored1.previous_payload.is_none());

        insert(&mut store, uri, second.clone());
        let stored2 = store.get_event(2).expect("event 2 should exist");
        assert_eq!(*stored2.payload, second);
        assert!(Arc::ptr_eq(
            stored2.previous_payload.as_ref().unwrap(),
            &stored1.payload
        ));

        let state = store.get_state(SourceId::LeagueClient, uri).unwrap();
        assert_eq!(state.count, 2);
        assert!(Arc::ptr_eq(&state.current, &stored2.payload));
        assert_eq!(*store.current(SourceId::LeagueClient, uri).unwrap(), second);
    }

    #[test]
    fn secrets_are_redacted_before_storage() {
        let mut store = EventStore::default();
        let meta = store.insert_event(
            SourceId::RiotClient,
            EventKind::Update,
            "/rso-auth/v1/authorization".to_string(),
            json!({ "token": "sensitive_session_token_12345", "account": { "username": "player1" } }),
            &SecretVault::new(),
        );
        assert!(meta.has_secrets);
        let stored = store.get_event(meta.id).unwrap();
        assert!(stored.payload["token"].get("$secret").is_some());
    }

    #[test]
    fn phase_follows_gameflow_events() {
        let mut store = EventStore::default();
        assert_eq!(store.phase(), None);

        insert(&mut store, GAMEFLOW_PHASE_URI, json!("Lobby"));
        assert_eq!(store.phase().unwrap().phase, "Lobby");

        insert(
            &mut store,
            GAMEFLOW_PHASE_URI,
            json!({ "phase": "ChampSelect" }),
        );
        assert_eq!(store.phase().unwrap().phase, "ChampSelect");
    }

    #[test]
    fn history_is_bounded_by_count_and_bytes() {
        let mut store = EventStore::with_limits(5, u64::MAX, ChangeSignal::new());
        for i in 1..=8 {
            insert(&mut store, &format!("/uri/{i}"), json!({ "i": i }));
        }
        assert_eq!(store.len(), 5);
        assert!(store.get_event(3).is_none());
        assert!(store.get_event(4).is_some());
        let recent: Vec<u64> = store.get_recent_meta(3).iter().map(|m| m.id).collect();
        assert_eq!(recent, [6, 7, 8]);

        let mut store = EventStore::with_limits(1000, 100, ChangeSignal::new());
        for i in 0..20 {
            insert(
                &mut store,
                "/big",
                json!({ "blob": "x".repeat(40), "i": i }),
            );
        }
        assert!(store.len() <= 2, "byte budget must evict old payloads");
        assert_eq!(store.latest_event_id(), 20);
    }

    #[test]
    fn reset_state_forgets_payloads_and_phase() {
        let mut store = EventStore::default();
        insert(
            &mut store,
            "/lol-summoner/v1/current-summoner",
            json!({ "name": "Demo" }),
        );
        insert(&mut store, GAMEFLOW_PHASE_URI, json!("InProgress"));
        let before = store.state_revision();

        store.reset_state();

        assert!(store
            .current(SourceId::LeagueClient, "/lol-summoner/v1/current-summoner")
            .is_none());
        assert!(store.phase().is_none());
        assert!(store.is_empty());
        assert!(store.state_revision() > before);
    }

    #[test]
    fn projection_only_mode_keeps_just_projected_state() {
        let signal = ChangeSignal::new();
        let mut store = EventStore::new(signal.clone());
        insert(&mut store, "/lol-chat/v1/conversations", json!([1, 2, 3]));
        insert(&mut store, "/lol-lobby/v2/lobby", json!({ "partyId": "a" }));
        store.set_capture(false);

        assert!(store.is_empty());
        assert!(store
            .get_state(SourceId::LeagueClient, "/lol-chat/v1/conversations")
            .is_none());
        assert!(store
            .current(SourceId::LeagueClient, "/lol-lobby/v2/lobby")
            .is_some());
        assert!(!store.wants(SourceId::LeagueClient, "/lol-chat/v1/me"));
        assert!(store.wants(SourceId::LeagueClient, "/lol-lobby/v2/lobby"));

        // Irrelevant traffic is counted but neither stored nor signalled.
        let (rev, state_rev) = (signal.revision(), store.state_revision());
        insert(
            &mut store,
            "/lol-chat/v1/me",
            json!({ "availability": "chat" }),
        );
        assert_eq!(store.source_stats(SourceId::LeagueClient).0, 3);
        assert!(store
            .get_state(SourceId::LeagueClient, "/lol-chat/v1/me")
            .is_none());
        assert_eq!(
            (signal.revision(), store.state_revision()),
            (rev, state_rev)
        );

        // Re-sending an identical projected payload is not a change either.
        insert(&mut store, "/lol-lobby/v2/lobby", json!({ "partyId": "a" }));
        assert_eq!(
            (signal.revision(), store.state_revision()),
            (rev, state_rev)
        );

        insert(&mut store, "/lol-lobby/v2/lobby", json!({ "partyId": "b" }));
        assert_eq!(store.state_revision(), state_rev + 1);
        assert!(signal.revision() > rev);
        assert!(store.is_empty(), "no history outside capture mode");
    }
}
