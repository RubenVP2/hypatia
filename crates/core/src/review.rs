//! Local Game Review capture, persistence, and LCU timeline enrichment.
//!
//! - [`LiveTraceRecorder`] passively samples `/liveclientdata/allgamedata` during Summoner's Rift
//!   games (15s snapshots + deaths with unspent gold + epic objectives + item buys).
//! - [`ReviewStore`] persists up to [`MAX_CACHED_TRACES`] (50) identity-free [`MatchTrace`]s in
//!   `reviews.json` inside Hypatia's config directory, and computes localized [`GameReview`]s on
//!   demand for both `EndOfGame` and `My Stats` (`Match inspector`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use hypatia_shared::{
    analyze_match, build_player_habit_profile, extract_raw_champion_alias, ChampionBuild,
    ChampionLookup, DeathEvent, EndOfGameSummary, EpicKind, GameReview, ItemBuyEvent, Lang,
    MatchTrace, ObjectiveEvent, PlayerChampionStats, PlayerGame, PlayerHabitProfile, Position,
    ReviewInput, TraceSample,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

use crate::assets::resolve_live_lcu_creds;
use crate::discovery::DiscoveryManager;
use crate::riot_http::ReadOnlyRiotClient;
use crate::settings::default_config_dir;
use crate::signal::ChangeSignal;
use crate::stats::lcu::load_game_timeline;

/// Maximum number of recent match traces retained in `reviews.json`.
pub const MAX_CACHED_TRACES: usize = 50;

/// Minimum interval between periodic [`TraceSample`] snapshots.
const SAMPLE_INTERVAL_SEC: u32 = 15;

#[derive(Debug, Default)]
struct RecorderInner {
    trace: MatchTrace,
    last_time_sec: u32,
    last_sample_sec: Option<u32>,
    last_deaths: u32,
    was_dead: bool,
    last_alive_gold: i64,
    seen_event_ids: HashSet<i64>,
    seen_items: HashSet<i64>,
}

