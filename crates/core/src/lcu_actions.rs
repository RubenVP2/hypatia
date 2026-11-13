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
    builtin_skins_for_localized, default_rune_trees_localized, default_summoner_spells_localized,
    keystone_name_from_id_localized, recommended_pages_for_localized, rune_style_name_localized,
    spell_name_from_id_localized, ChampionLookup, EventKind, Lang, Position,
    RemoteChampSelectState, RemoteCommand, RemoteIncomingSwap, RemoteLiveFeedItem,
    RemoteLiveObjective, RemoteLivePlayer, RemoteLiveState, RemoteLobbyState,
    RemoteMatchmakingState, RemoteRunePage, RemoteSkinOption, RemoteStateSnapshot,
    RemoteTeamMember, SourceId, SummonerSummary,
};
use reqwest::Method;
use serde_json::{json, Value};

use crate::AppState;

const VALID_STYLES: &[i64] = &[8000, 8100, 8200, 8300, 8400];

/// Describes a resolved LCU HTTP mutation produced from a `RemoteCommand`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLcuCall {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

/// Validates a `RemoteCommand` with localized validation errors and maps it to its allowlisted LCU endpoint(s).
pub fn resolve_lcu_command_localized(
    cmd: &RemoteCommand,
    editable_page_id: Option<i64>,
    lang: Lang,
) -> Result<Vec<ResolvedLcuCall>> {
    match cmd {
        RemoteCommand::SetLobbyPositions { first, second } => {
            let f = first.trim().to_ascii_uppercase();
            let s = second.trim().to_ascii_uppercase();
            if !Position::VALID_LCU_CODES.contains(&f.as_str()) {
                bail!(
                    "{}: '{}'",
                    lang.pick("Invalid primary position", "Position principale invalide"),
                    first
                );
            }
            if !Position::VALID_LCU_CODES.contains(&s.as_str()) {
                bail!(
                    "{}: '{}'",
                    lang.pick("Invalid secondary position", "Position secondaire invalide"),
                    second
                );
            }
            if f != "FILL" && f == s {
                bail!(
                    "{}",
                    lang.pick(
                        "Primary and secondary positions must be different",
                        "Les postes principal et secondaire doivent être différents",
                    )
                );
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
                bail!("{}: {}", lang.pick("Invalid action_id", "action_id invalide"), action_id);
            }
            if *champion_id <= 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid champion_id", "champion_id invalide"),
                    champion_id
                );
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PATCH,
                path: format!("/lol-champ-select/v1/session/actions/{action_id}"),
                body: Some(json!({ "championId": champion_id })),
            }])
        }
        RemoteCommand::LockAction { action_id } => {
            if *action_id < 0 {
                bail!("{}: {}", lang.pick("Invalid action_id", "action_id invalide"), action_id);
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
                bail!(
                    "{}",
                    lang.pick(
                        "Two distinct summoner spells are required",
                        "Deux sorts d'invocateur distincts sont requis",
                    )
                );
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
                bail!("{}: {}", lang.pick("Invalid skin_id", "skin_id invalide"), skin_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::PATCH,
                path: "/lol-champ-select/v1/session/my-selection".to_string(),
                body: Some(json!({ "selectedSkinId": skin_id })),
            }])
        }
        RemoteCommand::SelectRunePage { page_id } => {
            if *page_id <= 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid rune page ID", "ID de page de runes invalide"),
                    page_id
                );
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
                bail!(
                    "{}",
                    lang.pick("Invalid rune style ID", "ID de branche de runes invalide")
                );
            }
            if primary_style_id == sub_style_id {
                bail!(
                    "{}",
                    lang.pick(
                        "Primary and secondary rune styles must differ",
                        "Les branches principale et secondaire doivent être différentes",
                    )
                );
            }
            if selected_perk_ids.len() < 6 || selected_perk_ids.len() > 9 {
                bail!(
                    "{}",
                    lang.pick(
                        "Rune page requires 6 to 9 perk IDs",
                        "La page de runes nécessite entre 6 et 9 runes",
                    )
                );
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
                bail!("{}: {}", lang.pick("Invalid swap_id", "swap_id invalide"), swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondPositionSwap { swap_id, accept } => {
            if *swap_id < 0 {
                bail!("{}: {}", lang.pick("Invalid swap_id", "swap_id invalide"), swap_id);
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
                bail!("{}: {}", lang.pick("Invalid swap_id", "swap_id invalide"), swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::RequestPickOrderSwap { swap_id } => {
            if *swap_id < 0 {
                bail!("{}: {}", lang.pick("Invalid swap_id", "swap_id invalide"), swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondPickOrderSwap { swap_id, accept } => {
            if *swap_id < 0 {
                bail!("{}: {}", lang.pick("Invalid swap_id", "swap_id invalide"), swap_id);
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
                bail!("{}: {}", lang.pick("Invalid swap_id", "swap_id invalide"), swap_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::RequestChampionTrade { trade_id } => {
            if *trade_id < 0 {
                bail!("{}: {}", lang.pick("Invalid trade_id", "trade_id invalide"), trade_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/trades/{trade_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondChampionTrade { trade_id, accept } => {
            if *trade_id < 0 {
                bail!("{}: {}", lang.pick("Invalid trade_id", "trade_id invalide"), trade_id);
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
                bail!("{}: {}", lang.pick("Invalid trade_id", "trade_id invalide"), trade_id);
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/trades/{trade_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::DemoSetPhase { .. } | RemoteCommand::DemoSimulateIncomingSwap => {
            bail!(
                "{}",
                lang.pick(
                    "Demo commands are only available in Demo mode",
                    "Les commandes de démo ne sont disponibles qu'en mode Démo",
                )
            )
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

    pub async fn execute_localized(
        &self,
        port: u16,
        auth_token: Option<&str>,
        cmd: &RemoteCommand,
        editable_page_id: Option<i64>,
        lang: Lang,
    ) -> Result<Value> {
        let calls = resolve_lcu_command_localized(cmd, editable_page_id, lang)?;
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
            let resp = req.send().await.with_context(|| {
                format!(
                    "{} {} {}",
                    lang.pick("LCU request failed for", "La requête LCU a échoué pour"),
                    call.method,
                    call.path
                )
            })?;
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
                    .unwrap_or(lang.pick(
                        "League client rejected the action",
                        "Le client League a refusé l'action",
                    ));
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
    last_lang: Lang,
    pub last_kind: Option<String>,
    pub last_title: Option<String>,
    pub last_body: Option<String>,
}

impl AlertTracker {
    pub fn observe(
        &mut self,
        lang: Lang,
        signature: String,
        kind: &str,
        title: String,
        body: String,
    ) {
        if signature != self.last_signature {
            self.last_signature = signature;
            self.last_lang = lang;
            if !kind.is_empty() {
                self.seq += 1;
                self.last_kind = Some(kind.to_string());
                self.last_title = Some(title);
                self.last_body = Some(body);
            }
        } else if lang != self.last_lang && !kind.is_empty() {
            self.last_lang = lang;
            self.last_title = Some(title);
            self.last_body = Some(body);
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
        }
    }
}

impl DemoRemoteState {
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
                    "championName": app_state.assets.resolve_champion_name_localized(cid, app_state.lang()),
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
                    "adjustedTimeLeftInPhase": 24_000,
                    "totalTimeInPhase": 30_000
                }
            }),
            vault,
        );
    }
}

/// Builds a complete `RemoteStateSnapshot` from the current `AppState` and `DemoRemoteState`.
/// Projects the Mobile Remote view from [`AppState::get_summary`] enriched with
/// remote-only draft controls (pickable/bannable filter, skin carousel, rune pages, alerts).
pub fn extract_remote_state(
    app_state: &AppState,
    demo_state: &DemoRemoteState,
    alerts: &mut AlertTracker,
) -> RemoteStateSnapshot {
    let lang = app_state.lang();
    let summary = app_state.get_summary();

    let (pickable_val, bannable_val, skins_val, pages_val) = {
        let guard = app_state.store.read().unwrap();
        let lcu = |uri: &str| guard.current(SourceId::LeagueClient, uri);
        (
            lcu("/lol-champ-select/v1/pickable-champion-ids"),
            lcu("/lol-champ-select/v1/bannable-champion-ids"),
            lcu("/lol-champ-select/v1/skin-carousel-skins"),
            lcu("/lol-perks/v1/pages"),
        )
    };

    let phase = summary
        .phase
        .as_ref()
        .map(|p| p.phase.clone())
        .unwrap_or_else(|| "None".to_string());

    let summoner_name = summary
        .summoner
        .as_ref()
        .map(SummonerSummary::riot_id)
        .unwrap_or_default();
    let profile_icon_id = summary
        .summoner
        .as_ref()
        .map(|s| s.profile_icon_id)
        .unwrap_or(0);

    // 1. Lobby state (projected via SummarySnapshot)
    let lobby = summary.lobby.as_ref().map(|l| RemoteLobbyState {
        queue_id: l.queue_id,
        queue_name: l.queue_name.clone(),
        first_position: l.local_first_position.clone().unwrap_or_default(),
        second_position: l.local_second_position.clone().unwrap_or_default(),
        is_leader: l.local_is_leader,
        member_count: l.members.len(),
    });

    // 2. Matchmaking + ReadyCheck state (projected via SummarySnapshot)
    let matchmaking = Some({
        let mm = summary.matchmaking.as_ref();
        let queue_name = mm
            .map(|m| m.queue_name.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| lobby.as_ref().map(|l| l.queue_name.clone()))
            .unwrap_or_default();
        let in_queue = mm.is_some_and(|m| m.is_currently_in_queue)
            || phase == "Matchmaking"
            || phase == "ReadyCheck";
        let time_in_queue_sec = mm.map(|m| m.time_in_queue_sec as u32).unwrap_or(0);
        let estimated_queue_time_sec = mm.map(|m| m.estimated_queue_time_sec as u32).unwrap_or(0);
        let ready_check_active = phase == "ReadyCheck"
            || mm.and_then(|m| m.ready_check_state.as_deref()) == Some("InProgress");
        let ready_check_timer_sec = mm.and_then(|m| m.ready_check_timer_sec).unwrap_or(0);
        let player_response = mm
            .and_then(|m| m.player_response.clone())
            .unwrap_or_else(|| "None".to_string());

        RemoteMatchmakingState {
            queue_name,
            in_queue,
            time_in_queue_sec,
            estimated_queue_time_sec,
            ready_check_active,
            ready_check_timer_sec,
            player_response,
        }
    });

    // 3. Champion Select state (projected via SummarySnapshot + remote-only pickable/skins/runes)
    let champ_select = summary.champ_select.as_ref().map(|cs| {
        let self_ally = cs.my_team.iter().find(|m| m.is_self);
        let assigned_position = self_ally
            .map(|m| m.assigned_position.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "FILL".to_string());
        let spell1_id = self_ally.map(|m| m.spell1_id).unwrap_or(0);
        let spell2_id = self_ally.map(|m| m.spell2_id).unwrap_or(0);
        let selected_skin_id = self_ally.map(|m| m.selected_skin_id).unwrap_or(0);
        let local_champ_id = self_ally.map(|m| m.champion_id).unwrap_or(0);
        let my_pos_label = Position::parse(&assigned_position)
            .unwrap_or(Position::Fill)
            .display_label(lang);

        let mut incoming_swaps = Vec::new();
        let my_team: Vec<RemoteTeamMember> = cs
            .my_team
            .iter()
            .map(|ally| {
                let champ_name = if ally.has_champion() {
                    ally.champion_name.clone()
                } else {
                    String::new()
                };
                if let (Some(id), Some(ref st)) = (ally.position_swap_id, &ally.position_swap_state) {
                    if st == "RECEIVED" {
                        let their_pos = Position::parse(&ally.assigned_position)
                            .unwrap_or(Position::Fill)
                            .display_label(lang);
                        incoming_swaps.push(RemoteIncomingSwap {
                            id,
                            kind: "position".to_string(),
                            caller_name: ally.summoner_name.clone(),
                            detail: if lang.is_fr() {
                                format!(
                                    "veut échanger vers votre poste {my_pos_label} (actuellement {their_pos})"
                                )
                            } else {
                                format!(
                                    "wants to swap for your {my_pos_label} (currently {their_pos})"
                                )
                            },
                        });
                    }
                }
                if let (Some(id), Some(ref st)) = (ally.pick_swap_id, &ally.pick_swap_state) {
                    if st == "RECEIVED" {
                        incoming_swaps.push(RemoteIncomingSwap {
                            id,
                            kind: "pick_order".to_string(),
                            caller_name: ally.summoner_name.clone(),
                            detail: lang
                                .pick(
                                    "wants to swap pick order with you",
                                    "veut échanger l'ordre de sélection avec vous",
                                )
                                .to_string(),
                        });
                    }
                }
                if let (Some(id), Some(ref st)) = (ally.trade_id, &ally.trade_state) {
                    if st == "RECEIVED" {
                        let champ_label = if champ_name.is_empty() {
                            lang.pick("Ally", "Allié")
                        } else {
                            &champ_name
                        };
                        incoming_swaps.push(RemoteIncomingSwap {
                            id,
                            kind: "champion".to_string(),
                            caller_name: ally.summoner_name.clone(),
                            detail: format!(
                                "{} ({champ_label})",
                                lang.pick("wants to trade champions", "veut échanger de champion")
                            ),
                        });
                    }
                }

                RemoteTeamMember {
                    cell_id: ally.cell_id,
                    assigned_position: ally.assigned_position.clone(),
                    champion_id: ally.champion_id,
                    champion_name: champ_name,
                    summoner_name: ally.summoner_name.clone(),
                    spell1_id: ally.spell1_id,
                    spell2_id: ally.spell2_id,
                    is_self: ally.is_self,
                    is_acting: ally.acting_type.is_some(),
                    acting_type: ally.acting_type.clone(),
                    position_swap_id: ally.position_swap_id,
                    position_swap_state: ally.position_swap_state.clone(),
                    pick_swap_id: ally.pick_swap_id,
                    pick_swap_state: ally.pick_swap_state.clone(),
                    trade_id: ally.trade_id,
                    trade_state: ally.trade_state.clone(),
                }
            })
            .collect();

        // Filter pickable / bannable champions using synced/builtin ChampionLookup catalog
        let pickable_ids: Option<Vec<i64>> = pickable_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect());
        let bannable_ids: Option<Vec<i64>> = bannable_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect());

        let mut champions = app_state.assets.champion_catalog();
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
            cs.local_action.as_ref().map(|a| a.champion_id).unwrap_or(0)
        };
        let available_skins: Vec<RemoteSkinOption> = skins_val
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|s| s.get("unlocked").and_then(|u| u.as_bool()).unwrap_or(true))
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
                let cname = app_state
                    .assets
                    .resolve_champion_name_localized(champ_for_skins, lang);
                builtin_skins_for_localized(champ_for_skins, &cname, selected_skin_id, lang)
            });

        // Rune pages from LCU `/lol-perks/v1/pages` (or DemoRemoteState in demo mode)
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
                            .map(|kid| keystone_name_from_id_localized(kid, lang))
                            .unwrap_or(lang.pick("Keystone", "Rune fondamentale"))
                            .to_string();
                        Some(RemoteRunePage {
                            id,
                            name,
                            primary_style_id,
                            primary_style_name: rune_style_name_localized(primary_style_id, lang)
                                .to_string(),
                            sub_style_id,
                            sub_style_name: rune_style_name_localized(sub_style_id, lang)
                                .to_string(),
                            keystone_name: keystone,
                            selected_perk_ids,
                            current,
                            is_preset: !is_editable,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                if !app_state.is_demo_active() {
                    return Vec::new();
                }
                demo_state
                    .rune_pages
                    .iter()
                    .cloned()
                    .map(|mut rp| {
                        rp.primary_style_name =
                            rune_style_name_localized(rp.primary_style_id, lang).to_string();
                        rp.sub_style_name =
                            rune_style_name_localized(rp.sub_style_id, lang).to_string();
                        if let Some(&kid) = rp.selected_perk_ids.first() {
                            rp.keystone_name = keystone_name_from_id_localized(kid, lang).to_string();
                        }
                        if rp.id == 101 && rp.name == "Mid Burst (Electrocute)" {
                            rp.name = lang
                                .pick("Mid Burst (Electrocute)", "Mid Burst (Électrocution)")
                                .to_string();
                        } else if rp.id == 102 && rp.name == "All-In Fighter (Conqueror)" {
                            rp.name = lang
                                .pick(
                                    "All-In Fighter (Conqueror)",
                                    "Combattant All-In (Conquérant)",
                                )
                                .to_string();
                        }
                        rp
                    })
                    .collect()
            });

        let recommended_rune_pages =
            recommended_pages_for_localized(champ_for_skins, &assigned_position, lang);

        RemoteChampSelectState {
            sub_phase: cs.sub_phase.clone(),
            time_left_sec: cs.time_left_sec,
            total_time_sec: cs.total_time_sec,
            queue_type: cs.queue_type.clone(),
            local_cell_id: cs.local_cell_id,
            assigned_position,
            active_action: cs.local_action.clone(),
            spell1_id,
            spell2_id,
            selected_skin_id,
            my_team,
            their_team: cs.their_team.clone(),
            my_team_bans: cs.my_team_bans.clone(),
            their_team_bans: cs.their_team_bans.clone(),
            my_team_ban_ids: cs.my_team_ban_ids.clone(),
            their_team_ban_ids: cs.their_team_ban_ids.clone(),
            incoming_swaps,
            champions,
            available_skins,
            available_spells: default_summoner_spells_localized(lang),
            rune_pages,
            recommended_rune_pages,
            rune_trees: default_rune_trees_localized(lang),
        }
    });

    // 4. Determine urgent alert state (for mobile Notification + Vibration + Audio chime)
    if let Some(ref mm) = matchmaking {
        if mm.ready_check_active && mm.player_response == "None" {
            alerts.observe(
                lang,
                "ready_check_pending".to_string(),
                "ready_check",
                lang.pick("Match Found!", "Partie trouvée !").to_string(),
                if lang.is_fr() {
                    format!(
                        "Appuyez sur ACCEPTER maintenant ({}s restantes)",
                        mm.ready_check_timer_sec
                    )
                } else {
                    format!("Tap ACCEPT now ({}s remaining)", mm.ready_check_timer_sec)
                },
            );
        }
    }
    if phase == "ChampSelect" {
        if let Some(ref cs) = champ_select {
            if let Some(swap) = cs.incoming_swaps.first() {
                alerts.observe(
                    lang,
                    format!("swap:{}:{}", swap.kind, swap.id),
                    "incoming_swap",
                    if lang.is_fr() {
                        format!("Demande d'échange de {}", swap.caller_name)
                    } else {
                        format!("{} requests a swap", swap.caller_name)
                    },
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
                        lang.pick("Your turn to BAN!", "À vous de bannir !").to_string()
                    } else {
                        let pos = Position::parse(&cs.assigned_position)
                            .unwrap_or(Position::Fill)
                            .display_label(lang);
                        if lang.is_fr() {
                            format!("À vous de choisir ({pos}) !")
                        } else {
                            format!("Your turn to PICK ({pos})!")
                        }
                    };
                    let s1 = spell_name_from_id_localized(cs.spell1_id, lang);
                    let s2 = spell_name_from_id_localized(cs.spell2_id, lang);
                    let body = if lang.is_fr() {
                        format!("Sorts : {s1} + {s2} · {}s restantes", cs.time_left_sec)
                    } else {
                        format!("Spells: {s1} + {s2} · {}s left", cs.time_left_sec)
                    };
                    alerts.observe(
                        lang,
                        format!("turn:{}:{}", act.action_id, act.action_type),
                        kind,
                        title,
                        body,
                    );
                }
            }
        }
    }

    // 5. Live Game + Board state (projected via SummarySnapshot)
    let live_game = if phase == "InProgress" {
        summary.live_game.as_ref().map(|lg| {
            let board = summary.board.clone().unwrap_or_default();
            let clock_sec = lg.game_time_sec.max(0.0).round() as u32;
            let clock = format!("{:02}:{:02}", clock_sec / 60, clock_sec % 60);
            let my_team_name = lg
                .players
                .iter()
                .find(|p| p.is_self)
                .map(|p| p.team.as_str())
                .unwrap_or("ORDER");
            let map_player = |p: &hypatia_shared::LiveGamePlayerSummary| RemoteLivePlayer {
                lane: Position::parse(&p.position)
                    .map_or("FLEX", Position::short_tag)
                    .to_string(),
                champion_id: p.champion_id,
                champion: p.champion_name.clone(),
                level: p.level,
                kda: format!("{}/{}/{}", p.kills, p.deaths, p.assists),
                cs: p.creep_score,
                is_self: p.is_self,
                dead: p.is_dead,
                respawn_sec: p.respawn_timer_sec.ceil() as i32,
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

            RemoteLiveState {
                game_mode: lg.game_mode.clone(),
                clock,
                my_kills: board.my_kills(),
                enemy_kills: board.enemy_kills(),
                my_towers,
                enemy_towers,
                active_champion_id: lg.active_champion_id,
                active_champion: lg.active_champion.clone(),
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
        lang,
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
        let calls =
            resolve_lcu_command_localized(&RemoteCommand::AcceptReadyCheck, None, Lang::En)
                .unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, Method::POST);
        assert_eq!(calls[0].path, "/lol-matchmaking/v1/ready-check/accept");

        let hover = resolve_lcu_command_localized(
            &RemoteCommand::HoverChampion {
                action_id: 7,
                champion_id: 103,
            },
            None,
            Lang::En,
        )
        .unwrap();
        assert_eq!(hover[0].method, Method::PATCH);
        assert_eq!(hover[0].path, "/lol-champ-select/v1/session/actions/7");
        assert_eq!(hover[0].body, Some(json!({"championId": 103})));

        let swap = resolve_lcu_command_localized(
            &RemoteCommand::RequestPositionSwap { swap_id: 3 },
            None,
            Lang::En,
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
        assert!(resolve_lcu_command_localized(
            &RemoteCommand::SetLobbyPositions {
                first: "MIDDLE".into(),
                second: "MIDDLE".into()
            },
            None,
            Lang::En,
        )
        .is_err());

        assert!(resolve_lcu_command_localized(
            &RemoteCommand::HoverChampion {
                action_id: -1,
                champion_id: 103
            },
            None,
            Lang::En,
        )
        .is_err());

        assert!(resolve_lcu_command_localized(
            &RemoteCommand::SetSummonerSpells {
                spell1_id: 4,
                spell2_id: 4
            },
            None,
            Lang::En,
        )
        .is_err());

        let en_err = resolve_lcu_command_localized(
            &RemoteCommand::SetSummonerSpells {
                spell1_id: 4,
                spell2_id: 4,
            },
            None,
            Lang::En,
        )
        .unwrap_err()
        .to_string();
        let fr_err = resolve_lcu_command_localized(
            &RemoteCommand::SetSummonerSpells {
                spell1_id: 4,
                spell2_id: 4,
            },
            None,
            Lang::Fr,
        )
        .unwrap_err()
        .to_string();
        assert_ne!(en_err, fr_err);
        assert!(fr_err.contains("sorts"));
    }

    #[tokio::test]
    async fn test_extract_remote_state_tracks_language_swaps_and_phases() {
        let app = AppState::start_with_lang(true, 2.0, Lang::En);
        let mut demo = DemoRemoteState::default();
        demo.sync_to_store(&app);
        let mut alerts = AlertTracker::default();

        let en = extract_remote_state(&app, &demo, &mut alerts);
        assert_eq!(en.lang, Lang::En);
        assert_eq!(en.lobby.as_ref().unwrap().queue_name, "Ranked Solo/Duo");
        assert_eq!(en.champ_select.as_ref().unwrap().available_spells[0].name, "Flash");
        assert_eq!(alerts.last_kind.as_deref(), Some("your_ban"));

        // Incoming swap in ChampSelect triggers alert with caller's true local position
        demo.apply_command(&app, &RemoteCommand::DemoSimulateIncomingSwap)
            .unwrap();
        let with_swap = extract_remote_state(&app, &demo, &mut alerts);
        assert!(!with_swap
            .champ_select
            .as_ref()
            .unwrap()
            .incoming_swaps
            .is_empty());
        assert_eq!(alerts.last_kind.as_deref(), Some("incoming_swap"));

        // Live InProgress phase derives board, K/D/A, and short lane tags via shared projection
        demo.apply_command(
            &app,
            &RemoteCommand::DemoSetPhase {
                phase: "InProgress".to_string(),
            },
        )
        .unwrap();
        {
            let mut store = app.store.write().unwrap();
            store.insert_event(
                SourceId::LiveGame,
                EventKind::Poll,
                "/liveclientdata/allgamedata".to_string(),
                json!({
                    "gameData": {"gameMode": "CLASSIC", "gameTime": 612.0},
                    "activePlayer": {"riotId": "HypatiaObserver#EUW", "level": 11, "currentGold": 1450},
                    "allPlayers": [
                        {"riotId": "HypatiaObserver#EUW", "championName": "Ahri", "team": "ORDER", "position": "MIDDLE", "level": 11},
                        {"riotId": "Ally#EUW", "championName": "Lee Sin", "team": "ORDER", "position": "JUNGLE", "level": 10}
                    ]
                }),
                &app.vault,
            );
        }
        let live_remote = extract_remote_state(&app, &demo, &mut alerts);
        let lg = live_remote.live_game.expect("expected live game projection");
        assert!(lg.allies.iter().any(|p| p.lane == "JGL"));
        assert!(lg.allies.iter().any(|p| p.lane == "MID"));

        // Language toggle propagates cleanly through projected names & spells
        demo.apply_command(
            &app,
            &RemoteCommand::DemoSetPhase {
                phase: "ChampSelect".to_string(),
            },
        )
        .unwrap();
        app.set_lang(Lang::Fr);
        let fr = extract_remote_state(&app, &demo, &mut alerts);
        assert_eq!(fr.lang, Lang::Fr);
        assert_eq!(fr.lobby.as_ref().unwrap().queue_name, "Classée Solo/Duo");
        assert_eq!(fr.champ_select.as_ref().unwrap().available_spells[0].name, "Saut éclair");
    }
}
