//! Local, deterministic Game Review (Layer 1 coach engine).
//!
//! Pure logic, zero I/O. Evaluates a finished Summoner's Rift match across five pillars
//! ([`ReviewPillar`]): Objectives, Economy, Build, Discipline, and Draft.
//!
//! Produces up to three [`FindingPolarity::GoodChoice`] and up to three
//! [`FindingPolarity::ToImprove`] findings ranked by impact, plus chronological
//! [`TurningPoint`]s and structured facts/features ready for a future opt-in LLM narrator
//! (Phase 2) or ML scoring calibration.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::champions::Position;
use crate::comp::{champion_traits, team_profile};
use crate::game_text::{game_name, game_name_or, GameTextKind};
use crate::i18n::Lang;
use crate::stats::{
    ChampionBuild, CompTrait, ItemStage, PlayerChampionStats, PlayerGame, StatsQueue, REMAKE_SECS,
};
use crate::summary::EndOfGameSummary;

/// Maximum number of findings shown per polarity (`GoodChoice` / `ToImprove`).
pub const MAX_FINDINGS_PER_SIDE: usize = 3;

/// Maximum findings from the same pillar in one top-3 list so the review stays balanced.
const MAX_PER_PILLAR_IN_TOP: usize = 2;

/// Coaching domain a finding belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReviewPillar {
    Objectives,
    Economy,
    Build,
    Discipline,
    Draft,
}

impl ReviewPillar {
    /// Short localized label shown on badges.
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Objectives => lang.pick("Objectives", "Objectifs"),
            Self::Economy => lang.pick("Economy", "Économie"),
            Self::Build => lang.pick("Build", "Build"),
            Self::Discipline => lang.pick("Discipline", "Discipline"),
            Self::Draft => lang.pick("Draft", "Draft"),
        }
    }
}

/// Whether a finding highlights a strength or an improvement area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FindingPolarity {
    GoodChoice,
    ToImprove,
}

/// How much data backed this review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ReviewCoverage {
    /// Live trace recorded during the match + post-game stats.
    Full,
    /// Post-game or LCU match history only (no live trace timestamps).
    #[default]
    PostGameOnly,
    /// Synthetic demo fixture (`--demo`).
    Demo,
}

impl ReviewCoverage {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Full => lang.pick("Full trace", "Trace complète"),
            Self::PostGameOnly => lang.pick("Post-game only", "Après-partie"),
            Self::Demo => lang.pick("Demo", "Démo"),
        }
    }
}

/// Periodic snapshot sampled from `/liveclientdata/allgamedata` (~every 15 s).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TraceSample {
    pub time_sec: u32,
    pub level: u32,
    pub cs: u32,
    pub lane_cs_diff: Option<i32>,
    pub current_gold: i64,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
}

/// A player death captured during the live trace or timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathEvent {
    pub time_sec: u32,
    /// Gold sitting unspent in the player's pocket right before dying (`None` if unknown).
    pub unspent_gold: Option<i64>,
}

/// Epic objective taken during the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EpicKind {
    Dragon,
    Herald,
    Grubs,
    Baron,
    Elder,
}

impl EpicKind {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Dragon => lang.pick("Dragon", "Dragon"),
            Self::Herald => lang.pick("Rift Herald", "Héraut de la Faille"),
            Self::Grubs => lang.pick("Voidgrubs", "Larves du Néant"),
            Self::Baron => lang.pick("Baron Nashor", "Baron Nashor"),
            Self::Elder => lang.pick("Elder Dragon", "Dragon Ancestral"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveEvent {
    pub time_sec: u32,
    pub kind: EpicKind,
    pub by_ally: bool,
    pub player_involved: bool,
}

/// Completed item purchase observed during the match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemBuyEvent {
    pub time_sec: u32,
    pub item_id: i64,
}

/// Lightweight, identity-free trace recorded while playing a Summoner's Rift match.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MatchTrace {
    pub game_id: Option<i64>,
    pub champion_id: i64,
    pub position: Option<Position>,
    pub samples: Vec<TraceSample>,
    pub deaths: Vec<DeathEvent>,
    pub objectives: Vec<ObjectiveEvent>,
    pub item_buys: Vec<ItemBuyEvent>,
}

impl MatchTrace {
    /// True when the trace carries at least one live sample or timestamped event.
    pub fn has_timeline_data(&self) -> bool {
        !self.samples.is_empty() || !self.deaths.is_empty() || !self.objectives.is_empty()
    }
}

/// One factual coaching finding, localized and carrying raw facts for future LLM/ML layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub id: String,
    pub pillar: ReviewPillar,
    pub polarity: FindingPolarity,
    /// Relative importance `0..=100` used to rank the top findings.
    pub impact: u8,
    pub timestamp_sec: Option<u32>,
    pub title: String,
    pub detail: String,
    /// Structured key-value facts (no PII) ready for a Phase 2 LLM prompt.
    #[serde(default)]
    pub facts: Vec<(String, String)>,
}

/// One chronological highlight in the collapsible turning-points timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurningPoint {
    pub time_sec: u32,
    pub pillar: ReviewPillar,
    pub polarity: FindingPolarity,
    pub label: String,
}

/// Normalized match features preserved alongside the review for future ML scoring.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReviewFeatures {
    pub duration_sec: u32,
    pub win: bool,
    pub cs_per_min: f32,
    pub vision_per_min: f32,
    pub kill_participation: Option<f32>,
    pub deaths: u32,
    pub pre_objective_deaths: u32,
    pub high_gold_deaths: u32,
    pub death_streaks: u32,
}

/// Anonymized scoreboard snapshot for one champion (zero player identity).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChampionLine {
    pub champion_id: i64,
    pub position: Option<Position>,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub cs: u32,
    pub gold: u32,
    pub damage: u32,
}

/// Compressed game-phase checkpoint sampled from `MatchTrace::samples` (e.g. ~10m, 15m, 20m, 25m).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PhaseCheckpoint {
    pub time_sec: u32,
    pub level: u32,
    pub cs: u32,
    pub lane_cs_diff: Option<i32>,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub current_gold: i64,
}

/// Player's own historical average on this champion (from local My Stats when `games >= 3`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PersonalBaseline {
    pub games: u32,
    pub win_rate: f32,
    pub avg_kills: f32,
    pub avg_deaths: f32,
    pub avg_assists: f32,
    pub avg_cs_per_min: f32,
    pub avg_dpm: f32,
    pub avg_vision_per_min: f32,
}

/// Tactical narrative classification of a single match (computed deterministically in Layer 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MatchArchetype {
    #[default]
    EvenBattle,
    SnowballConversion,
    LeadThrown,
    ComebackWin,
    CarryInDefeat,
    CarriedVictory,
    LaneDiffDefeat,
}

impl MatchArchetype {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::EvenBattle => lang.pick("Close macro game", "Partie serrée"),
            Self::SnowballConversion => lang.pick("Clean snowball", "Snowball maîtrisé"),
            Self::LeadThrown => lang.pick("Lead thrown", "Avance perdue"),
            Self::ComebackWin => lang.pick("Comeback win", "Remontada"),
            Self::CarryInDefeat => lang.pick("Carry in defeat", "Carry dans la défaite"),
            Self::CarriedVictory => lang.pick("Shaky win", "Victoire fragile"),
            Self::LaneDiffDefeat => lang.pick("Early deficit", "Retard early"),
        }
    }

    /// Short tactical framing injected into the LLM dossier so the coach sets the right tone.
    pub fn coach_framing(self, lang: Lang) -> &'static str {
        match self {
            Self::EvenBattle => lang.pick(
                "Close match decided by mid/late macro and objective setups.",
                "Partie équilibrée décidée par la macro mid/late et les préparations d'objectifs.",
            ),
            Self::SnowballConversion => lang.pick(
                "Strong early lead converted into a clean win; highlight what to replicate and one small habit to sharpen.",
                "Solide avance early convertie proprement ; souligne ce qu'il faut reproduire et un détail à affiner.",
            ),
            Self::LeadThrown => lang.pick(
                "Player/team had a real lead and lost it—this is the highest-ROI coaching game: pinpoint the exact mid-game leak that gave away tempo.",
                "Le joueur/l'équipe avait une vraie avance et l'a perdue — c'est le match le plus rentable à coacher : identifie la fuite de tempo mid-game exacte.",
            ),
            Self::ComebackWin => lang.pick(
                "Rough early phase turned into a victory; credit the stabilization and identify how to avoid the early deficit.",
                "Début difficile retourné en victoire ; valorise la stabilisation et identifie comment éviter le déficit initial.",
            ),
            Self::CarryInDefeat => lang.pick(
                "Player performed at high impact despite the loss—validate their strong carry play, do NOT lecture them generically, and isolate the one macro/closing lever that could have flipped the game.",
                "Le joueur a eu un fort impact malgré la défaite — valide sa bonne prestation sans lui faire la morale, et isole l'unique levier macro/closing qui aurait pu retourner la game.",
            ),
            Self::CarriedVictory => lang.pick(
                "Team won despite weak individual numbers or high deaths—warn constructively against hidden bad habits masked by a victory.",
                "Victoire d'équipe malgré un impact individuel faible ou beaucoup de morts — alerte de façon constructive sur les mauvaises habitudes masquées par la win.",
            ),
            Self::LaneDiffDefeat => lang.pick(
                "Fell behind early in lane and could not recover; focus on early lane survival, wave control, and reset discipline.",
                "Retard pris tôt en lane sans parvenir à stabiliser ; concentre-toi sur la survie en lane, la gestion de vague et les backs.",
            ),
        }
    }
}

/// Longitudinal, identity-free player habit profile computed from recent local Rift games
/// and cached live traces (zero cloud, zero PII).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerHabitProfile {
    pub sample_games: u32,
    pub wins: u32,
    pub losses: u32,
    /// Win vs Loss fingerprint (only populated when `wins >= 2 && losses >= 2`).
    pub win_avg_deaths: Option<f32>,
    pub loss_avg_deaths: Option<f32>,
    pub win_avg_cs_per_min: Option<f32>,
    pub loss_avg_cs_per_min: Option<f32>,
    pub win_avg_kp: Option<f32>,
    pub loss_avg_kp: Option<f32>,
    pub win_avg_vision_per_min: Option<f32>,
    pub loss_avg_vision_per_min: Option<f32>,
    /// Recurring trace patterns across cached live traces.
    pub tracked_trace_games: u32,
    pub pre_objective_death_games: u32,
    pub high_gold_death_games: u32,
    pub death_streak_games: u32,
    /// Current session (games within the last 6 hours) and streak (`>0` win streak, `<0` loss streak).
    pub session_wins: u32,
    pub session_losses: u32,
    pub current_streak: i32,
}

impl PlayerHabitProfile {
    /// True when the profile holds enough longitudinal signal to enrich coaching.
    pub fn has_signal(&self) -> bool {
        self.sample_games >= 4 || self.tracked_trace_games >= 2
    }

    /// Deterministic one-line summary of the player's biggest recurring habit/leak (shown in UI & prompt).
    pub fn primary_pattern(&self, lang: Lang) -> Option<String> {
        if self.tracked_trace_games >= 3
            && self.pre_objective_death_games * 2 >= self.tracked_trace_games
        {
            return Some(
                lang.pick(
                    "Recurring pattern: pre-objective deaths in {n}/{t} tracked games",
                    "Schéma récurrent : morts avant objectif dans {n}/{t} parties suivies",
                )
                .replace("{n}", &self.pre_objective_death_games.to_string())
                .replace("{t}", &self.tracked_trace_games.to_string()),
            );
        }
        if self.tracked_trace_games >= 3
            && self.high_gold_death_games * 2 >= self.tracked_trace_games
        {
            return Some(
                lang.pick(
                    "Recurring pattern: dying with 1,500+ unspent gold in {n}/{t} tracked games",
                    "Schéma récurrent : mort avec 1 500+ PO en poche dans {n}/{t} parties suivies",
                )
                .replace("{n}", &self.high_gold_death_games.to_string())
                .replace("{t}", &self.tracked_trace_games.to_string()),
            );
        }
        if let (Some(wd), Some(ld)) = (self.win_avg_deaths, self.loss_avg_deaths) {
            if ld - wd >= 2.2 {
                return Some(
                    lang.pick(
                        "Win/Loss gap: {wd:.1} avg deaths in wins vs {ld:.1} in losses",
                        "Écart Victoire/Défaite : {wd:.1} morts moy. en win contre {ld:.1} en défaite",
                    )
                    .replace("{wd:.1}", &format!("{wd:.1}"))
                    .replace("{ld:.1}", &format!("{ld:.1}")),
                );
            }
        }
        if let (Some(wcs), Some(lcs)) = (self.win_avg_cs_per_min, self.loss_avg_cs_per_min) {
            if wcs - lcs >= 1.1 {
                return Some(
                    lang.pick(
                        "Win/Loss gap: {wcs:.1} CS/min in wins vs {lcs:.1} in losses",
                        "Écart Victoire/Défaite : {wcs:.1} CS/min en win contre {lcs:.1} en défaite",
                    )
                    .replace("{wcs:.1}", &format!("{wcs:.1}"))
                    .replace("{lcs:.1}", &format!("{lcs:.1}")),
                );
            }
        }
        None
    }
}

/// Analytical coaching lens selectable by the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CoachFocus {
    #[default]
    Overview,
    Matchup,
    Macro,
    Build,
}

impl CoachFocus {
    pub fn as_index(self) -> i32 {
        match self {
            Self::Overview => 0,
            Self::Matchup => 1,
            Self::Macro => 2,
            Self::Build => 3,
        }
    }

    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Matchup,
            2 => Self::Macro,
            3 => Self::Build,
            _ => Self::Overview,
        }
    }

    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Overview => lang.pick("Full debrief", "Complet"),
            Self::Matchup => lang.pick("Lane & Matchup", "Lane & Matchup"),
            Self::Macro => lang.pick("Macro & Tempo", "Macro & Tempo"),
            Self::Build => lang.pick("Build & Spikes", "Build & Spikes"),
        }
    }
}

/// One completed follow-up Q&A exchange in a grounded coach session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoachTurn {
    pub question: String,
    pub answer: String,
}

/// One chat message in OpenAI-compatible `{role, content}` format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoachMessage {
    pub role: &'static str,
    pub content: String,
}

/// High-signal, identity-free match dossier captured alongside [`GameReview`] for the opt-in
/// Layer 2 LLM coach.
///
/// Carries only champion IDs, roles, items, runes, spells, numeric metrics, compressed phase
/// checkpoints, deaths/objectives, and personal averages—never a summoner name, PUUID, or secret.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CoachContext {
    pub queue: Option<StatsQueue>,
    #[serde(default)]
    pub archetype: MatchArchetype,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
    pub total_gold: u32,
    pub damage_to_champions: u32,
    pub vision_score: u32,
    pub damage_share: Option<f32>,
    pub gold_share: Option<f32>,
    pub placement: Option<u8>,
    pub performance_score: Option<u8>,
    /// Team totals `(ally, enemy)` when the full scoreboard was available.
    pub team_kills: Option<(u32, u32)>,
    pub team_gold: Option<(u32, u32)>,
    /// Player setup: keystone rune ID, secondary tree ID, summoner spells, final items.
    pub keystone_id: Option<i64>,
    pub sub_style_id: Option<i64>,
    pub spell_ids: [i64; 2],
    pub items: Vec<i64>,
    /// Completed item spike timestamps from `MatchTrace::item_buys`.
    pub item_spikes: Vec<ItemBuyEvent>,
    /// Recommended core items when a build was available.
    pub recommended_core: Vec<i64>,
    /// Direct lane opponent champion ID and optional scoreboard line.
    pub opponent_champion_id: Option<i64>,
    pub opponent_line: Option<ChampionLine>,
    /// Ally and enemy team champion IDs (empty/partial if unavailable).
    pub ally_champions: Vec<i64>,
    pub enemy_champions: Vec<i64>,
    /// Most dangerous enemy champion by combat impact (from EoG scoreboard, no identity).
    pub top_enemy_threat: Option<ChampionLine>,
    /// Compressed early/mid/late trajectory checkpoints (~10m, 15m, 20m, 25m).
    pub checkpoints: Vec<PhaseCheckpoint>,
    /// Recorded deaths with timestamps and unspent gold (capped to 10).
    pub deaths_log: Vec<DeathEvent>,
    /// Recorded epic objectives (Ally vs Enemy + player involvement).
    pub objectives_log: Vec<ObjectiveEvent>,
    /// Player's own baseline on this champion when `games >= 3`.
    pub personal_baseline: Option<PersonalBaseline>,
    /// Cross-game longitudinal habit profile (Win/Loss fingerprint + recurring trace patterns).
    #[serde(default)]
    pub habit_profile: Option<PlayerHabitProfile>,
}

/// Complete Layer 1 review of one Summoner's Rift match.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameReview {
    pub game_id: i64,
    pub champion_id: i64,
    pub position: Option<Position>,
    pub coverage: ReviewCoverage,
    pub good_choices: Vec<ReviewFinding>,
    pub improvements: Vec<ReviewFinding>,
    pub turning_points: Vec<TurningPoint>,
    pub features: ReviewFeatures,
    #[serde(default)]
    pub context: CoachContext,
}

