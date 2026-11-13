use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::time::Duration;

use hypatia_shared::{EndpointInfo, EndpointOrigin, EventKind, SourceId};
use serde_json::json;
use tokio::time::Instant;
use tracing::info;

use crate::catalog::{extract_namespace, CatalogStore};
use crate::redaction::SecretVault;
use crate::store::EventStore;

/// Populates the catalog with known default endpoints for demo mode.
pub fn seed_demo_catalog(catalog: &Arc<RwLock<CatalogStore>>) {
    let mut cat = catalog.write().unwrap();
    let mut endpoints = Vec::new();

    let lcu_endpoints = [
        ("/lol-gameflow/v1/gameflow-phase", "GET"),
        ("/lol-champ-select/v1/session", "GET"),
        ("/lol-lobby/v2/lobby", "GET"),
        ("/lol-matchmaking/v1/search", "GET"),
        ("/lol-summoner/v1/current-summoner", "GET"),
        ("/lol-summoner/v2/summoners/puuid/{puuid}", "GET"),
        ("/lol-chat/v1/conversations", "GET"),
        ("/lol-ranked/v1/current-ranked-stats", "GET"),
        ("/lol-ranked/v1/ranked-stats/{puuid}", "GET"),
        ("/lol-end-of-game/v1/eog-stats-block", "GET"),
    ];
    for (uri, method) in lcu_endpoints {
        endpoints.push(EndpointInfo {
            source: SourceId::LeagueClient,
            method: method.to_string(),
            path_template: uri.to_string(),
            namespace: extract_namespace(uri),
            description: None,
            origin: EndpointOrigin::Declared,
            last_seen: None,
            event_count: 0,
            bytes_total: 0,
        });
    }

    let rc_endpoints = [
        ("/chat/v4/presences", "GET"),
        ("/riotclient/region-locale", "GET"),
        ("/entitlements/v1/token", "GET"),
        ("/rso-auth/v1/authorization", "GET"),
    ];
    for (uri, method) in rc_endpoints {
        endpoints.push(EndpointInfo {
            source: SourceId::RiotClient,
            method: method.to_string(),
            path_template: uri.to_string(),
            namespace: extract_namespace(uri),
            description: None,
            origin: EndpointOrigin::Declared,
            last_seen: None,
            event_count: 0,
            bytes_total: 0,
        });
    }

    let live_endpoints = [
        ("/liveclientdata/allgamedata", "GET"),
        ("/liveclientdata/activeplayer", "GET"),
        ("/liveclientdata/eventdata", "GET"),
    ];
    for (uri, method) in live_endpoints {
        endpoints.push(EndpointInfo {
            source: SourceId::LiveGame,
            method: method.to_string(),
            path_template: uri.to_string(),
            namespace: extract_namespace(uri),
            description: None,
            origin: EndpointOrigin::Declared,
            last_seen: None,
            event_count: 0,
            bytes_total: 0,
        });
    }

    cat.add_declared(endpoints);
}

/// Locks the store for a synthetic write, unless Demo was switched off meanwhile.
///
/// The flag is read *under* the lock: `AppState::set_demo_active(false)` clears the flag and
/// then wipes the store under the same lock, so a synthetic event either lands before that wipe
/// (and is erased) or is never written. Nothing synthetic can survive Demo stopping.
fn lock_if_active<'a>(
    store: &'a RwLock<EventStore>,
    active: &AtomicBool,
) -> Option<RwLockWriteGuard<'a, EventStore>> {
    let guard = store.write().unwrap();
    active.load(Ordering::SeqCst).then_some(guard)
}

/// Endpoint carrying the demo post-game summary.
pub(crate) const DEMO_EOG_URI: &str = "/lol-end-of-game/v1/eog-stats-block";

/// Post-game stats block for the demo match (Ahri, victory).
pub(crate) fn demo_eog_stats() -> serde_json::Value {
    json!({
        "gameId": 894512301,
        "gameLength": 1694,
        "gameMode": "CLASSIC",
        "gameResult": "Victory",
        "localPlayer": {
            "championName": "Ahri",
            "stats": {
                "CHAMPIONS_KILLED": 9,
                "NUM_DEATHS": 2,
                "ASSISTS": 14,
                "GOLD_EARNED": 14950,
                "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 34820
            }
        }
    })
}

