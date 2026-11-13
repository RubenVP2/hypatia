use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board::{extract_live_board_localized, LiveBoard};
use crate::champions::{
    extract_raw_champion_alias, is_champion_placeholder, ChampionLookup, Position,
};
use crate::event::GamePhase;
use crate::i18n::Lang;
use crate::remote::RemoteActiveAction;
use crate::source::SourceState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SummonerSummary {
    pub display_name: String,
    pub tag_line: String,
    pub summoner_level: u32,
    pub xp_percent: u32,
    pub puuid: String,
    #[serde(default)]
    pub profile_icon_id: i64,
    pub rank_tier: Option<String>,
    pub rank_division: Option<String>,
    pub rank_lp: Option<i32>,
    pub rank_wins: Option<u32>,
    pub rank_losses: Option<u32>,
    pub presence_state: Option<String>,
    pub status_message: Option<String>,
}

impl SummonerSummary {
    /// Formatted Riot ID (`display_name#tag_line`, or just `display_name` when `tag_line` is empty).
    pub fn riot_id(&self) -> String {
        if self.tag_line.is_empty() {
            self.display_name.clone()
        } else {
            format!("{}#{}", self.display_name, self.tag_line)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbyMemberSummary {
    pub summoner_name: String,
    pub puuid: Option<String>,
    pub is_leader: bool,
    pub ready: bool,
    pub first_position: Option<String>,
    pub second_position: Option<String>,
    #[serde(default)]
    pub is_self: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbySummary {
    pub party_id: String,
    pub queue_id: i64,
    pub queue_name: String,
    pub members: Vec<LobbyMemberSummary>,
    #[serde(default)]
    pub local_first_position: Option<String>,
    #[serde(default)]
    pub local_second_position: Option<String>,
    #[serde(default)]
    pub local_is_leader: bool,
}

impl LobbySummary {
    /// Returns the identified local party member (falling back to the first member).
    pub fn local_member(&self) -> Option<&LobbyMemberSummary> {
        self.members
            .iter()
            .find(|m| m.is_self)
            .or_else(|| self.members.first())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchmakingSummary {
    pub queue_id: i64,
    #[serde(default)]
    pub queue_name: String,
    pub time_in_queue_sec: f64,
    pub estimated_queue_time_sec: f64,
    pub is_currently_in_queue: bool,
    pub ready_check_state: Option<String>,
    pub player_response: Option<String>,
    #[serde(default)]
    pub ready_check_timer_sec: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChampSelectAlly {
    pub cell_id: i64,
    pub assigned_position: String,
    #[serde(default)]
    pub champion_id: i64,
    pub champion_name: String,
    #[serde(default)]
    pub spell1_id: i64,
    #[serde(default)]
    pub spell2_id: i64,
    pub summoner_name: String,
    pub puuid: String,
    pub rank_label: Option<String>,
    pub is_anonymized: bool,
    pub is_self: bool,
    pub is_premade: bool,
    #[serde(default)]
    pub selected_skin_id: i64,
    #[serde(default)]
    pub acting_type: Option<String>,
    #[serde(default)]
    pub position_swap_id: Option<i64>,
    #[serde(default)]
    pub position_swap_state: Option<String>,
    #[serde(default)]
    pub pick_swap_id: Option<i64>,
    #[serde(default)]
    pub pick_swap_state: Option<String>,
    #[serde(default)]
    pub trade_id: Option<i64>,
    #[serde(default)]
    pub trade_state: Option<String>,
}

impl ChampSelectAlly {
    /// Returns `true` when this slot has a concrete champion selected/hovered (non-placeholder).
    pub fn has_champion(&self) -> bool {
        self.champion_id > 0 && !is_champion_placeholder(&self.champion_name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChampSelectSummary {
    pub sub_phase: String,
    pub time_left_sec: i64,
    /// Full length of the current timer phase, in seconds (0 when the LCU omits it).
    #[serde(default)]
    pub total_time_sec: i64,
    pub queue_type: String,
    pub is_ranked_solo_anonymized: bool,
    pub my_team: Vec<ChampSelectAlly>,
    pub my_team_bans: Vec<String>,
    pub their_team_bans: Vec<String>,
    #[serde(default)]
    pub my_team_ban_ids: Vec<i64>,
    #[serde(default)]
    pub their_team_ban_ids: Vec<i64>,
    /// Enemy picks in cell order; identities are never available here.
    #[serde(default)]
    pub their_team: Vec<ChampSelectEnemy>,
    /// Cell currently acting (picking or banning), when the session exposes it.
    #[serde(default)]
    pub acting_cell_id: Option<i64>,
    /// `pick` or `ban` for the acting cell.
    #[serde(default)]
    pub acting_type: Option<String>,
    #[serde(default)]
    pub local_cell_id: i64,
    #[serde(default)]
    pub local_action: Option<RemoteActiveAction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChampSelectEnemy {
    pub cell_id: i64,
    pub champion_id: i64,
    /// Empty while the enemy has not picked yet.
    pub champion_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGameEventSummary {
    pub event_id: i64,
    pub event_name: String,
    pub event_time_sec: f64,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGamePlayerSummary {
    pub riot_id: String,
    #[serde(default)]
    pub champion_id: i64,
    pub champion_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub champion_alias: String,
    pub team: String,
    pub position: String,
    pub level: u32,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
    pub is_self: bool,
    #[serde(default)]
    pub is_dead: bool,
    #[serde(default)]
    pub respawn_timer_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGameSummary {
    pub game_mode: String,
    pub map_name: String,
    pub game_time_sec: f64,
    #[serde(default)]
    pub active_champion_id: i64,
    pub active_champion: String,
    pub level: u32,
    pub current_gold: i64,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
    pub recent_events: Vec<LiveGameEventSummary>,
    #[serde(default)]
    pub players: Vec<LiveGamePlayerSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndOfGameSummary {
    pub result: String,
    pub game_length_sec: u32,
    #[serde(default)]
    pub champion_id: i64,
    pub champion_name: String,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub total_gold: u32,
    pub damage_to_champions: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SummarySnapshot {
    pub phase: Option<GamePhase>,
    pub sources: Vec<SourceState>,
    pub summoner: Option<SummonerSummary>,
    pub lobby: Option<LobbySummary>,
    pub matchmaking: Option<MatchmakingSummary>,
    pub champ_select: Option<ChampSelectSummary>,
    pub live_game: Option<LiveGameSummary>,
    #[serde(default)]
    pub board: Option<LiveBoard>,
    pub end_of_game: Option<EndOfGameSummary>,
}

/// Borrowed raw Riot JSON payloads needed to project a complete [`SummarySnapshot`].
#[derive(Debug, Default)]
pub struct MatchInputs<'a> {
    pub phase: Option<GamePhase>,
    pub sources: Vec<SourceState>,
    pub summoner: Option<&'a Value>,
    pub ranked: Option<&'a Value>,
    pub presences: Option<&'a Value>,
    pub lobby: Option<&'a Value>,
    pub matchmaking_search: Option<&'a Value>,
    pub ready_check: Option<&'a Value>,
    pub champ_select: Option<&'a Value>,
    pub live_allgame: Option<&'a Value>,
    pub live_active: Option<&'a Value>,
    pub live_events: Option<&'a Value>,
    pub end_of_game: Option<&'a Value>,
}

/// Returns the localized human-readable queue name for a Riot `queueId` (`""` when `queue_id <= 0`).
pub fn queue_name_from_id_localized(queue_id: i64, lang: Lang) -> String {
    if queue_id <= 0 {
        return String::new();
    }
    match (queue_id, lang) {
        (400, Lang::Fr) => "Normale Draft 5v5".to_string(),
        (420, Lang::Fr) => "Classée Solo/Duo".to_string(),
        (430, Lang::Fr) => "Normale Aveugle 5v5".to_string(),
        (440, Lang::Fr) => "Classée Flexible 5v5".to_string(),
        (400, Lang::En) => "Normal Draft 5v5".to_string(),
        (420, Lang::En) => "Ranked Solo/Duo".to_string(),
        (430, Lang::En) => "Normal Blind 5v5".to_string(),
        (440, Lang::En) => "Ranked Flex 5v5".to_string(),
        (450, _) => "ARAM".to_string(),
        (700, _) => "Clash".to_string(),
        (_, Lang::Fr) => format!("File #{queue_id}"),
        (_, Lang::En) => format!("Queue #{queue_id}"),
    }
}

/// Projects all raw Riot LCU and Live Client payloads into one normalized,
/// localized [`SummarySnapshot`].
///
/// Centralizes champion ID/name resolution through `champions`, ms-to-second draft
/// timer normalization, lobby premade propagation, and ranked Solo/Duo anonymization.
pub fn project_match(
    inputs: &MatchInputs<'_>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> SummarySnapshot {
    let summoner = extract_summoner_summary(inputs.summoner, inputs.ranked, inputs.presences, lang);
    let lobby = extract_lobby_summary(inputs.lobby, summoner.as_ref(), lang);
    let matchmaking = extract_matchmaking_summary(
        inputs.matchmaking_search,
        inputs.ready_check,
        lobby.as_ref(),
        lang,
    );
    let champ_select = extract_champ_select_summary(
        inputs.champ_select,
        summoner.as_ref(),
        lobby.as_ref(),
        champions,
        lang,
    );
    let live_game = extract_live_game_summary(
        inputs.live_allgame,
        inputs.live_active,
        inputs.live_events,
        champions,
        lang,
    );
    let board = live_game
        .as_ref()
        .map(|lg| extract_live_board_localized(inputs.live_allgame, inputs.live_events, lg, lang));
    let end_of_game = extract_eog_summary(inputs.end_of_game, champions, lang);

    SummarySnapshot {
        phase: inputs.phase.clone(),
        sources: inputs.sources.clone(),
        summoner,
        lobby,
        matchmaking,
        champ_select,
        live_game,
        board,
        end_of_game,
    }
}

/// Resolves a `(champion_id, champion_name)` pair using the synced/builtin catalog.
fn resolve_champion_pair(
    raw_id: i64,
    raw_name: Option<&str>,
    champions: &dyn ChampionLookup,
    lang: Lang,
    placeholder_when_empty: bool,
) -> (i64, String) {
    let explicit_name = raw_name
        .map(str::trim)
        .filter(|s| !s.is_empty() && !is_champion_placeholder(s));

    if raw_id > 0 {
        let resolved = champions.resolve_champion_name_localized(raw_id, lang);
        let name = if resolved.starts_with("Champion #") {
            explicit_name.map(str::to_string).unwrap_or(resolved)
        } else {
            resolved
        };
        return (raw_id, name);
    }

    if let Some(name) = explicit_name {
        let id = champions.resolve_champion_id(name);
        let display = if id > 0 {
            let resolved = champions.resolve_champion_name_localized(id, lang);
            if resolved.starts_with("Champion #") {
                name.to_string()
            } else {
                resolved
            }
        } else {
            name.to_string()
        };
        return (id, display);
    }

    if placeholder_when_empty {
        (0, champions.resolve_champion_name_localized(0, lang))
    } else {
        (0, String::new())
    }
}

fn extract_summoner_summary(
    summoner_val: Option<&Value>,
    ranked_val: Option<&Value>,
    presences_val: Option<&Value>,
    lang: Lang,
) -> Option<SummonerSummary> {
    let sum = summoner_val?;
    let display_name = sum
        .get("gameName")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            sum.get("displayName")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or(lang.pick("Unknown", "Inconnu"))
        .to_string();
    let tag_line = sum
        .get("tagLine")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let summoner_level = sum
        .get("summonerLevel")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;
    let xp_percent = sum
        .get("percentCompleteForNextLevel")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let puuid = sum
        .get("puuid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let profile_icon_id = sum
        .get("profileIconId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let mut rank_tier = None;
    let mut rank_division = None;
    let mut rank_lp = None;
    let mut rank_wins = None;
    let mut rank_losses = None;

    if let Some(rk) = ranked_val {
        let solo = rk
            .get("queueMap")
            .and_then(|qm| qm.get("RANKED_SOLO_5x5"))
            .or(Some(rk));
        if let Some(s) = solo {
            rank_tier = s.get("tier").and_then(|v| v.as_str()).map(|s| s.to_string());
            rank_division = s
                .get("division")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            rank_lp = s
                .get("leaguePoints")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            rank_wins = s.get("wins").and_then(|v| v.as_u64()).map(|v| v as u32);
            rank_losses = s.get("losses").and_then(|v| v.as_u64()).map(|v| v as u32);
        }
    }

    let mut presence_state = None;
    let mut status_message = None;
    if let Some(pres) = presences_val {
        let target = pres.get("local").or_else(|| {
            pres.get("presences")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
        });
        if let Some(node) = target {
            presence_state = node
                .get("summary")
                .or_else(|| node.get("state"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            status_message = node
                .get("statusMessage")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }

    Some(SummonerSummary {
        display_name,
        tag_line,
        summoner_level,
        xp_percent,
        puuid,
        profile_icon_id,
        rank_tier,
        rank_division,
        rank_lp,
        rank_wins,
        rank_losses,
        presence_state,
        status_message,
    })
}

fn extract_lobby_summary(
    lobby_val: Option<&Value>,
    summoner: Option<&SummonerSummary>,
    lang: Lang,
) -> Option<LobbySummary> {
    let lob = lobby_val?;
    let party_id = lob
        .get("partyId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let queue_id = lob
        .get("queueId")
        .or_else(|| lob.get("gameConfig").and_then(|gc| gc.get("queueId")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let queue_name = queue_name_from_id_localized(queue_id, lang);

    let local_obj = lob.get("localMember");
    let local_puuid = local_obj
        .and_then(|m| m.get("puuid"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| summoner.map(|s| s.puuid.as_str()).filter(|s| !s.is_empty()));
    let local_summoner_id = local_obj
        .and_then(|m| m.get("summonerId"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let local_name = local_obj
        .and_then(|m| m.get("summonerName").or_else(|| m.get("gameName")))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| summoner.map(|s| s.display_name.as_str()).filter(|s| !s.is_empty()));

    let mut members = Vec::new();
    let mut matched_any_self = false;
    if let Some(arr) = lob.get("members").and_then(|v| v.as_array()) {
        for m in arr {
            let summoner_name = m
                .get("summonerName")
                .or_else(|| m.get("gameName"))
                .and_then(|v| v.as_str())
                .unwrap_or(lang.pick("Member", "Membre"))
                .to_string();
            let puuid = m
                .get("puuid")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let m_summoner_id = m
                .get("summonerId")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let is_leader = m
                .get("isLeader")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let ready = m.get("ready").and_then(|v| v.as_bool()).unwrap_or(true);
            let first_position = m
                .get("firstPositionPreference")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let second_position = m
                .get("secondPositionPreference")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let is_self = puuid
                .as_deref()
                .zip(local_puuid)
                .is_some_and(|(a, b)| a == b)
                || (local_summoner_id > 0 && m_summoner_id == local_summoner_id)
                || local_name.is_some_and(|ln| {
                    summoner_name == ln
                        || summoner.is_some_and(|s| summoner_name == s.riot_id())
                });
            if is_self {
                matched_any_self = true;
            }

            members.push(LobbyMemberSummary {
                summoner_name,
                puuid,
                is_leader,
                ready,
                first_position,
                second_position,
                is_self,
            });
        }
    }

    if !matched_any_self {
        let single_member = members.len() == 1;
        if let Some(first) = members.first_mut() {
            if single_member || first.puuid.is_none() {
                first.is_self = true;
            }
        }
    }

    let self_member = members
        .iter()
        .find(|m| m.is_self)
        .or_else(|| members.first());
    let local_first_position = local_obj
        .and_then(|m| m.get("firstPositionPreference"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .or_else(|| {
            self_member
                .and_then(|m| m.first_position.as_deref())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_uppercase())
        });
    let local_second_position = local_obj
        .and_then(|m| m.get("secondPositionPreference"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .or_else(|| {
            self_member
                .and_then(|m| m.second_position.as_deref())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_uppercase())
        });
    let local_is_leader = local_obj
        .and_then(|m| m.get("isLeader"))
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| self_member.is_some_and(|m| m.is_leader));

    Some(LobbySummary {
        party_id,
        queue_id,
        queue_name,
        members,
        local_first_position,
        local_second_position,
        local_is_leader,
    })
}

fn extract_matchmaking_summary(
    search_val: Option<&Value>,
    ready_val: Option<&Value>,
    lobby: Option<&LobbySummary>,
    lang: Lang,
) -> Option<MatchmakingSummary> {
    if search_val.is_none() && ready_val.is_none() {
        return None;
    }

    let queue_id = search_val
        .and_then(|s| s.get("queueId"))
        .and_then(|v| v.as_i64())
        .filter(|&id| id > 0)
        .or_else(|| lobby.map(|l| l.queue_id))
        .unwrap_or(0);
    let queue_name = queue_name_from_id_localized(queue_id, lang);
    let time_in_queue_sec = search_val
        .and_then(|s| s.get("timeInQueue"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let estimated_queue_time_sec = search_val
        .and_then(|s| s.get("estimatedQueueTime"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let is_currently_in_queue = search_val
        .and_then(|s| s.get("isCurrentlyInQueue"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let ready_check_state = ready_val
        .and_then(|r| r.get("state"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let player_response = ready_val
        .and_then(|r| r.get("playerResponse"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let ready_check_timer_sec = ready_val
        .and_then(|r| r.get("timer"))
        .and_then(|v| v.as_f64())
        .map(|f| f as i32);

    Some(MatchmakingSummary {
        queue_id,
        queue_name,
        time_in_queue_sec,
        estimated_queue_time_sec,
        is_currently_in_queue,
        ready_check_state,
        player_response,
        ready_check_timer_sec,
    })
}

fn parse_swap_map(cs: &Value, key: &str) -> HashMap<i64, (i64, String)> {
    let mut map = HashMap::new();
    let Some(arr) = cs.get(key).and_then(|v| v.as_array()) else {
        return map;
    };
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
    map
}

fn parse_champ_select_actions(
    cs: &Value,
    local_cell_id: i64,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> (
    Option<i64>,
    Option<String>,
    HashMap<i64, String>,
    Option<RemoteActiveAction>,
) {
    let mut acting_cell_id: Option<i64> = None;
    let mut acting_type: Option<String> = None;
    let mut cell_acting: HashMap<i64, String> = HashMap::new();
    let mut local_action: Option<RemoteActiveAction> = None;

    let Some(turns) = cs.get("actions").and_then(|v| v.as_array()) else {
        return (acting_cell_id, acting_type, cell_acting, local_action);
    };

    for act in turns.iter().filter_map(|t| t.as_array()).flatten() {
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
            if acting_cell_id.is_none() {
                acting_cell_id = Some(actor_cell);
                acting_type = Some(action_type.clone());
            }
            cell_acting.insert(actor_cell, action_type.clone());
        } else if in_progress && acting_cell_id.is_none() {
            acting_cell_id = Some(actor_cell);
            acting_type = Some(action_type.clone());
        }

        if actor_cell == local_cell_id {
            let should_take = match &local_action {
                None => true,
                Some(prev) => {
                    (in_progress && !completed && !prev.is_my_turn)
                        || (!completed && prev.completed)
                }
            };
            if should_take {
                local_action = Some(RemoteActiveAction {
                    action_id,
                    action_type,
                    champion_id: champ_id,
                    champion_name: if champ_id > 0 {
                        champions.resolve_champion_name_localized(champ_id, lang)
                    } else {
                        String::new()
                    },
                    completed,
                    is_my_turn: in_progress && !completed,
                });
            }
        }
    }

    (acting_cell_id, acting_type, cell_acting, local_action)
}

fn extract_champ_select_summary(
    cs_val: Option<&Value>,
    summoner: Option<&SummonerSummary>,
    lobby: Option<&LobbySummary>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Option<ChampSelectSummary> {
    let cs = cs_val?;
    let timer = cs.get("timer");
    let sub_phase = timer
        .and_then(|t| t.get("phase"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let raw_time = timer
        .and_then(|t| t.get("adjustedTimeLeftInPhase"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    // The LCU (and the demo fixtures, which mirror it) report milliseconds. Round up so
    // "0" only shows once the phase has really expired.
    let time_left_sec = (raw_time.max(0) + 999) / 1000;
    let total_time_sec = timer
        .and_then(|t| t.get("totalTimeInPhase"))
        .and_then(|v| v.as_i64())
        .map_or(0, |ms| (ms.max(0) + 999) / 1000);
    let local_cell_id = cs
        .get("localPlayerCellId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let queue_type = cs
        .get("queueType")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let pos_swaps = parse_swap_map(cs, "positionSwaps");
    let pick_swaps = parse_swap_map(cs, "swaps");
    let trades = parse_swap_map(cs, "trades");
    let (acting_cell_id, acting_type, cell_acting, local_action) =
        parse_champ_select_actions(cs, local_cell_id, champions, lang);

    let premade_puuids: Vec<String> = lobby
        .map(|l| l.members.iter().filter_map(|m| m.puuid.clone()).collect())
        .unwrap_or_default();

    let mut my_team = Vec::new();
    let mut any_anonymized = false;

    if let Some(arr) = cs.get("myTeam").and_then(|v| v.as_array()) {
        for (idx, member) in arr.iter().enumerate() {
            let cell_id = member
                .get("cellId")
                .and_then(|v| v.as_i64())
                .unwrap_or(idx as i64);
            let assigned_position = member
                .get("assignedPosition")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    Position::parse(s)
                        .map_or_else(|| s.to_ascii_uppercase(), |p| p.lcu_code().to_string())
                })
                .unwrap_or_default();

            let raw_champ_id = member
                .get("championId")
                .or_else(|| member.get("championPickIntent"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let (champ_id, champion_name) = resolve_champion_pair(
                raw_champ_id,
                member.get("championName").and_then(|v| v.as_str()),
                champions,
                lang,
                true,
            );
            let spell1_id = member
                .get("spell1Id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let spell2_id = member
                .get("spell2Id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let selected_skin_id = member
                .get("selectedSkinId")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let raw_puuid = member
                .get("puuid")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let summoner_id = member
                .get("summonerId")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let is_self = cell_id == local_cell_id;
            let is_premade = !is_self
                && (!raw_puuid.is_empty() && premade_puuids.iter().any(|p| p == raw_puuid)
                    || member
                        .get("isPremade")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false));

            let is_anonymized =
                !is_self && !is_premade && (raw_puuid.is_empty() || summoner_id == 0);
            if is_anonymized {
                any_anonymized = true;
            }

            let summoner_name = if is_anonymized {
                format!("{} {}", lang.pick("Ally", "Allié"), idx + 1)
            } else {
                member
                    .get("summonerName")
                    .or_else(|| member.get("gameName"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        if is_self {
                            summoner.map(SummonerSummary::riot_id).filter(|s| !s.is_empty())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        if is_self {
                            lang.pick("You", "Vous").to_string()
                        } else {
                            lang.pick("Teammate", "Coéquipier").to_string()
                        }
                    })
            };

            let rank_label = if is_anonymized {
                None
            } else {
                member
                    .get("rank")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            };

            let p_swap = pos_swaps.get(&cell_id).cloned();
            let o_swap = pick_swaps.get(&cell_id).cloned();
            let c_trade = trades.get(&cell_id).cloned();

            my_team.push(ChampSelectAlly {
                cell_id,
                assigned_position,
                champion_id: champ_id,
                champion_name,
                spell1_id,
                spell2_id,
                summoner_name,
                puuid: raw_puuid.to_string(),
                rank_label,
                is_anonymized,
                is_self,
                is_premade,
                selected_skin_id,
                acting_type: cell_acting.get(&cell_id).cloned(),
                position_swap_id: p_swap.as_ref().map(|x| x.0),
                position_swap_state: p_swap.map(|x| x.1),
                pick_swap_id: o_swap.as_ref().map(|x| x.0),
                pick_swap_state: o_swap.map(|x| x.1),
                trade_id: c_trade.as_ref().map(|x| x.0),
                trade_state: c_trade.map(|x| x.1),
            });
        }
    }

    let (my_team_bans, my_team_ban_ids) =
        extract_ban_list(cs.get("bans"), "myTeamBans", champions, lang);
    let (their_team_bans, their_team_ban_ids) =
        extract_ban_list(cs.get("bans"), "theirTeamBans", champions, lang);

    let mut their_team = Vec::new();
    if let Some(arr) = cs.get("theirTeam").and_then(|v| v.as_array()) {
        for (idx, member) in arr.iter().enumerate() {
            let cell_id = member
                .get("cellId")
                .and_then(|v| v.as_i64())
                .unwrap_or(5 + idx as i64);
            let raw_champ_id = member
                .get("championId")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let (champion_id, champion_name) = resolve_champion_pair(
                raw_champ_id,
                member.get("championName").and_then(|v| v.as_str()),
                champions,
                lang,
                false,
            );
            their_team.push(ChampSelectEnemy {
                cell_id,
                champion_id,
                champion_name,
            });
        }
    }

    Some(ChampSelectSummary {
        their_team,
        acting_cell_id,
        acting_type,
        local_cell_id,
        local_action,
        sub_phase,
        time_left_sec,
        total_time_sec,
        queue_type,
        is_ranked_solo_anonymized: any_anonymized,
        my_team,
        my_team_bans,
        their_team_bans,
        my_team_ban_ids,
        their_team_ban_ids,
    })
}

fn extract_ban_list(
    bans: Option<&Value>,
    key: &str,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> (Vec<String>, Vec<i64>) {
    let mut names = Vec::new();
    let mut ids = Vec::new();
    let Some(arr) = bans.and_then(|b| b.get(key)).and_then(|v| v.as_array()) else {
        return (names, ids);
    };
    for entry in arr {
        if let Some(id) = entry.as_i64() {
            names.push(champions.resolve_champion_name_localized(id, lang));
            ids.push(id);
        } else if let Some(s) = entry.as_str() {
            ids.push(champions.resolve_champion_id(s));
            names.push(s.to_string());
        }
    }
    (names, ids)
}

fn extract_live_game_summary(
    allgame_val: Option<&Value>,
    active_val: Option<&Value>,
    events_val: Option<&Value>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Option<LiveGameSummary> {
    if allgame_val.is_none() && active_val.is_none() {
        return None;
    }

    let active = active_val.or_else(|| allgame_val.and_then(|ag| ag.get("activePlayer")));
    let game_data = allgame_val.and_then(|ag| ag.get("gameData"));

    let game_mode = game_data
        .and_then(|gd| gd.get("gameMode"))
        .and_then(|v| v.as_str())
        .unwrap_or("CLASSIC")
        .to_string();
    let map_name = game_data
        .and_then(|gd| gd.get("mapName"))
        .and_then(|v| v.as_str())
        .unwrap_or(lang.pick("Summoner's Rift", "Faille de l'invocateur"))
        .to_string();
    let game_time_sec = game_data
        .and_then(|gd| gd.get("gameTime"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let active_riot_id = active
        .and_then(|a| a.get("riotId").or_else(|| a.get("summonerName")))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let active_game_name = active
        .and_then(|a| a.get("riotIdGameName"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let active_summoner_name = active
        .and_then(|a| a.get("summonerName"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut players = Vec::new();
    let mut matched_self: Option<LiveGamePlayerSummary> = None;

    if let Some(all_players) = allgame_val
        .and_then(|ag| ag.get("allPlayers"))
        .and_then(|v| v.as_array())
    {
        for p in all_players {
            let p_riot_id = p
                .get("riotId")
                .or_else(|| p.get("summonerName"))
                .and_then(|v| v.as_str())
                .unwrap_or(lang.pick("Player", "Joueur"));
            let p_game_name = p
                .get("riotIdGameName")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let p_summoner_name = p
                .get("summonerName")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let is_self = (!active_riot_id.is_empty() && p_riot_id == active_riot_id)
                || (!active_game_name.is_empty() && p_game_name == active_game_name)
                || (!active_summoner_name.is_empty()
                    && !p_summoner_name.is_empty()
                    && p_summoner_name == active_summoner_name);

            let champ = p
                .get("championName")
                .and_then(|v| v.as_str())
                .unwrap_or(lang.pick("Unknown", "Inconnu"))
                .to_string();
            let champion_alias = p
                .get("rawChampionName")
                .and_then(|v| v.as_str())
                .map(extract_raw_champion_alias)
                .unwrap_or_default();
            let team = p
                .get("team")
                .and_then(|v| v.as_str())
                .unwrap_or("ORDER")
                .to_string();
            let position = p
                .get("position")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let p_level = p.get("level").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let p_scores = p.get("scores");
            let p_kills = p_scores
                .and_then(|s| s.get("kills"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let p_deaths = p_scores
                .and_then(|s| s.get("deaths"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let p_assists = p_scores
                .and_then(|s| s.get("assists"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let p_cs = p_scores
                .and_then(|s| s.get("creepScore"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let is_dead = p.get("isDead").and_then(|v| v.as_bool()).unwrap_or(false);
            let respawn_timer_sec = p.get("respawnTimer").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let champion_id = p
                .get("championId")
                .and_then(|v| v.as_i64())
                .filter(|&id| id > 0)
                .unwrap_or_else(|| champions.resolve_champion_id_with_alias(&champ, &champion_alias));
            let summary = LiveGamePlayerSummary {
                is_dead,
                respawn_timer_sec,
                riot_id: p_riot_id.to_string(),
                champion_id,
                champion_name: champ,
                champion_alias,
                team,
                position,
                level: p_level,
                kills: p_kills,
                deaths: p_deaths,
                assists: p_assists,
                creep_score: p_cs,
                is_self,
            };
            if is_self {
                matched_self = Some(summary.clone());
            }
            players.push(summary);
        }
    }

    let level = active
        .and_then(|a| a.get("level"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or_else(|| matched_self.as_ref().map(|m| m.level))
        .unwrap_or(1);
    let current_gold = active
        .and_then(|a| a.get("currentGold"))
        .and_then(|v| v.as_f64().map(|f| f as i64).or_else(|| v.as_i64()))
        .unwrap_or(0);

    let active_alias = active
        .and_then(|a| a.get("rawChampionName"))
        .and_then(|v| v.as_str())
        .map(extract_raw_champion_alias)
        .unwrap_or_default();
    let raw_active_champion = matched_self
        .as_ref()
        .map(|m| m.champion_name.as_str())
        .or_else(|| active.and_then(|a| a.get("championName")).and_then(|v| v.as_str()))
        .unwrap_or("");
    let active_champion_id = matched_self
        .as_ref()
        .map(|m| m.champion_id)
        .filter(|&id| id > 0)
        .unwrap_or_else(|| {
            champions.resolve_champion_id_with_alias(raw_active_champion, &active_alias)
        });
    let active_champion = if !raw_active_champion.is_empty() {
        raw_active_champion.to_string()
    } else if active_champion_id > 0 {
        champions.resolve_champion_name_localized(active_champion_id, lang)
    } else {
        lang.pick("Unknown", "Inconnu").to_string()
    };

    let scores = active.and_then(|a| a.get("scores"));
    let kills = matched_self
        .as_ref()
        .map(|m| m.kills)
        .or_else(|| {
            scores
                .and_then(|s| s.get("kills"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
        })
        .unwrap_or(0);
    let deaths = matched_self
        .as_ref()
        .map(|m| m.deaths)
        .or_else(|| {
            scores
                .and_then(|s| s.get("deaths"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
        })
        .unwrap_or(0);
    let assists = matched_self
        .as_ref()
        .map(|m| m.assists)
        .or_else(|| {
            scores
                .and_then(|s| s.get("assists"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
        })
        .unwrap_or(0);
    let creep_score = matched_self
        .as_ref()
        .map(|m| m.creep_score)
        .or_else(|| {
            scores
                .and_then(|s| s.get("creepScore"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
        })
        .unwrap_or(0);

    let mut recent_events = Vec::new();
    let ev_array = events_val
        .and_then(|ev| ev.get("Events"))
        .or_else(|| allgame_val.and_then(|ag| ag.get("events")).and_then(|e| e.get("Events")))
        .and_then(|v| v.as_array());

    if let Some(arr) = ev_array {
        for ev in arr.iter().rev().take(5) {
            let event_id = ev.get("EventID").and_then(|v| v.as_i64()).unwrap_or(0);
            let event_name = ev
                .get("EventName")
                .and_then(|v| v.as_str())
                .unwrap_or("GameEvent")
                .to_string();
            let event_time_sec = ev
                .get("EventTime")
                .and_then(|v| v.as_f64())
                .unwrap_or(game_time_sec);
            let killer = ev.get("KillerName").and_then(|v| v.as_str());
            let victim = ev.get("VictimName").and_then(|v| v.as_str());
            let description = match (killer, victim) {
                (Some(k), Some(v)) => format!("{k} → {v}"),
                (Some(k), None) => k.to_string(),
                _ => event_name.clone(),
            };
            recent_events.push(LiveGameEventSummary {
                event_id,
                event_name,
                event_time_sec,
                description,
            });
        }
    }

    Some(LiveGameSummary {
        game_mode,
        map_name,
        game_time_sec,
        active_champion_id,
        active_champion,
        level,
        current_gold,
        kills,
        deaths,
        assists,
        creep_score,
        recent_events,
        players,
    })
}

fn extract_eog_summary(
    eog_val: Option<&Value>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Option<EndOfGameSummary> {
    let eog = eog_val?;
    let result = eog
        .get("gameResult")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let game_length_sec = eog
        .get("gameLength")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let local = eog.get("localPlayer");
    let raw_champ_id = local
        .and_then(|l| l.get("championId"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let (champion_id, champion_name) = resolve_champion_pair(
        raw_champ_id,
        local.and_then(|l| l.get("championName")).and_then(|v| v.as_str()),
        champions,
        lang,
        false,
    );
    let stats = local.and_then(|l| l.get("stats"));
    let kills = stats
        .and_then(|s| s.get("CHAMPIONS_KILLED"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let deaths = stats
        .and_then(|s| s.get("NUM_DEATHS"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let assists = stats
        .and_then(|s| s.get("ASSISTS"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let total_gold = stats
        .and_then(|s| s.get("GOLD_EARNED"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let damage_to_champions = stats
        .and_then(|s| s.get("TOTAL_DAMAGE_DEALT_TO_CHAMPIONS"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    Some(EndOfGameSummary {
        result,
        game_length_sec,
        champion_id,
        champion_name,
        kills,
        deaths,
        assists,
        total_gold,
        damage_to_champions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::champions::BuiltinChampionCatalog;
    use serde_json::json;

    struct CustomSyncedLookup;

    impl ChampionLookup for CustomSyncedLookup {
        fn resolve_champion_id(&self, name: &str) -> i64 {
            if name == "SyncedCDragonName" {
                103
            } else {
                BuiltinChampionCatalog.resolve_champion_id(name)
            }
        }

        fn resolve_champion_name_localized(&self, champion_id: i64, lang: Lang) -> String {
            if champion_id == 103 && lang.is_fr() {
                "Ahri Synchronisée".to_string()
            } else {
                BuiltinChampionCatalog.resolve_champion_name_localized(champion_id, lang)
            }
        }
    }

    #[test]
    fn projects_lobby_matchmaking_and_ready_check_across_languages() {
        let summoner = json!({"gameName": "Ezo", "tagLine": "EUW", "puuid": "p-self", "profileIconId": 548});
        let lobby = json!({
            "queueId": 420,
            "localMember": {
                "puuid": "p-self",
                "isLeader": true,
                "firstPositionPreference": "mid",
                "secondPositionPreference": "top"
            },
            "members": [
                {"summonerName": "LeaderP1", "puuid": "p-other", "isLeader": false},
                {"summonerName": "Ezo#EUW", "puuid": "p-self", "isLeader": true, "firstPositionPreference": "MIDDLE"},
                {}
            ]
        });
        let ready = json!({"state": "InProgress", "timer": 8.7, "playerResponse": "Accepted"});

        let en = project_match(
            &MatchInputs {
                summoner: Some(&summoner),
                lobby: Some(&lobby),
                ready_check: Some(&ready),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let en_sum = en.summoner.unwrap();
        assert_eq!(en_sum.profile_icon_id, 548);
        assert_eq!(en_sum.riot_id(), "Ezo#EUW");

        let en_lobby = en.lobby.unwrap();
        assert_eq!(en_lobby.queue_name, "Ranked Solo/Duo");
        assert!(!en_lobby.members[0].is_self);
        assert!(en_lobby.members[1].is_self);
        assert_eq!(en_lobby.members[2].summoner_name, "Member");
        assert_eq!(en_lobby.local_first_position.as_deref(), Some("MID"));
        assert!(en_lobby.local_is_leader);

        let mm = en.matchmaking.unwrap();
        assert_eq!(mm.queue_id, 420);
        assert_eq!(mm.ready_check_state.as_deref(), Some("InProgress"));
        assert_eq!(mm.ready_check_timer_sec, Some(8));

        let fr = project_match(
            &MatchInputs {
                lobby: Some(&lobby),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        let fr_lobby = fr.lobby.unwrap();
        assert_eq!(fr_lobby.queue_name, "Classée Solo/Duo");
        assert_eq!(fr_lobby.members[2].summoner_name, "Membre");
    }

    #[test]
    fn projects_champ_select_timers_swaps_premade_and_anonymization() {
        let lobby = json!({
            "queueId": 420,
            "members": [{"puuid": "p-self"}, {"puuid": "p-duo"}]
        });
        let cs = json!({
            "localPlayerCellId": 1,
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 28500},
            "actions": [[
                {"id": 10, "actorCellId": 1, "type": "pick", "championId": 103, "completed": false, "isInProgress": true},
                {"id": 11, "actorCellId": 2, "type": "ban", "championId": 0, "completed": false, "isInProgress": true}
            ]],
            "positionSwaps": [{"id": 99, "cellId": 0, "state": "RECEIVED"}],
            "trades": [{"id": 88, "cellId": 2, "state": "AVAILABLE"}],
            "myTeam": [
                {"cellId": 0, "assignedPosition": "TOP", "championId": 266, "summonerName": "SecretSolo", "puuid": ""},
                {"cellId": 1, "assignedPosition": "mid", "championId": 103, "spell1Id": 4, "spell2Id": 14, "selectedSkinId": 103027, "summonerId": 1, "puuid": "p-self"},
                {"cellId": 2, "assignedPosition": "UTILITY", "championId": 0, "summonerId": 2, "puuid": "p-duo", "summonerName": "DuoPartner"}
            ],
            "bans": {"myTeamBans": [238], "theirTeamBans": ["Yasuo"]}
        });

        let snap = project_match(
            &MatchInputs {
                lobby: Some(&lobby),
                champ_select: Some(&cs),
                ..Default::default()
            },
            &CustomSyncedLookup,
            Lang::Fr,
        );
        let p = snap.champ_select.unwrap();
        assert_eq!(p.time_left_sec, 29);
        assert_eq!(p.queue_type, "");
        assert!(p.is_ranked_solo_anonymized);
        assert_eq!(p.local_cell_id, 1);

        // Anonymized solo teammate vs revealed premade duo vs self synced name
        assert!(p.my_team[0].is_anonymized);
        assert_eq!(p.my_team[0].summoner_name, "Allié 1");
        assert_eq!(p.my_team[0].position_swap_id, Some(99));
        assert_eq!(p.my_team[0].position_swap_state.as_deref(), Some("RECEIVED"));

        assert!(p.my_team[1].is_self);
        assert_eq!(p.my_team[1].assigned_position, "MIDDLE");
        assert_eq!(p.my_team[1].champion_name, "Ahri Synchronisée");
        assert_eq!(p.my_team[1].spell1_id, 4);
        assert_eq!(p.my_team[1].spell2_id, 14);
        assert_eq!(p.my_team[1].selected_skin_id, 103027);
        assert_eq!(p.my_team[1].summoner_name, "Vous");

        assert!(p.my_team[2].is_premade);
        assert!(!p.my_team[2].has_champion());
        assert_eq!(p.my_team[2].champion_name, "Sélection...");
        assert_eq!(p.my_team[2].trade_id, Some(88));
        assert_eq!(p.my_team[2].acting_type.as_deref(), Some("ban"));

        assert_eq!(p.my_team_ban_ids, vec![238]);
        assert_eq!(p.their_team_ban_ids, vec![157]);

        let act = p.local_action.unwrap();
        assert_eq!(act.action_id, 10);
        assert_eq!(act.champion_name, "Ahri Synchronisée");
        assert!(act.is_my_turn);
    }

    #[test]
    fn projects_live_game_board_and_neutral_end_of_game_without_demo_defaults() {
        let allgame = json!({
            "gameData": {"gameMode": "CLASSIC", "mapName": "SR", "gameTime": 100.0},
            "activePlayer": {"riotId": "Ezo#EUW", "level": 6},
            "allPlayers": [
                {
                    "riotId": "Ezo#EUW",
                    "championName": "Kai'Sa",
                    "rawChampionName": "game_character_displayname_Kaisa",
                    "team": "ORDER",
                    "position": "BOTTOM"
                },
                {
                    "riotId": "Ally#EUW",
                    "championName": "SyncedCDragonName",
                    "rawChampionName": "game_character_displayname_MasterYi",
                    "team": "ORDER",
                    "position": "JUNGLE"
                }
            ]
        });
        let eog_empty = json!({});
        let snap = project_match(
            &MatchInputs {
                live_allgame: Some(&allgame),
                end_of_game: Some(&eog_empty),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );

        let live = snap.live_game.unwrap();
        assert_eq!(live.active_champion_id, 145);
        assert_eq!(live.current_gold, 0);
        assert_eq!(live.players[0].champion_id, 145);
        assert_eq!(live.players[1].champion_id, 11);
        assert_eq!(live.players[1].champion_alias, "MasterYi");
        assert!(snap.board.is_some());

        let eog = snap.end_of_game.unwrap();
        assert_eq!(eog.result, "");
        assert_eq!(eog.game_length_sec, 0);
        assert_eq!(eog.champion_id, 0);
        assert_eq!(eog.champion_name, "");
    }
}