impl GameReview {
    /// Whether the review produced at least one actionable finding.
    pub fn has_findings(&self) -> bool {
        !self.good_choices.is_empty() || !self.improvements.is_empty()
    }
}

/// Normalized inputs for [`analyze_match`], constructible from either [`EndOfGameSummary`]
/// or [`PlayerGame`]. Carries zero player identity.
#[derive(Debug, Clone, Default)]
pub struct ReviewInput<'a> {
    pub game_id: i64,
    pub champion_id: i64,
    pub position: Option<Position>,
    pub queue: Option<StatsQueue>,
    pub win: bool,
    pub duration_sec: u32,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub creep_score: u32,
    pub total_gold: u32,
    pub damage_to_champions: u32,
    pub vision_score: u32,
    pub kill_participation: Option<f32>,
    pub damage_share: Option<f32>,
    pub gold_share: Option<f32>,
    pub placement: Option<u8>,
    pub performance_score: Option<u8>,
    pub keystone_id: Option<i64>,
    pub sub_style_id: Option<i64>,
    pub spell_ids: [i64; 2],
    pub team_kills: Option<(u32, u32)>,
    pub team_gold: Option<(u32, u32)>,
    pub opponent_line: Option<ChampionLine>,
    pub top_enemy_threat: Option<ChampionLine>,
    pub items: Vec<i64>,
    pub ally_champions: Vec<i64>,
    pub enemy_champions: Vec<i64>,
    pub opponent_champion_id: Option<i64>,
    pub trace: Option<&'a MatchTrace>,
    pub personal_champ: Option<&'a PlayerChampionStats>,
    pub recommended_build: Option<&'a ChampionBuild>,
    pub habit_profile: Option<&'a PlayerHabitProfile>,
    pub is_demo: bool,
}

impl<'a> ReviewInput<'a> {
    /// Attaches a longitudinal [`PlayerHabitProfile`] to this review input.
    pub fn with_habit_profile(mut self, habit: Option<&'a PlayerHabitProfile>) -> Self {
        self.habit_profile = habit;
        self
    }

    /// Builds a [`ReviewInput`] from an [`EndOfGameSummary`]. Returns `None` for remakes or
    /// non-5v5 Summoner's Rift blocks.
    pub fn from_end_of_game(
        eog: &EndOfGameSummary,
        game_id: i64,
        trace: Option<&'a MatchTrace>,
        personal_champ: Option<&'a PlayerChampionStats>,
        recommended_build: Option<&'a ChampionBuild>,
        is_demo: bool,
    ) -> Option<Self> {
        if eog.game_length_sec < REMAKE_SECS || eog.champion_id <= 0 {
            return None;
        }
        let ally_team = eog.teams.iter().find(|t| t.is_player_team);
        let enemy_team = eog.teams.iter().find(|t| !t.is_player_team);
        let self_row = ally_team.and_then(|t| t.players.iter().find(|p| p.is_self));
        let position = self_row
            .and_then(|p| Position::parse(&p.position))
            .or_else(|| trace.and_then(|t| t.position));

        let ally_champions: Vec<i64> = ally_team
            .map(|t| t.players.iter().map(|p| p.champion_id).collect())
            .unwrap_or_default();
        let enemy_champions: Vec<i64> = enemy_team
            .map(|t| t.players.iter().map(|p| p.champion_id).collect())
            .unwrap_or_default();
        let opponent_row = position.and_then(|pos| {
            enemy_team.and_then(|t| {
                t.players
                    .iter()
                    .find(|p| Position::parse(&p.position) == Some(pos))
            })
        });
        let opponent_champion_id = opponent_row.map(|p| p.champion_id);
        let opponent_line = opponent_row
            .filter(|p| p.champion_id > 0)
            .map(|p| ChampionLine {
                champion_id: p.champion_id,
                position: Position::parse(&p.position),
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                cs: p.creep_score,
                gold: p.gold_earned,
                damage: p.damage_to_champions,
            });
        let top_enemy_threat = enemy_team
            .and_then(|t| {
                t.players
                    .iter()
                    .filter(|p| {
                        p.champion_id > 0 && (p.kills >= 4 || p.damage_to_champions >= 15_000)
                    })
                    .max_by_key(|p| p.kills * 3 + p.assists + (p.damage_to_champions / 2_500))
            })
            .map(|p| ChampionLine {
                champion_id: p.champion_id,
                position: Position::parse(&p.position),
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                cs: p.creep_score,
                gold: p.gold_earned,
                damage: p.damage_to_champions,
            });

        let team_kills = match (ally_team, enemy_team) {
            (Some(a), Some(e)) if a.total_kills > 0 || e.total_kills > 0 => {
                Some((a.total_kills, e.total_kills))
            }
            _ => None,
        };
        let team_gold = match (ally_team, enemy_team) {
            (Some(a), Some(e)) if a.total_gold > 0 && e.total_gold > 0 => {
                Some((a.total_gold, e.total_gold))
            }
            _ => None,
        };
        let gold_share = match (self_row, ally_team) {
            (Some(me), Some(a)) if me.gold_earned > 0 && a.total_gold > 0 => {
                Some(me.gold_earned as f32 / a.total_gold as f32)
            }
            _ => None,
        };

        Some(Self {
            game_id,
            champion_id: eog.champion_id,
            position,
            queue: None,
            win: eog.result.eq_ignore_ascii_case("victory"),
            duration_sec: eog.game_length_sec,
            kills: eog.kills,
            deaths: eog.deaths,
            assists: eog.assists,
            creep_score: eog.creep_score,
            total_gold: eog.total_gold,
            damage_to_champions: eog.damage_to_champions,
            vision_score: eog.vision_score,
            kill_participation: self_row.map(|p| p.kill_participation),
            damage_share: self_row.map(|p| p.damage_share).filter(|&d| d > 0.0),
            gold_share,
            placement: self_row.map(|p| p.placement).filter(|&pl| pl > 0),
            performance_score: self_row.map(|p| p.performance_score).filter(|&s| s > 0),
            keystone_id: self_row.map(|p| p.keystone_id).filter(|&k| k > 0),
            sub_style_id: self_row.map(|p| p.sub_style_id).filter(|&s| s > 0),
            spell_ids: self_row
                .map(|p| [p.spell1_id, p.spell2_id])
                .unwrap_or([0, 0]),
            team_kills,
            team_gold,
            opponent_line,
            top_enemy_threat,
            items: self_row.map(|p| p.items.clone()).unwrap_or_default(),
            ally_champions,
            enemy_champions,
            opponent_champion_id,
            trace,
            personal_champ,
            recommended_build,
            habit_profile: None,
            is_demo,
        })
    }

    /// Builds a [`ReviewInput`] from a historical [`PlayerGame`]. Returns `None` for remakes or
    /// non-Summoner's Rift queues (ARAM, Arena).
    pub fn from_player_game(
        game: &PlayerGame,
        trace: Option<&'a MatchTrace>,
        personal_champ: Option<&'a PlayerChampionStats>,
        recommended_build: Option<&'a ChampionBuild>,
        is_demo: bool,
    ) -> Option<Self> {
        if game.is_remake() || game.champion_id <= 0 {
            return None;
        }
        if let Some(q) = game.queue {
            if !q.on_summoners_rift() {
                return None;
            }
        } else if game.position.is_none() {
            return None;
        }
        let enemy_champions = game
            .opponent_champion_id
            .filter(|&id| id > 0)
            .into_iter()
            .collect();
        let keystone_id = game
            .runes
            .as_ref()
            .and_then(|r| r.perk_ids.first().copied())
            .filter(|&id| id > 0);
        let sub_style_id = game
            .runes
            .as_ref()
            .map(|r| r.sub_style_id)
            .filter(|&id| id > 0);
        Some(Self {
            game_id: game.game_id,
            champion_id: game.champion_id,
            position: game.position.or_else(|| trace.and_then(|t| t.position)),
            queue: game.queue,
            win: game.win,
            duration_sec: game.duration_secs,
            kills: game.kills,
            deaths: game.deaths,
            assists: game.assists,
            creep_score: game.cs,
            total_gold: game.gold,
            damage_to_champions: game.damage_to_champions,
            vision_score: game.vision_score,
            kill_participation: game.kill_participation,
            damage_share: game.damage_share,
            gold_share: game.gold_share,
            placement: game.placement,
            performance_score: game.performance_score,
            keystone_id,
            sub_style_id,
            spell_ids: [game.spells.first, game.spells.second],
            team_kills: None,
            team_gold: None,
            opponent_line: None,
            top_enemy_threat: None,
            items: game.items.clone(),
            ally_champions: vec![game.champion_id],
            enemy_champions,
            opponent_champion_id: game.opponent_champion_id,
            trace,
            personal_champ,
            recommended_build,
            habit_profile: None,
            is_demo,
        })
    }
}

/// Static item IDs providing Grievous Wounds (anti-heal).
const ANTI_HEAL_ITEMS: &[i64] = &[
    3033, // Mortal Reminder
    3075, // Thornmail
    3076, // Bramble Vest
    3123, // Executioner's Calling
    3165, // Morellonomicon
    3916, // Oblivion Orb
    6609, // Chempunk Chainsword
];

/// Static item IDs providing % Armor or Magic Penetration.
const PERCENT_PEN_ITEMS: &[i64] = &[
    3033, // Mortal Reminder
    3035, // Last Whisper
    3036, // Lord Dominik's Regards
    3071, // Black Cleaver
    3135, // Void Staff
    3137, // Cryptbloom
    3302, // Terminus
    4630, // Blighting Jewel
    6694, // Serylda's Grudge
    4646, // Bloodletter's Curse
];

/// Static item IDs for completed tier-2 boots (plus Magical Footwear / Cassiopeia exception).
const COMPLETED_BOOTS: &[i64] = &[
    3006, // Berserker's Greaves
    3009, // Boots of Swiftness
    3020, // Sorcerer's Shoes
    3047, // Plated Steelcaps
    3111, // Mercury's Treads
    3117, // Mobility Boots
    3158, // Ionian Boots of Lucidity
    3010, // Symbiotic Soles
    3013, // Synchronized Souls
];

/// Cassiopeia cannot buy boots.
const CASSIOPEIA_ID: i64 = 69;

/// Champions whose core kit revolves around high sustain / healing.
const HEAVY_HEAL_CHAMPIONS: &[i64] = &[
    8,   // Vladimir
    16,  // Soraka
    19,  // Warwick
    36,  // Dr. Mundo
    37,  // Sona
    50,  // Swain
    122, // Darius
    154, // Zac
    266, // Aatrox
    350, // Yuumi
    420, // Illaoi
    517, // Sylas
    777, // Yone
    887, // Gwen
    233, // Briar
];

/// Format seconds as `mm:ss`.
pub fn format_mm_ss(sec: u32) -> String {
    format!("{:02}:{:02}", sec / 60, sec % 60)
}

/// Runs the Layer 1 deterministic coach review over a single Summoner's Rift match.
pub fn analyze_match(input: &ReviewInput<'_>, lang: Lang) -> GameReview {
    let minutes = (input.duration_sec as f32 / 60.0).max(1.0);
    let cs_per_min = input.creep_score as f32 / minutes;
    let vision_per_min = input.vision_score as f32 / minutes;

    let coverage = if input.is_demo {
        ReviewCoverage::Demo
    } else if input.trace.is_some_and(MatchTrace::has_timeline_data) {
        ReviewCoverage::Full
    } else {
        ReviewCoverage::PostGameOnly
    };

    let mut findings: Vec<ReviewFinding> = Vec::new();
    let mut turning_points: Vec<TurningPoint> = Vec::new();
    let mut features = ReviewFeatures {
        duration_sec: input.duration_sec,
        win: input.win,
        cs_per_min,
        vision_per_min,
        kill_participation: input.kill_participation,
        deaths: input.deaths,
        ..ReviewFeatures::default()
    };

    eval_objectives(
        input,
        lang,
        &mut findings,
        &mut turning_points,
        &mut features,
    );
    eval_economy(
        input,
        minutes,
        cs_per_min,
        lang,
        &mut findings,
        &mut turning_points,
        &mut features,
    );
    eval_build(input, lang, &mut findings);
    eval_discipline(
        input,
        vision_per_min,
        lang,
        &mut findings,
        &mut turning_points,
        &mut features,
    );
    eval_draft(input, lang, &mut findings);

    let good_choices = select_top(
        findings
            .iter()
            .filter(|f| f.polarity == FindingPolarity::GoodChoice)
            .cloned()
            .collect(),
    );
    let improvements = select_top(
        findings
            .iter()
            .filter(|f| f.polarity == FindingPolarity::ToImprove)
            .cloned()
            .collect(),
    );

    turning_points.sort_by_key(|tp| tp.time_sec);
    turning_points.dedup_by(|a, b| a.time_sec == b.time_sec && a.label == b.label);

    GameReview {
        game_id: input.game_id,
        champion_id: input.champion_id,
        position: input.position,
        coverage,
        good_choices,
        improvements,
        turning_points,
        context: build_coach_context(input, &features),
        features,
    }
}

/// Pillar 1: Objectives & tempo.
fn eval_objectives(
    input: &ReviewInput<'_>,
    lang: Lang,
    findings: &mut Vec<ReviewFinding>,
    turning_points: &mut Vec<TurningPoint>,
    features: &mut ReviewFeatures,
) {
    let Some(trace) = input.trace.filter(|t| t.has_timeline_data()) else {
        return;
    };

    // Detect deaths occurring 5..=45 seconds before an enemy-secured epic objective.
    let mut pre_obj_hits: Vec<(&DeathEvent, &ObjectiveEvent, u32)> = Vec::new();
    for obj in trace.objectives.iter().filter(|o| !o.by_ally) {
        if let Some(death) = trace
            .deaths
            .iter()
            .filter(|d| d.time_sec < obj.time_sec && obj.time_sec - d.time_sec <= 45)
            .max_by_key(|d| d.time_sec)
        {
            let delta = obj.time_sec - death.time_sec;
            if delta >= 5 {
                pre_obj_hits.push((death, obj, delta));
                turning_points.push(TurningPoint {
                    time_sec: death.time_sec,
                    pillar: ReviewPillar::Objectives,
                    polarity: FindingPolarity::ToImprove,
                    label: lang
                        .pick(
                            "Died {delta}s before enemy {obj}",
                            "Mort {delta}s avant le {obj} ennemi",
                        )
                        .replace("{delta}", &delta.to_string())
                        .replace("{obj}", obj.kind.label(lang)),
                });
            }
        }
    }

    features.pre_objective_deaths = pre_obj_hits.len() as u32;

    if let Some(&(death, obj, delta)) = pre_obj_hits.iter().max_by_key(|(_, o, _)| match o.kind {
        EpicKind::Baron | EpicKind::Elder => 3,
        EpicKind::Dragon => 2,
        EpicKind::Herald | EpicKind::Grubs => 1,
    }) {
        let count = pre_obj_hits.len();
        let impact = (78 + (count.min(3) as u8 * 7)).min(98);
        let obj_name = obj.kind.label(lang);
        let title = if count > 1 {
            lang.pick(
                "{n} deaths right before enemy objectives",
                "{n} morts juste avant des objectifs ennemis",
            )
            .replace("{n}", &count.to_string())
        } else {
            lang.pick(
                "Died {delta}s before enemy {obj}",
                "Mort {delta}s avant le {obj} ennemi",
            )
            .replace("{delta}", &delta.to_string())
            .replace("{obj}", obj_name)
        };
        let detail = lang
            .pick(
                "At {ts}, your death left your team outnumbered for {obj}. Aim to reset 45s before spawn.",
                "À {ts}, ta mort a laissé ton équipe en infériorité pour le {obj}. Vise un back 45s avant le spawn.",
            )
            .replace("{ts}", &format_mm_ss(death.time_sec))
            .replace("{obj}", obj_name);

        findings.push(ReviewFinding {
            id: "obj_pre_death".into(),
            pillar: ReviewPillar::Objectives,
            polarity: FindingPolarity::ToImprove,
            impact,
            timestamp_sec: Some(death.time_sec),
            title,
            detail,
            facts: vec![
                ("pre_objective_deaths".into(), count.to_string()),
                ("secs_before_objective".into(), delta.to_string()),
                ("objective".into(), format!("{:?}", obj.kind)),
            ],
        });
    } else if input.duration_sec >= 1200 && !trace.objectives.is_empty() {
        findings.push(ReviewFinding {
            id: "obj_clean_timers".into(),
            pillar: ReviewPillar::Objectives,
            polarity: FindingPolarity::GoodChoice,
            impact: 72,
            timestamp_sec: None,
            title: lang
                .pick(
                    "Clean discipline around objective timers",
                    "Bonne discipline autour des timers d'objectifs",
                )
                .into(),
            detail: lang
                .pick(
                    "Zero deaths in the 45s window before enemy epic objectives across the whole match.",
                    "Aucune mort dans les 45s précédant un objectif épique ennemi sur toute la partie.",
                )
                .into(),
            facts: vec![("pre_objective_deaths".into(), "0".into())],
        });
    }

    // Ally epic objectives with player participation.
    let involved: Vec<&ObjectiveEvent> = trace
        .objectives
        .iter()
        .filter(|o| o.by_ally && o.player_involved)
        .collect();
    for obj in &involved {
        turning_points.push(TurningPoint {
            time_sec: obj.time_sec,
            pillar: ReviewPillar::Objectives,
            polarity: FindingPolarity::GoodChoice,
            label: lang
                .pick("Secured {obj} with team", "{obj} sécurisé avec l'équipe")
                .replace("{obj}", obj.kind.label(lang)),
        });
    }
    if involved.len() >= 2 {
        let first_ts = involved.first().map(|o| o.time_sec);
        findings.push(ReviewFinding {
            id: "obj_presence".into(),
            pillar: ReviewPillar::Objectives,
            polarity: FindingPolarity::GoodChoice,
            impact: (70 + (involved.len().min(4) as u8 * 5)).min(92),
            timestamp_sec: first_ts,
            title: lang
                .pick(
                    "Present on {n} allied epic objectives",
                    "Présent sur {n} objectifs épiques alliés",
                )
                .replace("{n}", &involved.len().to_string()),
            detail: lang
                .pick(
                    "Consistent rotations helped your team convert map priority into neutral objectives.",
                    "Tes rotations régulières ont aidé l'équipe à convertir la priorité en objectifs neutres.",
                )
                .into(),
            facts: vec![("allied_objectives_involved".into(), involved.len().to_string())],
        });
    }
}

