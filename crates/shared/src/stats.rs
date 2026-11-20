//! Champion Stats: the contract shared by every dpm.lol-style view (champion build, tier list,
//! matchups, team comp, My Stats, champ-select helper).
//!
//! Types only, no I/O. Two kinds of source fill them:
//! - **Local** (always on): the client's own game data (items, runes, spells), its recommended
//!   rune pages, and the player's match history, all read with `GET` from `127.0.0.1`.
//! - **Online** (opt-in, off by default, `Settings::online_stats`): global aggregates fetched by a
//!   [`StatsSource`] implementation in `hypatia-core`. Switched off, it performs no I/O.
//!
//! Every value says where it came from ([`StatsOrigin`]) so the UI can label it honestly, and
//! demo values stay labelled as demo.

use serde::{Deserialize, Serialize};

use crate::champions::Position;

/// Where a stats value comes from. Shown to the player next to the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StatsOrigin {
    /// Built from the player's own local match history.
    #[default]
    Local,
    /// Riot's own recommendation exposed by the local client (e.g. recommended rune pages).
    RiotClient,
    /// Global aggregates from the opt-in online provider.
    Online,
    /// Demo fixtures (`--demo`).
    Demo,
}

/// Rank bracket a global aggregate was computed over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RankBracket {
    All,
    #[default]
    EmeraldPlus,
    DiamondPlus,
    MasterPlus,
}

/// Queue the stats describe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StatsQueue {
    #[default]
    RankedSolo,
    RankedFlex,
    Normal,
    Aram,
    Arena,
}

impl StatsQueue {
    /// Riot queue ids: Ranked Solo/Duo 420, Flex 440, Draft/Blind/Quickplay 400/430/490/480,
    /// ARAM 450, Arena 1700/1710.
    pub fn from_queue_id(queue_id: i64) -> Option<Self> {
        match queue_id {
            420 => Some(Self::RankedSolo),
            440 => Some(Self::RankedFlex),
            400 | 430 | 480 | 490 => Some(Self::Normal),
            // Howling Abyss: ARAM, ARAM Clash, Butcher's Bridge, ARAM: Mayhem.
            100 | 450 | 720 | 2400 => Some(Self::Aram),
            1700 | 1710 => Some(Self::Arena),
            _ => None,
        }
    }

    /// Whether games in this queue are played on Summoner's Rift, with lanes.
    pub fn on_summoners_rift(self) -> bool {
        !matches!(self, Self::Aram | Self::Arena)
    }
}

/// What a query asks for. `patch` is `None` for "current patch".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct StatsQuery {
    pub champion_id: i64,
    pub position: Option<Position>,
    pub queue: StatsQueue,
    pub bracket: RankBracket,
    pub patch: Option<String>,
}

/// Win / pick / ban rates as fractions in `0.0..=1.0`, with the sample they rest on.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rates {
    pub win_rate: f32,
    pub pick_rate: f32,
    pub ban_rate: f32,
    pub games: u32,
}

/// One rune page: primary and secondary styles, the six perks in order, the three shards.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RuneSetup {
    pub primary_style_id: i64,
    pub sub_style_id: i64,
    /// Keystone first, then the three primary and two secondary perks.
    pub perk_ids: Vec<i64>,
    pub shard_ids: Vec<i64>,
}

/// Summoner spell pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SpellPair {
    pub first: i64,
    pub second: i64,
}

/// A build path stage: starting items, core items, boots, situational options, or slot-by-slot
/// purchase choices (`Slot1`..`Slot5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemStage {
    Starting,
    Core,
    Boots,
    Situational,
    Slot1,
    Slot2,
    Slot3,
    Slot4,
    Slot5,
}

impl ItemStage {
    /// Whether this stage represents an individual purchase slot (`Item 1`..`Item 5`).
    pub fn is_slot(self) -> bool {
        matches!(
            self,
            Self::Slot1 | Self::Slot2 | Self::Slot3 | Self::Slot4 | Self::Slot5
        )
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ItemBlock {
    pub stage: Option<ItemStage>,
    pub item_ids: Vec<i64>,
    pub rates: Rates,
}

/// Skill letters, `Q W E R`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Skill {
    Q,
    W,
    E,
    R,
}

/// Level-by-level skill order (index 0 = level 1) and the max priority (e.g. Q > E > W).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SkillOrder {
    pub levels: Vec<Skill>,
    pub priority: Vec<Skill>,
    pub rates: Rates,
}