impl RecorderInner {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Thread-safe passive recorder fed by the Live Client Data poller.
#[derive(Clone, Default)]
pub struct LiveTraceRecorder {
    inner: Arc<Mutex<RecorderInner>>,
}

impl LiveTraceRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingests one `/liveclientdata/allgamedata` payload. Ignores non-Summoner's Rift modes.
    pub fn record_allgamedata(&self, body: &Value, champions: &impl ChampionLookup) {
        let game_mode = body
            .pointer("/gameData/gameMode")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !game_mode.eq_ignore_ascii_case("CLASSIC") {
            return;
        }
        let time_sec = body
            .pointer("/gameData/gameTime")
            .and_then(Value::as_f64)
            .map(|t| t.max(0.0) as u32)
            .unwrap_or(0);

        let active = body.get("activePlayer");
        let active_riot_id = active
            .and_then(|a| a.get("riotId").or_else(|| a.get("summonerName")))
            .and_then(Value::as_str)
            .unwrap_or("");
        let active_short = active
            .and_then(|a| a.get("riotIdGameName").or_else(|| a.get("summonerName")))
            .and_then(Value::as_str)
            .unwrap_or(active_riot_id);
        let current_gold = active
            .and_then(|a| a.get("currentGold"))
            .and_then(Value::as_f64)
            .map(|g| g.round() as i64)
            .unwrap_or(0);
        let level = active
            .and_then(|a| a.get("level"))
            .and_then(Value::as_u64)
            .unwrap_or(1) as u32;

        let players = match body.get("allPlayers").and_then(Value::as_array) {
            Some(p) if !p.is_empty() => p,
            _ => return,
        };

        let self_player = players.iter().find(|p| {
            let rid = p.get("riotId").and_then(Value::as_str).unwrap_or("");
            let sname = p.get("summonerName").and_then(Value::as_str).unwrap_or("");
            let gname = p
                .get("riotIdGameName")
                .and_then(Value::as_str)
                .unwrap_or("");
            (!active_riot_id.is_empty() && (rid == active_riot_id || sname == active_riot_id))
                || (!active_short.is_empty() && (gname == active_short || sname == active_short))
        });
        let Some(me) = self_player else {
            return;
        };

        let my_team = me.get("team").and_then(Value::as_str).unwrap_or("");
        let position = me
            .get("position")
            .and_then(Value::as_str)
            .and_then(Position::parse);
        let champ_name = me.get("championName").and_then(Value::as_str).unwrap_or("");
        let raw_alias = me
            .get("rawChampionName")
            .and_then(Value::as_str)
            .map(extract_raw_champion_alias)
            .unwrap_or_default();
        let champion_id = champions.resolve_champion_id_with_alias(champ_name, &raw_alias);

        let scores = me.get("scores");
        let kills = scores
            .and_then(|s| s.get("kills"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let deaths = scores
            .and_then(|s| s.get("deaths"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let assists = scores
            .and_then(|s| s.get("assists"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let cs = scores
            .and_then(|s| s.get("creepScore"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let is_dead = me.get("isDead").and_then(Value::as_bool).unwrap_or(false);

        let lane_cs_diff = position.and_then(|pos| {
            players
                .iter()
                .find(|p| {
                    p.get("team").and_then(Value::as_str).unwrap_or("") != my_team
                        && p.get("position")
                            .and_then(Value::as_str)
                            .and_then(Position::parse)
                            == Some(pos)
                })
                .and_then(|opp| opp.pointer("/scores/creepScore").and_then(Value::as_u64))
                .map(|opp_cs| cs as i32 - opp_cs as i32)
        });

        let mut ally_names: HashSet<&str> = HashSet::new();
        for p in players {
            if p.get("team").and_then(Value::as_str).unwrap_or("") == my_team {
                for key in ["riotIdGameName", "summonerName", "riotId"] {
                    if let Some(n) = p.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                        ally_names.insert(n);
                    }
                }
            }
        }

        let mut inner = self.inner.lock().unwrap();
        // Reset when a new match starts (game clock jumps backwards significantly).
        if time_sec + 30 < inner.last_time_sec {
            inner.reset();
        }
        inner.last_time_sec = time_sec;

        if champion_id > 0 {
            inner.trace.champion_id = champion_id;
        }
        if position.is_some() {
            inner.trace.position = position;
        }

        if !is_dead {
            inner.last_alive_gold = current_gold;
        }

        // Record newly observed deaths with the gold held right before dying.
        if deaths > inner.last_deaths {
            let gold_at_death = if is_dead {
                inner.last_alive_gold.max(current_gold)
            } else {
                current_gold
            };
            inner.trace.deaths.push(DeathEvent {
                time_sec,
                unspent_gold: Some(gold_at_death),
            });
            inner.last_deaths = deaths;
        }
        inner.was_dead = is_dead;

        // Periodic sample every SAMPLE_INTERVAL_SEC.
        let due_sample = inner
            .last_sample_sec
            .is_none_or(|prev| time_sec >= prev.saturating_add(SAMPLE_INTERVAL_SEC));
        if due_sample && time_sec >= 60 {
            inner.trace.samples.push(TraceSample {
                time_sec,
                level,
                cs,
                lane_cs_diff,
                current_gold,
                kills,
                deaths,
                assists,
            });
            inner.last_sample_sec = Some(time_sec);
        }

        // Item buys.
        if let Some(items) = me.get("items").and_then(Value::as_array) {
            for item in items {
                let id = item.get("itemID").and_then(Value::as_i64).unwrap_or(0);
                let price = item.get("price").and_then(Value::as_i64).unwrap_or(0);
                if id > 0 && price >= 900 && inner.seen_items.insert(id) {
                    inner.trace.item_buys.push(ItemBuyEvent {
                        time_sec,
                        item_id: id,
                    });
                }
            }
        }

        // Epic objective events.
        if let Some(events) = body.pointer("/events/Events").and_then(Value::as_array) {
            for ev in events {
                let Some(ev_id) = ev.get("EventID").and_then(Value::as_i64) else {
                    continue;
                };
                if !inner.seen_event_ids.insert(ev_id) {
                    continue;
                }
                let ev_name = ev.get("EventName").and_then(Value::as_str).unwrap_or("");
                let kind = match ev_name {
                    "DragonKill" => {
                        if ev
                            .get("DragonType")
                            .and_then(Value::as_str)
                            .is_some_and(|d| d.eq_ignore_ascii_case("Elder"))
                        {
                            EpicKind::Elder
                        } else {
                            EpicKind::Dragon
                        }
                    }
                    "HeraldKill" => EpicKind::Herald,
                    "HordeKill" => EpicKind::Grubs,
                    "BaronKill" => EpicKind::Baron,
                    _ => continue,
                };
                let ev_time = ev
                    .get("EventTime")
                    .and_then(Value::as_f64)
                    .map(|t| t.max(0.0) as u32)
                    .unwrap_or(time_sec);
                let killer = ev.get("KillerName").and_then(Value::as_str).unwrap_or("");
                let assisters: Vec<&str> = ev
                    .get("Assisters")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();

                let by_ally = !killer.is_empty() && ally_names.contains(killer);
                let is_self_name =
                    |n: &str| !n.is_empty() && (n == active_short || n == active_riot_id);
                let player_involved =
                    is_self_name(killer) || assisters.iter().any(|&a| is_self_name(a));

                inner.trace.objectives.push(ObjectiveEvent {
                    time_sec: ev_time,
                    kind,
                    by_ally,
                    player_involved,
                });
            }
        }
    }

    /// Returns a snapshot of the recorded trace when it contains timeline data.
    pub fn snapshot_trace(&self) -> Option<MatchTrace> {
        let inner = self.inner.lock().unwrap();
        if inner.trace.has_timeline_data() {
            Some(inner.trace.clone())
        } else {
            None
        }
    }

    pub fn reset(&self) {
        self.inner.lock().unwrap().reset();
    }
}

/// On-disk JSON structure for `reviews.json` (identity-free).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedTraces {
    version: u32,
    entries: Vec<CachedTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedTrace {
    game_id: i64,
    trace: MatchTrace,
}

/// Persistent local store of recent match traces + on-demand Layer 1 review computation.
#[derive(Clone)]
pub struct ReviewStore {
    recorder: LiveTraceRecorder,
    traces: Arc<RwLock<Vec<CachedTrace>>>,
    path: Option<PathBuf>,
    discovery: Arc<DiscoveryManager>,
    client: ReadOnlyRiotClient,
    signal: ChangeSignal,
    in_flight: Arc<Mutex<HashSet<i64>>>,
    attempted_timelines: Arc<Mutex<HashSet<i64>>>,
}

impl ReviewStore {
    pub fn new(
        discovery: Arc<DiscoveryManager>,
        client: ReadOnlyRiotClient,
        signal: ChangeSignal,
        path: Option<PathBuf>,
    ) -> Self {
        let loaded = path.as_deref().map(load_traces_from).unwrap_or_default();
        Self {
            recorder: LiveTraceRecorder::new(),
            traces: Arc::new(RwLock::new(loaded)),
            path,
            discovery,
            client,
            signal,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            attempted_timelines: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn default_path() -> PathBuf {
        default_config_dir().join("reviews.json")
    }

    pub fn recorder(&self) -> LiveTraceRecorder {
        self.recorder.clone()
    }

    /// Inserts or updates a trace for `game_id`, capping the cache to [`MAX_CACHED_TRACES`].
    pub fn save_trace(&self, game_id: i64, mut trace: MatchTrace) {
        if game_id <= 0 || !trace.has_timeline_data() {
            return;
        }
        trace.game_id = Some(game_id);
        let snapshot_to_save = {
            let mut list = self.traces.write().unwrap();
            if let Some(pos) = list.iter().position(|e| e.game_id == game_id) {
                list.remove(pos);
            }
            list.push(CachedTrace { game_id, trace });
            if list.len() > MAX_CACHED_TRACES {
                let excess = list.len() - MAX_CACHED_TRACES;
                list.drain(0..excess);
            }
            list.clone()
        };
        if let Some(path) = &self.path {
            save_traces_to(path, &snapshot_to_save);
        }
        self.signal.bump();
    }

    /// Returns a cached trace for `game_id`, if present.
    pub fn get_trace(&self, game_id: i64) -> Option<MatchTrace> {
        self.traces
            .read()
            .unwrap()
            .iter()
            .find(|e| e.game_id == game_id)
            .map(|e| e.trace.clone())
    }

    /// Computes the player's longitudinal [`PlayerHabitProfile`] from local match history + cached traces.
    pub fn habit_profile(&self, recent_games: &[PlayerGame]) -> Option<PlayerHabitProfile> {
        if self.discovery.is_demo_override() {
            return Some(demo_habit_profile());
        }
        let traces: Vec<MatchTrace> = self
            .traces
            .read()
            .unwrap()
            .iter()
            .map(|e| e.trace.clone())
            .collect();
        build_player_habit_profile(recent_games, &traces)
    }

    /// Computes a [`GameReview`] for the active `EndOfGame` screen.
    ///
    /// Uses the recorded live trace when available (or the synthetic demo trace in demo mode).
    pub fn review_for_end_of_game(
        &self,
        eog: &EndOfGameSummary,
        game_id: i64,
        personal_champ: Option<&PlayerChampionStats>,
        recommended_build: Option<&ChampionBuild>,
        recent_games: &[PlayerGame],
        lang: Lang,
    ) -> Option<GameReview> {
        let is_demo = self.discovery.is_demo_override();
        let trace = if is_demo {
            Some(demo_trace())
        } else {
            let live = self.recorder.snapshot_trace();
            if let Some(ref t) = live {
                if game_id > 0 && self.get_trace(game_id).is_none() {
                    self.save_trace(game_id, t.clone());
                }
            }
            live.or_else(|| (game_id > 0).then(|| self.get_trace(game_id)).flatten())
        };
        let habit = self.habit_profile(recent_games);

        let input = ReviewInput::from_end_of_game(
            eog,
            game_id,
            trace.as_ref(),
            personal_champ,
            recommended_build,
            is_demo,
        )?
        .with_habit_profile(habit.as_ref());
        let rev = analyze_match(&input, lang);
        rev.has_findings().then_some(rev)
    }

    /// Computes a [`GameReview`] for a historical [`PlayerGame`] in `My Stats`.
    ///
    /// If no local trace is cached yet and the LCU is live, spawns a one-shot best-effort fetch
    /// to `/lol-match-history/v1/game-timelines/{gameId}` and bumps [`ChangeSignal`] if it lands.
    pub fn review_for_player_game(
        &self,
        game: &PlayerGame,
        personal_champ: Option<&PlayerChampionStats>,
        recommended_build: Option<&ChampionBuild>,
        recent_games: &[PlayerGame],
        lang: Lang,
    ) -> Option<GameReview> {
        let is_demo = self.discovery.is_demo_override();
        let trace = if is_demo {
            // Give the most recent demo Summoner's Rift game the rich demo trace.
            (game.game_id % 2 == 0).then(demo_trace)
        } else {
            let cached = self.get_trace(game.game_id);
            if cached.is_none() && game.game_id > 0 && game.on_summoners_rift() && !game.is_remake()
            {
                self.maybe_fetch_lcu_timeline(game.game_id, game.champion_id, game.position);
            }
            cached
        };
        let habit = self.habit_profile(recent_games);

        let input = ReviewInput::from_player_game(
            game,
            trace.as_ref(),
            personal_champ,
            recommended_build,
            is_demo,
        )?
        .with_habit_profile(habit.as_ref());
        let rev = analyze_match(&input, lang);
        rev.has_findings().then_some(rev)
    }

    fn maybe_fetch_lcu_timeline(&self, game_id: i64, champion_id: i64, position: Option<Position>) {
        {
            let mut attempted = self.attempted_timelines.lock().unwrap();
            if !attempted.insert(game_id) {
                return;
            }
        }
        {
            let mut in_flight = self.in_flight.lock().unwrap();
            if !in_flight.insert(game_id) {
                return;
            }
        }
        let Some((port, token)) = resolve_live_lcu_creds(&self.discovery) else {
            self.in_flight.lock().unwrap().remove(&game_id);
            return;
        };

        let store = self.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let fetched =
                load_game_timeline(&client, port, &token, game_id, champion_id, position).await;
            store.in_flight.lock().unwrap().remove(&game_id);
            if let Some(trace) = fetched {
                debug!("Loaded LCU timeline for game {game_id}");
                store.save_trace(game_id, trace);
            }
        });
    }
}

/// Synthetic demo [`MatchTrace`] matching `fixtures.rs`'s Ahri demo game.
pub fn demo_trace() -> MatchTrace {
    MatchTrace {
        game_id: Some(900_001),
        champion_id: 103, // Ahri
        position: Some(Position::Mid),
        samples: vec![
            TraceSample {
                time_sec: 900,
                level: 11,
                cs: 118,
                lane_cs_diff: Some(22),
                current_gold: 1850,
                kills: 4,
                deaths: 1,
                assists: 4,
            },
            TraceSample {
                time_sec: 1500,
                level: 15,
                cs: 186,
                lane_cs_diff: Some(26),
                current_gold: 950,
                kills: 7,
                deaths: 2,
                assists: 10,
            },
        ],
        deaths: vec![
            DeathEvent {
                time_sec: 1045,
                unspent_gold: Some(1950),
            },
            DeathEvent {
                time_sec: 1410,
                unspent_gold: Some(620),
            },
        ],
        objectives: vec![
            ObjectiveEvent {
                time_sec: 540,
                kind: EpicKind::Grubs,
                by_ally: true,
                player_involved: true,
            },
            ObjectiveEvent {
                time_sec: 1072,
                kind: EpicKind::Dragon,
                by_ally: false,
                player_involved: false,
            },
            ObjectiveEvent {
                time_sec: 1320,
                kind: EpicKind::Dragon,
                by_ally: true,
                player_involved: true,
            },
            ObjectiveEvent {
                time_sec: 1620,
                kind: EpicKind::Baron,
                by_ally: true,
                player_involved: true,
            },
        ],
        item_buys: vec![
            ItemBuyEvent {
                time_sec: 690,
                item_id: 3118,
            },
            ItemBuyEvent {
                time_sec: 1200,
                item_id: 4645,
            },
        ],
    }
}

/// Synthetic longitudinal [`PlayerHabitProfile`] matching `--demo` mode.
pub fn demo_habit_profile() -> PlayerHabitProfile {
    PlayerHabitProfile {
        sample_games: 14,
        wins: 8,
        losses: 6,
        win_avg_deaths: Some(2.9),
        loss_avg_deaths: Some(6.4),
        win_avg_cs_per_min: Some(7.1),
        loss_avg_cs_per_min: Some(6.0),
        win_avg_kp: Some(0.66),
        loss_avg_kp: Some(0.49),
        win_avg_vision_per_min: Some(0.84),
        loss_avg_vision_per_min: Some(0.61),
        tracked_trace_games: 5,
        pre_objective_death_games: 3,
        high_gold_death_games: 3,
        death_streak_games: 2,
        session_wins: 2,
        session_losses: 1,
        current_streak: 2,
    }
}

fn load_traces_from(path: &Path) -> Vec<CachedTrace> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    match serde_json::from_slice::<PersistedTraces>(&bytes) {
        Ok(data) => data.entries,
        Err(e) => {
            warn!("Could not parse {}: {e}", path.display());
            Vec::new()
        }
    }
}

fn save_traces_to(path: &Path, entries: &[CachedTrace]) {
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("Could not create {}: {e}", parent.display());
            return;
        }
    }
    let payload = PersistedTraces {
        version: 1,
        entries: entries.to_vec(),
    };
    let Ok(json) = serde_json::to_vec_pretty(&payload) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::SecretVault;
    use hypatia_shared::BuiltinChampionCatalog;
    use serde_json::json;

    #[test]
    fn live_trace_recorder_captures_deaths_gold_objectives_and_resets() {
        let rec = LiveTraceRecorder::new();
        let catalog = BuiltinChampionCatalog;

        let tick1 = json!({
            "gameData": {"gameMode": "CLASSIC", "gameTime": 600.0},
            "activePlayer": {"riotId": "Hero#EUW", "riotIdGameName": "Hero", "currentGold": 1820.0, "level": 9},
            "allPlayers": [
                {"riotId": "Hero#EUW", "riotIdGameName": "Hero", "team": "ORDER", "position": "MIDDLE", "championName": "Ahri", "rawChampionName": "game_character_displayname_Ahri", "isDead": false, "scores": {"kills": 2, "deaths": 0, "assists": 1, "creepScore": 80}, "items": []},
                {"riotId": "Foe#EUW", "riotIdGameName": "Foe", "team": "CHAOS", "position": "MIDDLE", "championName": "Zed", "rawChampionName": "game_character_displayname_Zed", "isDead": false, "scores": {"kills": 0, "deaths": 2, "assists": 0, "creepScore": 65}, "items": []}
            ],
            "events": {"Events": []}
        });
        rec.record_allgamedata(&tick1, &catalog);

        let tick2 = json!({
            "gameData": {"gameMode": "CLASSIC", "gameTime": 620.0},
            "activePlayer": {"riotId": "Hero#EUW", "riotIdGameName": "Hero", "currentGold": 1820.0, "level": 9},
            "allPlayers": [
                {"riotId": "Hero#EUW", "riotIdGameName": "Hero", "team": "ORDER", "position": "MIDDLE", "championName": "Ahri", "rawChampionName": "game_character_displayname_Ahri", "isDead": true, "scores": {"kills": 2, "deaths": 1, "assists": 1, "creepScore": 80}, "items": []},
                {"riotId": "Foe#EUW", "riotIdGameName": "Foe", "team": "CHAOS", "position": "MIDDLE", "championName": "Zed", "rawChampionName": "game_character_displayname_Zed", "isDead": false, "scores": {"kills": 1, "deaths": 2, "assists": 0, "creepScore": 68}, "items": []}
            ],
            "events": {"Events": [
                {"EventID": 10, "EventName": "DragonKill", "EventTime": 645.0, "KillerName": "Foe", "Assisters": [], "DragonType": "Fire"}
            ]}
        });
        rec.record_allgamedata(&tick2, &catalog);

        let trace = rec.snapshot_trace().expect("recorded trace");
        assert_eq!(trace.champion_id, 103);
        assert_eq!(trace.position, Some(Position::Mid));
        assert_eq!(trace.deaths.len(), 1);
        assert_eq!(trace.deaths[0].unspent_gold, Some(1820));
        assert_eq!(trace.objectives.len(), 1);
        assert!(!trace.objectives[0].by_ally);
        assert_eq!(trace.samples[0].lane_cs_diff, Some(15));

        // ARAM payload is ignored and does not clobber the Rift trace.
        let aram = json!({"gameData": {"gameMode": "ARAM", "gameTime": 100.0}});
        rec.record_allgamedata(&aram, &catalog);
        assert_eq!(rec.snapshot_trace().unwrap().deaths.len(), 1);
    }

    #[test]
    fn review_store_caps_at_50_and_persists_without_pii() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reviews.json");
        let signal = ChangeSignal::new();
        let discovery = DiscoveryManager::new_demo(SecretVault::new(), signal.clone());
        let store = ReviewStore::new(
            discovery.clone(),
            ReadOnlyRiotClient::new(),
            signal.clone(),
            Some(path.clone()),
        );

        for id in 1..=55 {
            store.save_trace(id, demo_trace());
        }

        // Reload from disk.
        let reloaded = ReviewStore::new(
            discovery,
            ReadOnlyRiotClient::new(),
            signal,
            Some(path.clone()),
        );
        assert!(reloaded.get_trace(1).is_none(), "oldest evicted");
        assert!(reloaded.get_trace(5).is_none(), "first 5 evicted");
        assert!(reloaded.get_trace(6).is_some());
        assert!(reloaded.get_trace(55).is_some());

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("puuid"));
        assert!(!raw.contains("summonerName"));
        assert!(!raw.contains("riotId"));
    }
}
