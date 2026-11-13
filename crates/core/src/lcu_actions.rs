//! Opt-in LCU action client and state extractor for the Mobile Remote companion.
//!
//! Architectural boundary:
//! - `ReadOnlyRiotClient` remains strictly GET-only for the desktop observatory.
//! - `LcuActionClient` is only invoked by the Mobile Remote server when explicitly enabled
//!   by the user, and ONLY accepts variants of the closed `RemoteCommand` enum.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::prelude::*;
use hypatia_shared::{
    builtin_champion_catalog, builtin_skins_for, champion_name_from_id, default_rune_trees,
    default_summoner_spells, extract_champ_select_summary, extract_live_board,
    extract_live_game_summary, keystone_name_from_id, queue_name_from_id, recommended_pages_for,
    rune_style_name, spell_name_from_id, EventKind, RemoteActiveAction, RemoteChampSelectState,
    RemoteCommand, RemoteIncomingSwap, RemoteLiveFeedItem, RemoteLiveObjective,
    RemoteLivePlayer, RemoteLiveState, RemoteLobbyState, RemoteMatchmakingState,
    RemoteRunePage, RemoteSkinOption, RemoteStateSnapshot, RemoteTeamMember, SourceId,
};
use reqwest::Method;
use serde_json::{json, Value};

use crate::AppState;

const VALID_POSITIONS: &[&str] = &["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY", "FILL"];
const VALID_STYLES: &[i64] = &[8000, 8100, 8200, 8300, 8400];

/// Describes a resolved LCU HTTP mutation produced from a `RemoteCommand`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLcuCall {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

