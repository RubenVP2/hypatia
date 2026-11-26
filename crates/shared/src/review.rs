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
use crate::game_text::{game_name_or, GameTextKind};
use crate::i18n::Lang;
use crate::stats::{
    ChampionBuild, CompTrait, PlayerChampionStats, PlayerGame, StatsQueue, REMAKE_SECS,
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
    pub items: Vec<i64>,
    pub ally_champions: Vec<i64>,
    pub enemy_champions: Vec<i64>,
    pub opponent_champion_id: Option<i64>,
    pub trace: Option<&'a MatchTrace>,
    pub personal_champ: Option<&'a PlayerChampionStats>,
    pub recommended_build: Option<&'a ChampionBuild>,
    pub is_demo: bool,
}

impl<'a> ReviewInput<'a> {
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
        let opponent_champion_id = position.and_then(|pos| {
            enemy_team.and_then(|t| {
                t.players
                    .iter()
                    .find(|p| Position::parse(&p.position) == Some(pos))
                    .map(|p| p.champion_id)
            })
        });

        Some(Self {
            game_id,
            champion_id: eog.champion_id,
            position,
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
            items: self_row.map(|p| p.items.clone()).unwrap_or_default(),
            ally_champions,
            enemy_champions,
            opponent_champion_id,
            trace,
            personal_champ,
            recommended_build,
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
        Some(Self {
            game_id: game.game_id,
            champion_id: game.champion_id,
            position: game.position.or_else(|| trace.and_then(|t| t.position)),
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
            items: game.items.clone(),
            ally_champions: vec![game.champion_id],
            enemy_champions,
            opponent_champion_id: game.opponent_champion_id,
            trace,
            personal_champ,
            recommended_build,
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

/// Anonymized system + user messages sent to an opt-in Layer 2 LLM narrator.
///
/// Built strictly from [`GameReview`], so it can never carry a summoner name, PUUID, or secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoachPrompt {
    pub system: String,
    pub user: String,
}

/// Builds a concise, fact-grounded prompt for an opt-in LLM coach narrator.
pub fn build_coach_prompt(review: &GameReview, lang: Lang) -> CoachPrompt {
    let system = lang
        .pick(
            "You are a concise, constructive League of Legends coach. \
             Write in English. Base every claim strictly on the verified match facts provided—\
             never invent timestamps, items, or plays not listed. \
             Output one short narrative paragraph (3–4 sentences) connecting the match's key choices, \
             followed by a single concrete priority for the next game. Keep it under 130 words, plain text only.",
            "Tu es un coach League of Legends concis et constructif. \
             Écris en français. Appuie chaque remarque strictement sur les faits vérifiés fournis—\
             n'invente jamais de timestamps, d'objets ou d'actions absents des données. \
             Rédige un court paragraphe narratif (3 à 4 phrases) qui relie les décisions clés de la partie, \
             suivi d'une priorité concrète pour la prochaine game. Reste sous 130 mots, texte brut uniquement.",
        )
        .to_string();

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

    let mut user = format!(
        "Champion: {champ} ({role})\nResult: {result} ({})\nCoverage: {}\nMetrics: {:.1} CS/min, {:.2} vision/min, {} deaths\n",
        format_mm_ss(review.features.duration_sec),
        review.coverage.label(lang),
        review.features.cs_per_min,
        review.features.vision_per_min,
        review.features.deaths,
    );

    if !review.good_choices.is_empty() {
        user.push_str("\nGood choices:\n");
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
        user.push_str("\nTo improve:\n");
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
        user.push_str("\nTurning points:\n");
        for tp in &review.turning_points {
            user.push_str(&format!(
                "- [{}] ({}) {}\n",
                format_mm_ss(tp.time_sec),
                tp.pillar.label(lang),
                tp.label
            ));
        }
    }

    CoachPrompt { system, user }
}

/// Deterministic demo coach debrief shown when `--demo` is active.
pub fn demo_coach_debrief(review: &GameReview, lang: Lang) -> String {
    let champ = game_name_or(GameTextKind::Champion, review.champion_id, lang, "Ahri");
    lang.pick(
        "Your laning discipline on {champ} gave your team a clear mid-game lead, and your rotations to Grubs, Dragon, and Baron converted that priority well. The main swing came at 17:25: fighting while holding 1,950 unspent gold cost both your life and the enemy Dragon 27 seconds later.\n\nNext game priority: reset 45 seconds before every Dragon or Baron spawn whenever you sit on 1,500+ gold.",
        "Ta phase de lane solide sur {champ} a donné une vraie avance mid-game à ton équipe, bien convertie par tes rotations sur les Larves, le Dragon et le Baron. Le vrai tournant survient à 17:25 : prendre un fight avec 1 950 PO non dépensées t'a coûté la vie et le Dragon ennemi 27 secondes plus tard.\n\nPriorité prochaine game : back 45 secondes avant chaque spawn de Dragon ou Baron dès que tu as plus de 1 500 PO en poche.",
    )
    .replace("{champ}", &champ)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> ReviewInput<'static> {
        ReviewInput {
            game_id: 42,
            champion_id: 103, // Ahri (magic, PICK)
            position: Some(Position::Mid),
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
            items: vec![3020, 6655, 3157], // Sorcs, Luden, Zhonya
            ally_champions: vec![54, 64, 103, 51, 89], // Malphite, Lee, Ahri, Cait, Leona
            enemy_champions: vec![266, 19, 8, 222, 16], // Aatrox, WW, Vlad, Jinx, Soraka (heavy heal!)
            opponent_champion_id: Some(8),
            trace: None,
            personal_champ: None,
            recommended_build: None,
            is_demo: false,
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
    fn coach_prompt_contains_verified_facts_and_respects_language() {
        let mut input = base_input();
        input.items.push(3165);
        let rev_fr = analyze_match(&input, Lang::Fr);
        let prompt_fr = build_coach_prompt(&rev_fr, Lang::Fr);
        assert!(prompt_fr.system.contains("français"));
        assert!(prompt_fr.user.contains("Ahri"));
        assert!(prompt_fr.user.contains("Hémorragie"));
        assert!(!prompt_fr.user.contains("puuid"));

        let rev_en = analyze_match(&input, Lang::En);
        let prompt_en = build_coach_prompt(&rev_en, Lang::En);
        assert!(prompt_en.system.contains("English"));
        assert!(prompt_en.user.contains("Grievous Wounds"));
    }
}