/// Everything the Champion Build view shows for one champion in one role.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChampionBuild {
    pub query: StatsQuery,
    pub origin: StatsOrigin,
    pub rates: Rates,
    /// Best first; the first entry is the recommended page.
    pub runes: Vec<(RuneSetup, Rates)>,
    pub spells: Vec<(SpellPair, Rates)>,
    pub items: Vec<ItemBlock>,
    pub skills: Vec<SkillOrder>,
    /// Role pickrate share across Summoner's Rift positions (e.g. `[(Mid, 0.86), (Support, 0.14)]`).
    #[serde(default)]
    pub role_distribution: Vec<(Position, f32)>,
    /// Tier standing and rank within the role (e.g. `(Tier::S, 4, 41)`).
    #[serde(default)]
    pub tier_rank: Option<(Tier, u32, u32)>,
    /// Link the provider requires next to its data (Online only).
    #[serde(default)]
    pub attribution: Option<Attribution>,
}

/// A champion's standing in one role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    S,
    A,
    B,
    C,
    D,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierEntry {
    pub champion_id: i64,
    pub position: Position,
    pub tier: Tier,
    pub rates: Rates,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TierList {
    pub queue: StatsQueue,
    pub bracket: RankBracket,
    pub patch: String,
    pub origin: StatsOrigin,
    pub entries: Vec<TierEntry>,
    /// Link the provider requires next to its data (Online only).
    #[serde(default)]
    pub attribution: Option<Attribution>,
}

/// How `champion_id` fares against `opponent_id` in the same role (lane matchup) or overall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Matchup {
    pub champion_id: i64,
    pub opponent_id: i64,
    pub position: Option<Position>,
    /// Win rate of `champion_id` in this matchup.
    pub rates: Rates,
    /// Gold difference at 15 minutes, from `champion_id`'s point of view, when known.
    pub gold_diff_15: Option<f32>,
    pub origin: StatsOrigin,
}

/// Matchups of one champion as the views receive them: best for the champion first, with the
/// provider link when the data is Online.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MatchupList {
    pub query: StatsQuery,
    pub origin: StatsOrigin,
    pub entries: Vec<Matchup>,
    #[serde(default)]
    pub attribution: Option<Attribution>,
}

/// Credit an online provider asks for: its name and the page the data came from. Views show it
/// as a visible, clickable link next to the data.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Attribution {
    pub provider: String,
    pub url: String,
}

/// Damage profile and comp traits of one team, for the team comp helper.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TeamCompProfile {
    /// Share of physical / magic / true damage, summing to ~1.0.
    pub physical: f32,
    pub magic: f32,
    pub true_damage: f32,
    pub traits: Vec<CompTrait>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompTrait {
    Engage,
    Peel,
    Poke,
    Splitpush,
    Teamfight,
    Pick,
    Scaling,
    EarlyGame,
    Frontline,
    LacksFrontline,
    LacksEngage,
    AllPhysical,
    AllMagic,
}

/// Recognition badge for standout performance in a single match (computed locally).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MatchBadge {
    /// Best performance score on the winning team.
    Mvp,
    /// Best performance score on the losing team.
    Ace,
    /// Multi-kill achievement (`3` = Triple, `4` = Quadra, `5` = Penta).
    Multikill(u8),
}

/// Per-champion summary of the player's own games (My Stats).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerChampionStats {
    pub champion_id: i64,
    pub games: u32,
    pub wins: u32,
    pub kills: f32,
    pub deaths: f32,
    pub assists: f32,
    /// Damage to champions per minute.
    pub dpm: f32,
    pub cs_per_min: f32,
    pub gold_per_min: f32,
    pub vision_per_min: f32,
    /// The player's most played rune page, summoner spells, and final items on this champion.
    pub runes: Option<RuneSetup>,
    #[serde(default)]
    pub spells: Option<SpellPair>,
    pub items: Vec<i64>,
}

/// Per-role summary of the player's Summoner's Rift games (My Stats).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleSummary {
    pub position: Position,
    pub games: u32,
    pub wins: u32,
    pub kda: f32,
    pub cs_per_min: f32,
    pub dpm: f32,
}

/// One day/session bucket of recent games in My Stats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionGroup {
    /// UTC calendar day (`started_at_ms.div_euclid(86_400_000)`).
    pub day_key: i64,
    pub wins: u32,
    pub losses: u32,
    pub avg_score: Option<f32>,
    /// Indices into the input game slice belonging to this session.
    pub game_indices: Vec<usize>,
}