/// Pillar 2: Economy & reset timing.
fn eval_economy(
    input: &ReviewInput<'_>,
    _minutes: f32,
    cs_per_min: f32,
    lang: Lang,
    findings: &mut Vec<ReviewFinding>,
    turning_points: &mut Vec<TurningPoint>,
    features: &mut ReviewFeatures,
) {
    // 2a. Dying with >= 1600 unspent gold in pocket (from live trace).
    if let Some(trace) = input.trace {
        let greedy: Vec<(&DeathEvent, i64)> = trace
            .deaths
            .iter()
            .filter_map(|d| d.unspent_gold.filter(|&g| g >= 1600).map(|g| (d, g)))
            .collect();
        features.high_gold_deaths = greedy.len() as u32;
        for &(d, gold) in &greedy {
            turning_points.push(TurningPoint {
                time_sec: d.time_sec,
                pillar: ReviewPillar::Economy,
                polarity: FindingPolarity::ToImprove,
                label: lang
                    .pick(
                        "Died holding {g}g unspent",
                        "Mort avec {g} PO non dépensées",
                    )
                    .replace("{g}", &gold.to_string()),
            });
        }
        if let Some(&(worst_death, max_gold)) = greedy.iter().max_by_key(|(_, g)| *g) {
            let count = greedy.len();
            let impact = (74 + ((max_gold - 1600) / 300).min(18) as u8).min(94);
            let title = if count > 1 {
                lang.pick(
                    "{n} deaths while holding 1,600+ unspent gold",
                    "{n} morts avec plus de 1 600 PO en poche",
                )
                .replace("{n}", &count.to_string())
            } else {
                lang.pick(
                    "Died with {g} gold unspent",
                    "Mort avec {g} PO non dépensées",
                )
                .replace("{g}", &max_gold.to_string())
            };
            let detail = lang
                .pick(
                    "At {ts}, you fought while sitting on {g}g. Spending that spike first turns close skirmishes.",
                    "À {ts}, tu as combattu avec {g} PO en poche. Dépenser ce pic d'or avant le fight fait la différence.",
                )
                .replace("{ts}", &format_mm_ss(worst_death.time_sec))
                .replace("{g}", &max_gold.to_string());

            findings.push(ReviewFinding {
                id: "econ_unspent_gold_death".into(),
                pillar: ReviewPillar::Economy,
                polarity: FindingPolarity::ToImprove,
                impact,
                timestamp_sec: Some(worst_death.time_sec),
                title,
                detail,
                facts: vec![
                    ("high_gold_deaths".into(), count.to_string()),
                    ("max_unspent_gold".into(), max_gold.to_string()),
                ],
            });
        }
    }

    // 2b. CS/min vs personal average on this champion (non-Support).
    if input.position != Some(Position::Support) && input.creep_score > 0 {
        if let Some(champ_stats) = input
            .personal_champ
            .filter(|s| s.games >= 3 && s.cs_per_min > 1.0)
        {
            let diff = cs_per_min - champ_stats.cs_per_min;
            if diff <= -1.3 {
                findings.push(ReviewFinding {
                    id: "econ_cs_below_personal".into(),
                    pillar: ReviewPillar::Economy,
                    polarity: FindingPolarity::ToImprove,
                    impact: 70,
                    timestamp_sec: None,
                    title: lang
                        .pick(
                            "Farm below your champion average ({cur:.1} vs {avg:.1} CS/m)",
                            "Farm sous ta moyenne sur ce champion ({cur:.1} vs {avg:.1} CS/m)",
                        )
                        .replace("{cur:.1}", &format!("{cs_per_min:.1}"))
                        .replace("{avg:.1}", &format!("{:.1}", champ_stats.cs_per_min)),
                    detail: lang
                        .pick(
                            "Catching side waves between fights keeps your gold curve steady even in rough games.",
                            "Récupérer les vagues latérales entre les fights maintient ta courbe d'or même en partie difficile.",
                        )
                        .into(),
                    facts: vec![
                        ("cs_per_min".into(), format!("{cs_per_min:.2}")),
                        ("personal_avg_cs_per_min".into(), format!("{:.2}", champ_stats.cs_per_min)),
                    ],
                });
            } else if diff >= 0.9 {
                findings.push(ReviewFinding {
                    id: "econ_cs_above_personal".into(),
                    pillar: ReviewPillar::Economy,
                    polarity: FindingPolarity::GoodChoice,
                    impact: 74,
                    timestamp_sec: None,
                    title: lang
                        .pick(
                            "Strong farm pace ({cur:.1} vs {avg:.1} CS/m avg)",
                            "Excellent rythme de farm ({cur:.1} vs {avg:.1} CS/m moy.)",
                        )
                        .replace("{cur:.1}", &format!("{cs_per_min:.1}"))
                        .replace("{avg:.1}", &format!("{:.1}", champ_stats.cs_per_min)),
                    detail: lang
                        .pick(
                            "You outpaced your usual CS/min on this champion, accelerating your item spikes.",
                            "Tu as dépassé ton rythme habituel sur ce champion, accélérant tes pics d'objets.",
                        )
                        .into(),
                    facts: vec![
                        ("cs_per_min".into(), format!("{cs_per_min:.2}")),
                        ("personal_avg_cs_per_min".into(), format!("{:.2}", champ_stats.cs_per_min)),
                    ],
                });
            }
        } else if cs_per_min >= 7.5 {
            findings.push(ReviewFinding {
                id: "econ_high_cs".into(),
                pillar: ReviewPillar::Economy,
                polarity: FindingPolarity::GoodChoice,
                impact: 72,
                timestamp_sec: None,
                title: lang
                    .pick("High CS/min ({cur:.1}/m)", "CS/min élevé ({cur:.1}/m)")
                    .replace("{cur:.1}", &format!("{cs_per_min:.1}")),
                detail: lang
                    .pick(
                        "Consistent wave collection kept your gold income ahead of pace.",
                        "Une collecte régulière des vagues a maintenu ton revenu d'or au-dessus du rythme.",
                    )
                    .into(),
                facts: vec![("cs_per_min".into(), format!("{cs_per_min:.2}"))],
            });
        } else if cs_per_min < 4.5 && input.position != Some(Position::Jungle) {
            findings.push(ReviewFinding {
                id: "econ_low_cs".into(),
                pillar: ReviewPillar::Economy,
                polarity: FindingPolarity::ToImprove,
                impact: 68,
                timestamp_sec: None,
                title: lang
                    .pick("Low farm income ({cur:.1} CS/m)", "Revenu de farm faible ({cur:.1} CS/m)")
                    .replace("{cur:.1}", &format!("{cs_per_min:.1}")),
                detail: lang
                    .pick(
                        "Prioritize catching waves before rotating so missed minions do not cost you a full item.",
                        "Priorise la prise des vagues avant de décaler pour ne pas perdre l'équivalent d'un objet.",
                    )
                    .into(),
                facts: vec![("cs_per_min".into(), format!("{cs_per_min:.2}"))],
            });
        }
    }

    // 2c. Lane CS lead/deficit around 15 min from live trace.
    if let Some(trace) = input.trace {
        if let Some(mid_sample) = trace
            .samples
            .iter()
            .filter(|s| (780..=960).contains(&s.time_sec))
            .min_by_key(|s| (s.time_sec as i32 - 900).abs())
        {
            if let Some(diff) = mid_sample.lane_cs_diff {
                if diff >= 20 {
                    turning_points.push(TurningPoint {
                        time_sec: mid_sample.time_sec,
                        pillar: ReviewPillar::Economy,
                        polarity: FindingPolarity::GoodChoice,
                        label: lang
                            .pick(
                                "+{d} CS lead over lane opponent",
                                "+{d} CS d'avance sur le vis-à-vis",
                            )
                            .replace("{d}", &diff.to_string()),
                    });
                    findings.push(ReviewFinding {
                        id: "econ_lane_lead_15".into(),
                        pillar: ReviewPillar::Economy,
                        polarity: FindingPolarity::GoodChoice,
                        impact: 78,
                        timestamp_sec: Some(mid_sample.time_sec),
                        title: lang
                            .pick("+{d} CS lane advantage at 15 min", "+{d} CS d'avance en lane à 15 min")
                            .replace("{d}", &diff.to_string()),
                        detail: lang
                            .pick(
                                "Strong laning discipline built a reliable gold lead heading into mid game.",
                                "Une phase de lane solide t'a construit une avance d'or fiable pour le mid game.",
                            )
                            .into(),
                        facts: vec![("lane_cs_diff_15".into(), diff.to_string())],
                    });
                } else if diff <= -20 {
                    turning_points.push(TurningPoint {
                        time_sec: mid_sample.time_sec,
                        pillar: ReviewPillar::Economy,
                        polarity: FindingPolarity::ToImprove,
                        label: lang
                            .pick(
                                "{d} CS deficit vs lane opponent",
                                "{d} CS de retard sur le vis-à-vis",
                            )
                            .replace("{d}", &diff.to_string()),
                    });
                }
            }
        }
    }
}

/// Pillar 3: Build, runes & itemization adaptation.
fn eval_build(input: &ReviewInput<'_>, lang: Lang, findings: &mut Vec<ReviewFinding>) {
    let has_anti_heal = input.items.iter().any(|id| ANTI_HEAL_ITEMS.contains(id));
    let has_percent_pen = input.items.iter().any(|id| PERCENT_PEN_ITEMS.contains(id));
    let has_boots = input.items.iter().any(|id| COMPLETED_BOOTS.contains(id));

    // 3a. Anti-heal check vs heavy-healing enemy composition.
    let heal_enemies: Vec<i64> = input
        .enemy_champions
        .iter()
        .copied()
        .filter(|id| HEAVY_HEAL_CHAMPIONS.contains(id))
        .collect();
    if heal_enemies.len() >= 2 && input.duration_sec >= 1080 {
        let names: Vec<String> = heal_enemies
            .iter()
            .take(3)
            .map(|&id| game_name_or(GameTextKind::Champion, id, lang, "Champion"))
            .collect();
        let list = names.join(", ");
        if has_anti_heal {
            findings.push(ReviewFinding {
                id: "build_anti_heal_good".into(),
                pillar: ReviewPillar::Build,
                polarity: FindingPolarity::GoodChoice,
                impact: 76,
                timestamp_sec: None,
                title: lang
                    .pick("Adapted build with Grievous Wounds", "Build adapté avec Hémorragie (anti-soin)")
                    .into(),
                detail: lang
                    .pick(
                        "Buying anti-heal against {list} directly cut their teamfight sustain.",
                        "Acheter de l'anti-soin face à {list} a directement réduit leur régénération en teamfight.",
                    )
                    .replace("{list}", &list),
                facts: vec![("enemy_healers".into(), heal_enemies.len().to_string())],
            });
        } else {
            findings.push(ReviewFinding {
                id: "build_missing_anti_heal".into(),
                pillar: ReviewPillar::Build,
                polarity: FindingPolarity::ToImprove,
                impact: 77,
                timestamp_sec: None,
                title: lang
                    .pick(
                        "No anti-heal against {n} healing threats",
                        "Pas d'anti-soin face à {n} champions à fort soin",
                    )
                    .replace("{n}", &heal_enemies.len().to_string()),
                detail: lang
                    .pick(
                        "Facing {list}, an early 800g Grievous Wounds component pays for itself in extended fights.",
                        "Face à {list}, un composant Hémorragie à 800 PO est vite rentabilisé sur les fights longs.",
                    )
                    .replace("{list}", &list),
                facts: vec![("enemy_healers".into(), heal_enemies.len().to_string())],
            });
        }
    }

    // 3b. % Penetration vs tank/frontline-heavy enemy comp (for non-Support carries).
    let enemy_frontline = input
        .enemy_champions
        .iter()
        .filter(|&&id| champion_traits(id).is_some_and(|t| t.has(CompTrait::Frontline)))
        .count();
    let is_carry = input.position != Some(Position::Support)
        && champion_traits(input.champion_id).is_some_and(|t| !t.has(CompTrait::Frontline));
    if is_carry && enemy_frontline >= 2 && input.duration_sec >= 1320 {
        if has_percent_pen {
            findings.push(ReviewFinding {
                id: "build_pen_good".into(),
                pillar: ReviewPillar::Build,
                polarity: FindingPolarity::GoodChoice,
                impact: 73,
                timestamp_sec: None,
                title: lang
                    .pick(
                        "Built % penetration against enemy frontline",
                        "Pénétration % achetée face à la frontline ennemie",
                    )
                    .into(),
                detail: lang
                    .pick(
                        "Smart itemization against {n} frontline champions kept your late-game damage relevant.",
                        "Bonne adaptation face à {n} champions frontline pour garder des dégâts efficaces en late game.",
                    )
                    .replace("{n}", &enemy_frontline.to_string()),
                facts: vec![("enemy_frontline".into(), enemy_frontline.to_string())],
            });
        } else {
            findings.push(ReviewFinding {
                id: "build_missing_pen".into(),
                pillar: ReviewPillar::Build,
                polarity: FindingPolarity::ToImprove,
                impact: 74,
                timestamp_sec: None,
                title: lang
                    .pick(
                        "Missing % penetration vs {n} frontline enemies",
                        "Pénétration % absente face à {n} champions frontline",
                    )
                    .replace("{n}", &enemy_frontline.to_string()),
                detail: lang
                    .pick(
                        "Without % armor/magic pen by 3rd item, enemy resistances heavily blunt your damage.",
                        "Sans pénétration % d'armure/RM en 3e objet, les résistances ennemies réduisent fortement tes dégâts.",
                    )
                    .into(),
                facts: vec![("enemy_frontline".into(), enemy_frontline.to_string())],
            });
        }
    }

    // 3c. Completed boots check after 18 min (except Cassiopeia).
    if input.champion_id != CASSIOPEIA_ID
        && input.duration_sec >= 1080
        && !input.items.is_empty()
        && !has_boots
    {
        findings.push(ReviewFinding {
            id: "build_no_boots".into(),
            pillar: ReviewPillar::Build,
            polarity: FindingPolarity::ToImprove,
            impact: 64,
            timestamp_sec: None,
            title: lang
                .pick("No tier-2 boots completed", "Aucune botte tier 2 complétée")
                .into(),
            detail: lang
                .pick(
                    "Upgraded boots give cheap mobility and defensive stats that improve rotations and skirmishes.",
                    "Les bottes améliorées offrent mobilité et stats défensives peu coûteuses pour mieux tourner sur la carte.",
                )
                .into(),
            facts: vec![("has_tier2_boots".into(), "false".into())],
        });
    }

    // 3d. Core build alignment when recommended build is known.
    if let Some(build) = input.recommended_build {
        let core_ids: Vec<i64> = build
            .items
            .iter()
            .filter(|b| b.stage == Some(crate::stats::ItemStage::Core))
            .flat_map(|b| b.item_ids.iter().copied())
            .collect();
        if !core_ids.is_empty() {
            let matched = core_ids
                .iter()
                .filter(|id| input.items.contains(id))
                .count();
            if matched >= 2 {
                let first_name = game_name_or(GameTextKind::Item, core_ids[0], lang, "Core");
                findings.push(ReviewFinding {
                    id: "build_core_aligned".into(),
                    pillar: ReviewPillar::Build,
                    polarity: FindingPolarity::GoodChoice,
                    impact: 62,
                    timestamp_sec: None,
                    title: lang
                        .pick("Solid core itemization", "Core build cohérent")
                        .into(),
                    detail: lang
                        .pick(
                            "Your build hit {n} core spikes (including {item}) for this matchup.",
                            "Ton build a atteint {n} objets clés (dont {item}) adaptés à ce matchup.",
                        )
                        .replace("{n}", &matched.to_string())
                        .replace("{item}", &first_name),
                    facts: vec![("core_items_matched".into(), matched.to_string())],
                });
            }
        }
    }
}

