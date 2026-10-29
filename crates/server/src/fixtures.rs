use std::sync::{Arc, RwLock};
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

/// Spawns the synthetic demo event generator.
pub fn spawn_demo_generator(
    store: Arc<RwLock<EventStore>>,
    catalog: Arc<RwLock<CatalogStore>>,
    vault: SecretVault,
    rate_hz: f32,
) -> tokio::task::JoinHandle<()> {
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
            ("ReadyCheck", 4.0),
            ("ChampSelect", 18.0),
            ("InProgress", 20.0),
            ("EndOfGame", 8.0),
        ];

        let mut phase_idx = 0;
        let mut phase_start = Instant::now();
        let mut tick_counter: u64 = 0;
        let mut game_time_sec: u32 = 1122;

        // Initialize baseline states in store so /api/summary is immediately rich
        {
            let mut s = store.write().unwrap();
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
                &vault,
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
                &vault,
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
                &vault,
            );
            s.insert_event(
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-gameflow/v1/gameflow-phase".to_string(),
                json!("Lobby"),
                &vault,
            );
        }

        loop {
            ticker.tick().await;
            tick_counter = tick_counter.wrapping_add(1);

            let (current_phase, phase_duration_sec) = phases[phase_idx];

            // Check if it is time to transition to the next phase
            if phase_start.elapsed().as_secs_f64() >= phase_duration_sec {
                phase_idx = (phase_idx + 1) % phases.len();
                let (next_phase, _) = phases[phase_idx];
                phase_start = Instant::now();

                // Phase transition event
                let mut s = store.write().unwrap();
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
                        "playerResponse": "Accepted",
                        "state": "InProgress",
                        "timer": (12 - ((tick_counter % 12) as i32))
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
                                        "summonerName": ""
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
                                        "isPremade": true
                                    },
                                    {
                                        "cellId": 2,
                                        "assignedPosition": "MIDDLE",
                                        "championId": 103,
                                        "championName": "Ahri",
                                        "summonerId": 12345678,
                                        "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
                                        "summonerName": "HypatiaObserver#EUW",
                                        "rank": "EMERALD II (54 LP)"
                                    },
                                    {
                                        "cellId": 3,
                                        "assignedPosition": "BOTTOM",
                                        "championId": 222,
                                        "championName": "Jinx",
                                        "summonerId": 0,
                                        "puuid": "",
                                        "summonerName": ""
                                    },
                                    {
                                        "cellId": 4,
                                        "assignedPosition": "UTILITY",
                                        "championId": 412,
                                        "championName": "Thresh",
                                        "summonerId": 0,
                                        "puuid": "",
                                        "summonerName": ""
                                    }
                                ],
                                "actions": [
                                    [
                                        {
                                            "actorCellId": 2,
                                            "championId": 103,
                                            "completed": true,
                                            "type": "pick"
                                        }
                                    ]
                                ],
                                "bans": {
                                    "myTeamBans": [238, 157, 35],
                                    "theirTeamBans": [777, 234, 233]
                                },
                                "lastSelectedChampion": chosen_pick,
                                "lastBannedChampion": chosen_ban,
                                "timer": {
                                    "phase": "BAN_PICK",
                                    "adjustedTimeLeftInPhase": (30 - ((tick_counter % 30) as i64))
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
                    game_time_sec += 1;
                    let sub = tick_counter % 3;
                    match sub {
                        0 => (
                            SourceId::LiveGame,
                            EventKind::Poll,
                            "/liveclientdata/allgamedata".to_string(),
                            json!({
                                "activePlayer": {
                                    "championName": "Ahri",
                                    "currentGold": (1450 + ((game_time_sec % 120) * 15)),
                                    "level": (1 + (game_time_sec / 90)).min(18),
                                    "summonerName": "HypatiaObserver",
                                    "scores": {
                                        "kills": 6,
                                        "deaths": 1,
                                        "assists": 8,
                                        "creepScore": 154
                                    }
                                },
                                "gameData": {
                                    "gameMode": "CLASSIC",
                                    "gameTime": game_time_sec as f32,
                                    "mapName": "Summoner's Rift"
                                }
                            }),
                        ),
                        1 => (
                            SourceId::LiveGame,
                            EventKind::Create,
                            "/liveclientdata/eventdata".to_string(),
                            json!({
                                "Events": [
                                    {
                                        "EventID": tick_counter,
                                        "EventName": "ChampionKill",
                                        "EventTime": game_time_sec as f32,
                                        "KillerName": "HypatiaObserver",
                                        "VictimName": "EnemyMid"
                                    }
                                ]
                            }),
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
                    "/lol-end-of-game/v1/eog-stats-block".to_string(),
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
                    }),
                ),
                _ => unreachable!(),
            };

            let meta = {
                let mut s = store.write().unwrap();
                s.insert_event(source, kind, uri.clone(), payload, &vault)
            };

            catalog
                .write()
                .unwrap()
                .record_observed(source, "GET", &uri, meta.size as u64);
        }
    })
}