/// One past game of the player, from local match history.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerGame {
    pub game_id: i64,
    pub champion_id: i64,
    pub position: Option<Position>,
    pub queue: Option<StatsQueue>,
    pub win: bool,
    pub duration_secs: u32,
    /// Unix epoch milliseconds.
    pub started_at_ms: i64,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub damage_to_champions: u32,
    pub cs: u32,
    pub gold: u32,
    pub vision_score: u32,
    pub items: Vec<i64>,
    pub runes: Option<RuneSetup>,
    pub spells: SpellPair,
    /// Direct enemy lane opponent's champion ID on Summoner's Rift, when full match data is known.
    #[serde(default)]
    pub opponent_champion_id: Option<i64>,
    /// Kill participation as a fraction in `0.0..=1.0` (`(kills + assists) / team_kills`).
    #[serde(default)]
    pub kill_participation: Option<f32>,
    /// Share of team champion damage in `0.0..=1.0`.
    #[serde(default)]
    pub damage_share: Option<f32>,
    /// Share of team gold in `0.0..=1.0`.
    #[serde(default)]
    pub gold_share: Option<f32>,
    /// Deterministic local `0..=100` performance rating.
    #[serde(default)]
    pub performance_score: Option<u8>,
    /// `1..=10` rank among all participants when full match data is available.
    #[serde(default)]
    pub placement: Option<u8>,
    /// Standout match badge (`MVP`, `ACE`, or multi-kill).
    #[serde(default)]
    pub badge: Option<MatchBadge>,
}

/// Games shorter than this are remakes: listed, but left out of every average.
pub const REMAKE_SECS: u32 = 300;

impl PlayerGame {
    /// A ranked or normal game on Summoner's Rift, with lanes.
    pub fn on_summoners_rift(&self) -> bool {
        self.queue.is_some_and(StatsQueue::on_summoners_rift)
    }

    /// Whether the match ended early as a remake (`duration_secs < REMAKE_SECS`).
    pub fn is_remake(&self) -> bool {
        self.duration_secs < REMAKE_SECS
    }
}

/// The games My Stats totals and per-champion summaries are built from: Summoner's Rift games
/// when there are any, so ARAM and event modes (with very different damage and farm) do not
/// skew the rates; otherwise every game. Remakes never count. Order is kept.
pub fn summarised_games(games: &[PlayerGame]) -> Vec<&PlayerGame> {
    let played = || games.iter().filter(|g| !g.is_remake());
    if played().any(PlayerGame::on_summoners_rift) {
        played().filter(|g| g.on_summoners_rift()).collect()
    } else {
        played().collect()
    }
}

#[cfg(test)]
mod summary_tests {
    use super::*;

    fn game(id: i64, queue: Option<StatsQueue>, secs: u32) -> PlayerGame {
        PlayerGame {
            game_id: id,
            queue,
            duration_secs: secs,
            ..PlayerGame::default()
        }
    }

    #[test]
    fn aram_mayhem_is_aram() {
        assert_eq!(StatsQueue::from_queue_id(2400), Some(StatsQueue::Aram));
        assert!(!StatsQueue::Aram.on_summoners_rift());
    }

    #[test]
    fn summaries_keep_to_the_rift_when_it_was_played() {
        let games = vec![
            game(1, Some(StatsQueue::Aram), 1200),
            game(2, Some(StatsQueue::RankedSolo), 1500),
            game(3, None, 1300),
            game(4, Some(StatsQueue::Normal), 200),
            game(5, Some(StatsQueue::Normal), 1400),
        ];
        let ids: Vec<i64> = summarised_games(&games).iter().map(|g| g.game_id).collect();
        assert_eq!(
            ids,
            vec![2, 5],
            "ARAM, unknown modes and remakes are left out"
        );

        let howling_only = vec![game(1, Some(StatsQueue::Aram), 1200), game(2, None, 200)];
        let ids: Vec<i64> = summarised_games(&howling_only)
            .iter()
            .map(|g| g.game_id)
            .collect();
        assert_eq!(ids, vec![1], "without Rift games every played game counts");
    }
}

