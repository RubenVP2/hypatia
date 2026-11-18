use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::time::Duration;

use anyhow::{bail, Result};
use hypatia_shared::{
    keystone_name_from_id_localized, rune_style_name_localized, ChampionLookup, EndpointInfo,
    EndpointOrigin, EventKind, Lang, RemoteCommand, RemoteRunePage, SourceId,
};
use serde_json::{json, Value};
use tokio::time::Instant;
use tracing::info;

use crate::assets::GameAssetCache;
use crate::catalog::{extract_namespace, CatalogStore};
use crate::lcu_actions::resolve_lcu_command_localized;
use crate::redaction::SecretVault;
use crate::store::EventStore;
use crate::AppState;

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
        ("/lol-perks/v1/pages", "GET"),
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

fn update_slot_state(states: &mut [String; 5], id: i64, state: &str) {
    let idx = (id as usize).min(4);
    if idx != 2 {
        states[idx] = state.to_string();
    }
}

/// Unified stateful demo simulator driving both automatic Demo mode cycles and interactive
/// Mobile Remote / desktop phase switches so all synthetic LCU payloads share one implementation.
#[derive(Debug, Clone)]
pub struct DemoRemoteState {
    pub manual_override: bool,
    pub phase: String,
    pub first_position: String,
    pub second_position: String,
    pub ready_response: String,
    pub sub_phase: String,
    pub my_action_id: i64,
    pub my_action_type: String,
    pub my_action_completed: bool,
    pub my_champion_id: i64,
    pub my_banned_id: i64,
    pub spell1_id: i64,
    pub spell2_id: i64,
    pub selected_skin_id: i64,
    pub ally_positions: [String; 5],
    pub ally_champions: [i64; 5],
    pub position_swap_state: [String; 5],
    pub pick_swap_state: [String; 5],
    pub trade_state: [String; 5],
    pub rune_pages: Vec<RemoteRunePage>,
    pub phase_time_left_ms: i64,
    pub ready_timer_sec: i32,
    pub queue_time_sec: u32,
    pub game_time_sec: u32,
    pub tick_counter: u64,
    /// Seconds since the draft began, which drive the scripted bans, locks and reveals.
    pub draft_sec: u32,
    /// Ends of game so far: odd rounds are victories, even rounds defeats, so both show.
    pub eog_round: u32,
}

impl Default for DemoRemoteState {
    fn default() -> Self {
        Self {
            manual_override: false,
            phase: "ChampSelect".to_string(),
            first_position: "MIDDLE".to_string(),
            second_position: "TOP".to_string(),
            ready_response: "None".to_string(),
            sub_phase: "BAN_PICK".to_string(),
            my_action_id: 10,
            my_action_type: "ban".to_string(),
            my_action_completed: false,
            my_champion_id: 103, // Ahri
            my_banned_id: 238,   // Zed
            spell1_id: 4,        // Flash
            spell2_id: 14,       // Ignite
            selected_skin_id: 103027,
            ally_positions: [
                "TOP".to_string(),
                "JUNGLE".to_string(),
                "MIDDLE".to_string(),
                "BOTTOM".to_string(),
                "UTILITY".to_string(),
            ],
            ally_champions: [266, 64, 103, 222, 412],
            position_swap_state: [
                "AVAILABLE".to_string(),
                "AVAILABLE".to_string(),
                "INVALID".to_string(),
                "AVAILABLE".to_string(),
                "AVAILABLE".to_string(),
            ],
            pick_swap_state: [
                "AVAILABLE".to_string(),
                "AVAILABLE".to_string(),
                "INVALID".to_string(),
                "AVAILABLE".to_string(),
                "AVAILABLE".to_string(),
            ],
            trade_state: [
                "AVAILABLE".to_string(),
                "AVAILABLE".to_string(),
                "INVALID".to_string(),
                "AVAILABLE".to_string(),
                "AVAILABLE".to_string(),
            ],
            rune_pages: vec![
                RemoteRunePage {
                    id: 101,
                    name: "Mid Burst (Electrocute)".to_string(),
                    primary_style_id: 8100,
                    primary_style_name: "Domination".to_string(),
                    sub_style_id: 8200,
                    sub_style_name: "Sorcery".to_string(),
                    keystone_name: "Electrocute".to_string(),
                    selected_perk_ids: vec![8112, 8139, 8138, 8106, 8226, 8210, 5008, 5008, 5001],
                    current: true,
                    is_preset: false,
                },
                RemoteRunePage {
                    id: 102,
                    name: "All-In Fighter (Conqueror)".to_string(),
                    primary_style_id: 8000,
                    primary_style_name: "Precision".to_string(),
                    sub_style_id: 8400,
                    sub_style_name: "Resolve".to_string(),
                    keystone_name: "Conqueror".to_string(),
                    selected_perk_ids: vec![8010, 9111, 9105, 8299, 8444, 8451, 5005, 5008, 5001],
                    current: false,
                    is_preset: false,
                },
            ],
            phase_time_left_ms: 24_000,
            ready_timer_sec: 10,
            queue_time_sec: 19,
            game_time_sec: 1185,
            tick_counter: 0,
            draft_sec: 0,
            eog_round: 0,
        }
    }
}