/// A pinned draft never advances on its own: once its countdown runs out, start it again
/// (alternating 30 s / 29 s so the projection sees a new timer and re-anchors its clock).
fn restart_demo_draft_timer(
    store: &RwLock<EventStore>,
    active: &AtomicBool,
    vault: &SecretVault,
    left_ms: u64,
) {
    const URI: &str = "/lol-champ-select/v1/session";
    let Some(mut s) = lock_if_active(store, active) else {
        return;
    };
    let Some(mut session) = s.current(SourceId::LeagueClient, URI).map(|v| (*v).clone()) else {
        return;
    };
    if let Some(timer) = session.get_mut("timer").and_then(|t| t.as_object_mut()) {
        timer.insert("adjustedTimeLeftInPhase".to_string(), json!(left_ms));
        timer.insert("totalTimeInPhase".to_string(), json!(30_000));
    }
    s.insert_event(SourceId::LeagueClient, EventKind::Update, URI.to_string(), session, vault);
}

/// Steps of the scripted pinned draft, one every [`DEMO_DRAFT_STEP`]: six alternating bans,
/// then ally locks and enemy reveals in draft order, a short hold, and a fresh draft again.
const DEMO_DRAFT_STEPS: u32 = 18;
const DEMO_DRAFT_STEP: Duration = Duration::from_millis(2500);

