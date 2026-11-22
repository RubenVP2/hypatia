//! Opt-in LCU action client and state extractor for the Mobile Remote companion.
//!
//! Architectural boundary:
//! - `ReadOnlyRiotClient` remains strictly GET-only for the desktop observatory.
//! - `LcuActionClient` is only invoked by the Mobile Remote server when explicitly enabled
//!   by the user, and ONLY accepts variants of the closed `RemoteCommand` enum.

use std::fmt;

use anyhow::{bail, Context, Result};
pub use hypatia_shared::AlertTracker;
use hypatia_shared::{project_remote_state, Lang, Position, RemoteCommand, RemoteStateSnapshot};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::riot_http::{build_loopback_client, format_riot_auth_header};
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid action_id", "action_id invalide"),
                    action_id
                );
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid action_id", "action_id invalide"),
                    action_id
                );
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid skin_id", "skin_id invalide"),
                    skin_id
                );
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid swap_id", "swap_id invalide"),
                    swap_id
                );
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondPositionSwap { swap_id, accept } => {
            if *swap_id < 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid swap_id", "swap_id invalide"),
                    swap_id
                );
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid swap_id", "swap_id invalide"),
                    swap_id
                );
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/position-swaps/{swap_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::RequestPickOrderSwap { swap_id } => {
            if *swap_id < 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid swap_id", "swap_id invalide"),
                    swap_id
                );
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondPickOrderSwap { swap_id, accept } => {
            if *swap_id < 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid swap_id", "swap_id invalide"),
                    swap_id
                );
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid swap_id", "swap_id invalide"),
                    swap_id
                );
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/swaps/{swap_id}/cancel"),
                body: None,
            }])
        }
        RemoteCommand::RequestChampionTrade { trade_id } => {
            if *trade_id < 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid trade_id", "trade_id invalide"),
                    trade_id
                );
            }
            Ok(vec![ResolvedLcuCall {
                method: Method::POST,
                path: format!("/lol-champ-select/v1/session/trades/{trade_id}/request"),
                body: None,
            }])
        }
        RemoteCommand::RespondChampionTrade { trade_id, accept } => {
            if *trade_id < 0 {
                bail!(
                    "{}: {}",
                    lang.pick("Invalid trade_id", "trade_id invalide"),
                    trade_id
                );
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
                bail!(
                    "{}: {}",
                    lang.pick("Invalid trade_id", "trade_id invalide"),
                    trade_id
                );
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
        Self {
            inner: build_loopback_client(),
        }
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
        self.execute_calls_localized(port, auth_token, &calls, lang)
            .await
    }

    pub async fn execute_calls_localized(
        &self,
        port: u16,
        auth_token: Option<&str>,
        calls: &[ResolvedLcuCall],
        lang: Lang,
    ) -> Result<Value> {
        let mut last_body = Value::Null;

        for call in calls {
            let url = format!("https://127.0.0.1:{}{}", port, call.path);
            let body_preview = call
                .body
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<none>".to_string());
            info!(
                "[remote:lcu] -> {} {} body={}",
                call.method, call.path, body_preview
            );
            let mut req = self.inner.request(call.method.clone(), &url);
            if let Some(token) = auth_token {
                req = req.header("Authorization", format_riot_auth_header(token));
            }
            if let Some(body) = &call.body {
                req = req.json(body);
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(err) => {
                    warn!(
                        "[remote:lcu] <- {} {} TRANSPORT_ERR: {:#}",
                        call.method, call.path, err
                    );
                    return Err(err).with_context(|| {
                        format!(
                            "{} {} {}",
                            lang.pick("LCU request failed for", "La requête LCU a échoué pour"),
                            call.method,
                            call.path
                        )
                    });
                }
            };
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
            let resp_text = last_body.to_string();
            let resp_preview: String = resp_text.chars().take(512).collect();
            if !status.is_success() {
                warn!(
                    "[remote:lcu] <- {} {} FAILED status={} body={}",
                    call.method,
                    call.path,
                    status.as_u16(),
                    resp_preview
                );
                let msg = last_body
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or(lang.pick(
                        "League client rejected the action",
                        "Le client League a refusé l'action",
                    ));
                bail!("LCU {} {}: {}", status.as_u16(), call.path, msg);
            }
            info!(
                "[remote:lcu] <- {} {} status={} body={}",
                call.method,
                call.path,
                status.as_u16(),
                resp_preview
            );
        }

        Ok(last_body)
    }
}