impl DemoRemoteState {
    /// Transitions the simulator directly to `phase` (with manual override enabled) and syncs
    /// the EventStore immediately.
    pub fn set_phase(&mut self, app_state: &AppState, phase: &str) -> Result<()> {
        self.apply_command(
            app_state,
            &RemoteCommand::DemoSetPhase {
                phase: phase.to_string(),
            },
        )
    }

    /// Advances the active phase's simulated clock by one second without modifying the store.
    pub fn advance_tick(&mut self) {
        self.tick_counter = self.tick_counter.wrapping_add(1);
        match self.phase.as_str() {
            "Matchmaking" => {
                self.queue_time_sec = self.queue_time_sec.saturating_add(1);
            }
            "ReadyCheck" => {
                self.ready_timer_sec = if self.ready_timer_sec > 1 {
                    self.ready_timer_sec - 1
                } else {
                    12
                };
            }
            "ChampSelect" => {
                self.phase_time_left_ms = if self.phase_time_left_ms > 1_000 {
                    self.phase_time_left_ms - 1_000
                } else {
                    30_000
                };
                self.draft_sec = self.draft_sec.wrapping_add(1);
            }
            "InProgress" => {
                self.game_time_sec = next_demo_game_time(self.game_time_sec);
            }
            _ => {}
        }
    }

    /// Advances one simulated second and syncs the updated synthetic LCU state into `EventStore`.
    pub fn step_tick(&mut self, app_state: &AppState) {
        self.advance_tick();
        self.sync_to_store(app_state);
    }

    fn reset_phase_timers(&mut self, phase: &str) {
        match phase {
            "Matchmaking" => {
                self.queue_time_sec = 0;
            }
            "ReadyCheck" => {
                self.ready_response = "None".to_string();
                self.ready_timer_sec = 12;
            }
            "ChampSelect" => {
                self.my_action_id = 10;
                self.my_action_type = "ban".to_string();
                self.my_action_completed = false;
                self.sub_phase = "BAN_PICK".to_string();
                self.phase_time_left_ms = 24_000;
                self.draft_sec = 0;
            }
            "InProgress" => {
                self.game_time_sec = 1185;
            }
            "EndOfGame" => {
                self.eog_round = self.eog_round.wrapping_add(1);
            }
            _ => {}
        }
    }