/// The My Stats profile: recent games and the per-champion & per-role summaries derived from them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub origin: StatsOrigin,
    pub games: Vec<PlayerGame>,
    pub champions: Vec<PlayerChampionStats>,
    #[serde(default)]
    pub roles: Vec<RoleSummary>,
}

/// Spatial slot layout of one rune tree (row 0 = keystones, rows 1..=3 = regular perk rows).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RuneStyleSlots {
    pub style_id: i64,
    pub rows: Vec<Vec<i64>>,
}

/// Standard 3x3 stat shard grid (Offense, Flex, Defense).
pub const STAT_SHARD_ROWS: [[i64; 3]; 3] =
    [[5008, 5005, 5007], [5008, 5010, 5001], [5011, 5013, 5001]];

/// Built-in spatial slot layout for the five rune trees, used as fallback when the client's
/// `perkstyles.json` has not been loaded (offline or `--demo`).
pub fn builtin_rune_style_slots(style_id: i64) -> Option<Vec<Vec<i64>>> {
    let rows: &[&[i64]] = match style_id {
        // Precision
        8000 => &[
            &[8005, 8008, 8021, 8010],
            &[9101, 9111, 8009],
            &[9104, 9105, 9103],
            &[8014, 8017, 8299],
        ],
        // Domination
        8100 => &[
            &[8112, 8128, 9923],
            &[8126, 8139, 8143],
            &[8137, 8140, 8141],
            &[8135, 8105, 8106],
        ],
        // Sorcery
        8200 => &[
            &[8214, 8229, 8230],
            &[8224, 8226, 8275],
            &[8210, 8234, 8233],
            &[8237, 8232, 8236],
        ],
        // Inspiration
        8300 => &[
            &[8351, 8360, 8369],
            &[8306, 8304, 8321],
            &[8313, 8352, 8345],
            &[8347, 8410, 8316],
        ],
        // Resolve
        8400 => &[
            &[8437, 8439, 8465],
            &[8446, 8463, 8401],
            &[8429, 8444, 8473],
            &[8451, 8453, 8242],
        ],
        _ => return None,
    };
    Some(rows.iter().map(|r| r.to_vec()).collect())
}

/// Static game data the stats views need to draw names and icons, from the local client
/// (`/lol-game-data/assets/v1/{items,perks,perkstyles,summoner-spells}.json`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameDataEntry {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// LCU asset path for the icon (fetched and cached by `hypatia-core::assets`).
    pub icon_path: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GameData {
    pub items: Vec<GameDataEntry>,
    pub perks: Vec<GameDataEntry>,
    pub perk_styles: Vec<GameDataEntry>,
    pub spells: Vec<GameDataEntry>,
    /// Item ids the client files under `Boots`, every tier.
    #[serde(default)]
    pub boots: Vec<i64>,
    /// Spatial row/slot layout of each rune style, parsed from `perkstyles.json`.
    #[serde(default)]
    pub perk_style_slots: Vec<RuneStyleSlots>,
}

impl GameData {
    /// Returns the spatial rows (keystone + 3 perk rows) for `style_id`, falling back to the
    /// built-in layout when the client's `perkstyles.json` is unavailable.
    pub fn style_rows(&self, style_id: i64) -> Option<Vec<Vec<i64>>> {
        self.perk_style_slots
            .iter()
            .find(|s| s.style_id == style_id && !s.rows.is_empty())
            .map(|s| s.rows.clone())
            .or_else(|| builtin_rune_style_slots(style_id))
    }
}

/// Why a stats lookup produced nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatsUnavailable {
    /// `Settings::online_stats` is off; the view offers to turn it on.
    OnlineDisabled,
    /// The League client is not running, so local data cannot be read.
    ClientOffline,
    /// The source answered but has no data for this query (new champion, rare role, few games).
    NoData,
    /// Network or parse failure; the message is already localized.
    Failed(String),
}

pub type StatsResult<T> = Result<T, StatsUnavailable>;

/// What a view draws for one piece of Champion Stats. Reads never block: `Loading` means a fetch
/// is under way and the Change Signal will be bumped when it lands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StatsState<T> {
    Loading,
    Ready(T),
    Unavailable(StatsUnavailable),
}

impl<T> StatsState<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

impl<T> From<StatsResult<T>> for StatsState<T> {
    fn from(result: StatsResult<T>) -> Self {
        match result {
            Ok(value) => Self::Ready(value),
            Err(why) => Self::Unavailable(why),
        }
    }
}