/// Pillar 4: Discipline & vision.
fn eval_discipline(
    input: &ReviewInput<'_>,
    vision_per_min: f32,
    lang: Lang,
    findings: &mut Vec<ReviewFinding>,
    turning_points: &mut Vec<TurningPoint>,
    features: &mut ReviewFeatures,
) {
    // 4a. Death streaks (>= 2 deaths within 3 minutes after 12:00) from live trace.
    if let Some(trace) = input.trace {
        let late_deaths: Vec<u32> = trace
            .deaths
            .iter()
            .map(|d| d.time_sec)
            .filter(|&t| t >= 720)
            .collect();
        let mut streaks = 0u32;
        let mut first_streak_ts: Option<u32> = None;
        for w in late_deaths.windows(2) {
            if w[1].saturating_sub(w[0]) <= 180 {
                streaks += 1;
                if first_streak_ts.is_none() {
                    first_streak_ts = Some(w[1]);
                }
                turning_points.push(TurningPoint {
                    time_sec: w[1],
                    pillar: ReviewPillar::Discipline,
                    polarity: FindingPolarity::ToImprove,
                    label: lang
                        .pick(
                            "Back-to-back death within 3 min",
                            "Morts rapprochées en moins de 3 min",
                        )
                        .into(),
                });
            }
        }
        features.death_streaks = streaks;
        if let Some(ts) = first_streak_ts {
            findings.push(ReviewFinding {
                id: "disc_death_streak".into(),
                pillar: ReviewPillar::Discipline,
                polarity: FindingPolarity::ToImprove,
                impact: (72 + (streaks.min(3) as u8 * 6)).min(90),
                timestamp_sec: Some(ts),
                title: lang
                    .pick(
                        "Chain deaths in mid/late game",
                        "Morts en chaîne en mid/late game",
                    )
                    .into(),
                detail: lang
                    .pick(
                        "Starting at {ts}, dying again right after respawn snowballed enemy tempo. Pause to stabilize vision first.",
                        "À partir de {ts}, remourir juste après le respawn a accéléré le tempo ennemi. Stabilise la vision d'abord.",
                    )
                    .replace("{ts}", &format_mm_ss(ts)),
                facts: vec![("death_streaks".into(), streaks.to_string())],
            });
        }
    }

    // 4b. Vision score benchmarked by role.
    if input.duration_sec >= 900 && input.vision_score > 0 {
        let (low_target, high_target) = match input.position {
            Some(Position::Support) => (1.10, 1.85),
            Some(Position::Jungle) => (0.55, 0.95),
            _ => (0.40, 0.78),
        };
        if vision_per_min < low_target {
            findings.push(ReviewFinding {
                id: "disc_low_vision".into(),
                pillar: ReviewPillar::Discipline,
                polarity: FindingPolarity::ToImprove,
                impact: 66,
                timestamp_sec: None,
                title: lang
                    .pick(
                        "Low vision control ({v:.2}/min)",
                        "Contrôle de vision faible ({v:.2}/min)",
                    )
                    .replace("{v:.2}", &format!("{vision_per_min:.2}")),
                detail: lang
                    .pick(
                        "Vision score ({score}) trailed the target for your role ({target:.2}/min). Drop trinkets before objective setups.",
                        "Ton score de vision ({score}) est sous la cible de ton rôle ({target:.2}/min). Pose tes balises avant les objectifs.",
                    )
                    .replace("{score}", &input.vision_score.to_string())
                    .replace("{target:.2}", &format!("{low_target:.2}")),
                facts: vec![
                    ("vision_score".into(), input.vision_score.to_string()),
                    ("vision_per_min".into(), format!("{vision_per_min:.2}")),
                ],
            });
        } else if vision_per_min >= high_target {
            findings.push(ReviewFinding {
                id: "disc_high_vision".into(),
                pillar: ReviewPillar::Discipline,
                polarity: FindingPolarity::GoodChoice,
                impact: 70,
                timestamp_sec: None,
                title: lang
                    .pick(
                        "Strong vision control ({v:.2}/min)",
                        "Très bon contrôle de vision ({v:.2}/min)",
                    )
                    .replace("{v:.2}", &format!("{vision_per_min:.2}")),
                detail: lang
                    .pick(
                        "Vision score ({score}) gave your team safer flanks and objective setups.",
                        "Ton score de vision ({score}) a sécurisé les flanks et préparations d'objectifs de l'équipe.",
                    )
                    .replace("{score}", &input.vision_score.to_string()),
                facts: vec![
                    ("vision_score".into(), input.vision_score.to_string()),
                    ("vision_per_min".into(), format!("{vision_per_min:.2}")),
                ],
            });
        }
    }

    // 4c. High kill participation with low deaths.
    if let Some(kp) = input.kill_participation {
        let kp_pct = (kp * 100.0).round() as u32;
        if kp >= 0.62 && input.deaths <= 3 {
            findings.push(ReviewFinding {
                id: "disc_high_kp_low_deaths".into(),
                pillar: ReviewPillar::Discipline,
                polarity: FindingPolarity::GoodChoice,
                impact: 82,
                timestamp_sec: None,
                title: lang
                    .pick(
                        "High impact with few deaths ({kp}% KP, {d} deaths)",
                        "Fort impact avec peu de morts ({kp}% KP, {d} morts)",
                    )
                    .replace("{kp}", &kp_pct.to_string())
                    .replace("{d}", &input.deaths.to_string()),
                detail: lang
                    .pick(
                        "You stayed involved in most team kills while denying shutdown gold to the enemy.",
                        "Tu as participé à la majorité des éliminations tout en évitant de donner des primes à l'ennemi.",
                    )
                    .into(),
                facts: vec![
                    ("kill_participation_pct".into(), kp_pct.to_string()),
                    ("deaths".into(), input.deaths.to_string()),
                ],
            });
        } else if input.deaths >= 8 {
            findings.push(ReviewFinding {
                id: "disc_high_deaths".into(),
                pillar: ReviewPillar::Discipline,
                polarity: FindingPolarity::ToImprove,
                impact: 75,
                timestamp_sec: None,
                title: lang
                    .pick("Too many deaths ({d})", "Trop de morts ({d})")
                    .replace("{d}", &input.deaths.to_string()),
                detail: lang
                    .pick(
                        "Cutting even 3–4 avoidable deaths keeps you on the map for waves and objective timers.",
                        "Éviter ne serait-ce que 3 à 4 morts évitables te garde présent sur la carte pour les vagues et objectifs.",
                    )
                    .into(),
                facts: vec![("deaths".into(), input.deaths.to_string())],
            });
        }
    }
}

/// Pillar 5: Draft & team composition fit.
fn eval_draft(input: &ReviewInput<'_>, lang: Lang, findings: &mut Vec<ReviewFinding>) {
    if input.ally_champions.len() < 3 {
        return;
    }
    let full_comp = team_profile(&input.ally_champions);
    let others: Vec<i64> = input
        .ally_champions
        .iter()
        .copied()
        .filter(|&id| id != input.champion_id)
        .collect();
    let without_self = team_profile(&others);

    let self_traits = champion_traits(input.champion_id);
    let champ_name = game_name_or(GameTextKind::Champion, input.champion_id, lang, "Champion");

    // Did the player's pick fill a frontline or engage gap?
    let filled_frontline = self_traits.is_some_and(|t| t.has(CompTrait::Frontline))
        && !without_self.traits.contains(&CompTrait::Frontline);
    let filled_engage = self_traits.is_some_and(|t| t.has(CompTrait::Engage))
        && !without_self.traits.contains(&CompTrait::Engage);

    if filled_frontline || filled_engage {
        let role_gap = match (filled_frontline, filled_engage) {
            (true, true) => lang.pick("frontline & engage", "frontline & engage"),
            (true, false) => lang.pick("frontline", "frontline"),
            (false, true) => lang.pick("engage", "engage"),
            (false, false) => unreachable!(),
        };
        findings.push(ReviewFinding {
            id: "draft_filled_gap".into(),
            pillar: ReviewPillar::Draft,
            polarity: FindingPolarity::GoodChoice,
            impact: 68,
            timestamp_sec: None,
            title: lang
                .pick(
                    "{champ} filled your comp's {gap} gap",
                    "{champ} a comblé le manque de {gap}",
                )
                .replace("{champ}", &champ_name)
                .replace("{gap}", role_gap),
            detail: lang
                .pick(
                    "Without your pick, your team lacked {gap} to start or absorb fights.",
                    "Sans ton pick, l'équipe manquait de {gap} pour initier ou encaisser les combats.",
                )
                .replace("{gap}", role_gap),
            facts: vec![("filled_gap".into(), role_gap.into())],
        });
    } else if full_comp.traits.contains(&CompTrait::AllPhysical) {
        findings.push(ReviewFinding {
            id: "draft_all_ad".into(),
            pillar: ReviewPillar::Draft,
            polarity: FindingPolarity::ToImprove,
            impact: 63,
            timestamp_sec: None,
            title: lang
                .pick("Full-AD team composition", "Composition 100 % dégâts physiques")
                .into(),
            detail: lang
                .pick(
                    "An all-physical draft lets enemies stack armor cheaply. An AP option or early armor pen helps.",
                    "Une draft tout AD permet aux ennemis de empiler l'armure facilement. Une option AP ou de la pénétration tôt aide.",
                )
                .into(),
            facts: vec![("comp_gap".into(), "AllPhysical".into())],
        });
    } else if full_comp.traits.contains(&CompTrait::AllMagic) {
        findings.push(ReviewFinding {
            id: "draft_all_ap".into(),
            pillar: ReviewPillar::Draft,
            polarity: FindingPolarity::ToImprove,
            impact: 63,
            timestamp_sec: None,
            title: lang
                .pick("Full-AP team composition", "Composition 100 % dégâts magiques")
                .into(),
            detail: lang
                .pick(
                    "An all-magic draft lets enemies stack MR cheaply. Consider an AD threat or Void Staff / Cryptbloom.",
                    "Une draft tout AP permet aux ennemis d'empiler la RM. Pense à une menace AD ou Bâton du Vide / Floraison funeste.",
                )
                .into(),
            facts: vec![("comp_gap".into(), "AllMagic".into())],
        });
    }
}

/// Sorts by impact descending and picks up to [`MAX_FINDINGS_PER_SIDE`] items while keeping
/// at most [`MAX_PER_PILLAR_IN_TOP`] per pillar.
fn select_top(mut items: Vec<ReviewFinding>) -> Vec<ReviewFinding> {
    items.sort_by(|a, b| b.impact.cmp(&a.impact).then_with(|| a.id.cmp(&b.id)));
    let mut counts: HashMap<ReviewPillar, usize> = HashMap::new();
    let mut out = Vec::with_capacity(MAX_FINDINGS_PER_SIDE);
    for item in items {
        let c = counts.entry(item.pillar).or_insert(0);
        if *c >= MAX_PER_PILLAR_IN_TOP {
            continue;
        }
        *c += 1;
        out.push(item);
        if out.len() == MAX_FINDINGS_PER_SIDE {
            break;
        }
    }
    out
}

/// Helper: returns true when `queue_id` is a Summoner's Rift queue eligible for review.
pub fn is_reviewable_queue(queue_id: i64) -> bool {
    StatsQueue::from_queue_id(queue_id).is_some_and(StatsQueue::on_summoners_rift)
}

/// Computes a longitudinal, identity-free [`PlayerHabitProfile`] from recent local Rift games
/// and cached [`MatchTrace`]s. Returns `None` when there is not enough history yet.
pub fn build_player_habit_profile(
    games: &[PlayerGame],
    traces: &[MatchTrace],
) -> Option<PlayerHabitProfile> {
    let rift: Vec<&PlayerGame> = games
        .iter()
        .filter(|g| g.on_summoners_rift() && !g.is_remake())
        .take(20)
        .collect();

    let mut profile = PlayerHabitProfile {
        sample_games: rift.len() as u32,
        ..PlayerHabitProfile::default()
    };

    let mut w_deaths = 0.0f32;
    let mut w_cs = 0.0f32;
    let mut w_vis = 0.0f32;
    let mut w_kp_sum = 0.0f32;
    let mut w_kp_n = 0u32;

    let mut l_deaths = 0.0f32;
    let mut l_cs = 0.0f32;
    let mut l_vis = 0.0f32;
    let mut l_kp_sum = 0.0f32;
    let mut l_kp_n = 0u32;

    for g in &rift {
        let mins = (g.duration_secs as f32 / 60.0).max(1.0);
        if g.win {
            profile.wins += 1;
            w_deaths += g.deaths as f32;
            w_cs += g.cs as f32 / mins;
            w_vis += g.vision_score as f32 / mins;
            if let Some(kp) = g.kill_participation {
                w_kp_sum += kp;
                w_kp_n += 1;
            }
        } else {
            profile.losses += 1;
            l_deaths += g.deaths as f32;
            l_cs += g.cs as f32 / mins;
            l_vis += g.vision_score as f32 / mins;
            if let Some(kp) = g.kill_participation {
                l_kp_sum += kp;
                l_kp_n += 1;
            }
        }
    }

    if profile.wins >= 2 && profile.losses >= 2 {
        let wn = profile.wins as f32;
        let ln = profile.losses as f32;
        profile.win_avg_deaths = Some(w_deaths / wn);
        profile.loss_avg_deaths = Some(l_deaths / ln);
        profile.win_avg_cs_per_min = Some(w_cs / wn);
        profile.loss_avg_cs_per_min = Some(l_cs / ln);
        profile.win_avg_vision_per_min = Some(w_vis / wn);
        profile.loss_avg_vision_per_min = Some(l_vis / ln);
        if w_kp_n >= 2 && l_kp_n >= 2 {
            profile.win_avg_kp = Some(w_kp_sum / w_kp_n as f32);
            profile.loss_avg_kp = Some(l_kp_sum / l_kp_n as f32);
        }
    }

    // Current win/loss streak (newest first) + session record within 6 hours of the latest game.
    if let Some(latest) = rift.first() {
        let first_win = latest.win;
        let mut streak = 0i32;
        for g in &rift {
            if g.win == first_win {
                streak += if first_win { 1 } else { -1 };
            } else {
                break;
            }
        }
        profile.current_streak = streak;

        const SESSION_WINDOW_MS: i64 = 6 * 3600 * 1000;
        let latest_ts = latest.started_at_ms;
        if latest_ts > 0 {
            for g in &rift {
                if g.started_at_ms > 0
                    && latest_ts.saturating_sub(g.started_at_ms) <= SESSION_WINDOW_MS
                {
                    if g.win {
                        profile.session_wins += 1;
                    } else {
                        profile.session_losses += 1;
                    }
                }
            }
        }
    }

    // Recurring trace patterns across the most recent tracked matches.
    let valid_traces: Vec<&MatchTrace> = traces
        .iter()
        .rev()
        .filter(|t| t.has_timeline_data())
        .take(15)
        .collect();
    profile.tracked_trace_games = valid_traces.len() as u32;
    for t in valid_traces {
        let has_pre_obj = t.objectives.iter().filter(|o| !o.by_ally).any(|obj| {
            t.deaths.iter().any(|d| {
                d.time_sec < obj.time_sec
                    && obj.time_sec - d.time_sec >= 5
                    && obj.time_sec - d.time_sec <= 45
            })
        });
        if has_pre_obj {
            profile.pre_objective_death_games += 1;
        }
        if t.deaths
            .iter()
            .any(|d| d.unspent_gold.is_some_and(|g| g >= 1500))
        {
            profile.high_gold_death_games += 1;
        }
        let late_deaths: Vec<u32> = t
            .deaths
            .iter()
            .map(|d| d.time_sec)
            .filter(|&ts| ts >= 720)
            .collect();
        if late_deaths
            .windows(2)
            .any(|w| w[1].saturating_sub(w[0]) <= 180)
        {
            profile.death_streak_games += 1;
        }
    }

    profile.has_signal().then_some(profile)
}

/// Classifies the tactical archetype of a match from verified signals.
fn classify_match_archetype(input: &ReviewInput<'_>, features: &ReviewFeatures) -> MatchArchetype {
    let cs_diff_15 = input.trace.and_then(|t| {
        t.samples
            .iter()
            .filter(|s| s.time_sec >= 780 && s.time_sec <= 1020)
            .min_by_key(|s| s.time_sec.abs_diff(900))
            .and_then(|s| s.lane_cs_diff)
    });
    let had_team_gold_lead = input
        .team_gold
        .is_some_and(|(ally, enemy)| ally as i64 - enemy as i64 >= 2500);
    let high_carry_impact = input.placement.is_some_and(|p| p <= 3)
        || (input.kill_participation.is_some_and(|kp| kp >= 0.62)
            && input.damage_share.is_some_and(|ds| ds >= 0.25)
            && input.deaths <= 5);
    let low_impact = input.deaths >= 8
        || (input.kill_participation.is_some_and(|kp| kp < 0.42)
            && input.deaths >= 5
            && input.position != Some(Position::Top));

    if features.win {
        if cs_diff_15.is_some_and(|d| d >= 15) {
            MatchArchetype::SnowballConversion
        } else if cs_diff_15.is_some_and(|d| d <= -15) {
            MatchArchetype::ComebackWin
        } else if low_impact {
            MatchArchetype::CarriedVictory
        } else {
            MatchArchetype::EvenBattle
        }
    } else if cs_diff_15.is_some_and(|d| d >= 15) || had_team_gold_lead {
        MatchArchetype::LeadThrown
    } else if high_carry_impact {
        MatchArchetype::CarryInDefeat
    } else if cs_diff_15.is_some_and(|d| d <= -15) {
        MatchArchetype::LaneDiffDefeat
    } else {
        MatchArchetype::EvenBattle
    }
}