    /// Applies a `RemoteCommand` to the interactive demo state and pushes updated
    /// synthetic LCU events into `AppState`'s `EventStore`.
    pub fn apply_command(&mut self, app_state: &AppState, cmd: &RemoteCommand) -> Result<()> {
        let lang = app_state.lang();
        self.manual_override = true;
        match cmd {
            RemoteCommand::SetLobbyPositions { first, second } => {
                let _ = resolve_lcu_command_localized(cmd, None, lang)?;
                self.first_position = first.trim().to_ascii_uppercase();
                self.second_position = second.trim().to_ascii_uppercase();
                self.ally_positions[2] = self.first_position.clone();
            }
            RemoteCommand::StartMatchmaking => {
                self.phase = "Matchmaking".to_string();
                self.ready_response = "None".to_string();
                self.queue_time_sec = 0;
            }
            RemoteCommand::StopMatchmaking => {
                self.phase = "Lobby".to_string();
            }
            RemoteCommand::AcceptReadyCheck => {
                self.ready_response = "Accepted".to_string();
                self.phase = "ChampSelect".to_string();
                self.reset_phase_timers("ChampSelect");
            }
            RemoteCommand::DeclineReadyCheck => {
                self.ready_response = "Declined".to_string();
                self.phase = "Lobby".to_string();
            }
            RemoteCommand::HoverChampion {
                action_id: _,
                champion_id,
            } => {
                if *champion_id <= 0 {
                    bail!(
                        "{}",
                        lang.pick("Select a champion first", "Sélectionnez d'abord un champion")
                    );
                }
                if self.my_action_type == "ban" {
                    self.my_banned_id = *champion_id;
                } else {
                    self.my_champion_id = *champion_id;
                    self.ally_champions[2] = *champion_id;
                    self.selected_skin_id = *champion_id * 1000;
                }
            }
            RemoteCommand::LockAction { action_id: _ } => {
                if self.my_action_type == "ban" {
                    self.my_action_id = 20;
                    self.my_action_type = "pick".to_string();
                    self.my_action_completed = false;
                } else {
                    self.my_action_completed = true;
                    self.sub_phase = "FINALIZATION".to_string();
                }
            }
            RemoteCommand::SetSummonerSpells {
                spell1_id,
                spell2_id,
            } => {
                let _ = resolve_lcu_command_localized(cmd, None, lang)?;
                self.spell1_id = *spell1_id;
                self.spell2_id = *spell2_id;
            }
            RemoteCommand::SetSkin { skin_id } => {
                let _ = resolve_lcu_command_localized(cmd, None, lang)?;
                self.selected_skin_id = *skin_id;
            }
            RemoteCommand::SelectRunePage { page_id } => {
                for p in &mut self.rune_pages {
                    p.current = p.id == *page_id;
                }
            }
            RemoteCommand::SaveRunePage {
                name,
                primary_style_id,
                sub_style_id,
                selected_perk_ids,
            } => {
                let _ = resolve_lcu_command_localized(cmd, Some(101), lang)?;
                let keystone = selected_perk_ids
                    .first()
                    .copied()
                    .map(|id| keystone_name_from_id_localized(id, lang))
                    .unwrap_or(lang.pick("Keystone", "Rune fondamentale"))
                    .to_string();
                for p in &mut self.rune_pages {
                    p.current = false;
                }
                if let Some(first) = self.rune_pages.first_mut() {
                    first.name = name.clone();
                    first.primary_style_id = *primary_style_id;
                    first.primary_style_name =
                        rune_style_name_localized(*primary_style_id, lang).to_string();
                    first.sub_style_id = *sub_style_id;
                    first.sub_style_name =
                        rune_style_name_localized(*sub_style_id, lang).to_string();
                    first.keystone_name = keystone;
                    first.selected_perk_ids = selected_perk_ids.clone();
                    first.current = true;
                }
            }
            RemoteCommand::RequestPositionSwap { swap_id } => {
                update_slot_state(&mut self.position_swap_state, *swap_id, "SENT");
            }
            RemoteCommand::RespondPositionSwap { swap_id, accept } => {
                let idx = (*swap_id as usize).min(4);
                if *accept && idx != 2 {
                    self.ally_positions.swap(2, idx);
                }
                update_slot_state(&mut self.position_swap_state, *swap_id, "AVAILABLE");
            }
            RemoteCommand::CancelPositionSwap { swap_id } => {
                update_slot_state(&mut self.position_swap_state, *swap_id, "AVAILABLE");
            }
            RemoteCommand::RequestPickOrderSwap { swap_id } => {
                update_slot_state(&mut self.pick_swap_state, *swap_id, "SENT");
            }
            RemoteCommand::RespondPickOrderSwap { swap_id, accept: _ }
            | RemoteCommand::CancelPickOrderSwap { swap_id } => {
                update_slot_state(&mut self.pick_swap_state, *swap_id, "AVAILABLE");
            }
            RemoteCommand::RequestChampionTrade { trade_id } => {
                update_slot_state(&mut self.trade_state, *trade_id, "SENT");
            }
            RemoteCommand::RespondChampionTrade { trade_id, accept } => {
                let idx = (*trade_id as usize).min(4);
                if *accept && idx != 2 {
                    self.ally_champions.swap(2, idx);
                    self.my_champion_id = self.ally_champions[2];
                }
                update_slot_state(&mut self.trade_state, *trade_id, "AVAILABLE");
            }
            RemoteCommand::CancelChampionTrade { trade_id } => {
                update_slot_state(&mut self.trade_state, *trade_id, "AVAILABLE");
            }
            RemoteCommand::DemoSetPhase { phase } => {
                self.phase = phase.clone();
                self.reset_phase_timers(phase);
            }
            RemoteCommand::DemoSimulateIncomingSwap => {
                self.phase = "ChampSelect".to_string();
                self.position_swap_state[0] = "RECEIVED".to_string();
            }
        }

        self.sync_to_store(app_state);
        Ok(())
    }

    pub fn sync_to_store(&self, app_state: &AppState) {
        let Some(mut store) = lock_if_active(&app_state.store, &app_state.discovery.demo_flag())
        else {
            return;
        };
        self.sync_to_store_parts(
            &mut store,
            &app_state.vault,
            &*app_state.assets,
            app_state.lang(),
        );
    }

    pub(crate) fn sync_to_store_parts(
        &self,
        store: &mut EventStore,
        vault: &SecretVault,
        champions: &dyn ChampionLookup,
        lang: Lang,
    ) {
        for (source, kind, uri, payload) in [
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-summoner/v1/current-summoner",
                self.summoner_payload(),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-ranked/v1/current-ranked-stats",
                Self::ranked_payload(),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-gameflow/v1/gameflow-phase",
                json!(self.phase),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-lobby/v2/lobby",
                self.lobby_payload(),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-matchmaking/v1/search",
                self.search_payload(),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-matchmaking/v1/ready-check",
                self.ready_check_payload(),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-champ-select/v1/session",
                self.champ_select_payload(champions, lang),
            ),
            (
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-perks/v1/pages",
                self.perks_payload(lang),
            ),
        ] {
            store.insert_event(source, kind, uri.to_string(), payload, vault);
        }

