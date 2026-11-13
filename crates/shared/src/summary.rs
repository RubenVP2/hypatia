use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board::LiveBoard;
use crate::event::GamePhase;
use crate::i18n::Lang;
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

pub fn queue_name_from_id(queue_id: i64) -> String {
    queue_name_from_id_localized(queue_id, Lang::En)
}

pub fn queue_name_from_id_localized(queue_id: i64, lang: Lang) -> String {
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

pub fn champion_name_from_id(champ_id: i64) -> String {
    champion_name_from_id_localized(champ_id, Lang::En)
}

pub fn champion_name_from_id_localized(champ_id: i64, lang: Lang) -> String {
    if champ_id <= 0 {
        return lang.pick("Selecting...", "Sélection...").to_string();
    }
    if let Some(found) = crate::remote::builtin_champion_catalog()
        .into_iter()
        .find(|c| c.id == champ_id)
    {
        return found.name;
    }
    match champ_id {
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

fn fold_latin_ascii(ch: char) -> char {
    match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => 'y',
        other => other,
    }
}

/// Normalizes a champion name or internal alias key (`"Kai'Sa"`, `"LeeSin"`, `"Maître Yi"`, `"K'Santé"`)
/// into lowercase ASCII alphanumeric form for reliable ID lookup across locales.
pub fn normalize_champion_key(name: &str) -> String {
    name.chars()
        .map(fold_latin_ascii)
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Strips Riot Live Client `rawChampionName` localization prefixes (`game_character_displayname_MasterYi`).
pub fn extract_raw_champion_alias(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["game_character_displayname_", "character_"] {
        if lower.starts_with(prefix) {
            return trimmed[prefix.len()..].to_string();
        }
    }
    String::new()
}

/// Resolves a built-in Riot champion ID from its display name or internal key (0 if unknown).
pub fn champion_id_from_name(name: &str) -> i64 {
    let key = normalize_champion_key(name);
    if matches!(
        key.as_str(),
        "" | "selecting" | "selection" | "none" | "aucun" | "unknown" | "inconnu"
    ) {
        return 0;
    }
    if let Some(found) = crate::remote::builtin_champion_catalog()
        .into_iter()
        .find(|c| normalize_champion_key(&c.name) == key)
    {
        return found.id;
    }
    match key.as_str() {
        "ambessa" => 799,
        "anivia" => 34,
        "annie" => 1,
        "aphelios" => 523,
        "aurelionsol" => 136,
        "aurora" => 893,
        "belveth" => 200,
        "cassiopeia" => 69,
        "chogath" => 31,
        "corki" => 42,
        "drmundo" | "mundo" => 36,
        "elise" => 60,
        "evelynn" => 28,
        "fiddlesticks" => 9,
        "fizz" => 105,
        "gangplank" => 41,
        "gnar" => 150,
        "hecarim" => 120,
        "heimerdinger" => 74,
        "illaoi" => 420,
        "ivern" => 427,
        "kalista" => 429,
        "karthus" => 30,
        "kassadin" => 38,
        "katarina" => 55,
        "kayle" => 10,
        "kennen" => 85,
        "kindred" => 203,
        "kled" => 240,
        "kogmaw" => 96,
        "ksante" => 897,
        "lissandra" => 127,
        "malzahar" => 90,
        "maokai" => 57,
        "maitreyi" | "masteryi" => 11,
        "mel" => 800,
        "milio" => 902,
        "monkeyking" | "wukong" => 62,
        "naafiri" => 950,
        "nasus" => 75,
        "nidalee" => 76,
        "nilah" => 895,
        "nocturne" => 56,
        "nunu" | "nunuetwillump" | "nunuwillump" => 20,
        "olaf" => 2,
        "poppy" => 78,
        "qiyana" => 246,
        "quinn" => 133,
        "rammus" => 33,
        "reksai" => 421,
        "rell" => 526,
        "renata" | "renataglasc" => 888,
        "rengar" => 107,
        "rumble" => 68,
        "ryze" => 13,
        "shyvana" => 102,
        "singed" => 27,
        "sion" => 14,
        "skarner" => 72,
        "sona" => 37,
        "swain" => 50,
        "tahmkench" => 223,
        "taliyah" => 163,
        "talon" => 91,
        "taric" => 44,
        "teemo" => 17,
        "trundle" => 48,
        "tryndamere" => 23,
        "twitch" => 29,
        "udyr" => 77,
        "urgot" => 6,
        "varus" => 110,
        "veigar" => 45,
        "velkoz" => 161,
        "vex" => 711,
        "viktor" => 112,
        "vladimir" => 8,
        "warwick" => 19,
        "xayah" => 498,
        "xerath" => 101,
        "xinzhao" => 5,
        "yorick" => 83,
        "yunara" => 804,
        "zaahen" => 805,
        "zilean" => 26,
        "zyra" => 143,
        _ => 0,
    }
}

pub fn extract_summoner_summary_localized(
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

pub fn extract_summoner_summary(
    summoner_val: Option<&Value>,
    ranked_val: Option<&Value>,
    presences_val: Option<&Value>,
) -> Option<SummonerSummary> {
    extract_summoner_summary_localized(summoner_val, ranked_val, presences_val, Lang::En)
}

pub fn extract_lobby_summary_localized(lobby_val: Option<&Value>, lang: Lang) -> Option<LobbySummary> {
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
    let queue_name = queue_name_from_id_localized(queue_id, lang);

    let mut members = Vec::new();
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

pub fn extract_lobby_summary(lobby_val: Option<&Value>) -> Option<LobbySummary> {
    extract_lobby_summary_localized(lobby_val, Lang::En)
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

pub fn extract_champ_select_summary_localized(
    cs_val: Option<&Value>,
    lobby: Option<&LobbySummary>,
    lang: Lang,
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

            let raw_champ_id = member
                .get("championId")
                .or_else(|| member.get("championPickIntent"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let champion_name = member
                .get("championName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| champion_name_from_id_localized(raw_champ_id, lang));
            let champ_id = if raw_champ_id > 0 {
                raw_champ_id
            } else {
                champion_id_from_name(&champion_name)
            };
            let spell1_id = member
                .get("spell1Id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let spell2_id = member
                .get("spell2Id")
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

            let is_anonymized = !is_self && !is_premade && (raw_puuid.is_empty() || summoner_id == 0);
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
                    .unwrap_or(if is_self {
                        "HypatiaObserver#EUW"
                    } else {
                        lang.pick("Teammate", "Coéquipier")
                    })
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
            champion_id: champion_id_from_name(default_pick),
            champion_name: default_pick.to_string(),
            spell1_id: 4,
            spell2_id: 14,
            summoner_name: "HypatiaObserver#EUW".to_string(),
            puuid: "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001".to_string(),
            rank_label: Some("EMERALD II (54 LP)".to_string()),
            is_anonymized: false,
            is_self: true,
            is_premade: false,
        });
    }

    let (my_team_bans, my_team_ban_ids) = extract_ban_list_localized(cs.get("bans"), "myTeamBans", lang);
    let (their_team_bans, their_team_ban_ids) =
        extract_ban_list_localized(cs.get("bans"), "theirTeamBans", lang);

    let mut their_team = Vec::new();
    if let Some(arr) = cs.get("theirTeam").and_then(|v| v.as_array()) {
        for (idx, member) in arr.iter().enumerate() {
            let cell_id = member.get("cellId").and_then(|v| v.as_i64()).unwrap_or(5 + idx as i64);
            let champion_id = member.get("championId").and_then(|v| v.as_i64()).unwrap_or(0);
            let champion_name = member
                .get("championName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    if champion_id == 0 {
                        String::new()
                    } else {
                        champion_name_from_id_localized(champion_id, lang)
                    }
                });
            their_team.push(ChampSelectEnemy { cell_id, champion_id, champion_name });
        }
    }

    let acting = cs
        .get("actions")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|turn| turn.as_array())
        .flatten()
        .find(|a| a.get("isInProgress").and_then(|v| v.as_bool()).unwrap_or(false));
    let acting_cell_id = acting.and_then(|a| a.get("actorCellId")).and_then(|v| v.as_i64());
    let acting_type = acting
        .and_then(|a| a.get("type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(ChampSelectSummary {
        their_team,
        acting_cell_id,
        acting_type,
        sub_phase,
        time_left_sec,
        queue_type,
        is_ranked_solo_anonymized: any_anonymized,
        my_team,
        my_team_bans,
        their_team_bans,
        my_team_ban_ids,
        their_team_ban_ids,
    })
}

pub fn extract_champ_select_summary(
    cs_val: Option<&Value>,
    lobby: Option<&LobbySummary>,
) -> Option<ChampSelectSummary> {
    extract_champ_select_summary_localized(cs_val, lobby, Lang::En)
}

fn extract_ban_list_localized(
    bans: Option<&Value>,
    key: &str,
    lang: Lang,
) -> (Vec<String>, Vec<i64>) {
    let mut names = Vec::new();
    let mut ids = Vec::new();
    let Some(arr) = bans.and_then(|b| b.get(key)).and_then(|v| v.as_array()) else {
        return (names, ids);
    };
    for entry in arr {
        if let Some(id) = entry.as_i64() {
            names.push(champion_name_from_id_localized(id, lang));
            ids.push(id);
        } else if let Some(s) = entry.as_str() {
            ids.push(champion_id_from_name(s));
            names.push(s.to_string());
        }
    }
    (names, ids)
}

pub fn extract_live_game_summary_localized(
    allgame_val: Option<&Value>,
    active_val: Option<&Value>,
    events_val: Option<&Value>,
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
                .unwrap_or("NONE")
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
                .unwrap_or_else(|| {
                    let from_name = champion_id_from_name(&champ);
                    if from_name > 0 {
                        from_name
                    } else if !champion_alias.is_empty() {
                        champion_id_from_name(&champion_alias)
                    } else {
                        0
                    }
                });
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
        .unwrap_or(500);
    let active_champion = matched_self
        .as_ref()
        .map(|m| m.champion_name.clone())
        .or_else(|| {
            active
                .and_then(|a| a.get("championName"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Ahri".to_string());

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

    let active_champion_id = matched_self
        .as_ref()
        .map(|m| m.champion_id)
        .filter(|&id| id > 0)
        .unwrap_or_else(|| champion_id_from_name(&active_champion));

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

pub fn extract_live_game_summary(
    allgame_val: Option<&Value>,
    active_val: Option<&Value>,
    events_val: Option<&Value>,
) -> Option<LiveGameSummary> {
    extract_live_game_summary_localized(allgame_val, active_val, events_val, Lang::En)
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
    let champion_id = local
        .and_then(|l| l.get("championId"))
        .and_then(|v| v.as_i64())
        .filter(|&id| id > 0)
        .unwrap_or_else(|| champion_id_from_name(&champion_name));
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
    use serde_json::json;

    #[test]
    fn resolves_champion_ids_from_names_and_aliases() {
        assert_eq!(champion_id_from_name("Ahri"), 103);
        assert_eq!(champion_id_from_name("Kai'Sa"), 145);
        assert_eq!(champion_id_from_name("KaiSa"), 145);
        assert_eq!(champion_id_from_name("Lee Sin"), 64);
        assert_eq!(champion_id_from_name("LeeSin"), 64);
        assert_eq!(champion_id_from_name("Wukong"), 62);
        assert_eq!(champion_id_from_name("Maître Yi"), 11);
        assert_eq!(champion_id_from_name("K'Santé"), 897);
        assert_eq!(champion_id_from_name("Nunu et Willump"), 20);
        assert_eq!(extract_raw_champion_alias("game_character_displayname_MasterYi"), "MasterYi");
        assert_eq!(champion_id_from_name("Sélection..."), 0);
        assert_eq!(champion_id_from_name("Selecting..."), 0);
    }

    #[test]
    fn extracts_icons_spells_and_bans_into_summaries() {
        let sum = extract_summoner_summary(
            Some(&json!({"gameName": "Ezo", "tagLine": "EUW", "profileIconId": 548})),
            None,
            None,
        )
        .unwrap();
        assert_eq!(sum.profile_icon_id, 548);

        let cs = extract_champ_select_summary(
            Some(&json!({
                "localPlayerCellId": 0,
                "myTeam": [{"cellId": 0, "championId": 103, "spell1Id": 4, "spell2Id": 14, "assignedPosition": "MIDDLE"}],
                "bans": {"myTeamBans": [238], "theirTeamBans": ["Yasuo"]}
            })),
            None,
        )
        .unwrap();
        assert_eq!(cs.my_team[0].spell1_id, 4);
        assert_eq!(cs.my_team[0].spell2_id, 14);
        assert_eq!(cs.my_team_ban_ids, vec![238]);
        assert_eq!(cs.their_team_ban_ids, vec![157]);

        let live = extract_live_game_summary(
            Some(&json!({
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
                        "championName": "Maître Yi",
                        "rawChampionName": "game_character_displayname_MasterYi",
                        "team": "ORDER",
                        "position": "JUNGLE"
                    }
                ]
            })),
            None,
            None,
        )
        .unwrap();
        assert_eq!(live.active_champion_id, 145);
        assert_eq!(live.players[0].champion_id, 145);
        assert_eq!(live.players[1].champion_id, 11);
        assert_eq!(live.players[1].champion_alias, "MasterYi");

        let en_lobby = extract_lobby_summary_localized(Some(&json!({"queueId": 420, "members": [{}]})), Lang::En).unwrap();
        let fr_lobby = extract_lobby_summary_localized(Some(&json!({"queueId": 420, "members": [{}]})), Lang::Fr).unwrap();
        assert_eq!(en_lobby.queue_name, "Ranked Solo/Duo");
        assert_eq!(fr_lobby.queue_name, "Classée Solo/Duo");
        assert_eq!(en_lobby.members[0].summoner_name, "Member");
        assert_eq!(fr_lobby.members[0].summoner_name, "Membre");

        let en_cs = extract_champ_select_summary_localized(Some(&json!({"localPlayerCellId": 0, "myTeam": [{"cellId": 0, "championId": 0}]})), None, Lang::En).unwrap();
        let fr_cs = extract_champ_select_summary_localized(Some(&json!({"localPlayerCellId": 0, "myTeam": [{"cellId": 0, "championId": 0}]})), None, Lang::Fr).unwrap();
        assert_eq!(en_cs.my_team[0].champion_name, "Selecting...");
        assert_eq!(fr_cs.my_team[0].champion_name, "Sélection...");
    }
}