pub use crate::fixtures::DemoRemoteState;

/// Error returned when dispatching a [`RemoteCommand`] through [`AppState::execute_remote_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteActionError {
    BadRequest(String),
    ClientUnavailable(String),
    GatewayError(String),
}

impl fmt::Display for RemoteActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(msg) | Self::ClientUnavailable(msg) | Self::GatewayError(msg) => {
                f.write_str(msg)
            }
        }
    }
}

impl std::error::Error for RemoteActionError {}

/// Returns only the LCU resources affected by `cmd` so post-action refresh skips unrelated URIs.
pub fn affected_lcu_uris(cmd: &RemoteCommand) -> &'static [&'static str] {
    match cmd {
        RemoteCommand::SetLobbyPositions { .. } => &["/lol-lobby/v2/lobby"],
        RemoteCommand::StartMatchmaking | RemoteCommand::StopMatchmaking => {
            &["/lol-matchmaking/v1/search", "/lol-lobby/v2/lobby"]
        }
        RemoteCommand::AcceptReadyCheck | RemoteCommand::DeclineReadyCheck => &[
            "/lol-matchmaking/v1/ready-check",
            "/lol-matchmaking/v1/search",
        ],
        RemoteCommand::HoverChampion { .. }
        | RemoteCommand::SetSummonerSpells { .. }
        | RemoteCommand::RequestPositionSwap { .. }
        | RemoteCommand::RespondPositionSwap { .. }
        | RemoteCommand::CancelPositionSwap { .. }
        | RemoteCommand::RequestPickOrderSwap { .. }
        | RemoteCommand::RespondPickOrderSwap { .. }
        | RemoteCommand::CancelPickOrderSwap { .. }
        | RemoteCommand::RequestChampionTrade { .. }
        | RemoteCommand::CancelChampionTrade { .. } => &["/lol-champ-select/v1/session"],
        RemoteCommand::LockAction { .. }
        | RemoteCommand::SetSkin { .. }
        | RemoteCommand::RespondChampionTrade { .. } => &[
            "/lol-champ-select/v1/session",
            "/lol-champ-select/v1/skin-carousel-skins",
        ],
        RemoteCommand::SelectRunePage { .. } | RemoteCommand::SaveRunePage { .. } => {
            &["/lol-perks/v1/pages"]
        }
        RemoteCommand::DemoSetPhase { .. } | RemoteCommand::DemoSimulateIncomingSwap => &[],
    }
}