        if self.phase == "InProgress" {
            seed_demo_live_snapshot(store, vault, self.game_time_sec);
        } else if self.phase == "EndOfGame" {
            store.insert_event(
                SourceId::LeagueClient,
                EventKind::Update,
                DEMO_EOG_URI.to_string(),
                self.eog_payload(),
                vault,
            );
        }
    }

    fn summoner_payload(&self) -> Value {
        json!({
            "accountId": 100000001,
            "displayName": "HypatiaObserver",
            "gameName": "HypatiaObserver",
            "tagLine": "EUW",
            "percentCompleteForNextLevel": ((65 + self.tick_counter * 5) % 100),
            "profileIconId": 548,
            "puuid": "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
            "summonerLevel": 120
        })
    }

    fn ranked_payload() -> Value {
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
        })
    }

    fn presences_payload(&self) -> Value {
        if self.phase == "ChampSelect" {
            json!({
                "local": {
                    "game_name": "HypatiaObserver",
                    "state": "chat",
                    "statusMessage": "Champ Select",
                    "summary": "champSelect"
                }
            })
        } else {
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
            })
        }
    }

    fn lobby_payload(&self) -> Value {
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
                    "firstPositionPreference": self.first_position,
                    "secondPositionPreference": self.second_position
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
        })
    }

    fn search_payload(&self) -> Value {
        json!({
            "dodgeData": { "state": "PartyInitialized" },
            "estimatedQueueTime": 68.0,
            "isCurrentlyInQueue": self.phase == "Matchmaking" || self.phase == "ReadyCheck",
            "queueId": 420,
            "timeInQueue": self.queue_time_sec
        })
    }

    fn ready_check_payload(&self) -> Value {
        json!({
            "declinerIds": [],
            "dodgeWarning": "None",
            "playerResponse": self.ready_response,
            "state": if self.phase == "ReadyCheck" { "InProgress" } else { "Invalid" },
            "timer": self.ready_timer_sec
        })
    }

    fn perks_payload(&self, lang: Lang) -> Value {
        let pages: Vec<Value> = self
            .rune_pages
            .iter()
            .map(|p| {
                let name = if p.id == 101 && p.name == "Mid Burst (Electrocute)" {
                    lang.pick("Mid Burst (Electrocute)", "Mid Burst (Électrocution)")
                } else if p.id == 102 && p.name == "All-In Fighter (Conqueror)" {
                    lang.pick("All-In Fighter (Conqueror)", "Combattant All-In (Conquérant)")
                } else {
                    p.name.as_str()
                };
                json!({
                    "id": p.id,
                    "name": name,
                    "primaryStyleId": p.primary_style_id,
                    "subStyleId": p.sub_style_id,
                    "selectedPerkIds": p.selected_perk_ids,
                    "current": p.current,
                    "isEditable": !p.is_preset
                })
            })
            .collect();
        json!(pages)
    }

    fn champ_select_payload(&self, champions: &dyn ChampionLookup, lang: Lang) -> Value {
        // Ranked Solo/Duo anonymization: solo teammates (0, 3, 4) keep empty PUUID and
        // summonerId 0, while premade ally 1 and self (cell 2) remain identified.
        let my_team: Vec<Value> = (0..5)
            .map(|i| {
                let cid = self.ally_champions[i];
                let (summoner_id, puuid, summoner_name, rank, is_premade) = match i {
                    1 => (
                        87654321u64,
                        "3c4411b2-7a88-4f10-9c11-demo00000002",
                        "KronoTrigger#EUW",
                        Some("DIAMOND IV (12 LP)"),
                        true,
                    ),
                    2 => (
                        12345678u64,
                        "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001",
                        "HypatiaObserver#EUW",
                        Some("EMERALD II (54 LP)"),
                        false,
                    ),
                    _ => (0u64, "", "", None, false),
                };
                json!({
                    "cellId": i,
                    "assignedPosition": self.ally_positions[i],
                    "championId": cid,
                    "championName": champions.resolve_champion_name_localized(cid, lang),
                    "summonerId": summoner_id,
                    "puuid": puuid,
                    "summonerName": summoner_name,
                    "rank": rank,
                    "isPremade": is_premade,
                    "spell1Id": if i == 2 { self.spell1_id } else { 4 },
                    "spell2Id": if i == 2 { self.spell2_id } else if i == 1 { 11 } else { 12 },
                    "selectedSkinId": if i == 2 { self.selected_skin_id } else { cid * 1000 }
                })
            })
            .collect();

        let swap_entries = |states: &[String; 5]| -> Vec<Value> {
            (0..5)
                .filter(|&i| i != 2)
                .map(|i| {
                    json!({
                        "id": i,
                        "cellId": i,
                        "state": states[i]
                    })
                })
                .collect()
        };

        let active_champ = if self.my_action_type == "ban" {
            self.my_banned_id
        } else {
            self.my_champion_id
        };

        // Scripted draft: six alternating bans, then ally locks and enemy reveals in draft
        // order, a short hold, and the script starts over.
        let step = (self.draft_sec * 2 / 5) % DEMO_DRAFT_STEPS;
        let my_bans = [self.my_banned_id, 157, 35];
        let their_bans = [777, 875, 233];
        let bans = step.min(6) as usize;
        let ally_actions: Vec<Value> = DEMO_ALLY_LOCK
            .iter()
            .map(|&(cell, at)| {
                json!({
                    "id": 100 + cell,
                    "actorCellId": cell,
                    "championId": self.ally_champions[cell as usize],
                    "completed": step >= at,
                    "isInProgress": false,
                    "type": "pick"
                })
            })
            .collect();

        json!({
            "localPlayerCellId": 2,
            "queueType": "RANKED_SOLO_5x5",
            "myTeam": my_team,
            "theirTeam": demo_their_team(step),
            "actions": [
                [
                    {
                        "id": self.my_action_id,
                        "actorCellId": 2,
                        "championId": active_champ,
                        "completed": self.my_action_completed,
                        "isInProgress": !self.my_action_completed,
                        "type": self.my_action_type
                    },
                ],
                ally_actions
            ],
            "bans": {
                "myTeamBans": &my_bans[..bans.div_ceil(2)],
                "theirTeamBans": &their_bans[..bans / 2]
            },
            "positionSwaps": swap_entries(&self.position_swap_state),
            "swaps": swap_entries(&self.pick_swap_state),
            "trades": swap_entries(&self.trade_state),
            "timer": {
                "phase": self.sub_phase,
                "adjustedTimeLeftInPhase": self.phase_time_left_ms,
                "totalTimeInPhase": 30_000
            }
        })
    }

    /// Post-game stats block for the demo match (Ahri), with both teams of `DEMO_PLAYERS`.
    fn eog_payload(&self) -> Value {
        let victory = self.eog_round % 2 == 1;
        const IDS: [i64; 10] = [86, 64, 103, 222, 412, 122, 234, 238, 145, 111];
        // (kills, deaths, assists, lane minions, jungle monsters, level, gold, damage, vision)
        const LINES: [(u32, u32, u32, u32, u32, u32, u32, u32, u32); 10] = [
            (4, 3, 6, 201, 4, 16, 11200, 18400, 14),
            (6, 2, 11, 38, 168, 15, 11950, 15200, 31),
            (9, 2, 14, 214, 12, 17, 14950, 34820, 28),
            (7, 4, 8, 236, 10, 16, 13800, 27100, 17),
            (1, 5, 17, 32, 0, 13, 8100, 6900, 64),
            (5, 6, 2, 188, 6, 16, 10900, 21300, 12),
            (3, 5, 6, 41, 152, 14, 9800, 12400, 26),
            (6, 7, 3, 196, 8, 15, 10600, 22900, 15),
            (3, 6, 5, 221, 6, 15, 11300, 19800, 13),
            (1, 4, 9, 29, 0, 12, 7400, 7100, 52),
        ];
        let players: Vec<Value> = DEMO_PLAYERS
            .iter()
            .enumerate()
            .map(|(i, (_, champ, _, pos))| {
                let (k, d, a, minions, monsters, level, gold, damage, vision) = LINES[i];
                let skin = IDS[i] * 1000 + DEMO_SKINS[i];
                json!({
                    "championId": IDS[i],
                    "championName": champ,
                    "detectedTeamPosition": pos,
                    "isLocalPlayer": i == 2,
                    "skinSplashPath": format!("/lol-game-data/assets/v1/champion-splashes/{}/{skin}.jpg", IDS[i]),
                    "stats": {
                        "CHAMPIONS_KILLED": k,
                        "NUM_DEATHS": d,
                        "ASSISTS": a,
                        "MINIONS_KILLED": minions,
                        "NEUTRAL_MINIONS_KILLED": monsters,
                        "LEVEL": level,
                        "GOLD_EARNED": gold,
                        "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": damage,
                        "VISION_SCORE": vision
                    }
                })
            })
            .collect();
        json!({
            "gameId": 894512301,
            "gameLength": 1694,
            "gameMode": "CLASSIC",
            "gameResult": if victory { "Victory" } else { "Defeat" },
            "localPlayer": players[2].clone(),
            "teams": [
                {"teamId": 100, "isPlayerTeam": true, "isWinningTeam": victory, "players": players[..5].to_vec()},
                {"teamId": 200, "isPlayerTeam": false, "isWinningTeam": !victory, "players": players[5..].to_vec()}
            ]
        })
    }
}

