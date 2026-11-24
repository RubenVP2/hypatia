use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::board::{extract_live_board_localized, LiveBoard};
use crate::champions::{
    builtin_skins_for_localized, builtin_ward_skins_localized, extract_raw_champion_alias,
    is_champion_placeholder, is_swiftplay_queue, keystone_name_from_id_localized,
    recommended_pages_for_localized, rune_style_name_localized, ChampionLookup, Position,
};
use crate::draft_helper::{project_champ_select_helper, ChampSelectHelper, HelperStats};
use crate::event::GamePhase;
use crate::game_text::{game_name, game_name_or, GameTextKind};
use crate::i18n::Lang;
use crate::remote::{
    RemoteActiveAction, RemoteChampionOption, RemoteChromaOption, RemoteIncomingSwap,
    RemoteQueueOption, RemoteRecommendedCard, RemoteRunePage, RemoteSkinOption,
    RemoteWardSkinOption, SwiftplaySlotSelection,
};
use crate::source::{SourceId, SourceState};
use crate::stats::StatsQuery;

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
    #[serde(default)]
    pub swiftplay_slots: Vec<SwiftplaySlotSelection>,
    #[serde(default)]
    pub swiftplay_slot_skins: Vec<Vec<RemoteSkinOption>>,
    #[serde(default)]
    pub swiftplay_champions: Vec<RemoteChampionOption>,
    #[serde(default)]
    pub swiftplay_rune_pages: Vec<RemoteRunePage>,
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
    /// True once this slot's pick is locked in; false while the champion is only hovered or
    /// declared. Slots of a session without pick actions count as locked when they hold one.
    #[serde(default)]
    pub pick_locked: bool,
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
    /// Epoch timestamp (ms) when the current phase timer expires, derived from
    /// `timer.internalNowInEpochMs + timer.adjustedTimeLeftInPhase` when the LCU provides it.
    #[serde(default)]
    pub timer_deadline_ms: Option<i64>,
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
    #[serde(default)]
    pub intent_action: Option<RemoteActiveAction>,
    #[serde(default)]
    pub incoming_swaps: Vec<RemoteIncomingSwap>,
    #[serde(default)]
    pub champions: Vec<RemoteChampionOption>,
    #[serde(default)]
    pub comfort_champion_ids: Vec<i64>,
    #[serde(default)]
    pub available_skins: Vec<RemoteSkinOption>,
    #[serde(default)]
    pub selected_skin_name: String,
    #[serde(default)]
    pub ward_skin_id: i64,
    #[serde(default)]
    pub available_wards: Vec<RemoteWardSkinOption>,
    #[serde(default)]
    pub rune_pages: Vec<RemoteRunePage>,
    #[serde(default)]
    pub recommended_rune_pages: Vec<RemoteRunePage>,
    #[serde(default)]
    pub recommended_cards: Vec<RemoteRecommendedCard>,
    #[serde(default)]
    pub editable_rune_page_id: Option<i64>,
    /// True in ARAM / ARAM: Mayhem when the bench swap pool is active.
    #[serde(default)]
    pub bench_enabled: bool,
    /// Champions currently available on the ARAM bench.
    #[serde(default)]
    pub bench_champions: Vec<ChampSelectBenchChampion>,
    #[serde(default)]
    pub rerolls_remaining: u32,
    /// Build, lane matchup and team comps for the player's own pick (champ-select helper).
    #[serde(default)]
    pub helper: ChampSelectHelper,
}

impl ChampSelectSummary {
    /// Remaining whole seconds in the current phase at `now_ms`, using `timer_deadline_ms`
    /// when the LCU supplied `internalNowInEpochMs` and falling back to `time_left_sec`.
    pub fn time_left_sec_at(&self, now_ms: i64) -> i64 {
        if let Some(deadline) = self.timer_deadline_ms {
            let start_ms = deadline.saturating_sub(self.time_left_sec.saturating_mul(1000));
            if now_ms >= start_ms.saturating_sub(5_000)
                && now_ms <= deadline.saturating_add(3_600_000)
            {
                let rem_ms = (deadline - now_ms).max(0);
                return (rem_ms + 999) / 1000;
            }
        }
        self.time_left_sec
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChampSelectBenchChampion {
    pub champion_id: i64,
    pub champion_name: String,
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
    /// Skin number within the champion (`skinID`, 0 for the base skin).
    #[serde(default)]
    pub skin_id: i64,
    /// Real-time CS per minute (`0.0` before 60s).
    #[serde(default)]
    pub cs_per_min: f32,
    /// Signed CS lead/deficit against the direct opposing laner on Summoner's Rift (from 90s).
    #[serde(default)]
    pub cs_diff_vs_lane: Option<i32>,
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
    #[serde(default)]
    pub self_cs_per_min: f32,
    #[serde(default)]
    pub lane_cs_diff: Option<i32>,
    #[serde(default)]
    pub personal_avg_cs_per_min: Option<f32>,
    pub recent_events: Vec<LiveGameEventSummary>,
    #[serde(default)]
    pub players: Vec<LiveGamePlayerSummary>,
}

/// One player's line in the post-game block (champion only: no summoner names).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EndOfGamePlayer {
    pub champion_id: i64,
    pub champion_name: String,
    /// Full skin id (`championId * 1000 + skin`), read from the splash path; 0 when unknown.
    #[serde(default)]
    pub skin_id: i64,
    /// Riot position (`TOP`, `JUNGLE`, `MIDDLE`, `BOTTOM`, `UTILITY`), empty when unknown.
    pub position: String,
    pub level: u32,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
    #[serde(default)]
    pub items: Vec<i64>,
    #[serde(default)]
    pub keystone_id: i64,
    #[serde(default)]
    pub sub_style_id: i64,
    #[serde(default)]
    pub spell1_id: i64,
    #[serde(default)]
    pub spell2_id: i64,
    #[serde(default)]
    pub damage_to_champions: u32,
    #[serde(default)]
    pub damage_taken: u32,
    #[serde(default)]
    pub gold_earned: u32,
    #[serde(default)]
    pub vision_score: u32,
    #[serde(default)]
    pub kill_participation: f32,
    #[serde(default)]
    pub damage_share: f32,
    #[serde(default)]
    pub performance_score: u8,
    #[serde(default)]
    pub placement: u8,
    #[serde(default)]
    pub is_mvp: bool,
    #[serde(default)]
    pub is_ace: bool,
    pub is_self: bool,
}

/// One team of the post-game block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EndOfGameTeam {
    pub is_player_team: bool,
    pub won: bool,
    #[serde(default)]
    pub total_kills: u32,
    #[serde(default)]
    pub total_gold: u32,
    #[serde(default)]
    pub total_damage: u32,
    #[serde(default)]
    pub total_vision: u32,
    pub players: Vec<EndOfGamePlayer>,
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
    /// Full skin id of the player's champion; 0 when unknown.
    #[serde(default)]
    pub skin_id: i64,
    /// Lane minions plus jungle monsters.
    #[serde(default)]
    pub creep_score: u32,
    #[serde(default)]
    pub vision_score: u32,
    /// Both teams, the player's own first; empty when the block carries none.
    #[serde(default)]
    pub teams: Vec<EndOfGameTeam>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SummarySnapshot {
    pub phase: Option<GamePhase>,
    pub sources: Vec<SourceState>,
    pub summoner: Option<SummonerSummary>,
    pub lobby: Option<LobbySummary>,
    pub matchmaking: Option<MatchmakingSummary>,
    pub champ_select: Option<ChampSelectSummary>,
    #[serde(default)]
    pub editable_rune_page_id: Option<i64>,
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
    pub pickable_champion_ids: Option<&'a Value>,
    pub bannable_champion_ids: Option<&'a Value>,
    pub skin_carousel: Option<&'a Value>,
    pub ward_skins: Option<&'a Value>,
    pub rune_pages: Option<&'a Value>,
    pub live_allgame: Option<&'a Value>,
    pub live_active: Option<&'a Value>,
    pub live_events: Option<&'a Value>,
    pub end_of_game: Option<&'a Value>,
}

/// Localized queue-type display label shared across Desktop and Mobile Remote.
pub fn queue_type_label(queue_type: &str, lang: Lang) -> String {
    let trimmed = queue_type.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "RANKED_SOLO_5X5" | "RANKED SOLO 5X5" => {
            lang.pick("Ranked Solo/Duo", "Classée Solo/Duo").to_string()
        }
        "RANKED_FLEX_SR" | "RANKED FLEX SR" => {
            lang.pick("Ranked Flex", "Classée Flexible").to_string()
        }
        "NORMAL" | "NORMAL_DRAFT" | "DRAFT_PICK" => {
            lang.pick("Normal Draft", "Faille · Draft").to_string()
        }
        "SWIFTPLAY" => lang.pick("Swiftplay", "Partie accélérée").to_string(),
        "QUICKPLAY" => lang.pick("Quickplay", "Partie rapide").to_string(),
        "ARAM" => "ARAM".to_string(),
        "URF" | "ARURF" => "URF".to_string(),
        "ARENA" | "CHERRY" => lang.pick("Arena", "Arène").to_string(),
        "PRACTICETOOL" | "PRACTICE_TOOL" => lang
            .pick("Practice Tool", "Outil d'entraînement")
            .to_string(),
        "CUSTOM" => lang.pick("Custom", "Partie personnalisée").to_string(),
        _ => trimmed.replace('_', " "),
    }
}