/// Extracts a compact, high-signal, identity-free [`CoachContext`] from [`ReviewInput`].
fn build_coach_context(input: &ReviewInput<'_>, features: &ReviewFeatures) -> CoachContext {
    let checkpoints = input
        .trace
        .map(|t| select_phase_checkpoints(&t.samples))
        .unwrap_or_default();
    let deaths_log = input
        .trace
        .map(|t| t.deaths.iter().take(10).cloned().collect())
        .unwrap_or_default();
    let objectives_log = input
        .trace
        .map(|t| t.objectives.clone())
        .unwrap_or_default();
    let item_spikes = input
        .trace
        .map(|t| t.item_buys.iter().take(6).cloned().collect())
        .unwrap_or_default();

    let recommended_core = input
        .recommended_build
        .and_then(|b| {
            b.items
                .iter()
                .find(|blk| blk.stage == Some(ItemStage::Core))
                .map(|blk| blk.item_ids.clone())
        })
        .unwrap_or_default();

    let personal_baseline =
        input
            .personal_champ
            .filter(|p| p.games >= 3)
            .map(|p| PersonalBaseline {
                games: p.games,
                win_rate: p.wins as f32 / p.games.max(1) as f32,
                avg_kills: p.kills,
                avg_deaths: p.deaths,
                avg_assists: p.assists,
                avg_cs_per_min: p.cs_per_min,
                avg_dpm: p.dpm,
                avg_vision_per_min: p.vision_per_min,
            });

    CoachContext {
        queue: input.queue,
        archetype: classify_match_archetype(input, features),
        kills: input.kills,
        deaths: input.deaths,
        assists: input.assists,
        creep_score: input.creep_score,
        total_gold: input.total_gold,
        damage_to_champions: input.damage_to_champions,
        vision_score: input.vision_score,
        damage_share: input.damage_share,
        gold_share: input.gold_share,
        placement: input.placement,
        performance_score: input.performance_score,
        team_kills: input.team_kills,
        team_gold: input.team_gold,
        keystone_id: input.keystone_id,
        sub_style_id: input.sub_style_id,
        spell_ids: input.spell_ids,
        items: input.items.iter().copied().filter(|&id| id > 0).collect(),
        item_spikes,
        recommended_core,
        opponent_champion_id: input.opponent_champion_id.filter(|&id| id > 0),
        opponent_line: input.opponent_line.clone(),
        ally_champions: input
            .ally_champions
            .iter()
            .copied()
            .filter(|&id| id > 0)
            .collect(),
        enemy_champions: input
            .enemy_champions
            .iter()
            .copied()
            .filter(|&id| id > 0)
            .collect(),
        top_enemy_threat: input.top_enemy_threat.clone(),
        checkpoints,
        deaths_log,
        objectives_log,
        personal_baseline,
        habit_profile: input.habit_profile.cloned(),
    }
}

/// Compresses periodic trace samples into up to 4 canonical phase checkpoints (~10m, 15m, 20m, 25m).
fn select_phase_checkpoints(samples: &[TraceSample]) -> Vec<PhaseCheckpoint> {
    if samples.is_empty() {
        return Vec::new();
    }
    const TARGETS: [u32; 4] = [600, 900, 1200, 1500];
    const MAX_DELTA: u32 = 90;
    let mut out: Vec<PhaseCheckpoint> = Vec::with_capacity(TARGETS.len());
    for target in TARGETS {
        if let Some(best) = samples
            .iter()
            .filter(|s| s.time_sec.abs_diff(target) <= MAX_DELTA)
            .min_by_key(|s| s.time_sec.abs_diff(target))
        {
            if out.last().is_none_or(|prev| prev.time_sec != best.time_sec) {
                out.push(PhaseCheckpoint {
                    time_sec: best.time_sec,
                    level: best.level,
                    cs: best.cs,
                    lane_cs_diff: best.lane_cs_diff,
                    kills: best.kills,
                    deaths: best.deaths,
                    assists: best.assists,
                    current_gold: best.current_gold,
                });
            }
        }
    }
    out
}

fn queue_label(queue: StatsQueue, lang: Lang) -> &'static str {
    match queue {
        StatsQueue::RankedSolo => lang.pick("Ranked Solo/Duo", "Classée Solo/Duo"),
        StatsQueue::RankedFlex => lang.pick("Ranked Flex", "Classée Flex"),
        StatsQueue::Normal => lang.pick("Normal Draft", "Normale"),
        StatsQueue::Aram => "ARAM",
        StatsQueue::Arena => "Arena",
    }
}

fn comp_trait_label(trait_: CompTrait, lang: Lang) -> &'static str {
    match trait_ {
        CompTrait::Engage => lang.pick("Engage", "Engage"),
        CompTrait::Peel => lang.pick("Peel", "Peel"),
        CompTrait::Poke => lang.pick("Poke", "Poke"),
        CompTrait::Splitpush => lang.pick("Splitpush", "Splitpush"),
        CompTrait::Teamfight => lang.pick("Teamfight", "Teamfight"),
        CompTrait::Pick => lang.pick("Pick", "Pick"),
        CompTrait::Scaling => lang.pick("Scaling", "Scaling"),
        CompTrait::EarlyGame => lang.pick("Early tempo", "Tempo early"),
        CompTrait::Frontline => lang.pick("Frontline", "Frontline"),
        CompTrait::LacksFrontline => lang.pick("No frontline", "Sans frontline"),
        CompTrait::LacksEngage => lang.pick("No engage", "Sans engage"),
        CompTrait::AllPhysical => lang.pick("Full AD", "100% AD"),
        CompTrait::AllMagic => lang.pick("Full AP", "100% AP"),
    }
}

fn role_coaching_lens(position: Option<Position>, lang: Lang) -> &'static str {
    match position {
        Some(Position::Top) => lang.pick(
            "Role lens (Top): weigh 1v1 wave states, recall/TP timings, side-lane pressure, and frontline/flank value in objective fights.",
            "Angle du rôle (Top) : évalue la gestion de vague en 1v1, les timings de back/TP, la pression side-lane et la valeur frontline/flank sur les objectifs.",
        ),
        Some(Position::Jungle) => lang.pick(
            "Role lens (Jungle): weigh clear/tempo efficiency, objective setup timing, gank conversion, and deaths before Smite fights.",
            "Angle du rôle (Jungle) : évalue le tempo de clear, la préparation des objectifs, la conversion des ganks et les morts avant les combats au Smite.",
        ),
        Some(Position::Mid) => lang.pick(
            "Role lens (Mid): weigh lane priority, roam/river windows around Grubs/Dragon, pick angles, and item spike resets.",
            "Angle du rôle (Mid) : évalue la priorité de lane, les fenêtres de roam/rivière autour des Larves/Dragons, les angles de pick et les backs sur pics d'objets.",
        ),
        Some(Position::Bot) => lang.pick(
            "Role lens (Bot/ADC): weigh CS/gold income, item power-spike timings, positioning/deaths before fights, and sustained DPS uptime.",
            "Angle du rôle (Bot/ADC) : évalue les revenus CS/PO, les timings de pics d'objets, le placement/morts avant les combats et la constance du DPS.",
        ),
        Some(Position::Support) => lang.pick(
            "Role lens (Support): weigh vision control before spawns, roam timings, kill participation, and engage or peel around your carries.",
            "Angle du rôle (Support) : évalue le contrôle de vision avant les spawns, les timings de roam, la participation aux éliminations et l'engage ou le peel pour tes carries.",
        ),
        Some(Position::Fill) | None => lang.pick(
            "Role lens: weigh lane/mid-game tempo, objective setup discipline, build adaptation, and fight execution.",
            "Angle du rôle : évalue le tempo lane/mid-game, la discipline autour des objectifs, l'adaptation du build et l'exécution des combats.",
        ),
    }
}

fn format_team_line(champ_ids: &[i64], lang: Lang) -> String {
    let names: Vec<String> = champ_ids
        .iter()
        .filter(|&&id| id > 0)
        .map(|&id| game_name_or(GameTextKind::Champion, id, lang, "Champion"))
        .collect();
    if names.is_empty() {
        return String::new();
    }
    let mut line = names.join(", ");
    if champ_ids.len() >= 3 {
        let profile = team_profile(champ_ids);
        let phys = (profile.physical * 100.0).round() as u32;
        let mag = (profile.magic * 100.0).round() as u32;
        let traits: Vec<&str> = profile
            .traits
            .iter()
            .map(|&t| comp_trait_label(t, lang))
            .collect();
        if traits.is_empty() {
            line.push_str(&format!(" ({phys}% AD / {mag}% AP)"));
        } else {
            line.push_str(&format!(" ({phys}% AD / {mag}% AP; {})", traits.join(", ")));
        }
    }
    line
}

/// Anonymized system + user messages sent to an opt-in Layer 2 LLM coach.
///
/// Built strictly from [`GameReview`], so it can never carry a summoner name, PUUID, or secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoachPrompt {
    pub system: String,
    pub user: String,
}

fn focus_instructions(focus: CoachFocus, lang: Lang) -> &'static str {
    match (focus, lang) {
        (CoachFocus::Overview, Lang::En) => {
            "OUTPUT FORMAT (plain text only, NO Markdown, NO **bold**, NO # headings, ~150-220 words total):\n\
             VERDICT\n\
             2-3 sentences explaining the true story of the game: the 5v5/matchup win condition, how the trajectory played out, and the main swing.\n\
             \n\
             KEY LEVERS\n\
             • 2-3 concise bullet points (starting with '• ') linking verified numbers/timings/items to concrete champion, matchup, or macro reasoning.\n\
             \n\
             NEXT GAME DRILL\n\
             1 concrete, measurable habit or in-game trigger to apply in the very next match."
        }
        (CoachFocus::Overview, Lang::Fr) => {
            "FORMAT DE SORTIE (texte brut uniquement, AUCUN Markdown, pas de **gras**, pas de #, environ 150 à 220 mots au total) :\n\
             DIAGNOSTIC\n\
             2 à 3 phrases expliquant la vraie histoire de la partie : condition de victoire (matchup/compo), déroulé et tournant principal.\n\
             \n\
             LEVIERS CLÉS\n\
             • 2 à 3 puces concises (commençant par « • ») reliant les chiffres/timings/objets vérifiés à un raisonnement précis de champion, matchup ou macro.\n\
             \n\
             EXERCICE PROCHAINE GAME\n\
             1 habitude concrète et mesurable (ou déclencheur précis en partie) à appliquer dès la prochaine game."
        }
        (CoachFocus::Matchup, Lang::En) => {
            "LENS: DEEP LANE & MATCHUP BREAKDOWN.\n\
             Focus heavily on the 1v1/2v2 lane matchup: champion kits, cooldown trading windows, wave states (push/freeze/crash), level 2/3/6 spikes, rune/spell setup, and the 10m/15m CS & gold trajectory.\n\
             OUTPUT FORMAT (plain text only, NO Markdown, ~150-220 words):\n\
             MATCHUP DYNAMIC\n\
             2 sentences on who controls the lane and how the matchup should be played.\n\
             \n\
             LANE EXECUTION\n\
             • 2-3 concise bullets ('• ') analyzing trading windows, wave/recall discipline, and spell/keystone value from the verified match facts.\n\
             \n\
             NEXT TIME IN THIS MATCHUP\n\
             1 specific lane rule or spike timing to play around."
        }
        (CoachFocus::Matchup, Lang::Fr) => {
            "ANGLE : ANALYSE APPROFONDIE LANE & MATCHUP.\n\
             Concentre-toi sur le 1v1/2v2 : kits des champions, fenêtres de trade sur cooldowns, états de vague (push/freeze/crash), pics niveau 2/3/6, choix runes/sorts et trajectoire CS/PO à 10-15 min.\n\
             FORMAT DE SORTIE (texte brut uniquement, AUCUN Markdown, ~150-220 mots) :\n\
             DYNAMIQUE DU MATCHUP\n\
             2 phrases sur qui dicte la lane et comment le matchup doit se jouer.\n\
             \n\
             EXÉCUTION EN LANE\n\
             • 2 à 3 puces concises (« • ») analysant les fenêtres de trade, la gestion de vague/backs et l'impact des runes/sorts à partir des faits vérifiés.\n\
             \n\
             PROCHAINE FOIS DANS CE MATCHUP\n\
             1 règle de lane ou pic de puissance précis à exploiter."
        }
        (CoachFocus::Macro, Lang::En) => {
            "LENS: 5V5 MACRO, WIN CONDITIONS & TEMPO.\n\
             Focus heavily on how the two 5v5 comps want to win (engage/pick/poke/scaling/sustain), objective setup timers (45s resets), unspent-gold risk, death timing cost, and cross-game recurring macro leaks.\n\
             OUTPUT FORMAT (plain text only, NO Markdown, ~150-220 words):\n\
             WIN CONDITION\n\
             2 sentences comparing ally vs enemy 5v5 win conditions and who held tempo.\n\
             \n\
             TEMPO & OBJECTIVES\n\
             • 2-3 concise bullets ('• ') connecting objective timers, recalls, deaths, and map rotations from verified facts.\n\
             \n\
             MACRO RULE\n\
             1 concrete mid-game timer or map rule to apply next game."
        }
        (CoachFocus::Macro, Lang::Fr) => {
            "ANGLE : MACRO 5V5, CONDITIONS DE VICTOIRE & TEMPO.\n\
             Concentre-toi sur la façon dont les deux compos 5v5 gagnent (engage/pick/poke/scaling/sustain), les timers de préparation d'objectifs (backs 45s avant), le risque des PO en poche, le coût des morts et les fuites macro récurrentes.\n\
             FORMAT DE SORTIE (texte brut uniquement, AUCUN Markdown, ~150-220 mots) :\n\
             CONDITION DE VICTOIRE\n\
             2 phrases comparant la condition de victoire alliée vs ennemie et le contrôle du tempo.\n\
             \n\
             TEMPO & OBJECTIFS\n\
             • 2 à 3 puces concises (« • ») reliant timers d'objectifs, backs, morts et rotations à partir des faits vérifiés.\n\
             \n\
             RÈGLE MACRO\n\
             1 règle concrète de timer ou de rotation mid-game à appliquer dès la prochaine partie."
        }
        (CoachFocus::Build, Lang::En) => {
            "LENS: BUILD, RUNES & POWER-SPIKE AUDIT.\n\
             Audit the player's keystone, summoner spells, final items, and item completion timings against the exact 5 enemy champions (healing/sustain, armor/MR stacking, burst, CC, fed enemy threats). Name concrete situational alternatives when relevant.\n\
             OUTPUT FORMAT (plain text only, NO Markdown, ~150-220 words):\n\
             BUILD VERDICT\n\
             2 sentences evaluating how well the setup and item spikes matched the enemy comp.\n\
             \n\
             ITEM & RUNE ADJUSTMENTS\n\
             • 2-3 concise bullets ('• ') citing verified items/timings and concrete counter-items or spike optimizations.\n\
             \n\
             BUILD CHECKPOINT\n\
             1 quick rule to check on Tab before buying your 2nd/3rd item next game."
        }
        (CoachFocus::Build, Lang::Fr) => {
            "ANGLE : AUDIT BUILD, RUNES & PICS D'OBJETS.\n\
             Audite la rune fondamentale, les sorts d'invocateur, les objets finaux et leurs timings d'achat face aux 5 champions ennemis exacts (soins/sustain, armure/RM, burst, CC, carry ennemi fed). Nomme des alternatives situationnelles concrètes si pertinent.\n\
             FORMAT DE SORTIE (texte brut uniquement, AUCUN Markdown, ~150-220 mots) :\n\
             VERDICT DU BUILD\n\
             2 phrases évaluant l'adéquation du setup et des pics d'objets face à la compo ennemie.\n\
             \n\
             AJUSTEMENTS OBJETS & RUNES\n\
             • 2 à 3 puces concises (« • ») citant les objets/timings vérifiés et des counter-items ou optimisations précises.\n\
             \n\
             RÉFLEXE BUILD\n\
             1 vérification rapide sur Tab avant d'acheter ton 2e/3e objet la prochaine fois."
        }
    }
}

/// Builds a high-signal, fact-grounded coaching prompt for the default [`CoachFocus::Overview`].
pub fn build_coach_prompt(review: &GameReview, lang: Lang) -> CoachPrompt {
    build_coach_prompt_with_focus(review, CoachFocus::Overview, lang)
}