/// Rewrites the pinned Champ Select session for draft step `step`, so the demo shows bans
/// landing, allies locking in and enemies revealing. The local player's own action (cell 2,
/// driven by the mobile remote) is kept as is.
fn apply_demo_draft_step(
    store: &RwLock<EventStore>,
    active: &AtomicBool,
    vault: &SecretVault,
    step: u32,
) {
    const URI: &str = "/lol-champ-select/v1/session";
    const MY_BANS: [i64; 3] = [238, 157, 35];
    const THEIR_BANS: [i64; 3] = [777, 875, 233];
    const ENEMIES: [(i64, &str); 5] =
        [(122, "Darius"), (234, "Viego"), (89, "Leona"), (81, "Ezreal"), (99, "Lux")];
    // Step at which each ally cell (0, 1, 3, 4) locks and each enemy (cells 5-9) reveals.
    const ALLY_LOCK: [(i64, u32); 4] = [(0, 7), (1, 10), (3, 11), (4, 13)];
    const ENEMY_REVEAL: [u32; 5] = [8, 9, 12, 14, 15];

    let Some(mut s) = lock_if_active(store, active) else {
        return;
    };
    let Some(mut session) = s.current(SourceId::LeagueClient, URI).map(|v| (*v).clone()) else {
        return;
    };
    let bans = step.min(6) as usize;
    session["bans"] = json!({
        "myTeamBans": &MY_BANS[..bans.div_ceil(2)],
        "theirTeamBans": &THEIR_BANS[..bans / 2],
    });
    session["theirTeam"] = json!(ENEMIES
        .iter()
        .zip(ENEMY_REVEAL)
        .enumerate()
        .map(|(i, (&(id, name), at))| {
            let revealed = step >= at;
            json!({
                "cellId": 5 + i,
                "championId": if revealed { id } else { 0 },
                "championName": if revealed { name } else { "" },
            })
        })
        .collect::<Vec<_>>());
    let own: Vec<serde_json::Value> = session["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|turn| turn.as_array())
        .flatten()
        .filter(|act| act["actorCellId"] == json!(2))
        .cloned()
        .collect();
    let allies: Vec<serde_json::Value> = ALLY_LOCK
        .iter()
        .map(|&(cell, at)| {
            json!({
                "id": 100 + cell,
                "actorCellId": cell,
                "type": "pick",
                "completed": step >= at,
                "isInProgress": false,
            })
        })
        .collect();
    session["actions"] = json!([own, allies]);
    s.insert_event(SourceId::LeagueClient, EventKind::Update, URI.to_string(), session, vault);
}

/// Writes the synthetic baseline (summoner, ranked, lobby, Lobby phase) so the projection is
/// immediately rich when Demo starts.
fn seed_demo_baseline(store: &RwLock<EventStore>, active: &AtomicBool, vault: &SecretVault) {
    let Some(mut s) = lock_if_active(store, active) else {
        return;
    };
    s.insert_event(
        SourceId::LeagueClient,
        EventKind::Update,
        "/lol-summoner/v1/current-summoner".to_string(),
        json!({
            "accountId": 100000001,
            "displayName": "HypatiaObserver",
            "gameName": "HypatiaObserver",
            "tagLine": "EUW",
            "percentCompleteForNextLevel": 65,
            "profileIconId": 548,
            "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
            "summonerLevel": 120
        }),
        vault,
    );
    s.insert_event(
        SourceId::LeagueClient,
        EventKind::Update,
        "/lol-ranked/v1/current-ranked-stats".to_string(),
        json!({
            "queueMap": {
                "RANKED_SOLO_5x5": {
                    "tier": "EMERALD",
                    "division": "II",
                    "leaguePoints": 54,
                    "wins": 49,
                    "losses": 43
                }
            }
        }),
        vault,
    );
    s.insert_event(
        SourceId::LeagueClient,
        EventKind::Update,
        "/lol-lobby/v2/lobby".to_string(),
        json!({
            "partyId": "party-7878-demo",
            "queueId": 420,
            "members": [
                {
                    "summonerId": 12345678,
                    "summonerName": "HypatiaObserver#EUW",
                    "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
                    "isLeader": true,
                    "ready": true,
                    "firstPositionPreference": "MIDDLE",
                    "secondPositionPreference": "TOP"
                },
                {
                    "summonerId": 87654321,
                    "summonerName": "KronoTrigger#EUW",
                    "puuid": "3c4411b2-7a88-4f10-9c11-demo00000002",
                    "isLeader": false,
                    "ready": true,
                    "firstPositionPreference": "JUNGLE",
                    "secondPositionPreference": "UTILITY"
                }
            ]
        }),
        vault,
    );
    s.insert_event(
        SourceId::LeagueClient,
        EventKind::Update,
        "/lol-gameflow/v1/gameflow-phase".to_string(),
        json!("Lobby"),
        vault,
    );
}

/// Spawns the synthetic demo event generator. It only produces events while `active` is set,
/// so leaving Demo mode stops all synthetic traffic.
pub fn spawn_demo_generator(
    store: Arc<RwLock<EventStore>>,
    catalog: Arc<RwLock<CatalogStore>>,
    vault: SecretVault,
    rate_hz: f32,
    demo_remote: Arc<RwLock<crate::lcu_actions::DemoRemoteState>>,
    active: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    let demo_remote = Some(demo_remote);
    tokio::spawn(async move {
        seed_demo_catalog(&catalog);

        let rate_clamped = rate_hz.clamp(0.5, 300.0);
        let interval_duration = Duration::from_secs_f64(1.0 / rate_clamped as f64);
        let mut ticker = tokio::time::interval(interval_duration);

        info!(
            "Synthetic generator started (rate: {:.1} events/s)",
            rate_clamped
        );

        let phases = [
            ("Lobby", 12.0),
            ("Matchmaking", 8.0),
            ("ReadyCheck", 6.0),
            ("ChampSelect", 30.0),
            ("InProgress", 45.0),
            ("EndOfGame", 8.0),
        ];

        let mut phase_idx = 0;
        let mut phase_start = Instant::now();
        let mut tick_counter: u64 = 0;
        let mut game_time_sec: u32 = 1122;
        let mut running = false;
        // Pinned Champ Select: when the current countdown ends, and which length comes next.
        let mut pinned_phase = String::new();
        let mut draft_deadline = Instant::now();
        let mut draft_flip = false;
        let mut draft_step: u32 = 0;
        let mut draft_step_at = Instant::now();

        loop {
            if !active.load(Ordering::Relaxed) {
                running = false;
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            if !running {
                // (Re)starting: begin a fresh cycle from a rich baseline.
                running = true;
                phase_idx = 0;
                phase_start = Instant::now();
                game_time_sec = 1122;
                seed_demo_baseline(&store, &active, &vault);
            }
            ticker.tick().await;
            if !active.load(Ordering::Relaxed) {
                continue;
            }
            tick_counter = tick_counter.wrapping_add(1);

            if let Some(ref dr) = demo_remote {
                let (overridden, override_phase) = {
                    let g = dr.read().unwrap();
                    (g.manual_override, g.phase.clone())
                };
                if overridden {
                    if override_phase != pinned_phase {
                        // `DemoSetPhase` just wrote a 24 s draft countdown.
                        draft_deadline = Instant::now() + Duration::from_secs(24);
                        pinned_phase = override_phase.clone();
                        draft_step = 0;
                        draft_step_at = Instant::now();
                    }
                    if override_phase == "ChampSelect" && Instant::now() >= draft_step_at {
                        apply_demo_draft_step(&store, &active, &vault, draft_step);
                        draft_step = (draft_step + 1) % DEMO_DRAFT_STEPS;
                        draft_step_at = Instant::now() + DEMO_DRAFT_STEP;
                    }
                    if override_phase == "ChampSelect" && Instant::now() >= draft_deadline {
                        let left_ms = if draft_flip { 29_000 } else { 30_000 };
                        draft_flip = !draft_flip;
                        draft_deadline = Instant::now() + Duration::from_millis(left_ms);
                        restart_demo_draft_timer(&store, &active, &vault, left_ms);
                    }
                    if override_phase == "InProgress" {
                        game_time_sec = next_demo_game_time(game_time_sec);
                        let Some(mut s) = lock_if_active(&store, &active) else {
                            continue;
                        };
                        s.insert_event(
                            SourceId::LiveGame,
                            EventKind::Poll,
                            "/liveclientdata/allgamedata".to_string(),
                            demo_allgamedata(game_time_sec),
                            &vault,
                        );
                        s.insert_event(
                            SourceId::LiveGame,
                            EventKind::Create,
                            "/liveclientdata/eventdata".to_string(),
                            demo_eventdata(game_time_sec),
                            &vault,
                        );
                    }
                    continue;
                }
                pinned_phase.clear();
            }

            let (current_phase, phase_duration_sec) = phases[phase_idx];

            // Check if it is time to transition to the next phase
            if phase_start.elapsed().as_secs_f64() >= phase_duration_sec {
                phase_idx = (phase_idx + 1) % phases.len();
                let (next_phase, _) = phases[phase_idx];
                phase_start = Instant::now();

                // Phase transition event
                let Some(mut s) = lock_if_active(&store, &active) else {
                    continue;
                };
                let meta = s.insert_event(
                    SourceId::LeagueClient,
                    EventKind::Update,
                    "/lol-gameflow/v1/gameflow-phase".to_string(),
                    json!(next_phase),
                    &vault,
                );
                catalog.write().unwrap().record_observed(
                    SourceId::LeagueClient,
                    "GET",
                    "/lol-gameflow/v1/gameflow-phase",
                    meta.size as u64,
                );
                continue;
            }

            // Generate event based on current phase and cycle
            let (source, kind, uri, payload) = match current_phase {
                "Lobby" => {
                    let sub = tick_counter % 4;
                    match sub {
                        0 => (
                            SourceId::RiotClient,
                            EventKind::Update,
                            "/chat/v4/presences".to_string(),
                            json!({
                                "local": {
                                    "game_name": "HypatiaObserver",
                                    "state": "chat",
                                    "statusMessage": "In Lobby (2/5)",
                                    "summary": "inLobby"
                                },
                                "presences": [
                                    {
                                        "game_name": "Faker",
                                        "game_tag": "KR1",
                                        "product": "league_of_legends",
                                        "state": "chat",
                                        "statusMessage": "Road to Worlds",
                                        "summary": "inQueue"
                                    },
                                    {
                                        "game_name": "Caps",
                                        "game_tag": "EUW",
                                        "product": "league_of_legends",
                                        "state": "dnd",
                                        "statusMessage": "Mid diff only",
                                        "summary": "inGame"
                                    }
                                ]
                            }),
                        ),
                        1 => (
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-lobby/v2/lobby".to_string(),
                            json!({
                                "partyId": "party-7878-demo",
                                "queueId": 420,
                                "members": [
                                    {
                                        "summonerId": 12345678,
                                        "summonerName": "HypatiaObserver#EUW",
                                        "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
                                        "isLeader": true,
                                        "ready": true,
                                        "firstPositionPreference": "MIDDLE",
                                        "secondPositionPreference": "TOP"
                                    },
                                    {
                                        "summonerId": 87654321,
                                        "summonerName": "KronoTrigger#EUW",
                                        "puuid": "3c4411b2-7a88-4f10-9c11-demo00000002",
                                        "isLeader": false,
                                        "ready": true,
                                        "firstPositionPreference": "JUNGLE",
                                        "secondPositionPreference": "UTILITY"
                                    }
                                ]
                            }),
                        ),
                        2 => (
                            SourceId::RiotClient,
                            EventKind::Create,
                            "/entitlements/v1/token".to_string(),
                            json!({
                                "accessToken": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6Ikh5cGF0aWFEZW1vVXNlciIsImlhdCI6MTUxNjIzOTAyMiwiZXhwIjoxOTk5OTk5OTk5LCJzY29wZXMiOlsib3BlbmlkIiwiY3BnIl19.qX_x89Z_fake_jwt_signature_here_for_testing_30s_reveal",
                                "entitlements": ["lol", "riot-client"],
                                "authorization": "Bearer eyJhbGciOiJIUzI1NiJ9.fake_auth_token.xyz123",
                                "issuer": "https://auth.riotgames.com"
                            }),
                        ),
                        _ => (
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-summoner/v1/current-summoner".to_string(),
                            json!({
                                "accountId": 100000001,
                                "displayName": "HypatiaObserver",
                                "gameName": "HypatiaObserver",
                                "tagLine": "EUW",
                                "percentCompleteForNextLevel": ((tick_counter * 5) % 100),
                                "profileIconId": 548,
                                "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
                                "summonerLevel": 120
                            }),
                        ),
                    }
                }
                "Matchmaking" => (
                    SourceId::LeagueClient,
                    EventKind::Update,
                    "/lol-matchmaking/v1/search".to_string(),
                    json!({
                        "dodgeData": { "state": "PartyInitialized" },
                        "estimatedQueueTime": 68.0,
                        "isCurrentlyInQueue": true,
                        "queueId": 420,
                        "timeInQueue": (tick_counter % 45)
                    }),
                ),
                "ReadyCheck" => (
                    SourceId::LeagueClient,
                    EventKind::Update,
                    "/lol-matchmaking/v1/ready-check".to_string(),
                    json!({
                        "declinerIds": [],
                        "dodgeWarning": "None",
                        "playerResponse": if phase_start.elapsed().as_secs_f64() < 4.5 { "None" } else { "Accepted" },
                        "state": "InProgress",
                        "timer": (12 - (phase_start.elapsed().as_secs() as i32)).max(1)
                    }),
                ),
                "ChampSelect" => {
                    let sub = tick_counter % 3;
                    let ban_champs = ["Zed", "Yasuo", "Yone", "Briar", "Shaco"];
                    let pick_champs = ["Ahri", "Jinx", "Thresh", "Lee Sin", "Aatrox"];
                    let chosen_ban = ban_champs[(tick_counter as usize) % ban_champs.len()];
                    let chosen_pick = pick_champs[(tick_counter as usize) % pick_champs.len()];

                    match sub {
                        0 => (
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-champ-select/v1/session".to_string(),
                            json!({
                                "localPlayerCellId": 2,
                                "queueType": "RANKED_SOLO_5x5",
                                "myTeam": [
                                    {
                                        "cellId": 0,
                                        "assignedPosition": "TOP",
                                        "championId": 266,
                                        "championName": "Aatrox",
                                        "summonerId": 0,
                                        "puuid": "",
                                        "summonerName": "",
                                        "spell1Id": 4,
                                        "spell2Id": 12
                                    },
                                    {
                                        "cellId": 1,
                                        "assignedPosition": "JUNGLE",
                                        "championId": 64,
                                        "championName": "Lee Sin",
                                        "summonerId": 87654321,
                                        "puuid": "3c4411b2-7a88-4f10-9c11-demo00000002",
                                        "summonerName": "KronoTrigger#EUW",
                                        "rank": "DIAMOND IV (12 LP)",
                                        "isPremade": true,
                                        "spell1Id": 4,
                                        "spell2Id": 11
                                    },
                                    {
                                        "cellId": 2,
                                        "assignedPosition": "MIDDLE",
                                        "championId": 103,
                                        "championName": "Ahri",
                                        "summonerId": 12345678,
                                        "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
                                        "summonerName": "HypatiaObserver#EUW",
                                        "rank": "EMERALD II (54 LP)",
                                        "spell1Id": 4,
                                        "spell2Id": 14,
                                        "selectedSkinId": 103027
                                    },
                                    {
                                        "cellId": 3,
                                        "assignedPosition": "BOTTOM",
                                        "championId": 222,
                                        "championName": "Jinx",
                                        "summonerId": 0,
                                        "puuid": "",
                                        "summonerName": "",
                                        "spell1Id": 4,
                                        "spell2Id": 7
                                    },
                                    {
                                        "cellId": 4,
                                        "assignedPosition": "UTILITY",
                                        "championId": 412,
                                        "championName": "Thresh",
                                        "summonerId": 0,
                                        "puuid": "",
                                        "summonerName": "",
                                        "spell1Id": 4,
                                        "spell2Id": 14
                                    }
                                ],
                                "actions": [
                                    [
                                        {
                                            "id": 10,
                                            "actorCellId": 2,
                                            "championId": 103,
                                            "completed": false,
                                            "isInProgress": true,
                                            "type": "pick"
                                        },
                                        {
                                            "id": 11,
                                            "actorCellId": 4,
                                            "championId": 0,
                                            "completed": false,
                                            "isInProgress": false,
                                            "type": "pick"
                                        }
                                    ]
                                ],
                                "positionSwaps": [
                                    { "id": 0, "cellId": 0, "state": "AVAILABLE" },
                                    { "id": 1, "cellId": 1, "state": "AVAILABLE" },
                                    { "id": 3, "cellId": 3, "state": "AVAILABLE" },
                                    { "id": 4, "cellId": 4, "state": "AVAILABLE" }
                                ],
                                "swaps": [
                                    { "id": 0, "cellId": 0, "state": "AVAILABLE" },
                                    { "id": 1, "cellId": 1, "state": "AVAILABLE" },
                                    { "id": 3, "cellId": 3, "state": "AVAILABLE" },
                                    { "id": 4, "cellId": 4, "state": "AVAILABLE" }
                                ],
                                "trades": [
                                    { "id": 0, "cellId": 0, "state": "AVAILABLE" },
                                    { "id": 1, "cellId": 1, "state": "AVAILABLE" },
                                    { "id": 3, "cellId": 3, "state": "AVAILABLE" },
                                    { "id": 4, "cellId": 4, "state": "AVAILABLE" }
                                ],
                                "theirTeam": demo_their_team(tick_counter),
                                "bans": {
                                    "myTeamBans": [238, 157, 35],
                                    "theirTeamBans": [777, 234, 233]
                                },
                                "lastSelectedChampion": chosen_pick,
                                "lastBannedChampion": chosen_ban,
                                "timer": {
                                    "phase": "BAN_PICK",
                                    "adjustedTimeLeftInPhase": (30 - ((tick_counter % 30) as i64)) * 1000,
                                    "totalTimeInPhase": 30_000
                                }
                            }),
                        ),
                        1 => (
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-chat/v1/conversations".to_string(),
                            json!({
                                "lastMessage": {
                                    "body": format!("Banning {} pls", chosen_ban),
                                    "fromSummonerId": 12345678,
                                    "timestamp": "2026-10-24T00:00:00Z"
                                }
                            }),
                        ),
                        _ => (
                            SourceId::RiotClient,
                            EventKind::Update,
                            "/chat/v4/presences".to_string(),
                            json!({
                                "local": {
                                    "game_name": "HypatiaObserver",
                                    "state": "chat",
                                    "statusMessage": "Champ Select",
                                    "summary": "champSelect"
                                }
                            }),
                        ),
                    }
                }
                "InProgress" => {
                    game_time_sec = next_demo_game_time(game_time_sec);
                    let sub = tick_counter % 3;
                    match sub {
                        0 => (
                            SourceId::LiveGame,
                            EventKind::Poll,
                            "/liveclientdata/allgamedata".to_string(),
                            demo_allgamedata(game_time_sec)
                        ),
                        1 => (
                            SourceId::LiveGame,
                            EventKind::Create,
                            "/liveclientdata/eventdata".to_string(),
                            demo_eventdata(game_time_sec)
                        ),
                        _ => (
                            SourceId::LiveGame,
                            EventKind::Poll,
                            "/liveclientdata/activeplayer".to_string(),
                            json!({
                                "championName": "Ahri",
                                "currentGold": (1500 + ((game_time_sec % 120) * 15)),
                                "level": (1 + (game_time_sec / 90)).min(18),
                                "scores": {
                                    "kills": 6,
                                    "deaths": 1,
                                    "assists": 8,
                                    "creepScore": 154
                                },
                                "championStats": {
                                    "attackDamage": 85.0 + (game_time_sec as f32 * 0.2),
                                    "abilityPower": 285.0,
                                    "currentHealth": 1480.0,
                                    "maxHealth": 1820.0
                                }
                            }),
                        ),
                    }
                }
                "EndOfGame" => (
                    SourceId::LeagueClient,
                    EventKind::Update,
                    DEMO_EOG_URI.to_string(),
                    demo_eog_stats(),
                ),
                _ => unreachable!(),
            };

            let meta = {
                let Some(mut s) = lock_if_active(&store, &active) else {
                    continue;
                };
                s.insert_event(source, kind, uri.clone(), payload, &vault)
            };

            catalog
                .write()
                .unwrap()
                .record_observed(source, "GET", &uri, meta.size as u64);
        }
    })
}

/// Enemy champion select picks, revealed progressively as the demo phase advances.
fn demo_their_team(tick: u64) -> serde_json::Value {
    let picks = [(122, "Darius"), (234, "Viego"), (0, ""), (0, ""), (0, "")];
    let revealed = 1 + ((tick / 40) % 2) as usize;
    let members: Vec<serde_json::Value> = picks
        .iter()
        .enumerate()
        .map(|(i, (id, name))| {
            if i <= revealed && *id != 0 {
                json!({ "cellId": 5 + i, "championId": id, "championName": name })
            } else {
                json!({ "cellId": 5 + i, "championId": 0 })
            }
        })
        .collect();
    json!(members)
}

/// The demo match replays this stretch of game clock, so kills, a tower, a drake and the
/// player's own death keep coming back (the live board's one-shot moments).
const DEMO_LIVE_LOOP: std::ops::RangeInclusive<u32> = 1122..=1260;

/// The demo game clock one tick later, looping over `DEMO_LIVE_LOOP`.
fn next_demo_game_time(game_time_sec: u32) -> u32 {
    let next = game_time_sec + 1;
    if DEMO_LIVE_LOOP.contains(&next) { next } else { *DEMO_LIVE_LOOP.start() }
}

/// Kills inside the demo loop: (game time, killer index, victim index) into `DEMO_PLAYERS`.
const DEMO_LATE_KILLS: [(u32, usize, usize); 3] = [(1172, 0, 5), (1205, 7, 2), (1245, 3, 8)];
/// The player (Ahri) is dead over this stretch of the loop.
const DEMO_SELF_DEATH: std::ops::Range<u32> = 1205..1220;
/// Skin number per demo player (Ahri wears the skin shown in the demo draft).
const DEMO_SKINS: [i64; 10] = [0, 0, 27, 0, 0, 0, 0, 0, 0, 0];

const DEMO_PLAYERS: [(&str, &str, &str, &str); 10] = [
    ("Nightfall#EUW", "Garen", "ORDER", "TOP"),
    ("KronoTrigger#EUW", "Lee Sin", "ORDER", "JUNGLE"),
    ("HypatiaObserver#EUW", "Ahri", "ORDER", "MIDDLE"),
    ("Sora#0001", "Jinx", "ORDER", "BOTTOM"),
    ("Moonwell#EUW", "Thresh", "ORDER", "UTILITY"),
    ("Blackwood#EUW", "Darius", "CHAOS", "TOP"),
    ("Hollow#EUW", "Viego", "CHAOS", "JUNGLE"),
    ("Kagero#KR1", "Zed", "CHAOS", "MIDDLE"),
    ("Vesper#EUW", "Kai'Sa", "CHAOS", "BOTTOM"),
    ("Anchor#EUW", "Nautilus", "CHAOS", "UTILITY"),
];

/// Seeds `/liveclientdata/*` events immediately for live game demo views.
pub(crate) fn seed_demo_live_snapshot(
    store: &mut EventStore,
    vault: &SecretVault,
    game_time_sec: u32,
) {
    store.insert_event(
        SourceId::LiveGame,
        EventKind::Poll,
        "/liveclientdata/allgamedata".to_string(),
        demo_allgamedata(game_time_sec),
        vault,
    );
    store.insert_event(
        SourceId::LiveGame,
        EventKind::Create,
        "/liveclientdata/eventdata".to_string(),
        demo_eventdata(game_time_sec),
        vault,
    );
}

/// Full Live Client Data payload for the demo match, consistent with `demo_eventdata`.
fn demo_allgamedata(game_time_sec: u32) -> serde_json::Value {
    let mut kda = [
        (2, 2, 3), (3, 1, 6), (6, 1, 8), (3, 2, 4), (0, 1, 9),
        (3, 3, 1), (2, 2, 2), (2, 4, 1), (1, 2, 2), (1, 2, 3),
    ];
    for &(t, killer, victim) in &DEMO_LATE_KILLS {
        if t <= game_time_sec {
            kda[killer].0 += 1;
            kda[victim].1 += 1;
        }
    }
    let levels = [11, 10, 12, 10, 9, 12, 10, 11, 11, 8];
    let zed_dead = game_time_sec % 40 < 14;
    let self_dead = DEMO_SELF_DEATH.contains(&game_time_sec);
    let players: Vec<serde_json::Value> = DEMO_PLAYERS
        .iter()
        .enumerate()
        .map(|(i, (riot_id, champ, team, pos))| {
            let (k, d, a) = kda[i];
            let dead = (i == 7 && zed_dead) || (i == 2 && self_dead);
            let respawn = match i {
                2 if self_dead => (DEMO_SELF_DEATH.end - game_time_sec) as f64,
                7 if zed_dead => (14 - game_time_sec % 40) as f64,
                _ => 0.0,
            };
            json!({
                "riotId": riot_id,
                "riotIdGameName": riot_id.split('#').next().unwrap_or(riot_id),
                "summonerName": riot_id,
                "championName": champ,
                "team": team,
                "position": pos,
                "level": levels[i],
                "isDead": dead,
                "respawnTimer": respawn,
                "skinID": DEMO_SKINS[i],
                "scores": { "kills": k, "deaths": d, "assists": a, "creepScore": 120 + i * 9 }
            })
        })
        .collect();
    json!({
        "activePlayer": {
            "championName": "Ahri",
            "riotId": "HypatiaObserver#EUW",
            "riotIdGameName": "HypatiaObserver",
            "summonerName": "HypatiaObserver#EUW",
            "currentGold": 1450 + (game_time_sec % 120) * 15,
            "level": 12,
            "scores": { "kills": kda[2].0, "deaths": kda[2].1, "assists": kda[2].2, "creepScore": 154 }
        },
        "allPlayers": players,
        "events": demo_eventdata(game_time_sec),
        "gameData": {
            "gameMode": "CLASSIC",
            "gameTime": game_time_sec as f32,
            "mapName": "Summoner's Rift"
        }
    })
}

/// Event log of the demo match; later entries unlock as the demo clock advances.
fn demo_eventdata(game_time_sec: u32) -> serde_json::Value {
    let log = [
        (0.0, json!({"EventName": "GameStart"})),
        (192.0, json!({"EventName": "FirstBlood", "Recipient": "KronoTrigger"})),
        (192.0, json!({"EventName": "ChampionKill", "KillerName": "KronoTrigger", "VictimName": "Kagero"})),
        (410.0, json!({"EventName": "ChampionKill", "KillerName": "Blackwood", "VictimName": "Nightfall"})),
        (587.0, json!({"EventName": "DragonKill", "DragonType": "Water", "KillerName": "Hollow", "Stolen": "False"})),
        (751.0, json!({"EventName": "HeraldKill", "KillerName": "KronoTrigger", "Stolen": "False"})),
        (842.0, json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T1_R_03_A", "KillerName": "Vesper"})),
        (903.0, json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T2_C_05_A", "KillerName": "HypatiaObserver"})),
        (970.0, json!({"EventName": "DragonKill", "DragonType": "Fire", "KillerName": "KronoTrigger", "Stolen": "False"})),
        (1066.0, json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T2_L_03_A", "KillerName": "Nightfall"})),
        (1120.0, json!({"EventName": "ChampionKill", "KillerName": "HypatiaObserver", "VictimName": "Kagero"})),
        (1135.0, json!({"EventName": "DragonKill", "DragonType": "Earth", "KillerName": "Sora", "Stolen": "False"})),
        (1150.0, json!({"EventName": "ChampionKill", "KillerName": "Vesper", "VictimName": "Moonwell"})),
        (1160.0, json!({"EventName": "BaronKill", "KillerName": "KronoTrigger", "Stolen": "True"})),
        (1172.0, json!({"EventName": "ChampionKill", "KillerName": "Nightfall", "VictimName": "Blackwood"})),
        (1188.0, json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T2_C_04_A", "KillerName": "HypatiaObserver"})),
        (1205.0, json!({"EventName": "ChampionKill", "KillerName": "Kagero", "VictimName": "HypatiaObserver"})),
        (1230.0, json!({"EventName": "DragonKill", "DragonType": "Air", "KillerName": "Hollow", "Stolen": "False"})),
        (1245.0, json!({"EventName": "ChampionKill", "KillerName": "Sora", "VictimName": "Vesper"})),
    ];
    let events: Vec<serde_json::Value> = log
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| *t <= game_time_sec as f64)
        .map(|(i, (t, ev))| {
            let mut ev = ev.clone();
            ev["EventID"] = json!(i);
            ev["EventTime"] = json!(t);
            ev
        })
        .collect();
    json!({ "Events": events })
}