/// Localized champion-select sub-phase label (`PLANNING`, `BAN_PICK`, `FINALIZATION`, ...).
pub fn sub_phase_label(sub_phase: &str, lang: Lang) -> String {
    let trimmed = sub_phase.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "PLANNING" => lang.pick("Planning", "Planification").to_string(),
        "BAN_PICK" => lang.pick("Ban / Pick", "Bannissements & Choix").to_string(),
        "FINALIZATION" => lang.pick("Finalization", "Finalisation").to_string(),
        "GAME_STARTING" => lang
            .pick("Game Starting", "Lancement de la partie")
            .to_string(),
        _ => trimmed.replace('_', " "),
    }
}

/// Localized ranked tier label (`IRON` -> `"Iron"` / `"Fer"`, etc.).
pub fn rank_tier_label(tier: &str, lang: Lang) -> &str {
    match tier.trim().to_ascii_uppercase().as_str() {
        "IRON" => lang.pick("Iron", "Fer"),
        "BRONZE" => "Bronze",
        "SILVER" => lang.pick("Silver", "Argent"),
        "GOLD" => lang.pick("Gold", "Or"),
        "PLATINUM" => lang.pick("Platinum", "Platine"),
        "EMERALD" => lang.pick("Emerald", "Émeraude"),
        "DIAMOND" => lang.pick("Diamond", "Diamant"),
        "MASTER" => lang.pick("Master", "Maître"),
        "GRANDMASTER" => lang.pick("Grandmaster", "Grand Maître"),
        "CHALLENGER" => "Challenger",
        _ => tier,
    }
}

/// Two-letter uppercase monogram initials for a champion or summoner name (empty on placeholders).
pub fn champ_initials_str(name: &str) -> String {
    let trimmed = name.trim();
    if is_champion_placeholder(trimmed) {
        return String::new();
    }
    let words: Vec<&str> = trimmed
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let s: String = if words.len() >= 2 {
        words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect()
    } else {
        trimmed.chars().take(2).collect()
    };
    s.to_uppercase()
}