/// Builds a high-signal, fact-grounded coaching prompt tailored to a specific [`CoachFocus`] lens.
///
/// Separates verified match facts + longitudinal player habit memory (never fabricated) from
/// League of Legends domain expertise (champion kits, lane matchups, 5v5 win conditions, item
/// spikes, macro tempo) so the LLM delivers concrete, causal coaching.
pub fn build_coach_prompt_with_focus(
    review: &GameReview,
    focus: CoachFocus,
    lang: Lang,
) -> CoachPrompt {
    let role_lens = role_coaching_lens(review.position, lang);
    let output_spec = focus_instructions(focus, lang);
    let system = match lang {
        Lang::En => format!(
            "You are an elite, constructive League of Legends coach debriefing one Summoner's Rift match directly to the player.\n\
             Write in English. Be specific, analytical, and encouraging—never give generic advice (like 'farm better' or 'ward more') without tying it to this exact champion, matchup, build, or timing.\n\
             \n\
             RULES:\n\
             1. Verified facts only: every stat, item, timestamp, KDA, objective, and champion you cite MUST come from the match dossier below. Never invent timestamps, plays, or items. If coverage is 'Post-game only', do not invent timestamps or guess when deaths happened.\n\
             2. Use your League of Legends expertise: actively interpret the verified facts using your knowledge of champion kits, lane matchup dynamics, 5v5 comp win conditions, rune/spell/item synergies, power spikes, and objective tempo. Connect causes to effects rather than just repeating the automated findings.\n\
             3. Longitudinal memory & tone: respect the match archetype framing (e.g., validate a Carry in Defeat; warn constructively on a Shaky Win) and, when cross-game habit memory or session streak is present, call out recurring patterns and calibrate tone to their momentum.\n\
             4. {role_lens}\n\
             \n\
             {output_spec}"
        ),
        Lang::Fr => format!(
            "Tu es un coach League of Legends de haut niveau, analytique et constructif, qui débriefe une partie de la Faille de l'invocateur directement au joueur (tutoie-le).\n\
             Écris en français. Sois précis et concret : ne donne jamais de conseil générique (« farm mieux », « pose plus de wards ») sans le relier précisément à ce champion, ce matchup, ce build ou ce timing.\n\
             \n\
             RÈGLES :\n\
             1. Faits vérifiés uniquement : chaque stat, objet, timestamp, KDA, objectif ou champion cité DOIT provenir du dossier de partie ci-dessous. N'invente jamais de timestamps, d'actions ou d'objets. Si la couverture est « Après-partie », n'invente aucun timestamp ni moment de mort.\n\
             2. Exploite ton expertise League of Legends : interprète activement les faits avec ta connaissance des kits de champions, de la dynamique du matchup, des conditions de victoire 5v5, des synergies runes/sorts/objets, des pics de puissance et du tempo d'objectifs. Relie les causes aux effets au lieu de simplement reformuler les constats automatiques.\n\
             3. Mémoire multi-parties & ton : respecte l'archétype du match (ex. valide un Carry dans la défaite ; alerte avec tact sur une Victoire fragile) et, si la mémoire d'habitudes multi-parties ou la série de session est présente, pointe les schémas récurrents et adapte le ton au momentum.\n\
             4. {role_lens}\n\
             \n\
             {output_spec}"
        ),
    };

    let ctx = &review.context;
    let champ = game_name_or(GameTextKind::Champion, review.champion_id, lang, "Champion");
    let role = review
        .position
        .map(|p| p.display_label(lang))
        .unwrap_or_else(|| lang.pick("Unknown", "Inconnu"));
    let result = if review.features.win {
        lang.pick("Victory", "Victoire")
    } else {
        lang.pick("Defeat", "Défaite")
    };
    let minutes = (review.features.duration_sec as f32 / 60.0).max(1.0);

    let mut user = String::with_capacity(1800);

    // 1. Match header & matchup
    user.push_str("=== MATCH ===\n");
    let champ_profile = champion_traits(review.champion_id)
        .map(|t| {
            let tags: Vec<&str> = t
                .traits()
                .into_iter()
                .map(|tr| comp_trait_label(tr, lang))
                .collect();
            if tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", tags.join(", "))
            }
        })
        .unwrap_or_default();

    let queue_str = ctx
        .queue
        .map(|q| format!(" | {}", queue_label(q, lang)))
        .unwrap_or_default();
    user.push_str(&format!(
        "Champion: {champ} ({role}){champ_profile}\nResult: {result} ({}){queue_str}\nArchetype: {} — {}\n",
        format_mm_ss(review.features.duration_sec),
        ctx.archetype.label(lang),
        ctx.archetype.coach_framing(lang),
    ));

    if let Some(opp_id) = ctx.opponent_champion_id {
        let opp_name = game_name_or(GameTextKind::Champion, opp_id, lang, "Opponent");
        if let Some(ref opp) = ctx.opponent_line {
            let gold_diff = ctx.total_gold as i64 - opp.gold as i64;
            let sign = if gold_diff >= 0 { "+" } else { "" };
            user.push_str(&format!(
                "Lane opponent: {opp_name} ({}/{}/{}, {} CS, {:.1}k gold; your gold diff: {sign}{gold_diff})\n",
                opp.kills,
                opp.deaths,
                opp.assists,
                opp.cs,
                opp.gold as f32 / 1000.0,
            ));
        } else {
            user.push_str(&format!("Lane opponent: {opp_name}\n"));
        }
    }

    let coverage_note = match review.coverage {
        ReviewCoverage::PostGameOnly => lang.pick(
            "Post-game only (no live timestamps available—do not invent timings)",
            "Après-partie uniquement (aucun timestamp live disponible—n'invente aucun timing)",
        ),
        _ => review.coverage.label(lang),
    };
    user.push_str(&format!("Data coverage: {coverage_note}\n"));

    // 2. Setup & Build
    let keystone = ctx
        .keystone_id
        .and_then(|id| game_name(GameTextKind::Perk, id, lang));
    let sub_tree = ctx
        .sub_style_id
        .and_then(|id| game_name(GameTextKind::PerkStyle, id, lang));
    let spells: Vec<&str> = ctx
        .spell_ids
        .iter()
        .filter(|&&id| id > 0)
        .filter_map(|&id| game_name(GameTextKind::Spell, id, lang))
        .collect();
    let item_names: Vec<&str> = ctx
        .items
        .iter()
        .filter_map(|&id| game_name(GameTextKind::Item, id, lang))
        .collect();
    let rec_names: Vec<&str> = ctx
        .recommended_core
        .iter()
        .filter_map(|&id| game_name(GameTextKind::Item, id, lang))
        .collect();

    if keystone.is_some()
        || !spells.is_empty()
        || !item_names.is_empty()
        || !ctx.item_spikes.is_empty()
    {
        user.push_str("\n=== SETUP & BUILD ===\n");
        if keystone.is_some() || !spells.is_empty() {
            let mut parts = Vec::new();
            if let Some(k) = keystone {
                if let Some(sub) = sub_tree {
                    parts.push(format!("Runes: {k} / {sub}"));
                } else {
                    parts.push(format!("Keystone: {k}"));
                }
            }
            if !spells.is_empty() {
                parts.push(format!("Spells: {}", spells.join(" + ")));
            }
            user.push_str(&format!("{}\n", parts.join(" | ")));
        }
        if !item_names.is_empty() {
            user.push_str(&format!("Final items: {}\n", item_names.join(", ")));
        }
        if !ctx.item_spikes.is_empty() {
            let spikes: Vec<String> = ctx
                .item_spikes
                .iter()
                .filter_map(|b| {
                    game_name(GameTextKind::Item, b.item_id, lang)
                        .map(|name| format!("{} {name}", format_mm_ss(b.time_sec)))
                })
                .collect();
            if !spikes.is_empty() {
                user.push_str(&format!("Item spike timings: {}\n", spikes.join(", ")));
            }
        }
        if !rec_names.is_empty() {
            user.push_str(&format!("Recommended core: {}\n", rec_names.join(", ")));
        }
    }

    // 3. Performance & Team Context
    user.push_str("\n=== PERFORMANCE ===\n");
    let has_kda = ctx.kills > 0 || ctx.deaths > 0 || ctx.assists > 0 || ctx.total_gold > 0;
    if has_kda {
        let kda_ratio = (ctx.kills + ctx.assists) as f32 / ctx.deaths.max(1) as f32;
        let gpm = ctx.total_gold as f32 / minutes;
        let dpm = ctx.damage_to_champions as f32 / minutes;
        user.push_str(&format!(
            "KDA: {}/{}/{} ({kda_ratio:.1}) | CS: {} ({:.1}/min) | Gold: {:.1}k ({gpm:.0}/min) | Damage: {:.1}k ({dpm:.0} DPM)\n",
            ctx.kills,
            ctx.deaths,
            ctx.assists,
            ctx.creep_score,
            review.features.cs_per_min,
            ctx.total_gold as f32 / 1000.0,
            ctx.damage_to_champions as f32 / 1000.0,
        ));
    } else {
        user.push_str(&format!(
            "CS/min: {:.1} | Vision/min: {:.2} | Deaths: {}\n",
            review.features.cs_per_min, review.features.vision_per_min, review.features.deaths,
        ));
    }

    let mut impact_parts: Vec<String> = Vec::new();
    if let Some(kp) = review.features.kill_participation {
        impact_parts.push(format!("KP: {}%", (kp * 100.0).round() as u32));
    }
    if let Some(ds) = ctx.damage_share {
        impact_parts.push(format!("Damage share: {}%", (ds * 100.0).round() as u32));
    }
    if let Some(gs) = ctx.gold_share {
        impact_parts.push(format!("Gold share: {}%", (gs * 100.0).round() as u32));
    }
    if ctx.vision_score > 0 || review.features.vision_per_min > 0.0 {
        impact_parts.push(format!(
            "Vision: {} ({:.2}/min)",
            ctx.vision_score, review.features.vision_per_min
        ));
    }
    if let Some(pl) = ctx.placement {
        impact_parts.push(format!("Lobby rank: #{pl}/10"));
    }
    if let Some(score) = ctx.performance_score {
        impact_parts.push(format!("Score: {score}/100"));
    }
    if !impact_parts.is_empty() {
        user.push_str(&format!("{}\n", impact_parts.join(" | ")));
    }

    if ctx.team_kills.is_some() || ctx.team_gold.is_some() {
        let mut team_parts = Vec::new();
        if let Some((ak, ek)) = ctx.team_kills {
            team_parts.push(format!("Team kills: {ak}-{ek}"));
        }
        if let Some((ag, eg)) = ctx.team_gold {
            let diff = ag as i64 - eg as i64;
            let sign = if diff >= 0 { "+" } else { "" };
            team_parts.push(format!(
                "Team gold: {:.1}k vs {:.1}k ({sign}{:.1}k)",
                ag as f32 / 1000.0,
                eg as f32 / 1000.0,
                diff as f32 / 1000.0,
            ));
        }
        user.push_str(&format!("{}\n", team_parts.join(" | ")));
    }

    // 4. Personal baseline
    if let Some(ref base) = ctx.personal_baseline {
        let cur_dpm = ctx.damage_to_champions as f32 / minutes;
        let cs_delta = review.features.cs_per_min - base.avg_cs_per_min;
        let death_delta = review.features.deaths as f32 - base.avg_deaths;
        let dpm_delta = cur_dpm - base.avg_dpm;
        user.push_str(&format!(
            "\n=== PERSONAL BASELINE ({} games on {champ}, {}% WR) ===\n\
             Usual avg: {:.1}/{:.1}/{:.1} KDA | {:.1} CS/min | {:.0} DPM | {:.2} vis/min\n\
             This game vs usual: {:+.1} CS/min, {:+.1} deaths, {:+.0} DPM\n",
            base.games,
            (base.win_rate * 100.0).round() as u32,
            base.avg_kills,
            base.avg_deaths,
            base.avg_assists,
            base.avg_cs_per_min,
            base.avg_dpm,
            base.avg_vision_per_min,
            cs_delta,
            death_delta,
            dpm_delta,
        ));
    }

    // 5. 5v5 Compositions & Threats
    if ctx.ally_champions.len() >= 3
        || ctx.enemy_champions.len() >= 3
        || ctx.top_enemy_threat.is_some()
    {
        user.push_str("\n=== COMPOSITIONS & THREATS ===\n");
        if ctx.ally_champions.len() >= 3 {
            user.push_str(&format!(
                "Ally comp: {}\n",
                format_team_line(&ctx.ally_champions, lang)
            ));
        }
        if ctx.enemy_champions.len() >= 3 {
            user.push_str(&format!(
                "Enemy comp: {}\n",
                format_team_line(&ctx.enemy_champions, lang)
            ));
            let healers: Vec<String> = ctx
                .enemy_champions
                .iter()
                .filter(|id| HEAVY_HEAL_CHAMPIONS.contains(id))
                .map(|&id| game_name_or(GameTextKind::Champion, id, lang, "Champion"))
                .collect();
            if healers.len() >= 2 {
                user.push_str(&format!("Enemy sustain threats: {}\n", healers.join(", ")));
            }
        }
        if let Some(ref threat) = ctx.top_enemy_threat {
            let tname = game_name_or(GameTextKind::Champion, threat.champion_id, lang, "Enemy");
            user.push_str(&format!(
                "Top enemy threat: {tname} ({}/{}/{}, {:.1}k dmg)\n",
                threat.kills,
                threat.deaths,
                threat.assists,
                threat.damage as f32 / 1000.0,
            ));
        }
    }

    // 6. Phase Trajectory
    if !ctx.checkpoints.is_empty() {
        user.push_str("\n=== PHASE TRAJECTORY ===\n");
        for cp in &ctx.checkpoints {
            let diff_str = cp
                .lane_cs_diff
                .map(|d| format!(" ({d:+} vs lane)"))
                .unwrap_or_default();
            user.push_str(&format!(
                "- {} | Lvl {}, {} CS{diff_str}, {}/{}/{}, {}g unspent\n",
                format_mm_ss(cp.time_sec),
                cp.level,
                cp.cs,
                cp.kills,
                cp.deaths,
                cp.assists,
                cp.current_gold,
            ));
        }
    }

    // 7. Deaths & Objectives Ledger
    if !ctx.deaths_log.is_empty() || !ctx.objectives_log.is_empty() {
        user.push_str("\n=== DEATHS & OBJECTIVES ===\n");
        if !ctx.deaths_log.is_empty() {
            let entries: Vec<String> = ctx
                .deaths_log
                .iter()
                .map(|d| match d.unspent_gold {
                    Some(g) if g > 0 => format!("{} ({}g unspent)", format_mm_ss(d.time_sec), g),
                    _ => format_mm_ss(d.time_sec),
                })
                .collect();
            user.push_str(&format!("Deaths: {}\n", entries.join(", ")));
        }
        if !ctx.objectives_log.is_empty() {
            let entries: Vec<String> = ctx
                .objectives_log
                .iter()
                .map(|o| {
                    let side = if o.by_ally { "Ally" } else { "Enemy" };
                    let inv = if o.player_involved {
                        "involved"
                    } else {
                        "absent"
                    };
                    format!(
                        "{} {} ({side}, {inv})",
                        format_mm_ss(o.time_sec),
                        o.kind.label(lang)
                    )
                })
                .collect();
            user.push_str(&format!("Objectives: {}\n", entries.join(" | ")));
        }
    }

    // 8. Layer 1 Automated Signals
    if !review.good_choices.is_empty()
        || !review.improvements.is_empty()
        || !review.turning_points.is_empty()
    {
        user.push_str("\n=== LAYER 1 VERIFIED SIGNALS ===\n");
        if !review.good_choices.is_empty() {
            user.push_str("Good choices:\n");
            for f in &review.good_choices {
                let ts = f
                    .timestamp_sec
                    .map(|s| format!(" [{}]", format_mm_ss(s)))
                    .unwrap_or_default();
                user.push_str(&format!(
                    "- [{}]{ts} {}: {}\n",
                    f.pillar.label(lang),
                    f.title,
                    f.detail
                ));
            }
        }
        if !review.improvements.is_empty() {
            user.push_str("To improve:\n");
            for f in &review.improvements {
                let ts = f
                    .timestamp_sec
                    .map(|s| format!(" [{}]", format_mm_ss(s)))
                    .unwrap_or_default();
                user.push_str(&format!(
                    "- [{}]{ts} {}: {}\n",
                    f.pillar.label(lang),
                    f.title,
                    f.detail
                ));
            }
        }
        if !review.turning_points.is_empty() {
            user.push_str("Turning points:\n");
            for tp in &review.turning_points {
                user.push_str(&format!(
                    "- [{}] ({}) {}\n",
                    format_mm_ss(tp.time_sec),
                    tp.pillar.label(lang),
                    tp.label
                ));
            }
        }
    }

    // 9. Cross-Game Habit Memory (longitudinal Win/Loss fingerprint + recurring trace patterns)
    if let Some(ref habit) = ctx.habit_profile {
        if habit.has_signal() {
            user.push_str(&format!(
                "\n=== CROSS-GAME HABIT MEMORY (last {} Rift games: {}W-{}L) ===\n",
                habit.sample_games, habit.wins, habit.losses,
            ));
            if let (Some(wd), Some(ld), Some(wcs), Some(lcs)) = (
                habit.win_avg_deaths,
                habit.loss_avg_deaths,
                habit.win_avg_cs_per_min,
                habit.loss_avg_cs_per_min,
            ) {
                let kp_part = match (habit.win_avg_kp, habit.loss_avg_kp) {
                    (Some(wkp), Some(lkp)) => format!(
                        " | KP {}% vs {}%",
                        (wkp * 100.0).round() as u32,
                        (lkp * 100.0).round() as u32
                    ),
                    _ => String::new(),
                };
                user.push_str(&format!(
                    "Win vs Loss fingerprint: {wd:.1} vs {ld:.1} deaths | {wcs:.1} vs {lcs:.1} CS/min{kp_part}\n"
                ));
            }
            if habit.tracked_trace_games >= 2 {
                user.push_str(&format!(
                    "Recurring trace leaks ({} tracked games): pre-objective deaths {}/{}, high unspent-gold deaths {}/{}, death streaks {}/{}\n",
                    habit.tracked_trace_games,
                    habit.pre_objective_death_games,
                    habit.tracked_trace_games,
                    habit.high_gold_death_games,
                    habit.tracked_trace_games,
                    habit.death_streak_games,
                    habit.tracked_trace_games,
                ));
            }
            if habit.session_wins + habit.session_losses >= 2 {
                let streak_str = if habit.current_streak >= 2 {
                    format!(", {}-win streak", habit.current_streak)
                } else if habit.current_streak <= -2 {
                    format!(", {}-loss streak", -habit.current_streak)
                } else {
                    String::new()
                };
                user.push_str(&format!(
                    "Current session momentum: {}W-{}L{streak_str}\n",
                    habit.session_wins, habit.session_losses,
                ));
            }
            if let Some(pattern) = habit.primary_pattern(lang) {
                user.push_str(&format!("Primary longitudinal pattern: {pattern}\n"));
            }
        }
    }

    CoachPrompt { system, user }
}