/// A provider of global aggregates. Implemented in `hypatia-core` (online provider, demo
/// provider); views only see this trait. Implementations cache per patch and never block the UI.
pub trait StatsSource: Send + Sync {
    fn build(&self, query: &StatsQuery) -> StatsResult<ChampionBuild>;
    fn tier_list(&self, queue: StatsQueue, bracket: RankBracket) -> StatsResult<TierList>;
    /// Matchups of `query.champion_id` in `query.position`, best for them first.
    fn matchups(&self, query: &StatsQuery) -> StatsResult<Vec<Matchup>>;
}

/// Names of game data ids (items, perks, styles, spells) in the client's language, with the
/// built-in names as fallback. Shared by the desktop Stats page and the Mobile Remote.
#[derive(Debug, Clone, Default)]
pub struct GameDataNames {
    items: std::collections::HashMap<i64, String>,
    perks: std::collections::HashMap<i64, String>,
    styles: std::collections::HashMap<i64, String>,
    spells: std::collections::HashMap<i64, String>,
}

impl GameDataNames {
    pub fn new(data: Option<&GameData>) -> Self {
        let index = |entries: Option<&Vec<GameDataEntry>>| {
            entries
                .map(|e| e.iter().map(|x| (x.id, x.name.clone())).collect())
                .unwrap_or_default()
        };
        Self {
            items: index(data.map(|d| &d.items)),
            perks: index(data.map(|d| &d.perks)),
            styles: index(data.map(|d| &d.perk_styles)),
            spells: index(data.map(|d| &d.spells)),
        }
    }

    /// The name of `id` in `category` (`item`, `perk`, `style`, `spell`), never empty.
    pub fn name(&self, category: &str, id: i64, lang: crate::i18n::Lang) -> String {
        use crate::game_text::{game_name, GameTextKind};
        let known = match category {
            "item" => self.items.get(&id),
            "perk" => self.perks.get(&id),
            "style" => self.styles.get(&id),
            _ => self.spells.get(&id),
        };
        if let Some(name) = known.filter(|n| !n.is_empty()) {
            return name.clone();
        }
        // Without client data (offline, demo), Game Text still names every id it knows.
        let (kind, generic) = match category {
            "item" => (GameTextKind::Item, lang.pick("Item", "Objet")),
            "perk" => (GameTextKind::Perk, lang.pick("Rune", "Rune")),
            "style" => (GameTextKind::PerkStyle, lang.pick("Rune path", "Voie")),
            _ => (GameTextKind::Spell, lang.pick("Spell", "Sort")),
        };
        game_name(kind, id, lang)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{generic} {id}"))
    }
}

/// The role to show stats for `champion_id`: the current one if they play it, else the role the
/// player played them in most, else their first catalog role.
pub fn default_stats_position(
    champion_id: i64,
    current: Option<Position>,
    profile: Option<&PlayerProfile>,
) -> Option<Position> {
    let roles: Vec<Position> = crate::champions::builtin_champion_roles(champion_id)
        .iter()
        .filter_map(|r| Position::parse(r))
        .collect();
    if let Some(p) = current.filter(|p| roles.contains(p)) {
        return Some(p);
    }
    if let Some(profile) = profile {
        let mut counts: std::collections::HashMap<Position, u32> = std::collections::HashMap::new();
        for g in profile
            .games
            .iter()
            .filter(|g| g.champion_id == champion_id)
        {
            if let Some(p) = g.position.filter(|p| *p != Position::Fill) {
                *counts.entry(p).or_default() += 1;
            }
        }
        if let Some((p, _)) = counts
            .into_iter()
            .max_by_key(|(p, n)| (*n, -p.desktop_index()))
        {
            return Some(p);
        }
    }
    roles.first().copied().or(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    #[test]
    fn default_role_prefers_the_current_then_the_catalog() {
        // Ahri is a mid laner in the catalog.
        assert_eq!(default_stats_position(103, None, None), Some(Position::Mid));
        assert_eq!(
            default_stats_position(103, Some(Position::Top), None),
            Some(Position::Mid)
        );
    }

    #[test]
    fn names_fall_back_to_game_text() {
        let names = GameDataNames::new(None);
        assert_eq!(names.name("spell", 4, Lang::Fr), "Saut éclair");
        assert_eq!(names.name("item", 3089, Lang::Fr), "Coiffe de Rabadon");
        assert_eq!(names.name("item", 999_999, Lang::En), "Item 999999");
    }
}
