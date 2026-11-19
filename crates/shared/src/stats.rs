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

/// A build path stage: starting items, core items, boots, situational options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemStage {
    Starting,
    Core,
    Boots,
    Situational,
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
    /// The player's most played rune page and final items on this champion.
    pub runes: Option<RuneSetup>,
    pub items: Vec<i64>,
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
}

/// Games shorter than this are remakes: listed, but left out of every average.
pub const REMAKE_SECS: u32 = 300;

impl PlayerGame {
    /// A ranked or normal game on Summoner's Rift, with lanes.
    pub fn on_summoners_rift(&self) -> bool {
        self.queue.is_some_and(StatsQueue::on_summoners_rift)
    }
}

/// The games My Stats totals and per-champion summaries are built from: Summoner's Rift games
/// when there are any, so ARAM and event modes (with very different damage and farm) do not
/// skew the rates; otherwise every game. Remakes never count. Order is kept.
pub fn summarised_games(games: &[PlayerGame]) -> Vec<&PlayerGame> {
    let played = || games.iter().filter(|g| g.duration_secs >= REMAKE_SECS);
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

/// The My Stats profile: recent games and the per-champion summary derived from them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub origin: StatsOrigin,
    pub games: Vec<PlayerGame>,
    pub champions: Vec<PlayerChampionStats>,
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
        use crate::champions::{
            keystone_name_from_id_localized, rune_style_name_localized,
            spell_name_from_id_localized,
        };
        let known = match category {
            "item" => self.items.get(&id),
            "perk" => self.perks.get(&id),
            "style" => self.styles.get(&id),
            _ => self.spells.get(&id),
        };
        if let Some(name) = known.filter(|n| !n.is_empty()) {
            return name.clone();
        }
        // Built-in names cover keystones, styles and spells; `-1` gives each table's unknown name.
        let builtin = |lookup: fn(i64, crate::i18n::Lang) -> &'static str| {
            let name = lookup(id, lang);
            (name != lookup(-1, lang)).then(|| name.to_string())
        };
        let (known, generic) = match category {
            "perk" => (
                builtin(keystone_name_from_id_localized),
                lang.pick("Rune", "Rune"),
            ),
            "style" => (
                builtin(rune_style_name_localized),
                lang.pick("Rune path", "Voie"),
            ),
            "spell" => (
                builtin(spell_name_from_id_localized),
                lang.pick("Spell", "Sort"),
            ),
            _ => (None, lang.pick("Item", "Objet")),
        };
        known.unwrap_or_else(|| format!("{generic} {id}"))
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
    fn names_fall_back_to_builtin_tables() {
        let names = GameDataNames::new(None);
        assert_eq!(names.name("spell", 4, Lang::Fr), "Saut éclair");
        assert_eq!(names.name("item", 3089, Lang::En), "Item 3089");
    }
}