/// Generates up to 3 high-signal, match-specific follow-up questions the player can click
/// directly inside the Coach panel to start a grounded coaching dialogue.
pub fn suggested_coach_questions(review: &GameReview, lang: Lang) -> Vec<String> {
    let ctx = &review.context;
    let champ = game_name_or(GameTextKind::Champion, review.champion_id, lang, "Champion");
    let mut out: Vec<String> = Vec::with_capacity(3);

    // 1. Matchup question when lane opponent is known.
    if let Some(opp_id) = ctx.opponent_champion_id {
        let opp = game_name_or(GameTextKind::Champion, opp_id, lang, "Opponent");
        out.push(
            lang.pick(
                "How should I play {champ} vs {opp} in lane?",
                "Comment jouer {champ} vs {opp} en lane ?",
            )
            .replace("{champ}", &champ)
            .replace("{opp}", &opp),
        );
    }

    // 2. Recurring habit or pre-objective/unspent-gold question when detected.
    if review.features.pre_objective_deaths > 0 || review.features.high_gold_deaths > 0 {
        out.push(
            lang.pick(
                "When should I have reset before objectives?",
                "Quand aurais-je dû back avant les objectifs ?",
            )
            .to_string(),
        );
    } else if ctx
        .habit_profile
        .as_ref()
        .and_then(|h| h.primary_pattern(lang))
        .is_some()
    {
        out.push(
            lang.pick(
                "How do I break my recurring loss pattern?",
                "Comment corriger mon schéma de défaite récurrent ?",
            )
            .to_string(),
        );
    }

    // 3. Build or 5v5 win condition question.
    if review
        .improvements
        .iter()
        .any(|f| f.pillar == ReviewPillar::Build)
    {
        out.push(
            lang.pick(
                "How should I adapt my build vs this comp?",
                "Comment adapter mon build face à cette compo ?",
            )
            .to_string(),
        );
    } else if ctx.ally_champions.len() >= 3 && ctx.enemy_champions.len() >= 3 {
        out.push(
            lang.pick(
                "What was our exact 5v5 win condition?",
                "Quelle était notre condition de victoire 5v5 ?",
            )
            .to_string(),
        );
    }

    // Fill to 3 with a concrete champion macro fallback if needed.
    if out.len() < 3 {
        out.push(
            lang.pick(
                "What is the #1 mid-game habit to fix on {champ}?",
                "Quel est le réflexe mid-game n°1 à corriger sur {champ} ?",
            )
            .replace("{champ}", &champ),
        );
    }
    out.truncate(3);
    out
}

/// Builds a multi-turn OpenAI-compatible message list for an interactive follow-up question
/// grounded in the exact match dossier, longitudinal habit memory, initial debrief, and up to
/// the last 6 prior Q&A turns.
pub fn build_followup_messages(
    review: &GameReview,
    focus: CoachFocus,
    initial_debrief: &str,
    history: &[CoachTurn],
    question: &str,
    lang: Lang,
) -> Vec<CoachMessage> {
    let base = build_coach_prompt_with_focus(review, focus, lang);
    let followup_system = match lang {
        Lang::En => format!(
            "{}\n\n\
             FOLLOW-UP DIALOGUE MODE:\n\
             The player is asking a direct follow-up question about this exact match or their recurring habits. \
             Answer concisely (70-130 words, plain text only, NO Markdown, NO **bold**, NO # headings). \
             Stay strictly grounded in the verified match dossier for facts/timings, while actively using your deep League of Legends expertise (champion kits, wave states, matchups, itemization, macro) to give a concrete, actionable answer.",
            base.system
        ),
        Lang::Fr => format!(
            "{}\n\n\
             MODE DIALOGUE DE SUIVI :\n\
             Le joueur te pose une question directe sur cette partie précise ou sur ses habitudes récurrentes. \
             Réponds de façon concise (70 à 130 mots, texte brut uniquement, AUCUN Markdown, pas de **gras**, pas de #). \
             Reste strictement fidèle aux faits vérifiés du dossier pour les stats/timings, tout en exploitant pleinement ton expertise League of Legends (kits, gestion de vague, matchups, itemisation, macro) pour donner une réponse concrète et actionnable.",
            base.system
        ),
    };

    let mut msgs = Vec::with_capacity(4 + history.len().min(6) * 2);
    msgs.push(CoachMessage {
        role: "system",
        content: followup_system,
    });
    msgs.push(CoachMessage {
        role: "user",
        content: base.user,
    });
    if !initial_debrief.trim().is_empty() {
        msgs.push(CoachMessage {
            role: "assistant",
            content: initial_debrief.trim().to_string(),
        });
    }
    let start = history.len().saturating_sub(6);
    for turn in &history[start..] {
        msgs.push(CoachMessage {
            role: "user",
            content: turn.question.clone(),
        });
        msgs.push(CoachMessage {
            role: "assistant",
            content: turn.answer.clone(),
        });
    }
    msgs.push(CoachMessage {
        role: "user",
        content: question.trim().to_string(),
    });
    msgs
}

/// Deterministic demo coach debrief shown when `--demo` is active (default `Overview` lens).
pub fn demo_coach_debrief(review: &GameReview, lang: Lang) -> String {
    demo_coach_debrief_for_focus(review, CoachFocus::Overview, lang)
}

/// Deterministic demo coach debrief tailored to each [`CoachFocus`] lens in `--demo` mode.
pub fn demo_coach_debrief_for_focus(review: &GameReview, focus: CoachFocus, lang: Lang) -> String {
    let champ = game_name_or(GameTextKind::Champion, review.champion_id, lang, "Ahri");
    let text = match (focus, lang) {
        (CoachFocus::Overview, Lang::En) => {
            "VERDICT\n\
             Your laning discipline on {champ} (+22 CS at 15:00) gave your pick comp the early mid priority it needed, and you converted it cleanly into Voidgrubs, Dragon, and Baron. The single real swing came at 17:25, when overstaying with 1,950 unspent gold handed over both your life and the enemy Dragon 27 seconds later—matching your recurring pre-objective reset leak across recent games.\n\
             \n\
             KEY LEVERS\n\
             • Tempo conversion: hitting Malignance at 11:30 and rotating to 09:00 Grubs and 22:00 Dragon played directly to {champ}'s mid-game pick spike.\n\
             • Unspent gold risk: fighting at 17:25 with 1,950g in pocket erased your item advantage right before a neutral timer.\n\
             \n\
             NEXT GAME DRILL\n\
             At 45 seconds before every Dragon or Baron spawn, push one wave and base immediately whenever you hold 1,400+ gold."
        }
        (CoachFocus::Overview, Lang::Fr) => {
            "DIAGNOSTIC\n\
             Ta discipline de lane sur {champ} (+22 CS à 15:00) a offert à ta compo pick la priorité mid nécessaire, bien convertie sur les Larves, le Dragon et le Baron. Le seul vrai tournant arrive à 17:25 : rester sur la carte avec 1 950 PO non dépensées t'a coûté la vie et le Dragon ennemi 27 secondes plus tard — un schéma qui revient dans tes dernières parties.\n\
             \n\
             LEVIERS CLÉS\n\
             • Conversion du tempo : boucler Malfaisance à 11:30 puis tourner sur les Larves (09:00) et le Dragon (22:00) a parfaitement exploité le pic mid-game de {champ}.\n\
             • Risque des PO en poche : prendre un combat à 17:25 avec 1 950 PO non dépensées a annulé ton avance juste avant un timer d'objectif.\n\
             \n\
             EXERCICE PROCHAINE GAME\n\
             À 45 secondes de chaque spawn de Dragon ou Baron, pousse une vague et back immédiatement dès que tu as 1 400 PO ou plus en poche."
        }
        (CoachFocus::Matchup, Lang::En) => {
            "MATCHUP DYNAMIC\n\
             On {champ}, your wave clear and post-6 Spirit Rush mobility let you dictate push priority and punish short-range trades before your lane opponent scales. Building a +22 CS lead by 15:00 confirms you controlled the wave state well.\n\
             \n\
             LANE EXECUTION\n\
             • Crash & roam windows: pushing the cannon wave before 09:00 Grubs let you arrive first without losing mid plates.\n\
             • Reset greed: staying on the map past 1,500g in pocket turns a winning lane into a fragile bounty if the enemy jungler shadows mid.\n\
             \n\
             NEXT TIME IN THIS MATCHUP\n\
             Crash a stacked wave on your first 1,300g spike for Lost Chapter and immediately ward enemy raptors to track jungle timers."
        }
        (CoachFocus::Matchup, Lang::Fr) => {
            "DYNAMIQUE DU MATCHUP\n\
             Sur {champ}, ton waveclear et la mobilité de l'Assaut spirituel niveau 6 te permettent de dicter la priorité de vague et de punir avant le scaling adverse. Ton avance de +22 CS à 15:00 confirme une très bonne tenue de lane.\n\
             \n\
             EXÉCUTION EN LANE\n\
             • Fenêtres de roam : crash la vague avant les Larves (09:00) t'a permis d'arriver en premier sans céder de plaques.\n\
             • Gourmandise de back : rester en lane avec plus de 1 500 PO transforme une lane gagnée en prime facile si le jungler adverse couvre le mid.\n\
             \n\
             PROCHAINE FOIS DANS CE MATCHUP\n\
             Crash une grosse vague dès tes 1 300 PO pour Chapitre perdu, puis pose une ward sur les corbins ennemis pour lire le jungler."
        }
        (CoachFocus::Macro, Lang::En) => {
            "WIN CONDITION\n\
             Your pick/skirmish comp wins through first move to river and short fog-of-war catches before enemy carries reach 3 items. You executed that cleanly on 09:00 Grubs, 22:00 Dragon, and 27:00 Baron.\n\
             \n\
             TEMPO & OBJECTIVES\n\
             • Costly 17:25 death: dying 27s before Dragon with 1,950 unspent gold broke your mid-game tempo and handed over a free objective.\n\
             • Recurring leak: pre-objective deaths are showing up across your recent tracked games—fixing this single window is your highest-EV LP gain.\n\
             \n\
             MACRO RULE\n\
             Check the objective timer every time you hit 1,400g: if Dragon or Baron is under 60s away, base first and arrive on the map with items and Sweeper."
        }
        (CoachFocus::Macro, Lang::Fr) => {
            "CONDITION DE VICTOIRE\n\
             Ta compo pick/escarmouche gagne en arrivant première en rivière et en forçant des picks dans le brouillard de guerre avant les 3 objets ennemis. C'est exactement ce que tu as réussi sur les Larves (09:00), le Dragon (22:00) et le Baron (27:00).\n\
             \n\
             TEMPO & OBJECTIFS\n\
             • Mort coûteuse à 17:25 : tomber 27s avant le Dragon avec 1 950 PO en poche a cassé ton tempo mid-game et offert l'objectif gratuitement.\n\
             • Fuite récurrente : les morts avant objectif reviennent dans tes dernières parties suivies — corriger cette fenêtre est ton plus gros gain de LP.\n\
             \n\
             RÈGLE MACRO\n\
             Regarde le timer d'objectif dès que tu dépasses 1 400 PO : si Dragon ou Baron est à moins de 60s, back d'abord et arrive avec tes objets et le brouilleur."
        }
        (CoachFocus::Build, Lang::En) => {
            "BUILD VERDICT\n\
             Completing Malignance at 11:30 and Shadowflame at 20:00 gave {champ} lethal pick pressure on squishy targets throughout the mid-game.\n\
             \n\
             ITEM & RUNE ADJUSTMENTS\n\
             • Spike timing: an 11:30 first-item completion puts you ahead of the curve—make sure to spend gold right after hitting that spike instead of sitting on 1,950g at 17:25.\n\
             • Situational pivot: when facing heavy sustain or double frontline, slot an early 800g Oblivion Orb or Cryptbloom/Void Staff before greedier AP.\n\
             \n\
             BUILD CHECKPOINT\n\
             Press Tab at 14:00: if two enemies already stack healing or MR components, queue Oblivion Orb or Blighting Jewel before finishing your 3rd item."
        }
        (CoachFocus::Build, Lang::Fr) => {
            "VERDICT DU BUILD\n\
             Boucler Malfaisance à 11:30 puis Flamme-ombre à 20:00 a donné à {champ} une vraie pression de pick sur les cibles fragiles tout le mid-game.\n\
             \n\
             AJUSTEMENTS OBJETS & RUNES\n\
             • Timing des pics : finir ton 1er objet à 11:30 te met en avance sur la courbe — capitalise en dépensant tes PO au lieu de garder 1 950 PO à 17:25.\n\
             • Pivot situationnel : face à beaucoup de soin ou une double frontline, intercale un Orbe de l'oubli (800 PO) ou Floraison funeste / Bâton du Vide tôt.\n\
             \n\
             RÉFLEXE BUILD\n\
             Appuie sur Tab à 14:00 : si deux ennemis accumulent déjà du soin ou de la RM, prépare Orbe de l'oubli ou Joyau putréfiant avant ton 3e objet."
        }
    };
    text.replace("{champ}", &champ)
}

