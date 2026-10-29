use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::GamePhase;
use crate::source::SourceState;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SummonerSummary {
    pub display_name: String,
    pub tag_line: String,
    pub summoner_level: u32,
    pub xp_percent: u32,
    pub puuid: String,
    pub rank_tier: Option<String>,
    pub rank_division: Option<String>,
    pub rank_lp: Option<i32>,
    pub rank_wins: Option<u32>,
    pub rank_losses: Option<u32>,
    pub presence_state: Option<String>,
    pub status_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbyMemberSummary {
    pub summoner_name: String,
    pub puuid: Option<String>,
    pub is_leader: bool,
    pub ready: bool,
    pub first_position: Option<String>,
    pub second_position: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbySummary {
    pub party_id: String,
    pub queue_id: i64,
    pub queue_name: String,
    pub members: Vec<LobbyMemberSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchmakingSummary {
    pub queue_id: i64,
    pub time_in_queue_sec: f64,
    pub estimated_queue_time_sec: f64,
    pub is_currently_in_queue: bool,
    pub ready_check_state: Option<String>,
    pub player_response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChampSelectAlly {
    pub cell_id: i64,
    pub assigned_position: String,
    pub champion_name: String,
    pub summoner_name: String,
    pub puuid: String,
    pub rank_label: Option<String>,
    pub is_anonymized: bool,
    pub is_self: bool,
    pub is_premade: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChampSelectSummary {
    pub sub_phase: String,
    pub time_left_sec: i64,
    pub queue_type: String,
    pub is_ranked_solo_anonymized: bool,
    pub my_team: Vec<ChampSelectAlly>,
    pub my_team_bans: Vec<String>,
    pub their_team_bans: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGameEventSummary {
    pub event_id: i64,
    pub event_name: String,
    pub event_time_sec: f64,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGameSummary {
    pub game_mode: String,
    pub map_name: String,
    pub game_time_sec: f64,
    pub active_champion: String,
    pub level: u32,
    pub current_gold: i64,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
    pub recent_events: Vec<LiveGameEventSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndOfGameSummary {
    pub result: String,
    pub game_length_sec: u32,
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
    pub end_of_game: Option<EndOfGameSummary>,
}

pub fn queue_name_from_id(queue_id: i64) -> String {
    match queue_id {
        400 => "Normal Draft 5v5".to_string(),
        420 => "Ranked Solo/Duo".to_string(),
        430 => "Normal Blind 5v5".to_string(),
        440 => "Ranked Flex 5v5".to_string(),
        450 => "ARAM".to_string(),
        700 => "Clash".to_string(),
        _ => format!("Queue #{queue_id}"),
    }
}

pub fn champion_name_from_id(champ_id: i64) -> String {
    match champ_id {
        0 => "Selecting...".to_string(),
        64 => "Lee Sin".to_string(),
        103 => "Ahri".to_string(),
        157 => "Yasuo".to_string(),
        222 => "Jinx".to_string(),
        233 => "Briar".to_string(),
        234 => "Viego".to_string(),
        238 => "Zed".to_string(),
        266 => "Aatrox".to_string(),
        412 => "Thresh".to_string(),
        777 => "Yone".to_string(),
        35 => "Shaco".to_string(),
        _ => format!("Champion #{champ_id}"),
    }
}

pub fn extract_summoner_summary(
    summoner_val: Option<&Value>,
    ranked_val: Option<&Value>,
    presences_val: Option<&Value>,
) -> Option<SummonerSummary> {
    let sum = summoner_val?;
    let display_name = sum
        .get("gameName")
        .or_else(|| sum.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let tag_line = sum
        .get("tagLine")
        .and_then(|v| v.as_str())
        .unwrap_or("EUW")
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
        if let Some(local) = pres.get("local") {
            presence_state = local
                .get("summary")
                .or_else(|| local.get("state"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            status_message = local
                .get("statusMessage")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        } else if let Some(arr) = pres.get("presences").and_then(|v| v.as_array()) {
            if let Some(first) = arr.first() {
                presence_state = first
                    .get("summary")
                    .or_else(|| first.get("state"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                status_message = first
                    .get("statusMessage")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
    }

    Some(SummonerSummary {
        display_name,
        tag_line,
        summoner_level,
        xp_percent,
        puuid,
        rank_tier,
        rank_division,
        rank_lp,
        rank_wins,
        rank_losses,
        presence_state,
        status_message,
    })
}

pub fn extract_lobby_summary(lobby_val: Option<&Value>) -> Option<LobbySummary> {
    let lob = lobby_val?;
    let party_id = lob
        .get("partyId")
        .and_then(|v| v.as_str())
        .unwrap_or("local-party")
        .to_string();
    let queue_id = lob
        .get("queueId")
        .or_else(|| lob.get("gameConfig").and_then(|gc| gc.get("queueId")))
        .and_then(|v| v.as_i64())
        .unwrap_or(420);
    let queue_name = queue_name_from_id(queue_id);

    let mut members = Vec::new();
    if let Some(arr) = lob.get("members").and_then(|v| v.as_array()) {
        for m in arr {
            let summoner_name = m
                .get("summonerName")
                .or_else(|| m.get("gameName"))
                .and_then(|v| v.as_str())
                .unwrap_or("Member")
                .to_string();
            let puuid = m
                .get("puuid")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
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

            members.push(LobbyMemberSummary {
                summoner_name,
                puuid,
                is_leader,
                ready,
                first_position,
                second_position,
            });
        }
    }

    Some(LobbySummary {
        party_id,
        queue_id,
        queue_name,
        members,
    })
}

pub fn extract_matchmaking_summary(
    search_val: Option<&Value>,
    ready_val: Option<&Value>,
) -> Option<MatchmakingSummary> {
    let s = search_val?;
    let queue_id = s.get("queueId").and_then(|v| v.as_i64()).unwrap_or(420);
    let time_in_queue_sec = s
        .get("timeInQueue")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let estimated_queue_time_sec = s
        .get("estimatedQueueTime")
        .and_then(|v| v.as_f64())
        .unwrap_or(60.0);
    let is_currently_in_queue = s
        .get("isCurrentlyInQueue")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let ready_check_state = ready_val
        .and_then(|r| r.get("state"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let player_response = ready_val
        .and_then(|r| r.get("playerResponse"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    Some(MatchmakingSummary {
        queue_id,
        time_in_queue_sec,
        estimated_queue_time_sec,
        is_currently_in_queue,
        ready_check_state,
        player_response,
    })
}

pub fn extract_champ_select_summary(
    cs_val: Option<&Value>,
    lobby: Option<&LobbySummary>,
) -> Option<ChampSelectSummary> {
    let cs = cs_val?;
    let timer = cs.get("timer");
    let sub_phase = timer
        .and_then(|t| t.get("phase"))
        .and_then(|v| v.as_str())
        .unwrap_or("BAN_PICK")
        .to_string();
    let time_left_sec = timer
        .and_then(|t| t.get("adjustedTimeLeftInPhase"))
        .and_then(|v| v.as_i64())
        .unwrap_or(25);
    let local_cell_id = cs
        .get("localPlayerCellId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let queue_type = cs
        .get("queueType")
        .and_then(|v| v.as_str())
        .unwrap_or("RANKED_SOLO_5x5")
        .to_string();

    let premade_puuids: Vec<String> = lobby
        .map(|l| {
            l.members
                .iter()
                .filter_map(|m| m.puuid.clone())
                .collect()
        })
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
                .filter(|s| !s.is_empty())
                .unwrap_or(match idx {
                    0 => "TOP",
                    1 => "JUNGLE",
                    2 => "MIDDLE",
                    3 => "BOTTOM",
                    _ => "UTILITY",
                })
                .to_uppercase();

            let champ_id = member
                .get("championId")
                .or_else(|| member.get("championPickIntent"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let champion_name = member
                .get("championName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| champion_name_from_id(champ_id));

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

            let is_anonymized = !is_self && !is_premade && (raw_puuid.is_empty() || summoner_id == 0);
            if is_anonymized {
                any_anonymized = true;
            }

            let summoner_name = if is_anonymized {
                format!("Ally {}", idx + 1)
            } else {
                member
                    .get("summonerName")
                    .or_else(|| member.get("gameName"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(if is_self { "HypatiaObserver#EUW" } else { "Teammate" })
                    .to_string()
            };

            let rank_label = if is_anonymized {
                None
            } else {
                member
                    .get("rank")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            };

            my_team.push(ChampSelectAlly {
                cell_id,
                assigned_position,
                champion_name,
                summoner_name,
                puuid: raw_puuid.to_string(),
                rank_label,
                is_anonymized,
                is_self,
                is_premade,
            });
        }
    } else {
        // Fallback when synthetic payload only contains actions/bans
        let default_pick = cs
            .get("lastSelectedChampion")
            .and_then(|v| v.as_str())
            .unwrap_or("Ahri");
        my_team.push(ChampSelectAlly {
            cell_id: 0,
            assigned_position: "MIDDLE".to_string(),
            champion_name: default_pick.to_string(),
            summoner_name: "HypatiaObserver#EUW".to_string(),
            puuid: "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001".to_string(),
            rank_label: Some("EMERALD II (54 LP)".to_string()),
            is_anonymized: false,
            is_self: true,
            is_premade: false,
        });
    }

    let mut my_team_bans = Vec::new();
    let mut their_team_bans = Vec::new();
    if let Some(bans) = cs.get("bans") {
        if let Some(arr) = bans.get("myTeamBans").and_then(|v| v.as_array()) {
            for b in arr {
                if let Some(id) = b.as_i64() {
                    my_team_bans.push(champion_name_from_id(id));
                } else if let Some(s) = b.as_str() {
                    my_team_bans.push(s.to_string());
                }
            }
        }
        if let Some(arr) = bans.get("theirTeamBans").and_then(|v| v.as_array()) {
            for b in arr {
                if let Some(id) = b.as_i64() {
                    their_team_bans.push(champion_name_from_id(id));
                } else if let Some(s) = b.as_str() {
                    their_team_bans.push(s.to_string());
                }
            }
        }
    }

    Some(ChampSelectSummary {
        sub_phase,
        time_left_sec,
        queue_type,
        is_ranked_solo_anonymized: any_anonymized,
        my_team,
        my_team_bans,
        their_team_bans,
    })
}

pub fn extract_live_game_summary(
    allgame_val: Option<&Value>,
    active_val: Option<&Value>,
    events_val: Option<&Value>,
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
        .unwrap_or("Summoner's Rift")
        .to_string();
    let game_time_sec = game_data
        .and_then(|gd| gd.get("gameTime"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let level = active
        .and_then(|a| a.get("level"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;
    let current_gold = active
        .and_then(|a| a.get("currentGold"))
        .and_then(|v| v.as_f64().map(|f| f as i64).or_else(|| v.as_i64()))
        .unwrap_or(500);
    let active_champion = active
        .and_then(|a| a.get("championName"))
        .and_then(|v| v.as_str())
        .unwrap_or("Ahri")
        .to_string();

    let scores = active.and_then(|a| a.get("scores"));
    let kills = scores
        .and_then(|s| s.get("kills"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let deaths = scores
        .and_then(|s| s.get("deaths"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let assists = scores
        .and_then(|s| s.get("assists"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let creep_score = scores
        .and_then(|s| s.get("creepScore"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

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
        active_champion,
        level,
        current_gold,
        kills,
        deaths,
        assists,
        creep_score,
        recent_events,
    })
}

pub fn extract_eog_summary(eog_val: Option<&Value>) -> Option<EndOfGameSummary> {
    let eog = eog_val?;
    let result = eog
        .get("gameResult")
        .and_then(|v| v.as_str())
        .unwrap_or("Victory")
        .to_string();
    let game_length_sec = eog
        .get("gameLength")
        .and_then(|v| v.as_u64())
        .unwrap_or(1680) as u32;
    let local = eog.get("localPlayer");
    let champion_name = local
        .and_then(|l| l.get("championName"))
        .and_then(|v| v.as_str())
        .unwrap_or("Ahri")
        .to_string();
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
        champion_name,
        kills,
        deaths,
        assists,
        total_gold,
        damage_to_champions,
    })
}