/// Formats `seconds` as `m:ss` (truncated whole seconds, e.g. `4:05`).
pub fn format_mm_ss_str(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Formats `seconds` as zero-padded `mm:ss` (rounded whole seconds, e.g. `04:05`).
pub fn format_clock_mm_ss(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// Formats a byte count with localized SI units (`B`/`o`, `KB`/`Ko`, `MB`/`Mo`).
pub fn format_bytes(bytes: u64, lang: Lang) -> String {
    if bytes >= 1_048_576 {
        format!(
            "{:.1} {}",
            bytes as f64 / 1_048_576.0,
            lang.pick("MB", "Mo")
        )
    } else if bytes >= 1024 {
        format!("{:.1} {}", bytes as f64 / 1024.0, lang.pick("KB", "Ko"))
    } else {
        format!("{} {}", bytes, lang.pick("B", "o"))
    }
}

/// Formats a millisecond epoch timestamp as `HH:MM:SS`.
pub fn format_ts_hms(ts_ms: i64) -> String {
    let total_secs = (ts_ms.max(0) / 1000) % 86_400;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

/// Short localized badge label and index for a [`SourceId`].
pub fn source_short_label(src: SourceId, lang: Lang) -> (&'static str, i32) {
    match src {
        SourceId::RiotClient => ("RC", 0),
        SourceId::LeagueClient => ("LCU", 1),
        SourceId::LiveGame => (lang.pick("GAME", "JEU"), 2),
        SourceId::System => ("SYS", 3),
    }
}

/// Allowlisted League queue IDs that Mobile Remote may create via `POST /lol-lobby/v2/lobby`.
pub const ALLOWED_LOBBY_QUEUE_IDS: &[i64] =
    &[400, 420, 440, 450, 480, 490, 870, 880, 890, 900, 1700, 1900];

/// Primary modern League queues offered on Mobile Remote for 1-tap Lobby creation/switching.
pub fn default_lobby_queues_localized(lang: Lang) -> Vec<RemoteQueueOption> {
    [
        (420, "ranked"),
        (440, "ranked"),
        (400, "rift"),
        (480, "rift"),
        (450, "modes"),
        (1700, "modes"),
    ]
    .into_iter()
    .map(|(queue_id, category)| RemoteQueueOption {
        queue_id,
        name: queue_name_from_id_localized(queue_id, lang),
        category: category.to_string(),
    })
    .collect()
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
        (480, Lang::Fr) => "Partie accélérée".to_string(),
        (490, Lang::Fr) => "Partie rapide".to_string(),
        (870..=890, Lang::Fr) => "Coop vs IA".to_string(),
        (1700 | 1710, Lang::Fr) => "Arène".to_string(),
        (400, Lang::En) => "Normal Draft 5v5".to_string(),
        (420, Lang::En) => "Ranked Solo/Duo".to_string(),
        (430, Lang::En) => "Normal Blind 5v5".to_string(),
        (440, Lang::En) => "Ranked Flex 5v5".to_string(),
        (480, Lang::En) => "Swiftplay".to_string(),
        (490, Lang::En) => "Quickplay".to_string(),
        (870..=890, Lang::En) => "Co-op vs. AI".to_string(),
        (1700 | 1710, Lang::En) => "Arena".to_string(),
        (450, _) => "ARAM".to_string(),
        (700, _) => "Clash".to_string(),
        (900, _) => "ARURF".to_string(),
        (1900, _) => "URF".to_string(),
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
    project_match_with(inputs, champions, None, lang)
}

/// [`project_match`] with Champion Stats for the champ-select helper
/// ([`ChampSelectSummary::helper`]); without them the helper holds only offline comp analysis.
pub fn project_match_with(
    inputs: &MatchInputs<'_>,
    champions: &dyn ChampionLookup,
    stats: Option<&dyn HelperStats>,
    lang: Lang,
) -> SummarySnapshot {
    let summoner = extract_summoner_summary(inputs.summoner, inputs.ranked, inputs.presences, lang);
    let lobby = extract_lobby_summary(
        inputs.lobby,
        inputs.rune_pages,
        inputs.skin_carousel,
        summoner.as_ref(),
        champions,
        lang,
    );
    let matchmaking = extract_matchmaking_summary(
        inputs.matchmaking_search,
        inputs.ready_check,
        lobby.as_ref(),
        lang,
    );
    let mut champ_select =
        extract_champ_select_summary(inputs, summoner.as_ref(), lobby.as_ref(), champions, lang);
    if let Some(cs) = champ_select.as_mut() {
        let queue_id = lobby
            .as_ref()
            .map(|l| l.queue_id)
            .filter(|&id| id > 0)
            .or_else(|| matchmaking.as_ref().map(|m| m.queue_id))
            .unwrap_or(0);
        cs.helper = project_champ_select_helper(cs, queue_id, champions, stats, lang);
        populate_champ_select_stats_enrichment(cs, stats, lang);
    }
    let editable_rune_page_id = champ_select
        .as_ref()
        .and_then(|cs| cs.editable_rune_page_id)
        .or_else(|| extract_rune_pages(inputs.rune_pages, lang).1);
    let mut live_game = extract_live_game_summary(
        inputs.live_allgame,
        inputs.live_active,
        inputs.live_events,
        champions,
        lang,
    );
    if let (Some(lg), Some(stats)) = (live_game.as_mut(), stats) {
        if lg.active_champion_id > 0 {
            if let Some(profile) = stats.player_profile().ready() {
                lg.personal_avg_cs_per_min = profile
                    .champions
                    .iter()
                    .find(|c| c.champion_id == lg.active_champion_id && c.games > 0)
                    .map(|c| c.cs_per_min);
            }
        }
    }
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
        editable_rune_page_id,
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
    let sum = summoner_val.filter(|v| v.is_object())?;
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
            rank_tier = s
                .get("tier")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
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
    rune_pages_val: Option<&Value>,
    skin_carousel_val: Option<&Value>,
    summoner: Option<&SummonerSummary>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Option<LobbySummary> {
    let lob = lobby_val.filter(|v| v.is_object())?;
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
        .or_else(|| {
            summoner
                .map(|s| s.display_name.as_str())
                .filter(|s| !s.is_empty())
        });

    let mut members = Vec::new();
    let mut matched_any_self = false;
    let mut member_slots_raw: Option<&Vec<Value>> = None;
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
            let m_summoner_id = m.get("summonerId").and_then(|v| v.as_u64()).unwrap_or(0);
            let is_leader = m.get("isLeader").and_then(|v| v.as_bool()).unwrap_or(false);
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
                    summoner_name == ln || summoner.is_some_and(|s| summoner_name == s.riot_id())
                });
            if is_self {
                matched_any_self = true;
                member_slots_raw = m.get("playerSlots").and_then(|v| v.as_array());
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

    let is_swiftplay = is_swiftplay_queue(queue_id, &queue_name);
    let (swiftplay_slots, swiftplay_slot_skins, swiftplay_champions, swiftplay_rune_pages) =
        if is_swiftplay {
            let (pages, _) = extract_rune_pages(rune_pages_val, lang);
            let active_page_id = pages
                .iter()
                .find(|p| p.current)
                .or_else(|| pages.first())
                .map(|p| p.id)
                .unwrap_or(0);
            let raw_slots = local_obj
                .and_then(|m| m.get("playerSlots"))
                .and_then(|v| v.as_array())
                .or(member_slots_raw);
            let mut slots = Vec::new();
            if let Some(arr) = raw_slots {
                for (idx, s) in arr.iter().take(2).enumerate() {
                    let cid = s.get("championId").and_then(|v| v.as_i64()).unwrap_or(0);
                    let pos = s
                        .get("positionPreference")
                        .or_else(|| s.get("position"))
                        .and_then(|v| v.as_str())
                        .and_then(Position::parse)
                        .map(|p| p.lcu_code().to_string())
                        .unwrap_or_else(|| {
                            if idx == 0 {
                                local_first_position
                                    .clone()
                                    .unwrap_or_else(|| "MIDDLE".to_string())
                            } else {
                                local_second_position
                                    .clone()
                                    .unwrap_or_else(|| "TOP".to_string())
                            }
                        });
                    let skin_id = s
                        .get("skinId")
                        .and_then(|v| v.as_i64())
                        .filter(|&id| id > 0)
                        .unwrap_or(if cid > 0 { cid * 1000 } else { 0 });
                    let spell1_id = s
                        .get("spell1")
                        .or_else(|| s.get("spell1Id"))
                        .and_then(|v| v.as_i64())
                        .filter(|&id| id > 0)
                        .unwrap_or(4);
                    let spell2_id = s
                        .get("spell2")
                        .or_else(|| s.get("spell2Id"))
                        .and_then(|v| v.as_i64())
                        .filter(|&id| id > 0)
                        .unwrap_or(if pos == "JUNGLE" { 11 } else { 14 });
                    let perk_page_id = s
                        .get("perkPageId")
                        .and_then(|v| v.as_i64())
                        .filter(|&id| id > 0)
                        .unwrap_or(active_page_id);
                    slots.push(SwiftplaySlotSelection {
                        champion_id: cid,
                        champion_name: if cid > 0 {
                            champions.resolve_champion_name_localized(cid, lang)
                        } else {
                            String::new()
                        },
                        position: pos,
                        skin_id,
                        spell1_id,
                        spell2_id,
                        perk_page_id,
                    });
                }
            }
            while slots.len() < 2 {
                let idx = slots.len();
                let pos = if idx == 0 {
                    local_first_position
                        .clone()
                        .filter(|p| p != "FILL")
                        .unwrap_or_else(|| "MIDDLE".to_string())
                } else {
                    let fallback = local_second_position
                        .clone()
                        .filter(|p| p != "FILL")
                        .unwrap_or_else(|| "TOP".to_string());
                    if slots.first().is_some_and(|s0| s0.position == fallback) {
                        if fallback == "MIDDLE" {
                            "TOP".to_string()
                        } else {
                            "MIDDLE".to_string()
                        }
                    } else {
                        fallback
                    }
                };
                slots.push(SwiftplaySlotSelection {
                    champion_id: 0,
                    champion_name: String::new(),
                    spell1_id: 4,
                    spell2_id: if pos == "JUNGLE" { 11 } else { 14 },
                    position: pos,
                    skin_id: 0,
                    perk_page_id: active_page_id,
                });
            }
            let slot_skins: Vec<Vec<RemoteSkinOption>> = slots
                .iter()
                .map(|sl| {
                    extract_available_skins(
                        skin_carousel_val.filter(|_| false),
                        sl.champion_id,
                        sl.skin_id,
                        champions,
                        lang,
                    )
                })
                .collect();
            (slots, slot_skins, champions.champion_catalog(lang), pages)
        } else {
            (Vec::new(), Vec::new(), Vec::new(), Vec::new())
        };

    Some(LobbySummary {
        party_id,
        queue_id,
        queue_name,
        members,
        local_first_position,
        local_second_position,
        local_is_leader,
        swiftplay_slots,
        swiftplay_slot_skins,
        swiftplay_champions,
        swiftplay_rune_pages,
    })
}

fn extract_matchmaking_summary(
    search_val: Option<&Value>,
    ready_val: Option<&Value>,
    lobby: Option<&LobbySummary>,
    lang: Lang,
) -> Option<MatchmakingSummary> {
    let search_val = search_val.filter(|v| v.is_object());
    let ready_val = ready_val.filter(|v| v.is_object());
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

/// Cells whose pick action is completed, or `None` when the session carries no pick action
/// at all (some queues and fixtures), in which case lock state is unknown.
fn completed_pick_cells(cs: &Value) -> Option<HashSet<i64>> {
    let turns = cs.get("actions")?.as_array()?;
    let mut any_pick = false;
    let mut locked = HashSet::new();
    for act in turns.iter().filter_map(|t| t.as_array()).flatten() {
        if act.get("type").and_then(|v| v.as_str()) != Some("pick") {
            continue;
        }
        any_pick = true;
        if act.get("completed").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(cell) = act.get("actorCellId").and_then(|v| v.as_i64()) {
                locked.insert(cell);
            }
        }
    }
    any_pick.then_some(locked)
}

#[derive(Default)]
struct ParsedChampSelectActions {
    acting_cell_id: Option<i64>,
    acting_type: Option<String>,
    cell_acting: HashMap<i64, String>,
    local_action: Option<RemoteActiveAction>,
    intent_action: Option<RemoteActiveAction>,
}

fn parse_champ_select_actions(
    cs: &Value,
    local_cell_id: i64,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> ParsedChampSelectActions {
    let mut out = ParsedChampSelectActions::default();

    let Some(turns) = cs.get("actions").and_then(|v| v.as_array()) else {
        return out;
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
        let champ_id = act.get("championId").and_then(|v| v.as_i64()).unwrap_or(0);

        if in_progress && !completed {
            if out.acting_cell_id.is_none() {
                out.acting_cell_id = Some(actor_cell);
                out.acting_type = Some(action_type.clone());
            }
            out.cell_acting.insert(actor_cell, action_type.clone());
        } else if in_progress && out.acting_cell_id.is_none() {
            out.acting_cell_id = Some(actor_cell);
            out.acting_type = Some(action_type.clone());
        }

        if actor_cell == local_cell_id {
            let champ_name = if champ_id > 0 {
                champions.resolve_champion_name_localized(champ_id, lang)
            } else {
                String::new()
            };
            if action_type == "pick" && !completed && out.intent_action.is_none() {
                out.intent_action = Some(RemoteActiveAction {
                    action_id,
                    action_type: action_type.clone(),
                    champion_id: champ_id,
                    champion_name: champ_name.clone(),
                    completed: false,
                    is_my_turn: in_progress,
                });
            }
            let should_take = match &out.local_action {
                None => true,
                Some(prev) => {
                    (in_progress && !completed && !prev.is_my_turn)
                        || (!completed && prev.completed)
                }
            };
            if should_take {
                out.local_action = Some(RemoteActiveAction {
                    action_id,
                    action_type,
                    champion_id: champ_id,
                    champion_name: champ_name,
                    completed,
                    is_my_turn: in_progress && !completed,
                });
            }
        }
    }

    out
}

fn extract_champ_select_summary(
    inputs: &MatchInputs<'_>,
    summoner: Option<&SummonerSummary>,
    lobby: Option<&LobbySummary>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Option<ChampSelectSummary> {
    let cs = inputs.champ_select.filter(|v| v.is_object())?;
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
    let internal_now_ms = timer
        .and_then(|t| t.get("internalNowInEpochMs"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let timer_deadline_ms =
        (internal_now_ms > 0 && raw_time > 0).then_some(internal_now_ms.saturating_add(raw_time));
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
    let ParsedChampSelectActions {
        acting_cell_id,
        acting_type,
        cell_acting,
        local_action,
        mut intent_action,
    } = parse_champ_select_actions(cs, local_cell_id, champions, lang);
    let locked_cells = completed_pick_cells(cs);

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

            // The LCU sends `championId: 0` until a pick is hovered; the declared intent fills
            // in, and is never a lock.
            let picked_id = member
                .get("championId")
                .and_then(|v| v.as_i64())
                .filter(|&id| id > 0);
            let raw_champ_id = picked_id
                .or_else(|| member.get("championPickIntent").and_then(|v| v.as_i64()))
                .unwrap_or(0);
            let (champ_id, champion_name) = resolve_champion_pair(
                raw_champ_id,
                member.get("championName").and_then(|v| v.as_str()),
                champions,
                lang,
                true,
            );
            let spell1_id = member.get("spell1Id").and_then(|v| v.as_i64()).unwrap_or(0);
            let spell2_id = member.get("spell2Id").and_then(|v| v.as_i64()).unwrap_or(0);
            let selected_skin_id = member
                .get("selectedSkinId")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let raw_puuid = member.get("puuid").and_then(|v| v.as_str()).unwrap_or("");
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
                            summoner
                                .map(SummonerSummary::riot_id)
                                .filter(|s| !s.is_empty())
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
                pick_locked: champ_id > 0
                    && picked_id.is_some()
                    && locked_cells
                        .as_ref()
                        .is_none_or(|cells| cells.contains(&cell_id)),
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

    let self_ally = my_team.iter().find(|m| m.is_self);
    let assigned_position = self_ally
        .map(|m| m.assigned_position.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("FILL");
    let local_champ_id = self_ally.map(|m| m.champion_id).unwrap_or(0);
    let selected_skin_id = self_ally.map(|m| m.selected_skin_id).unwrap_or(0);
    let my_pos_label = Position::parse(assigned_position)
        .unwrap_or(Position::Fill)
        .display_label(lang);

    let mut incoming_swaps = Vec::new();
    for ally in &my_team {
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
                        format!("wants to swap for your {my_pos_label} (currently {their_pos})")
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
                let champ_label = if ally.has_champion() {
                    ally.champion_name.as_str()
                } else {
                    lang.pick("Ally", "Allié")
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
    }

    let pickable_ids = parse_id_list(inputs.pickable_champion_ids);
    let bannable_ids = parse_id_list(inputs.bannable_champion_ids);
    let mut catalog = champions.champion_catalog(lang);
    for c in &mut catalog {
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

    if let Some(ref mut ia) = intent_action {
        if ia.champion_id <= 0 && local_champ_id > 0 {
            ia.champion_id = local_champ_id;
            ia.champion_name = champions.resolve_champion_name_localized(local_champ_id, lang);
        }
    }

    let champ_for_skins = if local_champ_id > 0 {
        local_champ_id
    } else {
        local_action
            .as_ref()
            .filter(|a| a.action_type == "pick")
            .map(|a| a.champion_id)
            .filter(|&id| id > 0)
            .or_else(|| intent_action.as_ref().map(|a| a.champion_id))
            .unwrap_or(0)
    };
    let available_skins = extract_available_skins(
        inputs.skin_carousel,
        champ_for_skins,
        selected_skin_id,
        champions,
        lang,
    );
    let selected_skin_name = resolve_selected_skin_name(&available_skins, selected_skin_id, lang);
    let ward_skin_id = cs
        .get("myTeam")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("cellId").and_then(|v| v.as_i64()) == Some(local_cell_id))
        })
        .and_then(|m| m.get("wardSkinId"))
        .or_else(|| cs.get("mySelection").and_then(|m| m.get("wardSkinId")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let available_wards = extract_available_wards(inputs.ward_skins, ward_skin_id, lang);

    let (rune_pages, editable_rune_page_id) = extract_rune_pages(inputs.rune_pages, lang);
    let mut recommended_rune_pages =
        recommended_pages_for_localized(champ_for_skins, assigned_position, lang);
    if let Some(active) = rune_pages.iter().find(|p| p.current) {
        for preset in &mut recommended_rune_pages {
            if preset.primary_style_id == active.primary_style_id
                && preset.sub_style_id == active.sub_style_id
                && preset.selected_perk_ids == active.selected_perk_ids
            {
                preset.current = true;
            }
        }
    }
    let recommended_cards =
        fallback_recommended_cards(&recommended_rune_pages, assigned_position, &rune_pages);

    let bench_enabled = cs
        .get("benchEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let rerolls_remaining = cs
        .get("rerollsRemaining")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let bench_champions: Vec<ChampSelectBenchChampion> = if bench_enabled {
        cs.get("benchChampions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let cid = entry
                            .get("championId")
                            .and_then(|v| v.as_i64())
                            .or_else(|| entry.as_i64())
                            .filter(|&id| id > 0)?;
                        Some(ChampSelectBenchChampion {
                            champion_id: cid,
                            champion_name: champions.resolve_champion_name_localized(cid, lang),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    Some(ChampSelectSummary {
        their_team,
        acting_cell_id,
        acting_type,
        local_cell_id,
        local_action,
        intent_action,
        incoming_swaps,
        champions: catalog,
        comfort_champion_ids: Vec::new(),
        available_skins,
        selected_skin_name,
        ward_skin_id,
        available_wards,
        rune_pages,
        recommended_rune_pages,
        recommended_cards,
        editable_rune_page_id,
        bench_enabled,
        bench_champions,
        rerolls_remaining,
        helper: ChampSelectHelper::default(),
        sub_phase,
        time_left_sec,
        total_time_sec,
        timer_deadline_ms,
        queue_type,
        is_ranked_solo_anonymized: any_anonymized,
        my_team,
        my_team_bans,
        their_team_bans,
        my_team_ban_ids,
        their_team_ban_ids,
    })
}

fn parse_id_list(val: Option<&Value>) -> Option<Vec<i64>> {
    val.and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect())
}

fn resolve_selected_skin_name(
    skins: &[RemoteSkinOption],
    selected_skin_id: i64,
    lang: Lang,
) -> String {
    for sk in skins {
        for ch in &sk.chromas {
            if ch.selected || ch.id == selected_skin_id {
                return ch.name.clone();
            }
        }
        if sk.selected || sk.id == selected_skin_id {
            return sk.name.clone();
        }
    }
    if selected_skin_id > 0 {
        if let Some(n) = game_name(GameTextKind::Skin, selected_skin_id, lang) {
            return n.to_string();
        }
    }
    skins.first().map(|s| s.name.clone()).unwrap_or_default()
}

fn extract_available_skins(
    skins_val: Option<&Value>,
    champ_for_skins: i64,
    selected_skin_id: i64,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Vec<RemoteSkinOption> {
    if let Some(arr) = skins_val.and_then(|v| v.as_array()) {
        let is_owned = |v: &Value| {
            v.get("unlocked")
                .or_else(|| v.get("ownership").and_then(|o| o.get("owned")))
                .and_then(|u| u.as_bool())
                .unwrap_or(true)
        };
        return arr
            .iter()
            .filter(|s| is_owned(s))
            .filter_map(|s| {
                let id = s.get("id")?.as_i64()?;
                let name = game_name_or(GameTextKind::Skin, id, lang, s.get("name")?.as_str()?);
                let tile_path = s
                    .get("tilePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let chromas: Vec<RemoteChromaOption> = s
                    .get("childSkins")
                    .and_then(|v| v.as_array())
                    .map(|children| {
                        children
                            .iter()
                            .filter(|c| is_owned(c))
                            .filter_map(|c| {
                                let cid = c.get("id")?.as_i64()?;
                                if cid == id {
                                    return None;
                                }
                                let raw_name =
                                    c.get("name").and_then(|v| v.as_str()).unwrap_or(&name);
                                let cname = game_name_or(GameTextKind::Skin, cid, lang, raw_name);
                                let colors: Vec<String> = c
                                    .get("colors")
                                    .and_then(|v| v.as_array())
                                    .map(|cols| {
                                        cols.iter()
                                            .filter_map(|col| col.as_str().map(str::to_string))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                Some(RemoteChromaOption {
                                    id: cid,
                                    name: cname,
                                    colors,
                                    selected: cid == selected_skin_id,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let selected = id == selected_skin_id || chromas.iter().any(|c| c.selected);
                Some(RemoteSkinOption {
                    id,
                    name,
                    selected,
                    tile_path,
                    chromas,
                })
            })
            .collect();
    }

    let cname = champions.resolve_champion_name_localized(champ_for_skins, lang);
    builtin_skins_for_localized(champ_for_skins, &cname, selected_skin_id, lang)
}

fn extract_available_wards(
    wards_val: Option<&Value>,
    selected_ward_id: i64,
    lang: Lang,
) -> Vec<RemoteWardSkinOption> {
    if let Some(arr) = wards_val.and_then(|v| v.as_array()) {
        let cur = if selected_ward_id == 0 {
            -1
        } else {
            selected_ward_id
        };
        let mut wards: Vec<RemoteWardSkinOption> = arr
            .iter()
            .filter(|w| {
                w.get("ownership")
                    .and_then(|o| o.get("owned"))
                    .or_else(|| w.get("unlocked"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            })
            .filter_map(|w| {
                let id = w.get("id")?.as_i64()?;
                if id == 0 {
                    return None;
                }
                let name = w
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(lang.pick("Sight Ward", "Balise de vision"))
                    .to_string();
                Some(RemoteWardSkinOption {
                    id,
                    name,
                    selected: id == cur,
                })
            })
            .collect();
        if !wards.is_empty() {
            if !wards.iter().any(|w| w.id == -1) {
                wards.insert(
                    0,
                    RemoteWardSkinOption {
                        id: -1,
                        name: lang
                            .pick("Default Sight Ward", "Balise de vision par défaut")
                            .to_string(),
                        selected: cur == -1,
                    },
                );
            }
            return wards;
        }
    }
    builtin_ward_skins_localized(selected_ward_id, lang)
}

fn default_spells_for_position(assigned_position: &str) -> (i64, i64) {
    match Position::parse(assigned_position) {
        Some(Position::Jungle) => (4, 11),
        Some(Position::Top) => (4, 12),
        Some(Position::Bot) => (4, 7),
        Some(Position::Support) => (4, 3),
        _ => (4, 14),
    }
}

fn fallback_recommended_cards(
    pages: &[RemoteRunePage],
    assigned_position: &str,
    active_pages: &[RemoteRunePage],
) -> Vec<RemoteRecommendedCard> {
    let (s1, s2) = default_spells_for_position(assigned_position);
    let active = active_pages.iter().find(|p| p.current);
    pages
        .iter()
        .take(3)
        .map(|pg| {
            let keystone_id = pg.selected_perk_ids.first().copied().unwrap_or(0);
            let current = pg.current
                || active.is_some_and(|a| {
                    a.primary_style_id == pg.primary_style_id
                        && a.sub_style_id == pg.sub_style_id
                        && a.selected_perk_ids == pg.selected_perk_ids
                });
            RemoteRecommendedCard {
                name: pg.name.clone(),
                primary_style_id: pg.primary_style_id,
                primary_style_name: pg.primary_style_name.clone(),
                sub_style_id: pg.sub_style_id,
                sub_style_name: pg.sub_style_name.clone(),
                keystone_id,
                keystone_name: pg.keystone_name.clone(),
                selected_perk_ids: pg.selected_perk_ids.clone(),
                spell1_id: s1,
                spell2_id: s2,
                current,
            }
        })
        .collect()
}

fn populate_champ_select_stats_enrichment(
    cs: &mut ChampSelectSummary,
    stats: Option<&dyn HelperStats>,
    lang: Lang,
) {
    let Some(stats) = stats else {
        return;
    };
    let self_ally = cs.my_team.iter().find(|m| m.is_self);
    let assigned_pos_str = self_ally
        .map(|m| m.assigned_position.as_str())
        .unwrap_or("FILL");
    let pos = Position::parse(assigned_pos_str).filter(|&p| p != Position::Fill);

    if let Some(profile) = stats.player_profile().ready() {
        let mut comfort = Vec::new();
        if let Some(role) = pos {
            let mut by_role: HashMap<i64, u32> = HashMap::new();
            for g in &profile.games {
                if g.position == Some(role) && g.champion_id > 0 {
                    *by_role.entry(g.champion_id).or_insert(0) += 1;
                }
            }
            let mut role_pairs: Vec<(i64, u32)> = by_role.into_iter().collect();
            role_pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            for (cid, _) in role_pairs.into_iter().take(6) {
                comfort.push(cid);
            }
        }
        for c in &profile.champions {
            if comfort.len() >= 8 {
                break;
            }
            if c.games > 0 && c.champion_id > 0 && !comfort.contains(&c.champion_id) {
                comfort.push(c.champion_id);
            }
        }
        cs.comfort_champion_ids = comfort;
    }

    let champ_id = self_ally
        .map(|m| m.champion_id)
        .filter(|&id| id > 0)
        .or_else(|| {
            cs.local_action
                .as_ref()
                .filter(|a| a.action_type == "pick")
                .map(|a| a.champion_id)
                .filter(|&id| id > 0)
        })
        .or_else(|| cs.intent_action.as_ref().map(|a| a.champion_id))
        .unwrap_or(0);
    if champ_id <= 0 {
        return;
    }

    let query = StatsQuery {
        champion_id: champ_id,
        position: pos,
        ..StatsQuery::default()
    };
    if let Some(rec) = stats.recommended_build(&query).ready() {
        let (def_s1, def_s2) = default_spells_for_position(assigned_pos_str);
        let active = cs.rune_pages.iter().find(|p| p.current);
        let cards: Vec<RemoteRecommendedCard> = rec
            .runes
            .iter()
            .take(3)
            .enumerate()
            .filter_map(|(idx, (setup, _))| {
                let mut perks = setup.perk_ids.clone();
                if perks.len() < 6 {
                    return None;
                }
                perks.truncate(6);
                let mut shards = setup.shard_ids.clone();
                for &fallback_shard in &[5008i64, 5008, 5001] {
                    if shards.len() >= 3 {
                        break;
                    }
                    shards.push(fallback_shard);
                }
                perks.extend(shards.into_iter().take(3));
                let keystone_id = perks.first().copied().unwrap_or(0);
                let keystone_name = keystone_name_from_id_localized(keystone_id, lang).to_string();
                let p_name = rune_style_name_localized(setup.primary_style_id, lang).to_string();
                let s_name = rune_style_name_localized(setup.sub_style_id, lang).to_string();
                let (s1, s2) = rec
                    .spells
                    .get(idx)
                    .or_else(|| rec.spells.first())
                    .map(|(sp, _)| (sp.first, sp.second))
                    .unwrap_or((def_s1, def_s2));
                let current = active.is_some_and(|a| {
                    a.primary_style_id == setup.primary_style_id
                        && a.sub_style_id == setup.sub_style_id
                        && a.selected_perk_ids == perks
                });
                Some(RemoteRecommendedCard {
                    name: format!("{keystone_name} · {s_name}"),
                    primary_style_id: setup.primary_style_id,
                    primary_style_name: p_name,
                    sub_style_id: setup.sub_style_id,
                    sub_style_name: s_name,
                    keystone_id,
                    keystone_name,
                    selected_perk_ids: perks,
                    spell1_id: s1,
                    spell2_id: s2,
                    current,
                })
            })
            .collect();
        if !cards.is_empty() {
            cs.recommended_cards = cards;
        }
    }
}

fn extract_rune_pages(pages_val: Option<&Value>, lang: Lang) -> (Vec<RemoteRunePage>, Option<i64>) {
    let Some(arr) = pages_val.and_then(|v| v.as_array()) else {
        return (Vec::new(), None);
    };
    let mut editable_id = None;
    let pages = arr
        .iter()
        .filter_map(|p| {
            let id = p.get("id")?.as_i64()?;
            let name = p.get("name")?.as_str()?.to_string();
            let primary_style_id = p
                .get("primaryStyleId")
                .and_then(|v| v.as_i64())
                .unwrap_or(8000);
            let sub_style_id = p.get("subStyleId").and_then(|v| v.as_i64()).unwrap_or(8400);
            let selected_perk_ids: Vec<i64> = p
                .get("selectedPerkIds")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
                .unwrap_or_default();
            let current = p.get("current").and_then(|v| v.as_bool()).unwrap_or(false);
            let explicit_editable = p.get("isEditable").and_then(|v| v.as_bool());
            if explicit_editable == Some(true) && editable_id.is_none() {
                editable_id = Some(id);
            }
            let is_preset = !explicit_editable.unwrap_or(true);
            let keystone_name = selected_perk_ids
                .first()
                .copied()
                .map(|kid| keystone_name_from_id_localized(kid, lang))
                .unwrap_or(lang.pick("Keystone", "Rune fondamentale"))
                .to_string();
            Some(RemoteRunePage {
                id,
                name,
                primary_style_id,
                primary_style_name: rune_style_name_localized(primary_style_id, lang).to_string(),
                sub_style_id,
                sub_style_name: rune_style_name_localized(sub_style_id, lang).to_string(),
                keystone_name,
                selected_perk_ids,
                current,
                is_preset,
            })
        })
        .collect();
    (pages, editable_id)
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
            let p_summoner_name = p.get("summonerName").and_then(|v| v.as_str()).unwrap_or("");
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
            let respawn_timer_sec = p
                .get("respawnTimer")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let skin_id = p.get("skinID").and_then(|v| v.as_i64()).unwrap_or(0);
            let champion_id = p
                .get("championId")
                .and_then(|v| v.as_i64())
                .filter(|&id| id > 0)
                .unwrap_or_else(|| {
                    champions.resolve_champion_id_with_alias(&champ, &champion_alias)
                });
            let cs_per_min = if game_time_sec >= 60.0 {
                p_cs as f32 / (game_time_sec as f32 / 60.0)
            } else {
                0.0
            };
            let summary = LiveGamePlayerSummary {
                is_dead,
                respawn_timer_sec,
                skin_id,
                cs_per_min,
                cs_diff_vs_lane: None,
                riot_id: p_riot_id.to_string(),
                champion_id,
                // The Live Client names champions in the client's language; Hypatia names them
                // by id in its own.
                champion_name: Some(champion_id)
                    .filter(|&id| id > 0)
                    .map(|id| champions.resolve_champion_name_localized(id, lang))
                    .filter(|name| !name.starts_with("Champion #"))
                    .unwrap_or(champ),
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
        .or_else(|| {
            active
                .and_then(|a| a.get("championName"))
                .and_then(|v| v.as_str())
        })
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
        .or_else(|| {
            allgame_val
                .and_then(|ag| ag.get("events"))
                .and_then(|e| e.get("Events"))
        })
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

    if game_mode.eq_ignore_ascii_case("CLASSIC") && game_time_sec >= 90.0 {
        let mut by_team_pos: HashMap<(bool, Position), u32> = HashMap::new();
        for p in &players {
            if let Some(pos) = Position::parse(&p.position).filter(|pos| *pos != Position::Fill) {
                by_team_pos
                    .entry((p.team == "ORDER", pos))
                    .or_insert(p.creep_score);
            }
        }
        for p in &mut players {
            if let Some(pos) = Position::parse(&p.position).filter(|pos| *pos != Position::Fill) {
                if let Some(&opp_cs) = by_team_pos.get(&(p.team != "ORDER", pos)) {
                    p.cs_diff_vs_lane = Some(p.creep_score as i32 - opp_cs as i32);
                }
            }
        }
    }

    let self_cs_per_min = if game_time_sec >= 60.0 {
        creep_score as f32 / (game_time_sec as f32 / 60.0)
    } else {
        0.0
    };
    let lane_cs_diff = players
        .iter()
        .find(|p| p.is_self)
        .and_then(|p| p.cs_diff_vs_lane);

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
        self_cs_per_min,
        lane_cs_diff,
        personal_avg_cs_per_min: None,
        recent_events,
        players,
    })
}

/// A post-game stat (`CHAMPIONS_KILLED`, `GOLD_EARNED`, ...) or 0.
fn eog_stat(stats: Option<&Value>, key: &str) -> u32 {
    stats
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}

/// Full skin id from a client splash path such as `.../champion-splashes/103/103027.jpg`.
fn skin_id_from_splash_path(path: &str) -> i64 {
    path.rsplit('/')
        .next()
        .and_then(|file| file.split('.').next())
        .and_then(|id| id.parse().ok())
        .unwrap_or(0)
}

fn eog_i64(stats: Option<&Value>, key: &str) -> i64 {
    stats
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

fn eog_player(
    p: &Value,
    local_puuid: &str,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> EndOfGamePlayer {
    let (champion_id, champion_name) = resolve_champion_pair(
        p.get("championId").and_then(|v| v.as_i64()).unwrap_or(0),
        p.get("championName").and_then(|v| v.as_str()),
        champions,
        lang,
        false,
    );
    let text = |key: &str| p.get(key).and_then(|v| v.as_str()).unwrap_or("");
    let position = [text("detectedTeamPosition"), text("selectedPosition")]
        .into_iter()
        .find(|pos| !pos.is_empty() && *pos != "NONE")
        .unwrap_or("")
        .to_string();
    let puuid = text("puuid");
    let is_self = p
        .get("isLocalPlayer")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || (!local_puuid.is_empty() && puuid == local_puuid);
    let stats = p.get("stats");
    let mut items: Vec<i64> = p
        .get("items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_i64())
                .filter(|&id| id > 0)
                .take(6)
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        for slot in 0..6 {
            let id = eog_i64(stats, &format!("ITEM{slot}"));
            if id > 0 {
                items.push(id);
            }
        }
    }
    let spell1_id = p
        .get("spell1Id")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| eog_i64(stats, "SPELL1_ID"));
    let spell2_id = p
        .get("spell2Id")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| eog_i64(stats, "SPELL2_ID"));
    EndOfGamePlayer {
        champion_id,
        champion_name,
        skin_id: skin_id_from_splash_path(text("skinSplashPath")),
        position,
        level: eog_stat(stats, "LEVEL"),
        kills: eog_stat(stats, "CHAMPIONS_KILLED"),
        deaths: eog_stat(stats, "NUM_DEATHS"),
        assists: eog_stat(stats, "ASSISTS"),
        creep_score: eog_stat(stats, "MINIONS_KILLED") + eog_stat(stats, "NEUTRAL_MINIONS_KILLED"),
        items,
        keystone_id: eog_i64(stats, "PERK0"),
        sub_style_id: eog_i64(stats, "PERK_SUB_STYLE"),
        spell1_id,
        spell2_id,
        damage_to_champions: eog_stat(stats, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS"),
        damage_taken: eog_stat(stats, "TOTAL_DAMAGE_TAKEN"),
        gold_earned: eog_stat(stats, "GOLD_EARNED"),
        vision_score: eog_stat(stats, "VISION_SCORE"),
        kill_participation: 0.0,
        damage_share: 0.0,
        performance_score: 0,
        placement: 0,
        is_mvp: false,
        is_ace: false,
        is_self,
    }
}

fn eog_performance_score(p: &EndOfGamePlayer, minutes: f32, won: bool) -> u8 {
    let kda = (p.kills + p.assists) as f32 / p.deaths.max(1) as f32;
    let cs_min = p.creep_score as f32 / minutes;
    let vs_min = p.vision_score as f32 / minutes;
    let mut score = 38.0;
    score += (kda.min(6.0) / 6.0) * 20.0;
    score += p.kill_participation.clamp(0.0, 1.0) * 16.0;
    score += (p.damage_share.clamp(0.0, 0.45) / 0.45) * 12.0;
    score += (cs_min.min(9.0) / 9.0) * 8.0;
    score += (vs_min.min(2.0) / 2.0) * 4.0;
    if won {
        score += 4.0;
    }
    score.round().clamp(1.0, 100.0) as u8
}

fn extract_eog_summary(
    eog_val: Option<&Value>,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> Option<EndOfGameSummary> {
    let eog = eog_val.filter(|v| v.is_object())?;
    let mut result = eog
        .get("gameResult")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let game_length_sec = eog.get("gameLength").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let minutes = (game_length_sec as f32 / 60.0).max(1.0);
    let local = eog.get("localPlayer");
    let raw_champ_id = local
        .and_then(|l| l.get("championId"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let (champion_id, champion_name) = resolve_champion_pair(
        raw_champ_id,
        local
            .and_then(|l| l.get("championName"))
            .and_then(|v| v.as_str()),
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

    let local_puuid = local
        .and_then(|l| l.get("puuid"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut teams: Vec<EndOfGameTeam> = eog
        .get("teams")
        .and_then(|v| v.as_array())
        .map(|teams| {
            teams
                .iter()
                .map(|t| {
                    let mut players: Vec<EndOfGamePlayer> = t
                        .get("players")
                        .and_then(|v| v.as_array())
                        .map(|ps| {
                            ps.iter()
                                .map(|p| eog_player(p, local_puuid, champions, lang))
                                .collect()
                        })
                        .unwrap_or_default();
                    let is_player_team = t
                        .get("isPlayerTeam")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        || players.iter().any(|p| p.is_self);
                    let won = t
                        .get("isWinningTeam")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let total_kills: u32 = players.iter().map(|p| p.kills).sum();
                    let total_gold: u32 = players.iter().map(|p| p.gold_earned).sum();
                    let total_damage: u32 = players.iter().map(|p| p.damage_to_champions).sum();
                    let total_vision: u32 = players.iter().map(|p| p.vision_score).sum();
                    for p in &mut players {
                        if total_kills > 0 {
                            p.kill_participation =
                                ((p.kills + p.assists) as f32 / total_kills as f32).min(1.0);
                        }
                        if total_damage > 0 {
                            p.damage_share = p.damage_to_champions as f32 / total_damage as f32;
                        }
                        p.performance_score = eog_performance_score(p, minutes, won);
                    }
                    // Assign MVP (best score on winning team) or ACE (best score on losing team).
                    if let Some((best_idx, _)) = players.iter().enumerate().max_by_key(|(_, p)| {
                        (
                            p.performance_score,
                            p.kills + p.assists,
                            p.damage_to_champions,
                        )
                    }) {
                        if won {
                            players[best_idx].is_mvp = true;
                        } else {
                            players[best_idx].is_ace = true;
                        }
                    }
                    EndOfGameTeam {
                        is_player_team,
                        won,
                        total_kills,
                        total_gold,
                        total_damage,
                        total_vision,
                        players,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Assign 1..=10 overall placement by performance_score descending.
    let mut ranked: Vec<(usize, usize, u8, u32)> = Vec::new();
    for (ti, team) in teams.iter().enumerate() {
        for (pi, p) in team.players.iter().enumerate() {
            ranked.push((ti, pi, p.performance_score, p.damage_to_champions));
        }
    }
    ranked.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.3.cmp(&a.3)));
    for (rank_idx, (ti, pi, _, _)) in ranked.into_iter().enumerate() {
        teams[ti].players[pi].placement = (rank_idx + 1) as u8;
    }

    teams.sort_by_key(|t| !t.is_player_team);
    if result.is_empty() {
        if let Some(mine) = teams.iter().find(|t| t.is_player_team) {
            result = if mine.won { "Victory" } else { "Defeat" }.to_string();
        }
    }

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
        skin_id: skin_id_from_splash_path(
            local
                .and_then(|l| l.get("skinSplashPath"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        ),
        creep_score: eog_stat(stats, "MINIONS_KILLED") + eog_stat(stats, "NEUTRAL_MINIONS_KILLED"),
        vision_score: eog_stat(stats, "VISION_SCORE"),
        teams,
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
        let summoner =
            json!({"gameName": "Ezo", "tagLine": "EUW", "puuid": "p-self", "profileIconId": 548});
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
        assert_eq!(
            p.my_team[0].position_swap_state.as_deref(),
            Some("RECEIVED")
        );

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
        // Only hovered so far: the session has pick actions and none is completed.
        assert!(!p.my_team[0].pick_locked);
        assert!(!p.my_team[1].pick_locked);
        assert!(!p.my_team[2].pick_locked);

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
                    "position": "BOTTOM",
                    "skinID": 14
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
        assert_eq!(live.players[0].skin_id, 14);
        assert_eq!(live.players[1].skin_id, 0);
        assert!(snap.board.is_some());

        let eog = snap.end_of_game.unwrap();
        assert_eq!(eog.result, "");
        assert_eq!(eog.game_length_sec, 0);
        assert_eq!(eog.champion_id, 0);
        assert_eq!(eog.champion_name, "");
    }

    #[test]
    fn end_of_game_reads_both_teams_and_the_players_skin() {
        let eog = json!({
            "gameLength": 1500,
            "localPlayer": {
                "championId": 103,
                "puuid": "me",
                "skinSplashPath": "/lol-game-data/assets/v1/champion-splashes/103/103027.jpg",
                "stats": {"CHAMPIONS_KILLED": 5, "MINIONS_KILLED": 180, "NEUTRAL_MINIONS_KILLED": 12, "VISION_SCORE": 31}
            },
            "teams": [
                {"isPlayerTeam": false, "isWinningTeam": false, "players": [
                    {"championId": 122, "detectedTeamPosition": "TOP", "stats": {"LEVEL": 15}}
                ]},
                {"isWinningTeam": true, "players": [
                    {"championId": 103, "puuid": "me", "selectedPosition": "MIDDLE",
                     "skinSplashPath": "/lol-game-data/assets/v1/champion-splashes/103/103027.jpg",
                     "stats": {"CHAMPIONS_KILLED": 5, "NUM_DEATHS": 1, "ASSISTS": 7, "MINIONS_KILLED": 180, "NEUTRAL_MINIONS_KILLED": 12, "LEVEL": 16}},
                    {"championId": 86, "detectedTeamPosition": "NONE", "selectedPosition": "TOP"}
                ]}
            ]
        });
        let summary = extract_eog_summary(Some(&eog), &BuiltinChampionCatalog, Lang::En).unwrap();

        // No gameResult in the block: the player's team decides it.
        assert_eq!(summary.result, "Victory");
        assert_eq!(summary.skin_id, 103027);
        assert_eq!(summary.creep_score, 192);
        assert_eq!(summary.vision_score, 31);
        assert_eq!(summary.teams.len(), 2);
        let mine = &summary.teams[0];
        assert!(mine.is_player_team && mine.won);
        assert!(mine.players[0].is_self);
        assert_eq!(mine.players[0].position, "MIDDLE");
        assert_eq!(mine.players[0].skin_id, 103027);
        assert_eq!(
            (
                mine.players[0].kills,
                mine.players[0].deaths,
                mine.players[0].assists
            ),
            (5, 1, 7)
        );
        assert_eq!(mine.players[0].creep_score, 192);
        assert_eq!(mine.players[1].position, "TOP");
        assert_eq!(mine.players[1].skin_id, 0);
        assert!(!summary.teams[1].is_player_team);
        assert_eq!(summary.teams[1].players[0].level, 15);
    }

    #[test]
    fn champ_select_marks_locked_picks() {
        let cs = json!({
            "localPlayerCellId": 0,
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 20000},
            "actions": [[
                {"id": 1, "actorCellId": 0, "type": "pick", "championId": 266, "completed": true, "isInProgress": false},
                {"id": 2, "actorCellId": 1, "type": "pick", "championId": 64, "completed": false, "isInProgress": true}
            ]],
            "myTeam": [
                {"cellId": 0, "assignedPosition": "TOP", "championId": 266, "summonerId": 1, "puuid": "a"},
                {"cellId": 1, "assignedPosition": "JUNGLE", "championId": 64, "summonerId": 2, "puuid": "b"}
            ]
        });
        let snap = project_match(
            &MatchInputs {
                champ_select: Some(&cs),
                ..Default::default()
            },
            &CustomSyncedLookup,
            Lang::En,
        );
        let team = snap.champ_select.unwrap().my_team;
        assert!(team[0].pick_locked);
        assert!(!team[1].pick_locked);

        // Without any pick action the lock state is unknown: a chosen champion counts as locked.
        let bare = json!({
            "localPlayerCellId": 0,
            "myTeam": [{"cellId": 0, "assignedPosition": "TOP", "championId": 266, "summonerId": 1, "puuid": "a"}]
        });
        let snap = project_match(
            &MatchInputs {
                champ_select: Some(&bare),
                ..Default::default()
            },
            &CustomSyncedLookup,
            Lang::En,
        );
        assert!(snap.champ_select.unwrap().my_team[0].pick_locked);
    }

    #[test]
    fn projects_champ_select_remote_controls_swaps_skins_and_runes() {
        let cs = json!({
            "localPlayerCellId": 1,
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 19_500, "totalTimeInPhase": 30_000},
            "actions": [[
                {"id": 10, "actorCellId": 1, "type": "pick", "championId": 103, "completed": false, "isInProgress": true}
            ]],
            "positionSwaps": [{"id": 5, "cellId": 0, "state": "RECEIVED"}],
            "myTeam": [
                {"cellId": 0, "assignedPosition": "TOP", "championId": 266, "summonerId": 0, "puuid": ""},
                {"cellId": 1, "assignedPosition": "MIDDLE", "championId": 103, "selectedSkinId": 103027, "summonerId": 1, "puuid": "p-self"}
            ]
        });
        let pickable = json!([103, 157]);
        let bannable = json!([238]);
        let skins = json!([
            {"id": 103000, "name": "Classic Ahri", "unlocked": true, "tilePath": "/lol-game-data/assets/v1/champion-tiles/103/103000.jpg"},
            {"id": 103027, "name": "Spirit Blossom Ahri", "unlocked": true, "tilePath": "/lol-game-data/assets/v1/champion-tiles/103/103027.jpg"},
            {"id": 103099, "name": "Locked Ahri", "unlocked": false}
        ]);
        let perks = json!([
            {"id": 10, "name": "Preset", "primaryStyleId": 8000, "subStyleId": 8400, "selectedPerkIds": [8010], "current": false},
            {"id": 42, "name": "Ahri Burst", "primaryStyleId": 8100, "subStyleId": 8200, "selectedPerkIds": [8112], "current": true, "isEditable": true}
        ]);

        let snap = project_match(
            &MatchInputs {
                champ_select: Some(&cs),
                pickable_champion_ids: Some(&pickable),
                bannable_champion_ids: Some(&bannable),
                skin_carousel: Some(&skins),
                rune_pages: Some(&perks),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        assert_eq!(snap.editable_rune_page_id, Some(42));
        let p = snap.champ_select.unwrap();
        assert_eq!(p.incoming_swaps.len(), 1);
        assert_eq!(p.incoming_swaps[0].kind, "position");
        assert!(p.champions.iter().find(|c| c.id == 103).unwrap().pickable);
        assert!(!p.champions.iter().find(|c| c.id == 266).unwrap().pickable);
        assert!(p.champions.iter().find(|c| c.id == 238).unwrap().bannable);
        assert_eq!(p.available_skins.len(), 2);
        assert!(p.available_skins[1].selected);
        assert_eq!(p.editable_rune_page_id, Some(42));
        assert_eq!(p.rune_pages[1].name, "Ahri Burst");
        assert_eq!(p.rune_pages[1].keystone_name, "Électrocution");
        assert!(!p.recommended_rune_pages.is_empty());
    }

    #[test]
    fn projects_aram_bench_live_cs_diff_and_enriched_end_of_game() {
        let cs = json!({
            "localPlayerCellId": 0,
            "benchEnabled": true,
            "benchChampions": [{"championId": 103}, {"championId": 222}],
            "myTeam": [{"cellId": 0, "championId": 81, "summonerId": 1, "puuid": "p-self"}]
        });
        let snap = project_match(
            &MatchInputs {
                champ_select: Some(&cs),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let p = snap.champ_select.unwrap();
        assert!(p.bench_enabled);
        assert_eq!(p.bench_champions.len(), 2);
        assert_eq!(p.bench_champions[0].champion_id, 103);

        let all_game = json!({
            "activePlayer": {
                "riotId": "Me#EUW",
                "level": 11,
                "currentGold": 1200.0
            },
            "allPlayers": [
                {"riotId": "Me#EUW", "championName": "Ahri", "team": "ORDER", "position": "MIDDLE", "level": 11, "scores": {"kills": 4, "deaths": 1, "assists": 3, "creepScore": 150}},
                {"riotId": "Enemy#EUW", "championName": "Syndra", "team": "CHAOS", "position": "MIDDLE", "level": 10, "scores": {"kills": 1, "deaths": 4, "assists": 1, "creepScore": 120}}
            ],
            "gameData": {"gameMode": "CLASSIC", "mapName": "Map11", "gameTime": 1200.0}
        });
        let live_snap = project_match(
            &MatchInputs {
                live_allgame: Some(&all_game),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let lg = live_snap.live_game.unwrap();
        assert!((lg.self_cs_per_min - 7.5).abs() < 1e-3);
        assert_eq!(lg.lane_cs_diff, Some(30));
        assert_eq!(lg.players[0].cs_diff_vs_lane, Some(30));
        assert_eq!(lg.players[1].cs_diff_vs_lane, Some(-30));

        let eog = json!({
            "gameResult": "Win",
            "gameLength": 1800,
            "localPlayer": {
                "puuid": "p-self",
                "championId": 103,
                "stats": {"CHAMPIONS_KILLED": 10, "NUM_DEATHS": 2, "ASSISTS": 8, "GOLD_EARNED": 14000, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 28000}
            },
            "teams": [
                {
                    "isPlayerTeam": true,
                    "isWinningTeam": true,
                    "players": [
                        {
                            "puuid": "p-self",
                            "championId": 103,
                            "selectedPosition": "MIDDLE",
                            "spell1Id": 4,
                            "spell2Id": 14,
                            "items": [3118, 4645, 3089],
                            "stats": {"LEVEL": 16, "CHAMPIONS_KILLED": 10, "NUM_DEATHS": 2, "ASSISTS": 8, "MINIONS_KILLED": 200, "NEUTRAL_MINIONS_KILLED": 10, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 28000, "TOTAL_DAMAGE_TAKEN": 12000, "GOLD_EARNED": 14000, "VISION_SCORE": 24, "PERK0": 8112, "PERK_SUB_STYLE": 8200}
                        },
                        {
                            "puuid": "p-ally",
                            "championId": 222,
                            "selectedPosition": "BOTTOM",
                            "stats": {"LEVEL": 15, "CHAMPIONS_KILLED": 5, "NUM_DEATHS": 4, "ASSISTS": 5, "MINIONS_KILLED": 180, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 12000, "GOLD_EARNED": 11000, "VISION_SCORE": 14}
                        }
                    ]
                },
                {
                    "isPlayerTeam": false,
                    "isWinningTeam": false,
                    "players": [
                        {
                            "puuid": "p-enemy",
                            "championId": 134,
                            "selectedPosition": "MIDDLE",
                            "stats": {"LEVEL": 15, "CHAMPIONS_KILLED": 6, "NUM_DEATHS": 5, "ASSISTS": 2, "MINIONS_KILLED": 190, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 20000, "GOLD_EARNED": 11500, "VISION_SCORE": 18}
                        }
                    ]
                }
            ]
        });
        let eog_snap = project_match(
            &MatchInputs {
                end_of_game: Some(&eog),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let summary = eog_snap.end_of_game.unwrap();
        assert_eq!(summary.teams[0].total_kills, 15);
        assert_eq!(summary.teams[0].total_damage, 40_000);
        let me = &summary.teams[0].players[0];
        assert_eq!(me.items, vec![3118, 4645, 3089]);
        assert_eq!(me.keystone_id, 8112);
        assert!(me.is_mvp);
        assert_eq!(me.placement, 1);
        assert!((me.damage_share - 0.7).abs() < 1e-3);
        let enemy = &summary.teams[1].players[0];
        assert!(enemy.is_ace);
    }

    #[test]
    fn champ_select_timer_deadline_and_empty_lcu_skins_respect_live_payloads() {
        let cs = json!({
            "localPlayerCellId": 0,
            "timer": {
                "phase": "BAN_PICK",
                "adjustedTimeLeftInPhase": 45_000,
                "totalTimeInPhase": 45_000,
                "internalNowInEpochMs": 1_000_000
            },
            "actions": [[
                {"id": 0, "actorCellId": 0, "type": "pick", "championId": 799, "completed": false, "isInProgress": true}
            ]],
            "myTeam": [
                {"cellId": 0, "assignedPosition": "TOP", "championId": 799, "summonerId": 1, "puuid": "p-self"}
            ]
        });
        let empty_skins = json!([]);
        let snap = project_match(
            &MatchInputs {
                champ_select: Some(&cs),
                skin_carousel: Some(&empty_skins),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        let p = snap.champ_select.unwrap();
        assert_eq!(p.time_left_sec, 45);
        // 38 seconds after internalNowInEpochMs, 7 seconds remain.
        assert_eq!(p.time_left_sec_at(1_038_000), 7);
        // Live LCU returning `[]` before lock must not fabricate fake demo skins.
        assert!(p.available_skins.is_empty());

        // Deleted resources (Value::Null) resolve to None rather than ghost summaries.
        let deleted = Value::Null;
        let cleared = project_match(
            &MatchInputs {
                lobby: Some(&deleted),
                champ_select: Some(&deleted),
                end_of_game: Some(&deleted),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        assert!(cleared.lobby.is_none());
        assert!(cleared.champ_select.is_none());
        assert!(cleared.end_of_game.is_none());
    }

    #[test]
    fn projects_swiftplay_lobby_slots_chromas_wards_intent_and_aram_mode_spells() {
        use crate::remote::{project_remote_state, AlertTracker};
        let gp = |p: &str| GamePhase {
            phase: p.to_string(),
            since: 0,
        };

        // 1. Swiftplay lobby (queueId 480) extracts 2 playerSlots with champion, role, skin, spells, and perk page.
        let swift_lobby = json!({
            "gameConfig": { "queueId": 480, "gameMode": "SWIFTPLAY" },
            "localMember": {
                "isLeader": true,
                "playerSlots": [
                    { "championId": 103, "positionPreference": "MIDDLE", "skinId": 103004, "spell1": 4, "spell2": 14, "perkId": 501 },
                    { "championId": 24, "positionPreference": "TOP", "skinId": 24000, "spell1": 4, "spell2": 12, "perkId": 502 }
                ]
            },
            "members": [{ "summonerId": 1 }]
        });
        let swift_snap = project_match(
            &MatchInputs {
                phase: Some(gp("Lobby")),
                lobby: Some(&swift_lobby),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let lob = swift_snap.lobby.as_ref().unwrap();
        assert_eq!(lob.queue_id, 480);
        assert_eq!(lob.swiftplay_slots.len(), 2);
        assert_eq!(lob.swiftplay_slots[0].champion_id, 103);
        assert_eq!(lob.swiftplay_slots[0].champion_name, "Ahri");
        assert_eq!(lob.swiftplay_slots[0].position, "MIDDLE");
        assert_eq!(lob.swiftplay_slots[1].champion_id, 24);
        assert_eq!(lob.swiftplay_slots[1].position, "TOP");

        let mut tracker = AlertTracker::default();
        let remote_swift = project_remote_state(&swift_snap, false, Lang::En, &mut tracker);
        assert!(!remote_swift.available_queues.is_empty());
        let rlob = remote_swift.lobby.unwrap();
        assert!(rlob.is_swiftplay);
        assert_eq!(rlob.swiftplay_slots.len(), 2);
        assert!(!rlob.champions.is_empty());

        // 2. Draft ChampSelect during Ban turn preserves uncompleted local pick as `intent_action`,
        //    marks selected childSkins chroma and parent skin, and extracts owned ward skins.
        let cs_draft = json!({
            "localPlayerCellId": 0,
            "timer": { "phase": "BAN_PICK", "adjustedTimeLeftInPhase": 25_000, "totalTimeInPhase": 30_000 },
            "actions": [
                [{ "id": 1, "actorCellId": 0, "type": "ban", "championId": 238, "completed": false, "isInProgress": true }],
                [{ "id": 20, "actorCellId": 0, "type": "pick", "championId": 103, "completed": false, "isInProgress": false }]
            ],
            "myTeam": [{
                "cellId": 0,
                "assignedPosition": "MIDDLE",
                "championId": 0,
                "championPickIntent": 103,
                "selectedSkinId": 103901,
                "wardSkinId": 12,
                "spell1Id": 4,
                "spell2Id": 14,
                "summonerId": 1,
                "puuid": "p-self"
            }]
        });
        let skin_carousel = json!([{
            "id": 103027,
            "name": "Spirit Blossom Ahri",
            "ownership": { "owned": true },
            "tilePath": "/lol-game-data/assets/v1/champion-tiles/103/103027.jpg",
            "childSkins": [
                {
                    "id": 103901,
                    "name": "Spirit Blossom Ahri (Ruby)",
                    "ownership": { "owned": true },
                    "colors": ["#c0392b", "#f39c12"]
                },
                {
                    "id": 103902,
                    "name": "Locked Chroma",
                    "ownership": { "owned": false },
                    "colors": ["#000000"]
                }
            ]
        }]);
        let ward_skins = json!([
            { "id": 1, "name": "Classic Ward", "ownership": { "owned": true } },
            { "id": 12, "name": "Hextech Ward", "ownership": { "owned": true } },
            { "id": 99, "name": "Locked Ward", "ownership": { "owned": false } }
        ]);
        let draft_snap = project_match(
            &MatchInputs {
                phase: Some(gp("ChampSelect")),
                champ_select: Some(&cs_draft),
                skin_carousel: Some(&skin_carousel),
                ward_skins: Some(&ward_skins),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let cs = draft_snap.champ_select.as_ref().unwrap();
        assert_eq!(
            cs.local_action
                .as_ref()
                .map(|a| (a.action_id, a.action_type.as_str())),
            Some((1, "ban"))
        );
        assert_eq!(
            cs.intent_action
                .as_ref()
                .map(|a| (a.action_id, a.champion_id)),
            Some((20, 103))
        );
        assert_eq!(cs.available_skins.len(), 1);
        assert!(cs.available_skins[0].selected);
        assert_eq!(cs.available_skins[0].chromas.len(), 1);
        assert!(cs.available_skins[0].chromas[0].selected);
        assert!(cs.selected_skin_name.contains("Ruby"));
        assert_eq!(cs.ward_skin_id, 12);
        assert_eq!(cs.available_wards.len(), 3); // Default (-1) + 2 owned wards (99 excluded)
        assert!(cs.available_wards.iter().any(|w| w.id == 12 && w.selected));
        assert!(!cs.available_wards.iter().any(|w| w.id == 99));

        // 3. ARAM (queueId 450) exposes rerolls_remaining, Mark (32), and excludes Smite (11) & Teleport (12);
        //    Arena (queueId 1700) marks spells_editable = false.
        let aram_lobby =
            json!({ "gameConfig": { "queueId": 450 }, "members": [{ "summonerId": 1 }] });
        let aram_cs = json!({
            "localPlayerCellId": 0,
            "queueType": "ARAM",
            "benchEnabled": true,
            "rerollsRemaining": 2,
            "myTeam": [{ "cellId": 0, "championId": 103, "spell1Id": 4, "spell2Id": 32, "summonerId": 1 }]
        });
        let aram_snap = project_match(
            &MatchInputs {
                phase: Some(gp("ChampSelect")),
                lobby: Some(&aram_lobby),
                champ_select: Some(&aram_cs),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        let aram_remote = project_remote_state(&aram_snap, false, Lang::Fr, &mut tracker)
            .champ_select
            .unwrap();
        assert!(aram_remote.is_roleless);
        assert!(aram_remote.spells_editable);
        assert_eq!(aram_remote.rerolls_remaining, 2);
        assert!(aram_remote.available_spells.iter().any(|s| s.id == 32));
        assert!(aram_remote.available_spells.iter().any(|s| s.id == 13));
        assert!(!aram_remote.available_spells.iter().any(|s| s.id == 11));
        assert!(!aram_remote.available_spells.iter().any(|s| s.id == 12));

        let arena_lobby =
            json!({ "gameConfig": { "queueId": 1700 }, "members": [{ "summonerId": 1 }] });
        let arena_cs = json!({
            "localPlayerCellId": 0,
            "queueType": "CHERRY",
            "myTeam": [{ "cellId": 0, "championId": 103, "spell1Id": 4, "spell2Id": 14, "summonerId": 1 }]
        });
        let arena_snap = project_match(
            &MatchInputs {
                phase: Some(gp("ChampSelect")),
                lobby: Some(&arena_lobby),
                champ_select: Some(&arena_cs),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::En,
        );
        let arena_remote = project_remote_state(&arena_snap, false, Lang::En, &mut tracker)
            .champ_select
            .unwrap();
        assert!(arena_remote.is_roleless);
        assert!(!arena_remote.spells_editable);
    }
}