/// Builds a complete `RemoteStateSnapshot` from `AppState`'s cached `Match Projection`.
pub fn extract_remote_state(
    app_state: &AppState,
    alerts: &mut AlertTracker,
) -> RemoteStateSnapshot {
    let summary = app_state.get_summary();
    let mut snapshot = project_remote_state(
        &summary,
        app_state.is_demo_active(),
        app_state.lang(),
        alerts,
    );
    snapshot.stats_revision = app_state.stats.revision();
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_shared::{EventKind, SourceId};

    #[test]
    fn test_resolve_allowlist_valid_commands() {
        let calls = resolve_lcu_command_localized(&RemoteCommand::AcceptReadyCheck, None, Lang::En)
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
        app.set_demo_phase("ChampSelect");
        let mut alerts = AlertTracker::default();

        let en = extract_remote_state(&app, &mut alerts);
        assert_eq!(en.lang, Lang::En);
        assert_eq!(en.lobby.as_ref().unwrap().queue_name, "Ranked Solo/Duo");
        assert_eq!(
            en.champ_select.as_ref().unwrap().available_spells[0].name,
            "Flash"
        );
        assert_eq!(alerts.last_kind.as_deref(), Some("your_ban"));

        // Incoming swap in ChampSelect triggers alert with caller's true local position
        app.demo_remote
            .write()
            .unwrap()
            .apply_command(&app, &RemoteCommand::DemoSimulateIncomingSwap)
            .unwrap();
        let with_swap = extract_remote_state(&app, &mut alerts);
        assert!(!with_swap
            .champ_select
            .as_ref()
            .unwrap()
            .incoming_swaps
            .is_empty());
        assert_eq!(alerts.last_kind.as_deref(), Some("incoming_swap"));

        // Live InProgress phase derives board, K/D/A, and short lane tags via shared projection
        app.set_demo_phase("InProgress");
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
        let live_remote = extract_remote_state(&app, &mut alerts);
        let lg = live_remote
            .live_game
            .expect("expected live game projection");
        assert!(lg.allies.iter().any(|p| p.lane == "JGL"));
        assert!(lg.allies.iter().any(|p| p.lane == "MID"));

        // Language toggle propagates cleanly through projected names & spells
        app.set_demo_phase("ChampSelect");
        app.set_lang(Lang::Fr);
        let fr = extract_remote_state(&app, &mut alerts);
        assert_eq!(fr.lang, Lang::Fr);
        assert_eq!(fr.lobby.as_ref().unwrap().queue_name, "Classée Solo/Duo");
        assert_eq!(
            fr.champ_select.as_ref().unwrap().available_spells[0].name,
            "Saut éclair"
        );
        assert_eq!(
            fr.champ_select.as_ref().unwrap().rune_pages[0].name,
            "Mid Burst (Électrocution)"
        );
    }

    #[test]
    fn test_affected_lcu_uris_targets_only_relevant_resources() {
        assert_eq!(
            affected_lcu_uris(&RemoteCommand::SetLobbyPositions {
                first: "MIDDLE".into(),
                second: "TOP".into()
            }),
            &["/lol-lobby/v2/lobby"]
        );
        assert_eq!(
            affected_lcu_uris(&RemoteCommand::SetSkin { skin_id: 103027 }),
            &[
                "/lol-champ-select/v1/session",
                "/lol-champ-select/v1/skin-carousel-skins",
            ]
        );
        assert_eq!(
            affected_lcu_uris(&RemoteCommand::RespondChampionTrade {
                trade_id: 1,
                accept: true,
            }),
            &[
                "/lol-champ-select/v1/session",
                "/lol-champ-select/v1/skin-carousel-skins",
            ]
        );
        assert_eq!(
            affected_lcu_uris(&RemoteCommand::SelectRunePage { page_id: 101 }),
            &["/lol-perks/v1/pages"]
        );
    }

    #[tokio::test]
    async fn test_execute_remote_command_dispatches_demo_and_reports_unavailable_lcu() {
        let app = AppState::start_with_lang(true, 1.0, Lang::En);
        let client = LcuActionClient::new();

        app.execute_remote_command(
            &client,
            &RemoteCommand::DemoSetPhase {
                phase: "ChampSelect".to_string(),
            },
        )
        .await
        .unwrap();

        let bad = app
            .execute_remote_command(
                &client,
                &RemoteCommand::HoverChampion {
                    action_id: 10,
                    champion_id: 0,
                },
            )
            .await;
        assert!(matches!(bad, Err(RemoteActionError::BadRequest(_))));

        app.set_demo_active(false, 1.0);
        // Validation errors return BadRequest even when live LCU is off/offline.
        let bad_live = app
            .execute_remote_command(
                &client,
                &RemoteCommand::SetSummonerSpells {
                    spell1_id: 4,
                    spell2_id: 4,
                },
            )
            .await;
        assert!(matches!(bad_live, Err(RemoteActionError::BadRequest(_))));

        // Only assert ClientUnavailable when no real League Client happens to be running on host.
        if app.discovery.lcu_args().and_then(|a| a.app_port).is_none() {
            let unavail = app
                .execute_remote_command(&client, &RemoteCommand::AcceptReadyCheck)
                .await;
            assert!(matches!(
                unavail,
                Err(RemoteActionError::ClientUnavailable(_))
            ));
        }
    }
}