/// Deterministic demo follow-up answer for `--demo` mode so the interactive Q&A works offline.
pub fn demo_coach_followup(review: &GameReview, question: &str, lang: Lang) -> String {
    let champ = game_name_or(GameTextKind::Champion, review.champion_id, lang, "Ahri");
    let q = question.to_ascii_lowercase();
    let reply = if q.contains("build") || q.contains("item") || q.contains("objet") {
        lang.pick(
            "On {champ}, Malignance (11:30) + Shadowflame (20:00) is ideal when ahead vs squishy targets. Only pivot if Tab shows heavy healing (sit on an 800g Oblivion Orb) or early MR stacking (Blighting Jewel into Cryptbloom as 3rd item) so your Charm pick combos keep one-shotting through mid-game.",
            "Sur {champ}, Malfaisance (11:30) + Flamme-ombre (20:00) est idéal quand tu es devant face à des cibles fragiles. Ne pivote que si Tab montre beaucoup de soin (garde un Orbe de l'oubli à 800 PO) ou de la RM tôt (Joyau putréfiant vers Floraison funeste en 3e objet) pour garder ton burst sur Charme.",
        )
    } else if q.contains("lane") || q.contains("matchup") || q.contains("vs") {
        lang.pick(
            "Play {champ} around wave priority and Charm discipline: fish with Q through the wave so you simultaneously shove and chip, and hold E until the enemy commits a dash or key cooldown. Once you hit 6 and Malignance components, crash the cannon wave and threaten river fog rather than forcing risky tower dives.",
            "Joue {champ} autour de la priorité de vague et de la discipline du Charme : poke au A à travers les sbires pour push en même temps, et garde ton E tant que l'adversaire n'a pas utilisé son dash ou un cooldown clé. Dès le niveau 6, crash la vague canon et menace la rivière plutôt que de forcer sous tour.",
        )
    } else {
        lang.pick(
            "Your biggest lever here was the 17:25 reset window: 45 seconds before Dragon spawns, shove mid once and recall immediately when sitting on 1,400+ gold. Returning to river with a fresh component spike turns a coin-flip skirmish into a clean pick for {champ} and locks down objective tempo.",
            "Ton plus gros levier ici était la fenêtre de back à 17:25 : 45 secondes avant le spawn du Dragon, pousse une vague mid et back immédiatement dès que tu as 1 400 PO ou plus. Revenir en rivière avec ton pic d'objet transforme une escarmouche risquée en pick propre pour {champ} et sécurise le tempo.",
        )
    };
    reply.replace("{champ}", &champ)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::{EndOfGamePlayer, EndOfGameTeam};

    fn base_input() -> ReviewInput<'static> {
        ReviewInput {
            game_id: 42,
            champion_id: 103, // Ahri (magic, PICK)
            position: Some(Position::Mid),
            queue: Some(StatsQueue::RankedSolo),
            win: false,
            duration_sec: 1800, // 30:00
            kills: 5,
            deaths: 5,
            assists: 7,
            creep_score: 180,
            total_gold: 11200,
            damage_to_champions: 22000,
            vision_score: 18,
            kill_participation: Some(0.55),
            damage_share: Some(0.28),
            keystone_id: Some(8112),                    // Electrocute
            sub_style_id: Some(8200),                   // Sorcery
            spell_ids: [4, 14],                         // Flash + Ignite
            items: vec![3020, 6655, 3157],              // Sorcs, Luden, Zhonya
            ally_champions: vec![54, 64, 103, 51, 89],  // Malphite, Lee, Ahri, Cait, Leona
            enemy_champions: vec![266, 19, 8, 222, 16], // Aatrox, WW, Vlad, Jinx, Soraka (heavy heal!)
            opponent_champion_id: Some(8),
            ..ReviewInput::default()
        }
    }

    #[test]
    fn detects_pre_objective_death_and_unspent_gold_and_anti_heal() {
        let trace = MatchTrace {
            game_id: Some(42),
            champion_id: 103,
            position: Some(Position::Mid),
            samples: vec![TraceSample {
                time_sec: 900,
                level: 11,
                cs: 115,
                lane_cs_diff: Some(24),
                current_gold: 1950,
                kills: 3,
                deaths: 1,
                assists: 2,
            }],
            deaths: vec![
                DeathEvent {
                    time_sec: 1050,
                    unspent_gold: Some(2150),
                },
                DeathEvent {
                    time_sec: 1350,
                    unspent_gold: Some(400),
                },
            ],
            objectives: vec![
                ObjectiveEvent {
                    time_sec: 1075,
                    kind: EpicKind::Dragon,
                    by_ally: false,
                    player_involved: false,
                },
                ObjectiveEvent {
                    time_sec: 600,
                    kind: EpicKind::Grubs,
                    by_ally: true,
                    player_involved: true,
                },
                ObjectiveEvent {
                    time_sec: 920,
                    kind: EpicKind::Herald,
                    by_ally: true,
                    player_involved: true,
                },
            ],
            item_buys: vec![],
        };

        let mut input = base_input();
        input.trace = Some(&trace);

        let rev = analyze_match(&input, Lang::En);
        assert_eq!(rev.coverage, ReviewCoverage::Full);
        assert!(rev.improvements.len() <= MAX_FINDINGS_PER_SIDE);
        assert!(rev.good_choices.len() <= MAX_FINDINGS_PER_SIDE);

        let imp_ids: Vec<&str> = rev.improvements.iter().map(|f| f.id.as_str()).collect();
        assert!(imp_ids.contains(&"obj_pre_death"), "got {imp_ids:?}");
        assert!(
            imp_ids.contains(&"econ_unspent_gold_death"),
            "got {imp_ids:?}"
        );
        assert!(
            imp_ids.contains(&"build_missing_anti_heal"),
            "got {imp_ids:?}"
        );

        let good_ids: Vec<&str> = rev.good_choices.iter().map(|f| f.id.as_str()).collect();
        assert!(good_ids.contains(&"econ_lane_lead_15"), "got {good_ids:?}");
        assert!(good_ids.contains(&"obj_presence"), "got {good_ids:?}");
        assert!(!rev.turning_points.is_empty());
    }

    #[test]
    fn post_game_only_degrades_gracefully_without_fake_timestamps() {
        let mut input = base_input();
        input.items.push(3165); // Morellonomicon -> anti-heal good choice
        input.vision_score = 32; // > 0.78/min -> high vision good choice
        input.kill_participation = Some(0.70);
        input.deaths = 2;

        let rev = analyze_match(&input, Lang::Fr);
        assert_eq!(rev.coverage, ReviewCoverage::PostGameOnly);
        assert!(rev.turning_points.is_empty());
        assert!(rev.good_choices.iter().all(|f| f.timestamp_sec.is_none()));
        assert!(
            rev.good_choices
                .iter()
                .any(|f| f.id == "build_anti_heal_good" && f.title.contains("Hémorragie")),
            "expected French anti-heal finding, got {:?}",
            rev.good_choices
        );

        let prompt = build_coach_prompt(&rev, Lang::Fr);
        assert!(prompt.user.contains("aucun timestamp"));
        assert!(!prompt.user.contains("=== PHASE TRAJECTORY ==="));
        assert!(!prompt.user.contains("=== DEATHS & OBJECTIVES ==="));
    }

    #[test]
    fn draft_pillar_rewards_filling_frontline_gap() {
        let mut input = base_input();
        // Replace Malphite/Leona so player (Amumu = 32, FRONT | ENG) is the only frontline.
        input.champion_id = 32;
        input.position = Some(Position::Jungle);
        input.ally_champions = vec![17, 32, 103, 51, 16]; // Teemo, Amumu, Ahri, Cait, Soraka

        let rev = analyze_match(&input, Lang::En);
        assert!(
            rev.good_choices.iter().any(|f| f.id == "draft_filled_gap"),
            "expected draft_filled_gap, got {:?}",
            rev.good_choices
        );
    }

    #[test]
    fn excludes_aram_and_remakes_from_player_game() {
        let aram = PlayerGame {
            game_id: 1,
            champion_id: 103,
            queue: Some(StatsQueue::Aram),
            duration_secs: 1200,
            ..PlayerGame::default()
        };
        assert!(ReviewInput::from_player_game(&aram, None, None, None, false).is_none());

        let remake = PlayerGame {
            game_id: 2,
            champion_id: 103,
            queue: Some(StatsQueue::RankedSolo),
            position: Some(Position::Mid),
            duration_secs: 180,
            ..PlayerGame::default()
        };
        assert!(ReviewInput::from_player_game(&remake, None, None, None, false).is_none());
    }

    #[test]
    fn coach_prompt_carries_rich_anonymized_dossier_in_both_languages() {
        let trace = MatchTrace {
            game_id: Some(42),
            champion_id: 103,
            position: Some(Position::Mid),
            samples: vec![
                TraceSample {
                    time_sec: 605,
                    level: 8,
                    cs: 78,
                    lane_cs_diff: Some(12),
                    current_gold: 1100,
                    kills: 2,
                    deaths: 0,
                    assists: 1,
                },
                TraceSample {
                    time_sec: 905,
                    level: 11,
                    cs: 118,
                    lane_cs_diff: Some(22),
                    current_gold: 1950,
                    kills: 4,
                    deaths: 1,
                    assists: 3,
                },
            ],
            deaths: vec![DeathEvent {
                time_sec: 1050,
                unspent_gold: Some(1950),
            }],
            objectives: vec![ObjectiveEvent {
                time_sec: 1075,
                kind: EpicKind::Dragon,
                by_ally: false,
                player_involved: false,
            }],
            item_buys: vec![ItemBuyEvent {
                time_sec: 690,
                item_id: 3118,
            }],
        };
        let personal = PlayerChampionStats {
            champion_id: 103,
            games: 18,
            wins: 10,
            kills: 6.5,
            deaths: 3.4,
            assists: 7.2,
            dpm: 610.0,
            cs_per_min: 6.8,
            gold_per_min: 410.0,
            vision_per_min: 0.72,
            ..PlayerChampionStats::default()
        };

        let mut input = base_input();
        input.items.push(3165); // Morellonomicon
        input.trace = Some(&trace);
        input.personal_champ = Some(&personal);
        input.top_enemy_threat = Some(ChampionLine {
            champion_id: 222, // Jinx
            position: Some(Position::Bot),
            kills: 11,
            deaths: 2,
            assists: 6,
            cs: 245,
            gold: 15200,
            damage: 36000,
        });

        let rev_fr = analyze_match(&input, Lang::Fr);
        let prompt_fr = build_coach_prompt(&rev_fr, Lang::Fr);
        assert!(prompt_fr.system.contains("français"));
        assert!(prompt_fr.system.contains("DIAGNOSTIC"));
        assert!(prompt_fr.system.contains("Angle du rôle (Mid)"));
        assert!(prompt_fr.user.contains("Ahri"));
        assert!(prompt_fr.user.contains("Vladimir"));
        assert!(prompt_fr.user.contains("Hémorragie"));
        assert!(prompt_fr.user.contains("=== COMPOSITIONS & THREATS ==="));
        assert!(prompt_fr.user.contains("Jinx (11/2/6"));
        assert!(prompt_fr.user.contains("=== PERSONAL BASELINE (18 games"));
        assert!(prompt_fr.user.contains("=== PHASE TRAJECTORY ==="));
        assert!(prompt_fr.user.contains("17:30 (1950g unspent)"));
        assert!(!prompt_fr.user.contains("puuid"));
        assert!(!prompt_fr.user.contains("summonerName"));

        let rev_en = analyze_match(&input, Lang::En);
        let prompt_en = build_coach_prompt(&rev_en, Lang::En);
        assert!(prompt_en.system.contains("English"));
        assert!(prompt_en.system.contains("VERDICT"));
        assert!(prompt_en.system.contains("Role lens (Mid)"));
        assert!(prompt_en.user.contains("Grievous Wounds"));
        assert!(prompt_en.user.contains("Enemy sustain threats"));
        assert!(prompt_en.user.contains("11:30"));
    }

    #[test]
    fn from_end_of_game_extracts_opponent_line_and_top_threat() {
        let eog = EndOfGameSummary {
            result: "Defeat".into(),
            game_length_sec: 1800,
            champion_id: 103,
            champion_name: "Ahri".into(),
            kills: 6,
            deaths: 4,
            assists: 8,
            total_gold: 12400,
            damage_to_champions: 24000,
            skin_id: 0,
            creep_score: 195,
            vision_score: 22,
            teams: vec![
                EndOfGameTeam {
                    is_player_team: true,
                    won: false,
                    total_kills: 20,
                    total_gold: 54000,
                    total_damage: 90000,
                    total_vision: 110,
                    players: vec![EndOfGamePlayer {
                        champion_id: 103,
                        champion_name: "Ahri".into(),
                        skin_id: 0,
                        position: "MIDDLE".into(),
                        level: 16,
                        kills: 6,
                        deaths: 4,
                        assists: 8,
                        creep_score: 195,
                        items: vec![3020, 3118],
                        keystone_id: 8112,
                        sub_style_id: 8200,
                        spell1_id: 4,
                        spell2_id: 14,
                        damage_to_champions: 24000,
                        damage_taken: 18000,
                        gold_earned: 12400,
                        vision_score: 22,
                        kill_participation: 0.70,
                        damage_share: 0.27,
                        performance_score: 74,
                        placement: 3,
                        is_mvp: false,
                        is_ace: true,
                        is_self: true,
                    }],
                },
                EndOfGameTeam {
                    is_player_team: false,
                    won: true,
                    total_kills: 28,
                    total_gold: 61000,
                    total_damage: 112000,
                    total_vision: 125,
                    players: vec![
                        EndOfGamePlayer {
                            champion_id: 8,
                            champion_name: "Vladimir".into(),
                            skin_id: 0,
                            position: "MIDDLE".into(),
                            level: 17,
                            kills: 7,
                            deaths: 3,
                            assists: 5,
                            creep_score: 225,
                            items: vec![3020, 3089],
                            keystone_id: 8230,
                            sub_style_id: 8300,
                            spell1_id: 4,
                            spell2_id: 6,
                            damage_to_champions: 29000,
                            damage_taken: 25000,
                            gold_earned: 13900,
                            vision_score: 18,
                            kill_participation: 0.43,
                            damage_share: 0.26,
                            performance_score: 78,
                            placement: 2,
                            is_mvp: false,
                            is_ace: false,
                            is_self: false,
                        },
                        EndOfGamePlayer {
                            champion_id: 222,
                            champion_name: "Jinx".into(),
                            skin_id: 0,
                            position: "BOTTOM".into(),
                            level: 17,
                            kills: 12,
                            deaths: 2,
                            assists: 8,
                            creep_score: 260,
                            items: vec![3006, 3031],
                            keystone_id: 8008,
                            sub_style_id: 8200,
                            spell1_id: 4,
                            spell2_id: 21,
                            damage_to_champions: 41000,
                            damage_taken: 14000,
                            gold_earned: 16200,
                            vision_score: 20,
                            kill_participation: 0.71,
                            damage_share: 0.37,
                            performance_score: 91,
                            placement: 1,
                            is_mvp: true,
                            is_ace: false,
                            is_self: false,
                        },
                    ],
                },
            ],
        };

        let input = ReviewInput::from_end_of_game(&eog, 99, None, None, None, false)
            .expect("reviewable EoG");
        assert_eq!(input.opponent_line.as_ref().map(|o| o.champion_id), Some(8));
        assert_eq!(
            input.top_enemy_threat.as_ref().map(|t| t.champion_id),
            Some(222)
        );
        assert_eq!(input.team_kills, Some((20, 28)));
        assert_eq!(input.team_gold, Some((54000, 61000)));
        assert_eq!(input.keystone_id, Some(8112));
        assert_eq!(input.spell_ids, [4, 14]);
    }

    #[test]
    fn builds_longitudinal_habit_profile_archetypes_lenses_and_followups() {
        let mk_game = |id: i64, win: bool, deaths: u32, cs: u32, ts: i64| PlayerGame {
            game_id: id,
            champion_id: 103,
            position: Some(Position::Mid),
            queue: Some(StatsQueue::RankedSolo),
            win,
            duration_secs: 1800,
            started_at_ms: ts,
            kills: if win { 8 } else { 3 },
            deaths,
            assists: 6,
            cs,
            kill_participation: Some(if win { 0.68 } else { 0.45 }),
            ..PlayerGame::default()
        };
        let now = 1_795_500_000_000i64;
        let games = vec![
            mk_game(1, false, 7, 175, now),
            mk_game(2, false, 8, 170, now - 2_400_000),
            mk_game(3, true, 2, 220, now - 5_000_000),
            mk_game(4, true, 3, 215, now - 90_000_000),
        ];
        let leak_trace = MatchTrace {
            game_id: Some(1),
            champion_id: 103,
            position: Some(Position::Mid),
            samples: vec![TraceSample {
                time_sec: 900,
                level: 11,
                cs: 115,
                lane_cs_diff: Some(20),
                current_gold: 1800,
                kills: 4,
                deaths: 1,
                assists: 2,
            }],
            deaths: vec![DeathEvent {
                time_sec: 1050,
                unspent_gold: Some(1900),
            }],
            objectives: vec![ObjectiveEvent {
                time_sec: 1075,
                kind: EpicKind::Dragon,
                by_ally: false,
                player_involved: false,
            }],
            item_buys: vec![],
        };
        let traces = vec![leak_trace.clone(), leak_trace.clone(), leak_trace.clone()];

        let habit = build_player_habit_profile(&games, &traces).expect("habit profile");
        assert_eq!(habit.sample_games, 4);
        assert_eq!(habit.current_streak, -2);
        assert_eq!(habit.session_wins, 1);
        assert_eq!(habit.session_losses, 2);
        assert_eq!(habit.pre_objective_death_games, 3);
        assert!(habit.primary_pattern(Lang::Fr).unwrap().contains("3/3"));

        let mut input = base_input();
        input.trace = Some(&leak_trace);
        input.habit_profile = Some(&habit);
        let rev = analyze_match(&input, Lang::Fr);
        assert_eq!(rev.context.archetype, MatchArchetype::LeadThrown);

        let macro_prompt = build_coach_prompt_with_focus(&rev, CoachFocus::Macro, Lang::Fr);
        assert!(macro_prompt.system.contains("CONDITION DE VICTOIRE"));
        assert!(macro_prompt.user.contains("CROSS-GAME HABIT MEMORY"));
        assert!(macro_prompt.user.contains("2-loss streak"));

        let qs = suggested_coach_questions(&rev, Lang::Fr);
        assert_eq!(qs.len(), 3);
        assert!(qs[0].contains("Ahri") && qs[0].contains("Vladimir"));

        let history: Vec<CoachTurn> = (0..8)
            .map(|i| CoachTurn {
                question: format!("Q{i}"),
                answer: format!("A{i}"),
            })
            .collect();
        let msgs = build_followup_messages(
            &rev,
            CoachFocus::Macro,
            "Initial debrief",
            &history,
            "New question?",
            Lang::En,
        );
        // system + dossier + initial_debrief + 6*2 history + new question = 16 messages.
        assert_eq!(msgs.len(), 16);
        assert_eq!(msgs.last().unwrap().content, "New question?");
    }
}
