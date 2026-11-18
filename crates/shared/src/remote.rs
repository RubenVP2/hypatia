use serde::{Deserialize, Serialize};

use crate::champions::Position;
use crate::i18n::Lang;
use crate::summary::{
    format_clock_mm_ss, ChampSelectEnemy, LiveGamePlayerSummary, SummarySnapshot, SummonerSummary,
};

/// Strict, closed allowlist of user-initiated commands accepted by the Mobile Remote.
///
/// INVARIANT: No arbitrary URI, HTTP method, or raw payload can be passed from the mobile
/// client. Every variant maps to one specific LCU endpoint with validated parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum RemoteCommand {
    /// Choose primary and secondary role in Lobby (`TOP`, `JUNGLE`, `MIDDLE`, `BOTTOM`, `UTILITY`, `FILL`).
    SetLobbyPositions {
        first: String,
        second: String,
    },
    /// Start matchmaking queue from Lobby.
    StartMatchmaking,
    /// Cancel matchmaking queue.
    StopMatchmaking,
    /// Accept the Ready Check ("Match Found").
    AcceptReadyCheck,
    /// Decline the Ready Check.
    DeclineReadyCheck,
    /// Hover a champion during the player's ban or pick action.
    HoverChampion {
        action_id: i64,
        champion_id: i64,
    },
    /// Lock in the currently hovered ban or pick action.
    LockAction {
        action_id: i64,
    },
    /// Select both summoner spells (`spell1_id` = D, `spell2_id` = F).
    SetSummonerSpells {
        spell1_id: i64,
        spell2_id: i64,
    },
    /// Select a skin for the locked-in champion.
    SetSkin {
        skin_id: i64,
    },
    /// Activate an existing rune page by ID.
    SelectRunePage {
        page_id: i64,
    },
    /// Save and activate a rune page (recommended preset or custom 1-hand builder).
    SaveRunePage {
        name: String,
        primary_style_id: i64,
        sub_style_id: i64,
        selected_perk_ids: Vec<i64>,
    },
    /// Request a role / lane swap with an ally (`position-swaps/{swap_id}/request`).
    RequestPositionSwap {
        swap_id: i64,
    },
    /// Accept or decline an incoming role / lane swap (`position-swaps/{swap_id}/accept|decline`).
    RespondPositionSwap {
        swap_id: i64,
        accept: bool,
    },
    /// Cancel an outgoing role / lane swap request.
    CancelPositionSwap {
        swap_id: i64,
    },
    /// Request a pick-order swap with an ally (`swaps/{swap_id}/request`).
    RequestPickOrderSwap {
        swap_id: i64,
    },
    /// Accept or decline an incoming pick-order swap.
    RespondPickOrderSwap {
        swap_id: i64,
        accept: bool,
    },
    /// Cancel an outgoing pick-order swap.
    CancelPickOrderSwap {
        swap_id: i64,
    },
    /// Request a champion trade with an ally (`trades/{trade_id}/request`).
    RequestChampionTrade {
        trade_id: i64,
    },
    /// Accept or decline an incoming champion trade.
    RespondChampionTrade {
        trade_id: i64,
        accept: bool,
    },
    /// Cancel an outgoing champion trade.
    CancelChampionTrade {
        trade_id: i64,
    },
    /// In Demo Mode only: jump directly to a phase (`Lobby`, `Matchmaking`, `ReadyCheck`, `ChampSelect`, `InProgress`).
    DemoSetPhase {
        phase: String,
    },
    /// In Demo Mode only: simulate an incoming role/lane swap request from an ally.
    DemoSimulateIncomingSwap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RemoteStateSnapshot {
    pub phase: String,
    pub is_demo: bool,
    #[serde(default)]
    pub lang: Lang,
    pub alert_seq: u64,
    pub alert_kind: Option<String>,
    pub alert_title: Option<String>,
    pub alert_body: Option<String>,
    pub summoner_name: String,
    #[serde(default)]
    pub profile_icon_id: i64,
    pub lobby: Option<RemoteLobbyState>,
    pub matchmaking: Option<RemoteMatchmakingState>,
    pub champ_select: Option<RemoteChampSelectState>,
    #[serde(default)]
    pub live_game: Option<RemoteLiveState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteLiveFeedItem {
    pub time: String,
    pub text: String,
    pub side: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLiveObjective {
    pub label: String,
    pub mine: bool,
    pub weight: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLivePlayer {
    pub lane: String,
    #[serde(default)]
    pub champion_id: i64,
    pub champion: String,
    pub level: u32,
    pub kda: String,
    pub cs: u32,
    pub is_self: bool,
    pub dead: bool,
    pub respawn_sec: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteLiveState {
    pub game_mode: String,
    pub clock: String,
    pub my_kills: u32,
    pub enemy_kills: u32,
    pub my_towers: u32,
    pub enemy_towers: u32,
    #[serde(default)]
    pub active_champion_id: i64,
    pub active_champion: String,
    pub level: u32,
    pub current_gold: i64,
    pub kda: String,
    pub cs: u32,
    pub allies: Vec<RemoteLivePlayer>,
    pub enemies: Vec<RemoteLivePlayer>,
    pub objectives: Vec<RemoteLiveObjective>,
    pub feed: Vec<RemoteLiveFeedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLobbyState {
    pub queue_id: i64,
    pub queue_name: String,
    pub first_position: String,
    pub second_position: String,
    pub is_leader: bool,
    pub member_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteMatchmakingState {
    pub queue_name: String,
    pub in_queue: bool,
    pub time_in_queue_sec: u32,
    pub estimated_queue_time_sec: u32,
    pub ready_check_active: bool,
    pub ready_check_timer_sec: i32,
    pub player_response: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteActiveAction {
    pub action_id: i64,
    /// `"ban"` or `"pick"`
    pub action_type: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub completed: bool,
    pub is_my_turn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTeamMember {
    pub cell_id: i64,
    pub assigned_position: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub summoner_name: String,
    pub spell1_id: i64,
    pub spell2_id: i64,
    pub is_self: bool,
    pub is_acting: bool,
    pub acting_type: Option<String>,
    pub position_swap_id: Option<i64>,
    pub position_swap_state: Option<String>,
    pub pick_swap_id: Option<i64>,
    pub pick_swap_state: Option<String>,
    pub trade_id: Option<i64>,
    pub trade_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIncomingSwap {
    pub id: i64,
    /// `"position"`, `"pick_order"`, or `"champion"`
    pub kind: String,
    pub caller_name: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteChampionOption {
    pub id: i64,
    pub name: String,
    pub roles: Vec<String>,
    pub pickable: bool,
    pub bannable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSkinOption {
    pub id: i64,
    pub name: String,
    pub selected: bool,
    #[serde(default)]
    pub tile_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSpellOption {
    pub id: i64,
    pub name: String,
    pub key: String,
    pub cooldown: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRunePage {
    pub id: i64,
    pub name: String,
    pub primary_style_id: i64,
    pub primary_style_name: String,
    pub sub_style_id: i64,
    pub sub_style_name: String,
    pub keystone_name: String,
    pub selected_perk_ids: Vec<i64>,
    pub current: bool,
    pub is_preset: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePerkOption {
    pub id: i64,
    pub name: String,
    pub short_desc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRuneTree {
    pub id: i64,
    pub name: String,
    pub tagline: String,
    pub keystones: Vec<RemotePerkOption>,
    pub slot1: Vec<RemotePerkOption>,
    pub slot2: Vec<RemotePerkOption>,
    pub slot3: Vec<RemotePerkOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteChampSelectState {
    pub sub_phase: String,
    pub time_left_sec: i64,
    /// Full length of the current timer phase in seconds (0 when unknown).
    #[serde(default)]
    pub total_time_sec: i64,
    pub queue_type: String,
    pub local_cell_id: i64,
    pub assigned_position: String,
    pub active_action: Option<RemoteActiveAction>,
    pub spell1_id: i64,
    pub spell2_id: i64,
    pub selected_skin_id: i64,
    pub my_team: Vec<RemoteTeamMember>,
    pub their_team: Vec<ChampSelectEnemy>,
    pub my_team_bans: Vec<String>,
    pub their_team_bans: Vec<String>,
    #[serde(default)]
    pub my_team_ban_ids: Vec<i64>,
    #[serde(default)]
    pub their_team_ban_ids: Vec<i64>,
    pub incoming_swaps: Vec<RemoteIncomingSwap>,
    pub champions: Vec<RemoteChampionOption>,
    pub available_skins: Vec<RemoteSkinOption>,
    pub available_spells: Vec<RemoteSpellOption>,
    pub rune_pages: Vec<RemoteRunePage>,
    pub recommended_rune_pages: Vec<RemoteRunePage>,
    pub rune_trees: Vec<RemoteRuneTree>,
}

pub use crate::champions::{
    builtin_skins_for_localized, default_rune_trees_localized, default_summoner_spells_localized,
    keystone_name_from_id_localized, recommended_pages_for_localized, rune_style_name_localized,
    spell_name_from_id_localized,
};

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

/// Derives the complete [`RemoteStateSnapshot`] purely from the single [`SummarySnapshot`]
/// without inspecting raw Riot payloads or demo internals.
pub fn project_remote_state(
    summary: &SummarySnapshot,
    is_demo: bool,
    lang: Lang,
    alerts: &mut AlertTracker,
) -> RemoteStateSnapshot {
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

    let lobby = summary.lobby.as_ref().map(|l| RemoteLobbyState {
        queue_id: l.queue_id,
        queue_name: l.queue_name.clone(),
        first_position: l.local_first_position.clone().unwrap_or_default(),
        second_position: l.local_second_position.clone().unwrap_or_default(),
        is_leader: l.local_is_leader,
        member_count: l.members.len(),
    });

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

    let champ_select = summary.champ_select.as_ref().map(|cs| {
        let self_ally = cs.my_team.iter().find(|m| m.is_self);
        let assigned_position = self_ally
            .map(|m| m.assigned_position.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "FILL".to_string());
        let spell1_id = self_ally.map(|m| m.spell1_id).unwrap_or(0);
        let spell2_id = self_ally.map(|m| m.spell2_id).unwrap_or(0);
        let selected_skin_id = self_ally.map(|m| m.selected_skin_id).unwrap_or(0);

        let my_team: Vec<RemoteTeamMember> = cs
            .my_team
            .iter()
            .map(|ally| {
                let champ_name = if ally.has_champion() {
                    ally.champion_name.clone()
                } else {
                    String::new()
                };
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
            incoming_swaps: cs.incoming_swaps.clone(),
            champions: cs.champions.clone(),
            available_skins: cs.available_skins.clone(),
            available_spells: default_summoner_spells_localized(lang),
            rune_pages: cs.rune_pages.clone(),
            recommended_rune_pages: cs.recommended_rune_pages.clone(),
            rune_trees: default_rune_trees_localized(lang),
        }
    });

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

    let live_game = if phase == "InProgress" {
        summary.live_game.as_ref().map(|lg| {
            let board = summary.board.clone().unwrap_or_default();
            let clock = format_clock_mm_ss(lg.game_time_sec);
            let my_team_name = lg
                .players
                .iter()
                .find(|p| p.is_self)
                .map(|p| p.team.as_str())
                .unwrap_or("ORDER");
            let map_player = |p: &LiveGamePlayerSummary| RemoteLivePlayer {
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
            let allies = lg
                .players
                .iter()
                .filter(|p| p.team == my_team_name)
                .map(map_player)
                .collect();
            let enemies = lg
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
                .map(|f| RemoteLiveFeedItem {
                    time: format_clock_mm_ss(f.time_sec),
                    text: f.text.clone(),
                    side: match f.by_my_team {
                        Some(true) => 1,
                        Some(false) => 0,
                        None => -1,
                    },
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
        is_demo,
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
    use crate::champions::BuiltinChampionCatalog;
    use crate::summary::{project_match, MatchInputs};
    use serde_json::json;

    #[test]
    fn projects_remote_state_purely_from_match_projection() {
        let cs = json!({
            "localPlayerCellId": 0,
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 15_000, "totalTimeInPhase": 30_000},
            "actions": [[{"id": 10, "actorCellId": 0, "type": "ban", "championId": 238, "completed": false, "isInProgress": true}]],
            "myTeam": [{"cellId": 0, "assignedPosition": "MIDDLE", "championId": 103, "spell1Id": 4, "spell2Id": 14, "summonerId": 1, "puuid": "p-self"}]
        });
        let summary = project_match(
            &MatchInputs {
                phase: Some(crate::event::GamePhase {
                    phase: "ChampSelect".to_string(),
                    since: 0,
                }),
                champ_select: Some(&cs),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        let mut alerts = AlertTracker::default();
        let remote = project_remote_state(&summary, true, Lang::Fr, &mut alerts);
        assert_eq!(remote.phase, "ChampSelect");
        assert!(remote.is_demo);
        assert_eq!(alerts.last_kind.as_deref(), Some("your_ban"));
        assert_eq!(remote.champ_select.unwrap().available_spells[0].name, "Saut éclair");
    }
}