/// Spawns the synthetic demo event generator. It only produces events while `active` is set,
/// so leaving Demo mode stops all synthetic traffic.
pub fn spawn_demo_generator(
    store: Arc<RwLock<EventStore>>,
    catalog: Arc<RwLock<CatalogStore>>,
    vault: SecretVault,
    rate_hz: f32,
    demo_remote: Arc<RwLock<DemoRemoteState>>,
    active: Arc<AtomicBool>,
    lang: Arc<AtomicU8>,
    assets: Arc<GameAssetCache>,
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
            ("ReadyCheck", 6.0),
            ("ChampSelect", 30.0),
            ("InProgress", 45.0),
            ("EndOfGame", 8.0),
        ];

        let mut phase_idx = 0;
        let mut phase_start = Instant::now();
        let mut last_clock_step = Instant::now();
        let mut running = false;

        let current_lang = || {
            if lang.load(Ordering::Relaxed) == Lang::Fr as u8 {
                Lang::Fr
            } else {
                Lang::En
            }
        };

        loop {
            if !active.load(Ordering::Relaxed) {
                running = false;
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
            if !running {
                running = true;
                phase_idx = 0;
                phase_start = Instant::now();
                last_clock_step = Instant::now();
                let should_seed_cycle = {
                    let mut dr = demo_remote.write().unwrap();
                    if !dr.manual_override {
                        dr.phase = "Lobby".to_string();
                        dr.game_time_sec = 1122;
                        true
                    } else {
                        false
                    }
                };
                if should_seed_cycle {
                    if let Some(mut s) = lock_if_active(&store, &active) {
                        let dr = demo_remote.read().unwrap();
                        dr.sync_to_store_parts(&mut s, &vault, &*assets, current_lang());
                    }
                }
            }
            ticker.tick().await;
            if !active.load(Ordering::Relaxed) {
                continue;
            }

            let step_second = if last_clock_step.elapsed() >= Duration::from_secs(1) {
                last_clock_step = Instant::now();
                true
            } else {
                false
            };

            let (overridden, current_phase) = {
                let mut dr = demo_remote.write().unwrap();
                if step_second {
                    dr.advance_tick();
                } else {
                    dr.tick_counter = dr.tick_counter.wrapping_add(1);
                }
                if !dr.manual_override && dr.phase == "ReadyCheck" && phase_start.elapsed().as_secs_f64() >= 4.5 {
                    dr.ready_response = "Accepted".to_string();
                }
                (dr.manual_override, dr.phase.clone())
            };

            if overridden {
                if !step_second {
                    continue;
                }
                let dr = demo_remote.read().unwrap().clone();
                let Some(mut s) = lock_if_active(&store, &active) else {
                    continue;
                };
                match current_phase.as_str() {
                    "Matchmaking" => {
                        s.insert_event(
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-matchmaking/v1/search".to_string(),
                            dr.search_payload(),
                            &vault,
                        );
                    }
                    "ReadyCheck" => {
                        s.insert_event(
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-matchmaking/v1/ready-check".to_string(),
                            dr.ready_check_payload(),
                            &vault,
                        );
                    }
                    "ChampSelect" => {
                        s.insert_event(
                            SourceId::LeagueClient,
                            EventKind::Update,
                            "/lol-champ-select/v1/session".to_string(),
                            dr.champ_select_payload(&*assets, current_lang()),
                            &vault,
                        );
                    }
                    "InProgress" => {
                        seed_demo_live_snapshot(&mut s, &vault, dr.game_time_sec);
                    }
                    _ => {}
                }
                continue;
            }

            let (cycle_phase, phase_duration_sec) = phases[phase_idx];
            if phase_start.elapsed().as_secs_f64() >= phase_duration_sec {
                phase_idx = (phase_idx + 1) % phases.len();
                let (next_phase, _) = phases[phase_idx];
                phase_start = Instant::now();
                let dr_snapshot = {
                    let mut dr = demo_remote.write().unwrap();
                    dr.phase = next_phase.to_string();
                    dr.reset_phase_timers(next_phase);
                    if next_phase == "InProgress" {
                        dr.game_time_sec = 1122;
                    }
                    dr.clone()
                };
                let Some(mut s) = lock_if_active(&store, &active) else {
                    continue;
                };
                dr_snapshot.sync_to_store_parts(&mut s, &vault, &*assets, current_lang());
                catalog.write().unwrap().record_observed(
                    SourceId::LeagueClient,
                    "GET",
                    "/lol-gameflow/v1/gameflow-phase",
                    next_phase.len() as u64 + 2,
                );
                continue;
            }

            let dr = demo_remote.read().unwrap().clone();
            let tick = dr.tick_counter;
            let l = current_lang();
            let (source, kind, uri, payload) = match cycle_phase {
                "Lobby" => match tick % 4 {
                    0 => (
                        SourceId::RiotClient,
                        EventKind::Update,
                        "/chat/v4/presences".to_string(),
                        dr.presences_payload(),
                    ),
                    1 => (
                        SourceId::LeagueClient,
                        EventKind::Update,
                        "/lol-lobby/v2/lobby".to_string(),
                        dr.lobby_payload(),
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
                        dr.summoner_payload(),
                    ),
                },
                "Matchmaking" => (
                    SourceId::LeagueClient,
                    EventKind::Update,
                    "/lol-matchmaking/v1/search".to_string(),
                    dr.search_payload(),
                ),
                "ReadyCheck" => (
                    SourceId::LeagueClient,
                    EventKind::Update,
                    "/lol-matchmaking/v1/ready-check".to_string(),
                    dr.ready_check_payload(),
                ),
                "ChampSelect" => match tick % 3 {
                    0 => (
                        SourceId::LeagueClient,
                        EventKind::Update,
                        "/lol-champ-select/v1/session".to_string(),
                        dr.champ_select_payload(&*assets, l),
                    ),
                    1 => (
                        SourceId::LeagueClient,
                        EventKind::Update,
                        "/lol-chat/v1/conversations".to_string(),
                        json!({
                            "lastMessage": {
                                "body": format!("Banning {} pls", assets.resolve_champion_name_localized(dr.my_banned_id, l)),
                                "fromSummonerId": 12345678,
                                "timestamp": "2026-10-24T00:00:00Z"
                            }
                        }),
                    ),
                    _ => (
                        SourceId::RiotClient,
                        EventKind::Update,
                        "/chat/v4/presences".to_string(),
                        dr.presences_payload(),
                    ),
                },
                "InProgress" => match tick % 3 {
                    0 => (
                        SourceId::LiveGame,
                        EventKind::Poll,
                        "/liveclientdata/allgamedata".to_string(),
                        demo_allgamedata(dr.game_time_sec),
                    ),
                    1 => (
                        SourceId::LiveGame,
                        EventKind::Create,
                        "/liveclientdata/eventdata".to_string(),
                        demo_eventdata(dr.game_time_sec),
                    ),
                    _ => (
                        SourceId::LiveGame,
                        EventKind::Poll,
                        "/liveclientdata/activeplayer".to_string(),
                        demo_activeplayer(dr.game_time_sec),
                    ),
                },
                "EndOfGame" => (
                    SourceId::LeagueClient,
                    EventKind::Update,
                    DEMO_EOG_URI.to_string(),
                    dr.eog_payload(),
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

/// Endpoint carrying the demo post-game summary.
const DEMO_EOG_URI: &str = "/lol-end-of-game/v1/eog-stats-block";

/// Steps of the scripted demo draft, one every 2.5 s of the draft clock.
const DEMO_DRAFT_STEPS: u32 = 18;
/// Step at which each ally cell (0, 1, 3, 4) locks its pick; the player's cell 2 is driven
/// by the Mobile Remote.
const DEMO_ALLY_LOCK: [(i64, u32); 4] = [(0, 7), (1, 10), (3, 11), (4, 13)];

/// Enemy champion select picks, each revealed at its step of the scripted draft.
fn demo_their_team(step: u32) -> Value {
    const ENEMIES: [(i64, &str); 5] =
        [(122, "Darius"), (234, "Viego"), (89, "Leona"), (81, "Ezreal"), (99, "Lux")];
    const REVEAL: [u32; 5] = [8, 9, 12, 14, 15];
    let members: Vec<Value> = ENEMIES
        .iter()
        .zip(REVEAL)
        .enumerate()
        .map(|(i, (&(id, name), at))| {
            if step >= at {
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

/// The demo game clock one second later, looping over `DEMO_LIVE_LOOP`.
fn next_demo_game_time(game_time_sec: u32) -> u32 {
    let next = game_time_sec + 1;
    if DEMO_LIVE_LOOP.contains(&next) {
        next
    } else {
        *DEMO_LIVE_LOOP.start()
    }
}

/// Kills inside the demo loop: (game time, killer index, victim index) into `DEMO_PLAYERS`.
const DEMO_LATE_KILLS: [(u32, usize, usize); 3] = [(1172, 0, 5), (1205, 7, 2), (1245, 3, 8)];
/// The player (Ahri) is dead over this stretch of the loop.
const DEMO_SELF_DEATH: std::ops::Range<u32> = 1205..1220;
/// Skin number per demo player (Ahri wears the skin shown in the demo draft).
const DEMO_SKINS: [i64; 10] = [0, 0, 27, 0, 0, 0, 0, 0, 0, 0];

/// Kills, deaths and assists per demo player at `game_time_sec`.
fn demo_kda(game_time_sec: u32) -> [(u32, u32, u32); 10] {
    let mut kda = [
        (2, 2, 3),
        (3, 1, 6),
        (6, 1, 8),
        (3, 2, 4),
        (0, 1, 9),
        (3, 3, 1),
        (2, 2, 2),
        (2, 4, 1),
        (1, 2, 2),
        (1, 2, 3),
    ];
    for &(t, killer, victim) in &DEMO_LATE_KILLS {
        if t <= game_time_sec {
            kda[killer].0 += 1;
            kda[victim].1 += 1;
        }
    }
    kda
}

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
    store.insert_event(
        SourceId::LiveGame,
        EventKind::Poll,
        "/liveclientdata/activeplayer".to_string(),
        demo_activeplayer(game_time_sec),
        vault,
    );
}

fn demo_activeplayer(game_time_sec: u32) -> Value {
    let (kills, deaths, assists) = demo_kda(game_time_sec)[2];
    json!({
        "championName": "Ahri",
        "currentGold": (1500 + ((game_time_sec % 120) * 15)),
        "level": (1 + (game_time_sec / 90)).min(18),
        "scores": {
            "kills": kills,
            "deaths": deaths,
            "assists": assists,
            "creepScore": 154
        },
        "championStats": {
            "attackDamage": 85.0 + (game_time_sec as f32 * 0.2),
            "abilityPower": 285.0,
            "currentHealth": 1480.0,
            "maxHealth": 1820.0
        }
    })
}

/// Full Live Client Data payload for the demo match, consistent with `demo_eventdata`.
fn demo_allgamedata(game_time_sec: u32) -> Value {
    let kda = demo_kda(game_time_sec);
    let levels = [11, 10, 12, 10, 9, 12, 10, 11, 11, 8];
    let zed_dead = game_time_sec % 40 < 14;
    let self_dead = DEMO_SELF_DEATH.contains(&game_time_sec);
    let players: Vec<Value> = DEMO_PLAYERS
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
fn demo_eventdata(game_time_sec: u32) -> Value {
    let log = [
        (0.0, json!({"EventName": "GameStart"})),
        (
            192.0,
            json!({"EventName": "FirstBlood", "Recipient": "KronoTrigger"}),
        ),
        (
            192.0,
            json!({"EventName": "ChampionKill", "KillerName": "KronoTrigger", "VictimName": "Kagero"}),
        ),
        (
            410.0,
            json!({"EventName": "ChampionKill", "KillerName": "Blackwood", "VictimName": "Nightfall"}),
        ),
        (
            587.0,
            json!({"EventName": "DragonKill", "DragonType": "Water", "KillerName": "Hollow", "Stolen": "False"}),
        ),
        (
            751.0,
            json!({"EventName": "HeraldKill", "KillerName": "KronoTrigger", "Stolen": "False"}),
        ),
        (
            842.0,
            json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T1_R_03_A", "KillerName": "Vesper"}),
        ),
        (
            903.0,
            json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T2_C_05_A", "KillerName": "HypatiaObserver"}),
        ),
        (
            970.0,
            json!({"EventName": "DragonKill", "DragonType": "Fire", "KillerName": "KronoTrigger", "Stolen": "False"}),
        ),
        (
            1066.0,
            json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T2_L_03_A", "KillerName": "Nightfall"}),
        ),
        (
            1120.0,
            json!({"EventName": "ChampionKill", "KillerName": "HypatiaObserver", "VictimName": "Kagero"}),
        ),
        (
            1135.0,
            json!({"EventName": "DragonKill", "DragonType": "Earth", "KillerName": "Sora", "Stolen": "False"}),
        ),
        (
            1150.0,
            json!({"EventName": "ChampionKill", "KillerName": "Vesper", "VictimName": "Moonwell"}),
        ),
        (
            1160.0,
            json!({"EventName": "BaronKill", "KillerName": "KronoTrigger", "Stolen": "True"}),
        ),
        (
            1172.0,
            json!({"EventName": "ChampionKill", "KillerName": "Nightfall", "VictimName": "Blackwood"}),
        ),
        (
            1188.0,
            json!({"EventName": "TurretKilled", "TurretKilled": "Turret_T2_C_04_A", "KillerName": "HypatiaObserver"}),
        ),
        (
            1205.0,
            json!({"EventName": "ChampionKill", "KillerName": "Kagero", "VictimName": "HypatiaObserver"}),
        ),
        (
            1230.0,
            json!({"EventName": "DragonKill", "DragonType": "Air", "KillerName": "Hollow", "Stolen": "False"}),
        ),
        (
            1245.0,
            json!({"EventName": "ChampionKill", "KillerName": "Sora", "VictimName": "Vesper"}),
        ),
    ];
    let events: Vec<Value> = log
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unified_demo_simulator_preserves_anonymization_perks_and_advances_clocks() {
        let app = crate::AppState::start_with_lang(true, 0.5, Lang::En);
        app.set_demo_phase("ChampSelect");

        let summary = app.get_summary();
        let cs = summary
            .champ_select
            .as_ref()
            .expect("champ_select should be projected");
        assert_eq!(cs.queue_type, "RANKED_SOLO_5x5");
        assert!(
            cs.is_ranked_solo_anonymized,
            "Ranked Solo/Duo teammates must remain anonymized"
        );
        assert!(cs.my_team[0].is_anonymized);
        assert!(cs.my_team[1].is_premade && !cs.my_team[1].is_anonymized);
        assert!(cs.my_team[2].is_self && !cs.my_team[2].is_anonymized);
        assert!(cs.my_team[3].is_anonymized);
        assert!(cs.my_team[4].is_anonymized);
        assert_eq!(cs.time_left_sec, 24);

        // /lol-perks/v1/pages must be emitted into EventStore like live LCU data.
        assert!(
            app.store
                .read()
                .unwrap()
                .current(SourceId::LeagueClient, "/lol-perks/v1/pages")
                .is_some()
        );

        // Stepping a tick advances the phase clock instead of freezing at 24s.
        app.demo_remote.write().unwrap().step_tick(&app);
        assert_eq!(
            app.get_summary().champ_select.as_ref().unwrap().time_left_sec,
            23
        );

        // Applying a Mobile Remote command preserves anonymization.
        app.demo_remote
            .write()
            .unwrap()
            .apply_command(
                &app,
                &RemoteCommand::HoverChampion {
                    action_id: 10,
                    champion_id: 157,
                },
            )
            .unwrap();
        let after_cmd = app.get_summary();
        assert!(
            after_cmd
                .champ_select
                .as_ref()
                .unwrap()
                .is_ranked_solo_anonymized
        );
    }
}