/// Validates a `RemoteCommand` and maps it to its allowlisted LCU endpoint(s).
pub fn resolve_lcu_command(
    cmd: &RemoteCommand,
    editable_page_id: Option<i64>,
) -> Result<Vec<ResolvedLcuCall>> {
    match cmd {
        RemoteCommand::SetLobbyPositions { first, second } => {
            let f = first.trim().to_ascii_uppercase();
            let s = second.trim().to_ascii_uppercase();
            if !VALID_POSITIONS.contains(&f.as_str()) {
                bail!("Invalid first position '{}'", first);
            }
            if !VALID_POSITIONS.contains(&s.as_str()) {
                bail!("Invalid second position '{}'", second);
            }
            if f != "FILL" && f == s {
                bail!("Primary and secondary roles must be different");
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PUT,
                path: "/lol-lobby/v2/lobby/members/localMember/position-preferences".to_string(),
                body: Some(json!({
                    "firstPreference": f,
                    "secondPreference": s,
                })),
            }])
        }
        RemoteCommand::StartMatchmaking => Ok(vec![ResolvedLcuCall {
            method: Method::POST,
            path: "/lol-lobby/v2/lobby/matchmaking/search".to_string(),
            body: None,
        }]),
        RemoteCommand::StopMatchmaking => Ok(vec![ResolvedLcuCall {
            method: Method::DELETE,
            path: "/lol-lobby/v2/lobby/matchmaking/search".to_string(),
            body: None,
        }]),
        RemoteCommand::AcceptReadyCheck => Ok(vec![ResolvedLcuCall {
            method: Method::POST,
            path: "/lol-matchmaking/v1/ready-check/accept".to_string(),
            body: None,
        }]),
        RemoteCommand::DeclineReadyCheck => Ok(vec![ResolvedLcuCall {
            method: Method::POST,
            path: "/lol-matchmaking/v1/ready-check/decline".to_string(),
            body: None,
        }]),
        RemoteCommand::HoverChampion {
            action_id,
            champion_id,
        } => {
            if *action_id < 0 {
                bail!("Invalid action_id {}", action_id);
            }
            if *champion_id <= 0 {
                bail!("Invalid champion_id {}", champion_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PATCH,
                path: format!("/lol-champ-select/v1/session/actions/{action_id}"),
                body: Some(json!({ "championId": champion_id })),
            }])
        }
        RemoteCommand::LockAction { action_id } => {
            if *action_id < 0 {
                bail!("Invalid action_id {}", action_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/actions/{action_id}/complete"),
                body: None,
            }])
        }
        RemoteCommand::SetSummonerSpells {
            spell1_id,
            spell2_id,
        } => {
            if *spell1_id <= 0 || *spell2_id <= 0 || spell1_id == spell2_id {
                bail!("Two distinct summoner spells are required");
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PATCH,
                path: "/lol-champ-select/v1/session/my-selection".to_string(),
                body: Some(json!({
                    "spell1Id": spell1_id,
                    "spell2Id": spell2_id,
                })),
            }])
        }
        RemoteCommand::SetSkin { skin_id } => {
            if *skin_id <= 0 {
                bail!("Invalid skin_id {}", skin_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PATCH,
                path: "/lol-champ-select/v1/session/my-selection".to_string(),
                body: Some(json!({ "selectedSkinId": skin_id })),
            }])
        }
        RemoteCommand::SelectRunePage { page_id } => {
            if *page_id <= 0 {
                bail!("Invalid rune page_id {}", page_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PUT,
                path: "/lol-perks/v1/currentpage".to_string(),
                body: Some(json!(page_id)),
            }])
        }
        RemoteCommand::SaveRunePage {
            name,
            primary_style_id,
            sub_style_id,
            selected_perk_ids,
        } => {
            if !VALID_STYLES.contains(primary_style_id) || !VALID_STYLES.contains(sub_style_id) {
                bail!("Invalid rune style ID");
            }
            if primary_style_id == sub_style_id {
                bail!("Primary and secondary rune trees must differ");
            }
            if selected_perk_ids.len() < 6 || selected_perk_ids.len() > 9 {
                bail!("Rune page requires between 6 and 9 perks");
            }
            let clean_name = if name.trim().is_empty() {
                "Hypatia Mobile".to_string()
            } else {
                name.trim().chars().take(32).collect()
            };
            let body = json!({
                "name": clean_name,
                "primaryStyleId": primary_style_id,
                "subStyleId": sub_style_id,
                "selectedPerkIds": selected_perk_ids,
                "current": true,
            });
            if let Some(page_id) = editable_page_id {
                Ok(vec![
                    ResolvedLcuCall {
                        method: Method::PUT,
                        path: format!("/lol-perks/v1/pages/{page_id}"),
                        body: Some(body),
                    },
                    ResolvedLcuCall {
                        method: Method::PUT,
                        path: "/lol-perks/v1/currentpage".to_string(),
                        body: Some(json!(page_id)),
                    },
                ])
            } else {
                Ok(vec![ResolvedLcuCall {
                    method: Method::POST,
                    path: "/lol-perks/v1/pages".to_string(),
                    body: Some(body),
                }])
            }
        }
        RemoteCommand::RequestPositionSwap { swap_id } => {
            if *swap_id < 0 {
                bail!("Invalid swap_id {}", swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondPositionSwap { swap_id, accept } => {
            if *swap_id < 0 {
                bail!("Invalid swap_id {}", swap_id);
            }
            let verb = if *accept { "accept" } else { "decline" };
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/{verb}"),
                body: None,
            }])
        }
        RemoteCommand::CancelPositionSwap { swap_id } => {
            if *swap_id < 0 {
                bail!("Invalid swap_id {}", swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::RequestPickOrderSwap { swap_id } => {
            if *swap_id < 0 {
                bail!("Invalid swap_id {}", swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondPickOrderSwap { swap_id, accept } => {
            if *swap_id < 0 {
                bail!("Invalid swap_id {}", swap_id);
            }
            let verb = if *accept { "accept" } else { "decline" };
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/{verb}"),
                body: None,
            }])
        }
        RemoteCommand::CancelPickOrderSwap { swap_id } => {
            if *swap_id < 0 {
                bail!("Invalid swap_id {}", swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::RequestChampionTrade { trade_id } => {
            if *trade_id < 0 {
                bail!("Invalid trade_id {}", trade_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/trades/{trade_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondChampionTrade { trade_id, accept } => {
            if *trade_id < 0 {
                bail!("Invalid trade_id {}", trade_id);
            }
            let verb = if *accept { "accept" } else { "decline" };
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/trades/{trade_id}/{verb}"),
                body: None,
            }])
        }
        RemoteCommand::CancelChampionTrade { trade_id } => {
            if *trade_id < 0 {
                bail!("Invalid trade_id {}", trade_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/trades/{trade_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::DemoSetPhase { .. } | RemoteCommand::DemoSimulateIncomingSwap => {
            bail!("Demo commands are only available in Demo Mode")
        }
    }
}

/// Dedicated LCU mutation client that only executes `RemoteCommand` variants.
#[derive(Clone)]
pub struct LcuActionClient {
    inner: reqwest::Client,
}

impl Default for LcuActionClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LcuActionClient {
    pub fn new() -> Self {
        let inner = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to initialize LcuActionClient");
        Self { inner }
    }

    pub async fn execute(
        &self,
        port: u16,
        auth_token: Option<&str>,
        cmd: &RemoteCommand,
        editable_page_id: Option<i64>,
    ) -> Result<Value> {
        let calls = resolve_lcu_command(cmd, editable_page_id)?;
        let mut last_body = Value::Null;

        for call in calls {
            let url = format!("https://127.0.0.1:{}{}", port, call.path);
            let mut req = self.inner.request(call.method.clone(), &url);
            if let Some(token) = auth_token {
                let auth_header = if token.starts_with("Basic ") || token.starts_with("Bearer ") {
                    token.to_string()
                } else {
                    format!("Basic {}", BASE64_STANDARD.encode(format!("riot:{token}")))
                };
                req = req.header("Authorization", auth_header);
            }
            if let Some(body) = &call.body {
                req = req.json(body);
            }
            let resp = req
                .send()
                .await
                .with_context(|| format!("LCU request {} {} failed", call.method, call.path))?;
            let status = resp.status();
            let bytes = resp.bytes().await.unwrap_or_default();
            last_body = if bytes.is_empty() {
                json!({ "ok": status.is_success(), "status": status.as_u16() })
            } else {
                serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                    json!({
                        "ok": status.is_success(),
                        "status": status.as_u16(),
                        "text": String::from_utf8_lossy(&bytes).to_string()
                    })
                })
            };
            if !status.is_success() {
                let msg = last_body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("LCU rejected the action");
                bail!("LCU {} {}: {}", status.as_u16(), call.path, msg);
            }
        }

        Ok(last_body)
    }
}

/// Tracks alert transitions so the mobile browser triggers notifications, vibration,
/// and audio chimes once per urgent event (ReadyCheck, Your Ban, Your Pick, Incoming Swap).
#[derive(Debug, Clone, Default)]
pub struct AlertTracker {
    pub seq: u64,
    last_signature: String,
    pub last_kind: Option<String>,
    pub last_title: Option<String>,
    pub last_body: Option<String>,
}

impl AlertTracker {
    pub fn observe(&mut self, signature: String, kind: &str, title: String, body: String) {
        if signature != self.last_signature {
            self.last_signature = signature;
            if !kind.is_empty() {
                self.seq += 1;
                self.last_kind = Some(kind.to_string());
                self.last_title = Some(title);
                self.last_body = Some(body);
            }
        }
    }
}

/// Interactive demo state so every mobile action works in `--demo` mode and persists
/// across demo ticks without being overwritten.
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
                    name: "Bruiser All-In (Conqueror)".to_string(),
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
        }
    }
}

impl DemoRemoteState {
    /// Applies a `RemoteCommand` to the interactive demo state and pushes updated
    /// synthetic LCU events into `AppState`'s `EventStore`.
    pub fn apply_command(&mut self, app_state: &AppState, cmd: &RemoteCommand) -> Result<()> {
        self.manual_override = true;
        match cmd {
            RemoteCommand::SetLobbyPositions { first, second } => {
                let _ = resolve_lcu_command(cmd, None)?;
                self.first_position = first.trim().to_ascii_uppercase();
                self.second_position = second.trim().to_ascii_uppercase();
                self.ally_positions[2] = self.first_position.clone();
            }
            RemoteCommand::StartMatchmaking => {
                self.phase = "Matchmaking".to_string();
                self.ready_response = "None".to_string();
            }
            RemoteCommand::StopMatchmaking => {
                self.phase = "Lobby".to_string();
            }
            RemoteCommand::AcceptReadyCheck => {
                self.ready_response = "Accepted".to_string();
                self.phase = "ChampSelect".to_string();
                self.my_action_id = 10;
                self.my_action_type = "ban".to_string();
                self.my_action_completed = false;
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
                    bail!("Select a champion first");
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
                    // Advance immediately from Ban turn to Pick turn so the user can test both!
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
                let _ = resolve_lcu_command(cmd, None)?;
                self.spell1_id = *spell1_id;
                self.spell2_id = *spell2_id;
            }
            RemoteCommand::SetSkin { skin_id } => {
                let _ = resolve_lcu_command(cmd, None)?;
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
                let _ = resolve_lcu_command(cmd, Some(101))?;
                let keystone = selected_perk_ids
                    .first()
                    .copied()
                    .map(keystone_name_from_id)
                    .unwrap_or("Keystone")
                    .to_string();
                for p in &mut self.rune_pages {
                    p.current = false;
                }
                if let Some(first) = self.rune_pages.first_mut() {
                    first.name = name.clone();
                    first.primary_style_id = *primary_style_id;
                    first.primary_style_name = rune_style_name(*primary_style_id).to_string();
                    first.sub_style_id = *sub_style_id;
                    first.sub_style_name = rune_style_name(*sub_style_id).to_string();
                    first.keystone_name = keystone;
                    first.selected_perk_ids = selected_perk_ids.clone();
                    first.current = true;
                }
            }
            RemoteCommand::RequestPositionSwap { swap_id } => {
                let idx = (*swap_id as usize).min(4);
                if idx != 2 {
                    self.position_swap_state[idx] = "SENT".to_string();
                }
            }
            RemoteCommand::RespondPositionSwap { swap_id, accept } => {
                let idx = (*swap_id as usize).min(4);
                if *accept && idx != 2 {
                    self.ally_positions.swap(2, idx);
                }
                self.position_swap_state[idx] = "AVAILABLE".to_string();
            }
            RemoteCommand::CancelPositionSwap { swap_id } => {
                let idx = (*swap_id as usize).min(4);
                self.position_swap_state[idx] = "AVAILABLE".to_string();
            }
            RemoteCommand::RequestPickOrderSwap { swap_id } => {
                let idx = (*swap_id as usize).min(4);
                if idx != 2 {
                    self.pick_swap_state[idx] = "SENT".to_string();
                }
            }
            RemoteCommand::RespondPickOrderSwap { swap_id, accept: _ } => {
                let idx = (*swap_id as usize).min(4);
                self.pick_swap_state[idx] = "AVAILABLE".to_string();
            }
            RemoteCommand::CancelPickOrderSwap { swap_id } => {
                let idx = (*swap_id as usize).min(4);
                self.pick_swap_state[idx] = "AVAILABLE".to_string();
            }
            RemoteCommand::RequestChampionTrade { trade_id } => {
                let idx = (*trade_id as usize).min(4);
                if idx != 2 {
                    self.trade_state[idx] = "SENT".to_string();
                }
            }
            RemoteCommand::RespondChampionTrade { trade_id, accept } => {
                let idx = (*trade_id as usize).min(4);
                if *accept && idx != 2 {
                    self.ally_champions.swap(2, idx);
                    self.my_champion_id = self.ally_champions[2];
                }
                self.trade_state[idx] = "AVAILABLE".to_string();
            }
            RemoteCommand::CancelChampionTrade { trade_id } => {
                let idx = (*trade_id as usize).min(4);
                self.trade_state[idx] = "AVAILABLE".to_string();
            }
            RemoteCommand::DemoSetPhase { phase } => {
                self.phase = phase.clone();
                if phase == "ReadyCheck" {
                    self.ready_response = "None".to_string();
                } else if phase == "ChampSelect" {
                    self.my_action_id = 10;
                    self.my_action_type = "ban".to_string();
                    self.my_action_completed = false;
                    self.sub_phase = "BAN_PICK".to_string();
                }
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
        let mut store = app_state.store.write().unwrap();
        let vault = &app_state.vault;

        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-gameflow/v1/gameflow-phase".to_string(),
            json!(self.phase),
            vault,
        );

        store.insert_event(
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
            }),
            vault,
        );

        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-matchmaking/v1/search".to_string(),
            json!({
                "estimatedQueueTime": 68.0,
                "isCurrentlyInQueue": self.phase == "Matchmaking" || self.phase == "ReadyCheck",
                "queueId": 420,
                "timeInQueue": 19.0
            }),
            vault,
        );

        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-matchmaking/v1/ready-check".to_string(),
            json!({
                "declinerIds": [],
                "dodgeWarning": "None",
                "playerResponse": self.ready_response,
                "state": if self.phase == "ReadyCheck" { "InProgress" } else { "Invalid" },
                "timer": 10
            }),
            vault,
        );

        let summoner_names = [
            "Nightfall#EUW",
            "KronoTrigger#EUW",
            "HypatiaObserver#EUW",
            "Sora#0001",
            "Moonwell#EUW",
        ];
        let ranks = [
            "GOLD II",
            "DIAMOND IV (12 LP)",
            "EMERALD II (54 LP)",
            "PLATINUM III",
            "GOLD I",
        ];
        let my_team: Vec<Value> = (0..5)
            .map(|i| {
                let cid = self.ally_champions[i];
                json!({
                    "cellId": i,
                    "assignedPosition": self.ally_positions[i],
                    "championId": cid,
                    "championName": champion_name_from_id(cid),
                    "summonerId": 1000 + i as u64,
                    "puuid": format!("demo-puuid-{i}"),
                    "summonerName": summoner_names[i],
                    "rank": ranks[i],
                    "isPremade": i == 1,
                    "spell1Id": if i == 2 { self.spell1_id } else { 4 },
                    "spell2Id": if i == 2 { self.spell2_id } else if i == 1 { 11 } else { 12 },
                    "selectedSkinId": if i == 2 { self.selected_skin_id } else { cid * 1000 }
                })
            })
            .collect();

        let pos_swaps: Vec<Value> = (0..5)
            .filter(|&i| i != 2)
            .map(|i| {
                json!({
                    "id": i,
                    "cellId": i,
                    "state": self.position_swap_state[i]
                })
            })
            .collect();

        if self.phase == "InProgress" {
            crate::fixtures::seed_demo_live_snapshot(&mut store, vault, 1185);
        }

        let pick_swaps: Vec<Value> = (0..5)
            .filter(|&i| i != 2)
            .map(|i| {
                json!({
                    "id": i,
                    "cellId": i,
                    "state": self.pick_swap_state[i]
                })
            })
            .collect();

        let trades: Vec<Value> = (0..5)
            .filter(|&i| i != 2)
            .map(|i| {
                json!({
                    "id": i,
                    "cellId": i,
                    "state": self.trade_state[i]
                })
            })
            .collect();

        let active_champ = if self.my_action_type == "ban" {
            self.my_banned_id
        } else {
            self.my_champion_id
        };

        store.insert_event(
            SourceId::LeagueClient,
            EventKind::Update,
            "/lol-champ-select/v1/session".to_string(),
            json!({
                "localPlayerCellId": 2,
                "queueType": "DRAFT_PICK",
                "myTeam": my_team,
                "theirTeam": [
                    { "cellId": 5, "championId": 122, "championName": "Darius" },
                    { "cellId": 6, "championId": 234, "championName": "Viego" },
                    { "cellId": 7, "championId": 0 },
                    { "cellId": 8, "championId": 0 },
                    { "cellId": 9, "championId": 0 }
                ],
                "actions": [
                    [
                        {
                            "id": self.my_action_id,
                            "actorCellId": 2,
                            "championId": active_champ,
                            "completed": self.my_action_completed,
                            "isInProgress": !self.my_action_completed,
                            "type": self.my_action_type
                        }
                    ]
                ],
                "bans": {
                    "myTeamBans": [self.my_banned_id, 157, 35],
                    "theirTeamBans": [777, 234, 233]
                },
                "positionSwaps": pos_swaps,
                "swaps": pick_swaps,
                "trades": trades,
                "timer": {
                    "phase": self.sub_phase,
                    "adjustedTimeLeftInPhase": 24
                }
            }),
            vault,
        );
    }
}

/// Builds a complete `RemoteStateSnapshot` from the current `AppState` and `DemoRemoteState`.
pub fn extract_remote_state(
    app_state: &AppState,
    demo_state: &DemoRemoteState,
    alerts: &mut AlertTracker,
) -> RemoteStateSnapshot {
    let guard = app_state.store.read().unwrap();
    let lcu = |uri: &str| guard.get_state(SourceId::LeagueClient, uri).map(|u| u.current);

    let phase = guard
        .phase()
        .map(|p| p.phase)
        .unwrap_or_else(|| "None".to_string());

    let summoner_val = lcu("/lol-summoner/v1/current-summoner");
    let lobby_val = lcu("/lol-lobby/v2/lobby");
    let search_val = lcu("/lol-matchmaking/v1/search");
    let ready_val = lcu("/lol-matchmaking/v1/ready-check");
    let cs_val = lcu("/lol-champ-select/v1/session");
    let pickable_val = lcu("/lol-champ-select/v1/pickable-champion-ids");
    let bannable_val = lcu("/lol-champ-select/v1/bannable-champion-ids");
    let skins_val = lcu("/lol-champ-select/v1/skin-carousel-skins");
    let pages_val = lcu("/lol-perks/v1/pages");
    drop(guard);

    let summoner_name = summoner_val
        .as_ref()
        .and_then(|s| {
            let gn = s
                .get("gameName")
                .or_else(|| s.get("displayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Player");
            let tag = s.get("tagLine").and_then(|v| v.as_str()).unwrap_or("");
            if tag.is_empty() {
                Some(gn.to_string())
            } else {
                Some(format!("{gn}#{tag}"))
            }
        })
        .unwrap_or_else(|| "HypatiaObserver#EUW".to_string());
    let profile_icon_id = summoner_val
        .as_ref()
        .and_then(|s| s.get("profileIconId"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // 1. Lobby state
    let lobby = lobby_val.as_ref().map(|lv| {
        let queue_id = lv
            .get("queueId")
            .or_else(|| lv.get("gameConfig").and_then(|gc| gc.get("queueId")))
            .and_then(|v| v.as_i64())
            .unwrap_or(420);
        let local_member = lv
            .get("localMember")
            .or_else(|| {
                lv.get("members")
                    .and_then(|m| m.as_array())
                    .and_then(|arr| arr.first())
            });
        let first_position = local_member
            .and_then(|m| m.get("firstPositionPreference"))
            .and_then(|v| v.as_str())
            .unwrap_or("MIDDLE")
            .to_uppercase();
        let second_position = local_member
            .and_then(|m| m.get("secondPositionPreference"))
            .and_then(|v| v.as_str())
            .unwrap_or("TOP")
            .to_uppercase();
        let is_leader = local_member
            .and_then(|m| m.get("isLeader"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let member_count = lv
            .get("members")
            .and_then(|m| m.as_array())
            .map(|a| a.len())
            .unwrap_or(1);

        RemoteLobbyState {
            queue_id,
            queue_name: queue_name_from_id(queue_id),
            first_position,
            second_position,
            is_leader,
            member_count,
        }
    });

    // 2. Matchmaking + ReadyCheck state
    let matchmaking = {
        let queue_id = search_val
            .as_ref()
            .and_then(|s| s.get("queueId"))
            .and_then(|v| v.as_i64())
            .or_else(|| lobby.as_ref().map(|l| l.queue_id))
            .unwrap_or(420);
        let in_queue = search_val
            .as_ref()
            .and_then(|s| s.get("isCurrentlyInQueue"))
            .and_then(|v| v.as_bool())
            .unwrap_or(phase == "Matchmaking" || phase == "ReadyCheck");
        let time_in_queue_sec = search_val
            .as_ref()
            .and_then(|s| s.get("timeInQueue"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as u32;
        let estimated_queue_time_sec = search_val
            .as_ref()
            .and_then(|s| s.get("estimatedQueueTime"))
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0) as u32;
        let rc_state = ready_val
            .as_ref()
            .and_then(|r| r.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("Invalid");
        let ready_check_active = phase == "ReadyCheck" || rc_state == "InProgress";
        let ready_check_timer_sec = ready_val
            .as_ref()
            .and_then(|r| r.get("timer"))
            .and_then(|v| v.as_f64())
            .map(|f| f as i32)
            .unwrap_or(10);
        let player_response = ready_val
            .as_ref()
            .and_then(|r| r.get("playerResponse"))
            .and_then(|v| v.as_str())
            .unwrap_or("None")
            .to_string();

        Some(RemoteMatchmakingState {
            queue_name: queue_name_from_id(queue_id),
            in_queue,
            time_in_queue_sec,
            estimated_queue_time_sec,
            ready_check_active,
            ready_check_timer_sec,
            player_response,
        })
    };

    // 3. Champion Select state
    let champ_select = cs_val.as_ref().map(|cs| {
        let base_summary = extract_champ_select_summary(Some(cs), None);
        let local_cell_id = cs
            .get("localPlayerCellId")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let timer = cs.get("timer");
        let sub_phase = timer
            .and_then(|t| t.get("phase"))
            .and_then(|v| v.as_str())
            .unwrap_or("BAN_PICK")
            .to_string();
        let raw_time = timer
            .and_then(|t| t.get("adjustedTimeLeftInPhase"))
            .and_then(|v| v.as_i64())
            .unwrap_or(25);
        // LCU sometimes reports milliseconds (>1000) and demo fixtures report seconds
        let time_left_sec = if raw_time > 500 { raw_time / 1000 } else { raw_time };
        let queue_type = cs
            .get("queueType")
            .and_then(|v| v.as_str())
            .unwrap_or("DRAFT_PICK")
            .to_string();

        // Parse swaps & trades maps by cellId
        let parse_swap_map = |key: &str| -> std::collections::HashMap<i64, (i64, String)> {
            let mut map = std::collections::HashMap::new();
            if let Some(arr) = cs.get(key).and_then(|v| v.as_array()) {
                for item in arr {
                    if let (Some(id), Some(cell_id)) = (
                        item.get("id").and_then(|v| v.as_i64()),
                        item.get("cellId").and_then(|v| v.as_i64()),
                    ) {
                        let state = item
                            .get("state")
                            .and_then(|v| v.as_str())
                            .unwrap_or("AVAILABLE")
                            .to_string();
                        map.insert(cell_id, (id, state));
                    }
                }
            }
            map
        };

        let pos_swaps = parse_swap_map("positionSwaps");
        let pick_swaps = parse_swap_map("swaps");
        let trades = parse_swap_map("trades");

        // Parse actions to find local player's active or pending action and acting teammate
        let mut active_action: Option<RemoteActiveAction> = None;
        let mut cell_acting: std::collections::HashMap<i64, String> =
            std::collections::HashMap::new();

        if let Some(turns) = cs.get("actions").and_then(|v| v.as_array()) {
            for turn in turns {
                if let Some(actions) = turn.as_array() {
                    for act in actions {
                        let actor_cell = act
                            .get("actorCellId")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(-1);
                        let action_id = act.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
                        let action_type = act
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pick")
                            .to_string();
                        let completed = act
                            .get("completed")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let in_progress = act
                            .get("isInProgress")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let champ_id = act
                            .get("championId")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        if in_progress && !completed {
                            cell_acting.insert(actor_cell, action_type.clone());
                        }

                        if actor_cell == local_cell_id {
                            // Prefer the in-progress action, or the next uncompleted action
                            let should_take = match &active_action {
                                None => true,
                                Some(prev) => {
                                    (in_progress && !prev.is_my_turn)
                                        || (!completed && prev.completed)
                                }
                            };
                            if should_take {
                                active_action = Some(RemoteActiveAction {
                                    action_id,
                                    action_type,
                                    champion_id: champ_id,
                                    champion_name: if champ_id > 0 {
                                        champion_name_from_id(champ_id)
                                    } else {
                                        String::new()
                                    },
                                    completed,
                                    is_my_turn: in_progress && !completed,
                                });
                            }
                        }
                    }
                }
            }
        }

        let mut assigned_position = "MIDDLE".to_string();
        let mut spell1_id = 4;
        let mut spell2_id = 14;
        let mut selected_skin_id = 0;
        let mut local_champ_id = 0;
        let mut my_team = Vec::new();
        let mut incoming_swaps = Vec::new();

        if let Some(arr) = cs.get("myTeam").and_then(|v| v.as_array()) {
            for (idx, m) in arr.iter().enumerate() {
                let cell_id = m
                    .get("cellId")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(idx as i64);
                let pos = m
                    .get("assignedPosition")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(match idx {
                        0 => "TOP",
                        1 => "JUNGLE",
                        2 => "MIDDLE",
                        3 => "BOTTOM",
                        _ => "UTILITY",
                    })
                    .to_uppercase();
                let cid = m
                    .get("championId")
                    .or_else(|| m.get("championPickIntent"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let cname = if cid > 0 {
                    m.get("championName")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| champion_name_from_id(cid))
                } else {
                    String::new()
                };
                let sname = m
                    .get("summonerName")
                    .or_else(|| m.get("gameName"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Ally {}", idx + 1));
                let s1 = m.get("spell1Id").and_then(|v| v.as_i64()).unwrap_or(4);
                let s2 = m.get("spell2Id").and_then(|v| v.as_i64()).unwrap_or(14);
                let skin = m
                    .get("selectedSkinId")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let is_self = cell_id == local_cell_id;
                if is_self {
                    assigned_position = pos.clone();
                    spell1_id = s1;
                    spell2_id = s2;
                    selected_skin_id = skin;
                    local_champ_id = cid;
                }

                let p_swap = pos_swaps.get(&cell_id).cloned();
                let o_swap = pick_swaps.get(&cell_id).cloned();
                let c_trade = trades.get(&cell_id).cloned();

                if let Some((id, ref st)) = p_swap {
                    if st == "RECEIVED" {
                        incoming_swaps.push(RemoteIncomingSwap {
                            id,
                            kind: "position".to_string(),
                            caller_name: sname.clone(),
                            detail: format!("wants to swap to your {assigned_position} lane (they have {pos})"),
                        });
                    }
                }
                if let Some((id, ref st)) = o_swap {
                    if st == "RECEIVED" {
                        incoming_swaps.push(RemoteIncomingSwap {
                            id,
                            kind: "pick_order".to_string(),
                            caller_name: sname.clone(),
                            detail: "wants to swap pick order with you".to_string(),
                        });
                    }
                }
                if let Some((id, ref st)) = c_trade {
                    if st == "RECEIVED" {
                        incoming_swaps.push(RemoteIncomingSwap {
                            id,
                            kind: "champion".to_string(),
                            caller_name: sname.clone(),
                            detail: format!(
                                "wants to trade champion ({})",
                                if cname.is_empty() { "Ally" } else { &cname }
                            ),
                        });
                    }
                }

                let acting_type = cell_acting.get(&cell_id).cloned();
                my_team.push(RemoteTeamMember {
                    cell_id,
                    assigned_position: pos,
                    champion_id: cid,
                    champion_name: cname,
                    summoner_name: sname,
                    spell1_id: s1,
                    spell2_id: s2,
                    is_self,
                    is_acting: acting_type.is_some(),
                    acting_type,
                    position_swap_id: p_swap.as_ref().map(|x| x.0),
                    position_swap_state: p_swap.map(|x| x.1),
                    pick_swap_id: o_swap.as_ref().map(|x| x.0),
                    pick_swap_state: o_swap.map(|x| x.1),
                    trade_id: c_trade.as_ref().map(|x| x.0),
                    trade_state: c_trade.map(|x| x.1),
                });
            }
        }

        // Filter pickable / bannable champions if LCU reported explicit lists
        let pickable_ids: Option<Vec<i64>> = pickable_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect());
        let bannable_ids: Option<Vec<i64>> = bannable_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect());

        let mut champions = builtin_champion_catalog();
        let synced_catalog = app_state.assets.synced_champion_catalog();
        if !synced_catalog.is_empty() {
            for extra in synced_catalog {
                if !champions.iter().any(|c| c.id == extra.id) {
                    champions.push(extra);
                }
            }
            champions.sort_by(|a, b| a.name.cmp(&b.name));
        }
        for c in &mut champions {
            if let Some(ref pids) = pickable_ids {
                if !pids.is_empty() {
                    c.pickable = pids.contains(&c.id);
                }
            }
            if let Some(ref bids) = bannable_ids {
                if !bids.is_empty() {
                    c.bannable = bids.contains(&c.id);
                }
            }
        }

        // Skins from LCU `/lol-champ-select/v1/skin-carousel-skins` or builtin fallback
        let champ_for_skins = if local_champ_id > 0 {
            local_champ_id
        } else {
            active_action
                .as_ref()
                .map(|a| a.champion_id)
                .unwrap_or(0)
        };
        let available_skins: Vec<RemoteSkinOption> = skins_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|s| {
                        s.get("unlocked")
                            .and_then(|u| u.as_bool())
                            .unwrap_or(true)
                    })
                    .filter_map(|s| {
                        let id = s.get("id")?.as_i64()?;
                        let name = s.get("name")?.as_str()?.to_string();
                        let tile_path = s
                            .get("tilePath")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !tile_path.is_empty() {
                            app_state.assets.register_skin_path(id, &tile_path);
                        }
                        Some(RemoteSkinOption {
                            id,
                            name,
                            selected: id == selected_skin_id,
                            tile_path,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                let cname = app_state.assets.resolve_champion_name(champ_for_skins);
                builtin_skins_for(champ_for_skins, &cname, selected_skin_id)
            });

        // Rune pages from LCU `/lol-perks/v1/pages` or demo state
        let rune_pages: Vec<RemoteRunePage> = pages_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let id = p.get("id")?.as_i64()?;
                        let name = p.get("name")?.as_str()?.to_string();
                        let primary_style_id =
                            p.get("primaryStyleId").and_then(|v| v.as_i64()).unwrap_or(8000);
                        let sub_style_id =
                            p.get("subStyleId").and_then(|v| v.as_i64()).unwrap_or(8400);
                        let selected_perk_ids: Vec<i64> = p
                            .get("selectedPerkIds")
                            .and_then(|v| v.as_array())
                            .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
                            .unwrap_or_default();
                        let current = p.get("current").and_then(|v| v.as_bool()).unwrap_or(false);
                        let is_editable = p
                            .get("isEditable")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        let keystone = selected_perk_ids
                            .first()
                            .copied()
                            .map(keystone_name_from_id)
                            .unwrap_or("Keystone")
                            .to_string();
                        Some(RemoteRunePage {
                            id,
                            name,
                            primary_style_id,
                            primary_style_name: rune_style_name(primary_style_id).to_string(),
                            sub_style_id,
                            sub_style_name: rune_style_name(sub_style_id).to_string(),
                            keystone_name: keystone,
                            selected_perk_ids,
                            current,
                            is_preset: !is_editable,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| demo_state.rune_pages.clone());

        let recommended_rune_pages = recommended_pages_for(champ_for_skins, &assigned_position);

        let my_team_bans = base_summary
            .as_ref()
            .map(|b| b.my_team_bans.clone())
            .unwrap_or_default();
        let their_team_bans = base_summary
            .as_ref()
            .map(|b| b.their_team_bans.clone())
            .unwrap_or_default();
        let my_team_ban_ids = my_team_bans
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let id = base_summary
                    .as_ref()
                    .and_then(|b| b.my_team_ban_ids.get(i).copied())
                    .unwrap_or(0);
                if id > 0 { id } else { app_state.assets.resolve_champion_id(n) }
            })
            .collect();
        let their_team_ban_ids = their_team_bans
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let id = base_summary
                    .as_ref()
                    .and_then(|b| b.their_team_ban_ids.get(i).copied())
                    .unwrap_or(0);
                if id > 0 { id } else { app_state.assets.resolve_champion_id(n) }
            })
            .collect();

        RemoteChampSelectState {
            sub_phase,
            time_left_sec,
            queue_type,
            local_cell_id,
            assigned_position,
            active_action,
            spell1_id,
            spell2_id,
            selected_skin_id,
            my_team,
            their_team: base_summary
                .as_ref()
                .map(|b| b.their_team.clone())
                .unwrap_or_default(),
            my_team_bans,
            their_team_bans,
            my_team_ban_ids,
            their_team_ban_ids,
            incoming_swaps,
            champions,
            available_skins,
            available_spells: default_summoner_spells(),
            rune_pages,
            recommended_rune_pages,
            rune_trees: default_rune_trees(),
        }
    });

    // 4. Determine urgent alert state (for mobile Notification + Vibration + Audio chime)
    if let Some(ref mm) = matchmaking {
        if mm.ready_check_active && mm.player_response == "None" {
            alerts.observe(
                "ready_check_pending".to_string(),
                "ready_check",
                "Match Found!".to_string(),
                format!("Tap ACCEPT now ({}s remaining)", mm.ready_check_timer_sec),
            );
        }
    }
    if phase == "ChampSelect" {
        if let Some(ref cs) = champ_select {
            if let Some(swap) = cs.incoming_swaps.first() {
                alerts.observe(
                    format!("swap:{}:{}", swap.kind, swap.id),
                    "incoming_swap",
                    format!("Swap Request from {}", swap.caller_name),
                    swap.detail.clone(),
                );
            } else if let Some(ref act) = cs.active_action {
                if act.is_my_turn {
                    let kind = if act.action_type == "ban" {
                        "your_ban"
                    } else {
                        "your_pick"
                    };
                    let title = if act.action_type == "ban" {
                        "Your Turn to Ban!".to_string()
                    } else {
                        format!("Your Turn to Pick ({})!", cs.assigned_position)
                    };
                    alerts.observe(
                        format!("turn:{}:{}", act.action_id, act.action_type),
                        kind,
                        title,
                        format!(
                            "Spells: {} + {} · {}s left",
                            spell_name_from_id(cs.spell1_id),
                            spell_name_from_id(cs.spell2_id),
                            cs.time_left_sec
                        ),
                    );
                }
            }
        }
    }

    let live_game = if phase == "InProgress" {
        let live_guard = app_state.store.read().unwrap();
        let live = |uri: &str| {
            live_guard
                .get_state(SourceId::LiveGame, uri)
                .map(|u| u.current)
        };
        let allgame_val = live("/liveclientdata/allgamedata");
        let active_val = live("/liveclientdata/activeplayer");
        let events_val = live("/liveclientdata/eventdata");
        drop(live_guard);
        extract_live_game_summary(
            allgame_val.as_ref(),
            active_val.as_ref(),
            events_val.as_ref(),
        )
        .map(|lg| {
            let board = extract_live_board(allgame_val.as_ref(), events_val.as_ref(), &lg);
            let clock_sec = lg.game_time_sec.max(0.0).round() as u32;
            let clock = format!("{:02}:{:02}", clock_sec / 60, clock_sec % 60);
            let my_team_name = lg
                .players
                .iter()
                .find(|p| p.is_self)
                .map(|p| p.team.clone())
                .unwrap_or_else(|| "ORDER".to_string());
            let map_player = |p: &hypatia_shared::LiveGamePlayerSummary| {
                let lane = match p.position.to_ascii_uppercase().as_str() {
                    "TOP" => "TOP",
                    "JUNGLE" => "JGL",
                    "MIDDLE" | "MID" => "MID",
                    "BOTTOM" | "BOT" => "BOT",
                    "UTILITY" | "SUPPORT" => "SUP",
                    _ => "FLEX",
                };
                let champion_id = if p.champion_id > 0 {
                    p.champion_id
                } else {
                    app_state
                        .assets
                        .resolve_champion_id_with_alias(&p.champion_name, &p.champion_alias)
                };
                RemoteLivePlayer {
                    lane: lane.to_string(),
                    champion_id,
                    champion: p.champion_name.clone(),
                    level: p.level,
                    kda: format!("{}/{}/{}", p.kills, p.deaths, p.assists),
                    cs: p.creep_score,
                    is_self: p.is_self,
                    dead: p.is_dead,
                    respawn_sec: p.respawn_timer_sec.ceil() as i32,
                }
            };
            let allies: Vec<RemoteLivePlayer> = lg
                .players
                .iter()
                .filter(|p| p.team == my_team_name)
                .map(map_player)
                .collect();
            let enemies: Vec<RemoteLivePlayer> = lg
                .players
                .iter()
                .filter(|p| p.team != my_team_name)
                .map(map_player)
                .collect();
            let my_towers = board
                .towers
                .iter()
                .filter(|t| t.order_side == board.my_team_is_order && t.standing)
                .count() as u32;
            let enemy_towers = board
                .towers
                .iter()
                .filter(|t| t.order_side != board.my_team_is_order && t.standing)
                .count() as u32;
            let objectives = board
                .objectives
                .iter()
                .rev()
                .take(8)
                .map(|o| RemoteLiveObjective {
                    label: o.label.clone(),
                    mine: o.by_order == board.my_team_is_order,
                    weight: o.kind.weight(),
                })
                .collect();
            let feed = board
                .feed
                .iter()
                .take(6)
                .map(|f| {
                    let sec = f.time_sec.max(0.0).round() as u32;
                    RemoteLiveFeedItem {
                        time: format!("{:02}:{:02}", sec / 60, sec % 60),
                        text: f.text.clone(),
                        side: match f.by_my_team {
                            Some(true) => 1,
                            Some(false) => 0,
                            None => -1,
                        },
                    }
                })
                .collect();

            let active_champion_id = if lg.active_champion_id > 0 {
                lg.active_champion_id
            } else {
                allies
                    .iter()
                    .find(|p| p.is_self && p.champion_id > 0)
                    .map(|p| p.champion_id)
                    .unwrap_or_else(|| app_state.assets.resolve_champion_id(&lg.active_champion))
            };
            RemoteLiveState {
                game_mode: lg.game_mode,
                clock,
                my_kills: board.my_kills(),
                enemy_kills: board.enemy_kills(),
                my_towers,
                enemy_towers,
                active_champion_id,
                active_champion: lg.active_champion,
                level: lg.level,
                current_gold: lg.current_gold,
                kda: format!("{}/{}/{}", lg.kills, lg.deaths, lg.assists),
                cs: lg.creep_score,
                allies,
                enemies,
                objectives,
                feed,
            }
        })
    } else {
        None
    };

    RemoteStateSnapshot {
        phase,
        is_demo: app_state.is_demo_active(),
        alert_seq: alerts.seq,
        alert_kind: alerts.last_kind.clone(),
        alert_title: alerts.last_title.clone(),
        alert_body: alerts.last_body.clone(),
        summoner_name,
        profile_icon_id,
        lobby,
        matchmaking,
        champ_select,
        live_game,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_allowlist_valid_commands() {
        let calls = resolve_lcu_command(&RemoteCommand::AcceptReadyCheck, None).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, Method::POST);
        assert_eq!(calls[0].path, "/lol-matchmaking/v1/ready-check/accept");

        let hover = resolve_lcu_command(
            &RemoteCommand::HoverChampion {
                action_id: 7,
                champion_id: 103,
            },
            None,
        )
        .unwrap();
        assert_eq!(hover[0].method, Method::PATCH);
        assert_eq!(hover[0].path, "/lol-champ-select/v1/session/actions/7");
        assert_eq!(hover[0].body, Some(json!({"championId": 103})));

        let swap = resolve_lcu_command(
            &RemoteCommand::RequestPositionSwap { swap_id: 3 },
            None,
        )
        .unwrap();
        assert_eq!(swap[0].method, Method::POST);
        assert_eq!(
            swap[0].path,
            "/lol-champ-select/v1/session/position-swaps/3/request"
        );
    }

    #[test]
    fn test_resolve_rejects_invalid_inputs() {
        assert!(resolve_lcu_command(
            &RemoteCommand::SetLobbyPositions {
                first: "MIDDLE".into(),
                second: "MIDDLE".into()
            },
            None
        )
        .is_err());

        assert!(resolve_lcu_command(
            &RemoteCommand::HoverChampion {
                action_id: -1,
                champion_id: 103
            },
            None
        )
        .is_err());

        assert!(resolve_lcu_command(
            &RemoteCommand::SetSummonerSpells {
                spell1_id: 4,
                spell2_id: 4
            },
            None
        )
        .is_err());
    }
}
