//! Champ-select helper: the build, lane matchup and team comps for the player's own pick.
//!
//! Projected once into [`ChampSelectSummary::helper`](crate::summary::ChampSelectSummary) by
//! [`project_match_with`](crate::summary::project_match_with), so the desktop and the Mobile
//! Remote show the same thing. Champion Stats come in through [`HelperStats`] (implemented by
//! `hypatia-core`'s `ChampionStats`); team comps, lane pairing and tips come from the offline
//! [`crate::comp`] engine.
//!
//! Sources, first available wins, each value keeping its [`StatsOrigin`]:
//! - runes and spells: Riot client recommended page, then the Online build, then the player's
//!   own history on the champion (Local);
//! - core items: the Online build's core block, then the player's own history (Local);
//! - lane matchup win rate: Online matchups only.
//!
//! The helper names champions only, never players: Ranked Solo/Duo allies stay as anonymized
//! as Riot leaves them. Missing data resolves to `None`, zeros and empty lists.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::champions::{
    is_champion_placeholder, keystone_name_from_id_localized, rune_style_name_localized,
    spell_name_from_id_localized, ChampionLookup, Position,
};
use crate::comp::{comp_advice, lane_opponents, team_profile};
use crate::i18n::Lang;
use crate::stats::{
    Attribution, ChampionBuild, CompTrait, GameData, GameDataEntry, ItemStage, MatchupList,
    PlayerProfile, Rates, StatsOrigin, StatsQuery, StatsQueue, StatsState, StatsUnavailable,
};
use crate::summary::ChampSelectSummary;

/// Core items the helper shows.
pub const HELPER_ITEMS: usize = 3;

/// Champion Stats as the helper reads them. Every call is non-blocking: `Loading` means the
/// data lands later and the projection is computed again then.
pub trait HelperStats {
    fn online_enabled(&self) -> bool;
    /// Riot client recommended rune page and spells (origin `RiotClient`, or `Demo`).
    fn recommended_build(&self, query: &StatsQuery) -> StatsState<ChampionBuild>;
    /// Global build from the opt-in online provider (origin `Online`, or `Demo`).
    fn online_build(&self, query: &StatsQuery) -> StatsState<ChampionBuild>;
    /// Global matchups of `query.champion_id` from the online provider.
    fn matchups(&self, query: &StatsQuery) -> StatsState<MatchupList>;
    /// The player's own recent games (origin `Local`, or `Demo`).
    fn player_profile(&self) -> StatsState<PlayerProfile>;
    /// Client names for items, runes, rune styles and spells.
    fn game_data(&self) -> StatsState<Arc<GameData>>;
}

/// An id with its display name in the active language (empty when unknown).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HelperEntry {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HelperRunes {
    pub origin: StatsOrigin,
    pub keystone: HelperEntry,
    pub primary_style: HelperEntry,
    pub sub_style: HelperEntry,
    /// Six perks then three shards, as `RemoteCommand::SaveRunePage` takes them; shorter when
    /// the source gave no shards.
    pub selected_perk_ids: Vec<i64>,
    /// Win rate of this page when the source has rates.
    pub win_rate: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HelperSpells {
    pub origin: StatsOrigin,
    pub first: HelperEntry,
    pub second: HelperEntry,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HelperItems {
    pub origin: StatsOrigin,
    /// At most [`HELPER_ITEMS`], in build order.
    pub items: Vec<HelperEntry>,
    pub win_rate: Option<f32>,
}

/// The player's lane opponent, inferred by the comp engine (enemy positions are hidden).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelperLane {
    pub position: Position,
    pub enemy_champion_id: i64,
    pub enemy_champion_name: String,
    /// The player's champion's win rate in this matchup, when Online data has it.
    pub win_rate: Option<f32>,
    pub games: u32,
    pub origin: Option<StatsOrigin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelperTrait {
    pub kind: CompTrait,
    pub label: String,
    /// A gap (`LacksFrontline`, `AllMagic`...) rather than a strength.
    pub gap: bool,
}

/// One team's comp for the damage split bar and trait chips.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HelperComp {
    /// Known champions the profile rests on.
    pub champions: u32,
    /// Shares in `0.0..=1.0`, summing to ~1.0 (all zero with no known champion).
    pub physical: f32,
    pub magic: f32,
    pub true_damage: f32,
    pub traits: Vec<HelperTrait>,
}

/// What Online stats are doing for the helper.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "state", content = "message")]
pub enum HelperOnline {
    /// Switched off in Settings: only local data shows, with a hint to turn it on.
    #[default]
    Off,
    Loading,
    Ready,
    /// On, but the provider has nothing for this champion or queue.
    NoData,
    /// On, but the provider only covers Summoner's Rift ranked games, not this mode (ARAM, Arena).
    OtherMode,
    /// On, but the fetch failed; the message is already localized.
    Failed(String),
}

/// The champ-select helper block of the Match Projection.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChampSelectHelper {
    /// The player's hovered or locked champion, `0` before any.
    pub champion_id: i64,
    pub champion_name: String,
    pub locked: bool,
    /// Assigned position; `None` in blind modes, ARAM and Fill.
    pub position: Option<Position>,
    /// The champion's global win rate, when Online (or Demo) data has it.
    pub win_rate: Option<f32>,
    pub runes: Option<HelperRunes>,
    pub spells: Option<HelperSpells>,
    pub items: Option<HelperItems>,
    pub lane: Option<HelperLane>,
    pub my_comp: HelperComp,
    pub their_comp: HelperComp,
    /// At most [`crate::comp::MAX_TIPS`], most important first.
    pub tips: Vec<String>,
    pub online: HelperOnline,
    /// The provider link to show while any value above is Online.
    pub attribution: Option<Attribution>,
}

impl ChampSelectHelper {
    /// True when a shown value came from the online provider.
    pub fn shows_online(&self) -> bool {
        let online = |o: Option<StatsOrigin>| o == Some(StatsOrigin::Online);
        online(self.runes.as_ref().map(|r| r.origin))
            || online(self.spells.as_ref().map(|s| s.origin))
            || online(self.items.as_ref().map(|i| i.origin))
            || online(self.lane.as_ref().and_then(|l| l.origin))
    }
}

/// Player-facing name of a value's origin.
pub fn stats_origin_label(origin: StatsOrigin, lang: Lang) -> &'static str {
    match origin {
        StatsOrigin::Local => lang.pick("Your games", "Vos parties"),
        StatsOrigin::RiotClient => lang.pick("Riot client", "Client Riot"),
        StatsOrigin::Online => lang.pick("Online", "En ligne"),
        StatsOrigin::Demo => lang.pick("Demo", "Démo"),
    }
}

/// Player-facing name of a comp trait.
pub fn comp_trait_label(t: CompTrait, lang: Lang) -> &'static str {
    match t {
        CompTrait::Engage => lang.pick("Engage", "Engagement"),
        CompTrait::Peel => lang.pick("Peel", "Protection"),
        CompTrait::Poke => lang.pick("Poke", "Harcèlement"),
        CompTrait::Splitpush => lang.pick("Splitpush", "Split push"),
        CompTrait::Teamfight => lang.pick("Teamfight", "Combat d'équipe"),
        CompTrait::Pick => lang.pick("Catch", "Attrape"),
        CompTrait::Scaling => lang.pick("Late game", "Fin de partie"),
        CompTrait::EarlyGame => lang.pick("Early game", "Début de partie"),
        CompTrait::Frontline => lang.pick("Frontline", "Ligne avant"),
        CompTrait::LacksFrontline => lang.pick("No frontline", "Pas de ligne avant"),
        CompTrait::LacksEngage => lang.pick("No engage", "Pas d'engagement"),
        CompTrait::AllPhysical => lang.pick("All physical", "Tout physique"),
        CompTrait::AllMagic => lang.pick("All magic", "Tout magique"),
    }
}

fn is_gap(t: CompTrait) -> bool {
    matches!(
        t,
        CompTrait::LacksFrontline
            | CompTrait::LacksEngage
            | CompTrait::AllPhysical
            | CompTrait::AllMagic
    )
}

/// Queue the helper asks stats for, from the lobby's queue id (the champ-select session carries
/// no queue), else the session's queue type. Summoner's Rift drafts read Ranked Solo/Duo
/// aggregates, the largest sample; ARAM and Arena have their own.
pub fn helper_queue(queue_id: i64, queue_type: &str) -> StatsQueue {
    match StatsQueue::from_queue_id(queue_id) {
        Some(q @ (StatsQueue::Aram | StatsQueue::Arena)) => return q,
        Some(_) => return StatsQueue::RankedSolo,
        None => {}
    }
    let q = queue_type.to_ascii_uppercase();
    if q.contains("ARAM") {
        StatsQueue::Aram
    } else if q.contains("CHERRY") || q.contains("ARENA") {
        StatsQueue::Arena
    } else {
        StatsQueue::RankedSolo
    }
}

/// Boots are their own build stage, not one of the three core items. The client's own list
/// ([`GameData::boots`]) wins; this one covers a client that has not answered yet.
const BOOTS: &[i64] = &[
    1001, 2422, 3005, 3006, 3009, 3010, 3013, 3020, 3047, 3111, 3117, 3158, 3170, 3171, 3172, 3173,
    3174, 3175, 3176,
];

fn comp(ids: &[i64], lang: Lang) -> HelperComp {
    let profile = team_profile(ids);
    HelperComp {
        champions: ids
            .iter()
            .filter(|&&id| crate::comp::champion_traits(id).is_some())
            .count() as u32,
        physical: profile.physical,
        magic: profile.magic,
        true_damage: profile.true_damage,
        traits: profile
            .traits
            .into_iter()
            .map(|kind| HelperTrait {
                kind,
                label: comp_trait_label(kind, lang).to_string(),
                gap: is_gap(kind),
            })
            .collect(),
    }
}

fn rate(r: &Rates) -> Option<f32> {
    (r.games > 0 && r.win_rate > 0.0).then_some(r.win_rate)
}

/// Names from the client's game data, with the built-in tables as fallback.
struct Names<'a> {
    data: Option<&'a GameData>,
    lang: Lang,
}

impl Names<'_> {
    fn find(list: Option<&Vec<GameDataEntry>>, id: i64) -> Option<String> {
        list?
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.name.clone())
            .filter(|n| !n.is_empty())
    }

    fn perk(&self, id: i64) -> HelperEntry {
        let name = Self::find(self.data.map(|d| &d.perks), id)
            .unwrap_or_else(|| keystone_name_from_id_localized(id, self.lang).to_string());
        HelperEntry { id, name }
    }

    fn style(&self, id: i64) -> HelperEntry {
        let name = Self::find(self.data.map(|d| &d.perk_styles), id)
            .unwrap_or_else(|| rune_style_name_localized(id, self.lang).to_string());
        HelperEntry { id, name }
    }

    fn spell(&self, id: i64) -> HelperEntry {
        let name = Self::find(self.data.map(|d| &d.spells), id)
            .unwrap_or_else(|| spell_name_from_id_localized(id, self.lang).to_string());
        HelperEntry { id, name }
    }

    fn is_boots(&self, id: i64) -> bool {
        match self.data.map(|d| &d.boots).filter(|b| !b.is_empty()) {
            Some(boots) => boots.contains(&id),
            None => BOOTS.contains(&id),
        }
    }

    fn item(&self, id: i64) -> HelperEntry {
        HelperEntry {
            id,
            name: Self::find(self.data.map(|d| &d.items), id).unwrap_or_default(),
        }
    }
}

fn runes_from(build: &ChampionBuild, names: &Names) -> Option<HelperRunes> {
    let (page, rates) = build.runes.first()?;
    if page.primary_style_id <= 0 || page.sub_style_id <= 0 {
        return None;
    }
    let keystone = *page.perk_ids.first()?;
    let mut selected_perk_ids = page.perk_ids.clone();
    selected_perk_ids.extend(&page.shard_ids);
    Some(HelperRunes {
        origin: build.origin,
        keystone: names.perk(keystone),
        primary_style: names.style(page.primary_style_id),
        sub_style: names.style(page.sub_style_id),
        selected_perk_ids,
        win_rate: rate(rates),
    })
}

fn spells_from(build: &ChampionBuild, names: &Names) -> Option<HelperSpells> {
    let (pair, _) = build.spells.first()?;
    (pair.first > 0 && pair.second > 0).then(|| HelperSpells {
        origin: build.origin,
        first: names.spell(pair.first),
        second: names.spell(pair.second),
    })
}

fn items_from(build: &ChampionBuild, names: &Names) -> Option<HelperItems> {
    let block = build
        .items
        .iter()
        .find(|b| b.stage == Some(ItemStage::Core) && !b.item_ids.is_empty())?;
    Some(HelperItems {
        origin: build.origin,
        items: block
            .item_ids
            .iter()
            .take(HELPER_ITEMS)
            .map(|&id| names.item(id))
            .collect(),
        win_rate: rate(&block.rates),
    })
}

/// The player's own runes, spells and items on `champion_id`, from their recent games.
fn local_build(
    profile: &PlayerProfile,
    champion_id: i64,
    names: &Names,
) -> (
    Option<HelperRunes>,
    Option<HelperSpells>,
    Option<HelperItems>,
) {
    let Some(stats) = profile
        .champions
        .iter()
        .find(|c| c.champion_id == champion_id)
    else {
        return (None, None, None);
    };
    let runes = stats.runes.as_ref().and_then(|page| {
        if page.primary_style_id <= 0 || page.sub_style_id <= 0 {
            return None;
        }
        let keystone = *page.perk_ids.first()?;
        let mut selected_perk_ids = page.perk_ids.clone();
        selected_perk_ids.extend(&page.shard_ids);
        Some(HelperRunes {
            origin: profile.origin,
            keystone: names.perk(keystone),
            primary_style: names.style(page.primary_style_id),
            sub_style: names.style(page.sub_style_id),
            selected_perk_ids,
            win_rate: None,
        })
    });
    // Most played spell pair on the champion, newest game first on ties.
    let mut pairs: Vec<((i64, i64), u32)> = Vec::new();
    for g in profile
        .games
        .iter()
        .filter(|g| g.champion_id == champion_id)
    {
        let key = (g.spells.first, g.spells.second);
        if key.0 <= 0 || key.1 <= 0 {
            continue;
        }
        match pairs.iter_mut().find(|(k, _)| *k == key) {
            Some((_, n)) => *n += 1,
            None => pairs.push((key, 1)),
        }
    }
    let best = pairs
        .iter()
        .enumerate()
        .max_by_key(|(i, (_, n))| (*n, usize::MAX - i))
        .map(|(_, p)| p.0);
    let spells = best.map(|(a, b)| HelperSpells {
        origin: profile.origin,
        first: names.spell(a),
        second: names.spell(b),
    });
    let core: Vec<HelperEntry> = stats
        .items
        .iter()
        .filter(|&&id| !names.is_boots(id))
        .take(HELPER_ITEMS)
        .map(|&id| names.item(id))
        .collect();
    let items = (!core.is_empty()).then_some(HelperItems {
        origin: profile.origin,
        items: core,
        win_rate: None,
    });
    (runes, spells, items)
}

/// The player's champion: the locked or hovered pick of their own slot, else the champion their
/// pick action is hovering.
fn own_champion(
    cs: &ChampSelectSummary,
    champions: &dyn ChampionLookup,
    lang: Lang,
) -> (i64, String, bool) {
    if let Some(me) = cs
        .my_team
        .iter()
        .find(|m| m.is_self)
        .filter(|m| m.has_champion())
    {
        return (me.champion_id, me.champion_name.clone(), me.pick_locked);
    }
    if let Some(act) = cs.local_action.as_ref() {
        if act.action_type == "pick"
            && act.champion_id > 0
            && !is_champion_placeholder(&act.champion_name)
        {
            let name = if act.champion_name.trim().is_empty() {
                champions.resolve_champion_name_localized(act.champion_id, lang)
            } else {
                act.champion_name.clone()
            };
            return (act.champion_id, name, act.completed);
        }
    }
    (0, String::new(), false)
}

/// Builds the helper block for `cs`. Without `stats` (tests, no service) only the offline comp
/// engine contributes.
pub fn project_champ_select_helper(
    cs: &ChampSelectSummary,
    queue_id: i64,
    champions: &dyn ChampionLookup,
    stats: Option<&dyn HelperStats>,
    lang: Lang,
) -> ChampSelectHelper {
    let (champion_id, champion_name, locked) = own_champion(cs, champions, lang);
    let queue = helper_queue(queue_id, &cs.queue_type);
    let me = cs.my_team.iter().find(|m| m.is_self);
    let position = me
        .and_then(|m| Position::parse(&m.assigned_position))
        .filter(|p| *p != Position::Fill);

    // Comps read champions only; a slot whose identity Riot hides still shows its champion.
    let ally_picks: Vec<(i64, Option<Position>)> = cs
        .my_team
        .iter()
        .map(|m| {
            let id = if m.is_self {
                champion_id
            } else if m.has_champion() {
                m.champion_id
            } else {
                0
            };
            (id, Position::parse(&m.assigned_position))
        })
        .collect();
    let ally_ids: Vec<i64> = ally_picks
        .iter()
        .map(|p| p.0)
        .filter(|&id| id > 0)
        .collect();
    let enemy_ids: Vec<i64> = cs
        .their_team
        .iter()
        .filter(|e| e.champion_id > 0 && !is_champion_placeholder(&e.champion_name))
        .map(|e| e.champion_id)
        .collect();

    let mut helper = ChampSelectHelper {
        champion_id,
        champion_name,
        locked,
        position,
        my_comp: comp(&ally_ids, lang),
        their_comp: comp(&enemy_ids, lang),
        tips: comp_advice(&ally_ids, &enemy_ids, lang),
        ..ChampSelectHelper::default()
    };

    // Lanes exist only on Summoner's Rift.
    if champion_id > 0 && queue.on_summoners_rift() {
        let enemy_picks: Vec<(i64, Option<Position>)> =
            enemy_ids.iter().map(|&id| (id, None)).collect();
        helper.lane = lane_opponents(&ally_picks, &enemy_picks)
            .into_iter()
            .find(|(ally, _, _)| *ally == champion_id)
            .map(|(_, enemy, lane)| HelperLane {
                position: lane,
                enemy_champion_id: enemy,
                enemy_champion_name: cs
                    .their_team
                    .iter()
                    .find(|e| e.champion_id == enemy)
                    .map(|e| e.champion_name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| champions.resolve_champion_name_localized(enemy, lang)),
                win_rate: None,
                games: 0,
                origin: None,
            });
    }

    let Some(stats) = stats else {
        return helper;
    };
    if champion_id <= 0 {
        helper.online = if stats.online_enabled() {
            HelperOnline::Ready
        } else {
            HelperOnline::Off
        };
        return helper;
    }

    let query = StatsQuery {
        champion_id,
        position,
        queue,
        ..StatsQuery::default()
    };
    let game_data = stats.game_data();
    let names = Names {
        data: game_data.ready().map(|g| &**g),
        lang,
    };

    // The Online provider is champion-wide: asking without a position shares its cache with
    // the Stats page and keeps one request per champion when the inferred lane moves.
    let champion_wide = StatsQuery {
        position: None,
        ..query.clone()
    };
    let recommended = stats.recommended_build(&query);
    let online = stats.online_build(&champion_wide);
    let recommended = recommended.ready();
    let online_build = online.ready();

    helper.online = match &online {
        StatsState::Ready(_) => HelperOnline::Ready,
        StatsState::Loading => HelperOnline::Loading,
        StatsState::Unavailable(StatsUnavailable::OnlineDisabled) => HelperOnline::Off,
        StatsState::Unavailable(StatsUnavailable::Failed(m)) => HelperOnline::Failed(m.clone()),
        StatsState::Unavailable(_) if !queue.on_summoners_rift() => HelperOnline::OtherMode,
        StatsState::Unavailable(_) => HelperOnline::NoData,
    };
    helper.win_rate = online_build.and_then(|b| rate(&b.rates));

    helper.runes = recommended
        .and_then(|b| runes_from(b, &names))
        .or_else(|| online_build.and_then(|b| runes_from(b, &names)));
    helper.spells = recommended
        .and_then(|b| spells_from(b, &names))
        .or_else(|| online_build.and_then(|b| spells_from(b, &names)));
    helper.items = online_build.and_then(|b| items_from(b, &names));

    if helper.runes.is_none() || helper.spells.is_none() || helper.items.is_none() {
        let profile = stats.player_profile();
        if let Some(profile) = profile.ready() {
            let (runes, spells, items) = local_build(profile, champion_id, &names);
            helper.runes = helper.runes.take().or(runes);
            helper.spells = helper.spells.take().or(spells);
            helper.items = helper.items.take().or(items);
        }
    }

    let mut matchup_attribution = None;
    if let Some(lane) = helper.lane.as_mut() {
        if let StatsState::Ready(list) = stats.matchups(&champion_wide) {
            if let Some(m) = list
                .entries
                .iter()
                .find(|m| m.opponent_id == lane.enemy_champion_id)
            {
                lane.win_rate = rate(&m.rates);
                lane.games = m.rates.games;
                lane.origin = lane.win_rate.map(|_| m.origin);
                matchup_attribution = list.attribution.clone();
            }
        }
    }

    if helper.shows_online() {
        let online = |o: Option<StatsOrigin>| o == Some(StatsOrigin::Online);
        let build_online = online(helper.runes.as_ref().map(|r| r.origin))
            || online(helper.spells.as_ref().map(|s| s.origin))
            || online(helper.items.as_ref().map(|i| i.origin));
        let build_attribution = online_build.and_then(|b| b.attribution.clone());
        // Link the page the Online values on screen came from.
        helper.attribution = if build_online {
            build_attribution.or(matchup_attribution)
        } else {
            matchup_attribution.or(build_attribution)
        };
    }
    helper
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::champions::BuiltinChampionCatalog;
    use crate::stats::{ItemBlock, Matchup, PlayerChampionStats, PlayerGame, RuneSetup, SpellPair};
    use crate::summary::{project_match, project_match_with, MatchInputs, SummarySnapshot};
    use serde_json::{json, Value};

    /// Answers every Champion Stats read from fixed states.
    struct FakeStats {
        online: bool,
        recommended: StatsState<ChampionBuild>,
        build: StatsState<ChampionBuild>,
        matchups: StatsState<MatchupList>,
        profile: StatsState<PlayerProfile>,
        game_data: StatsState<Arc<GameData>>,
    }

    impl Default for FakeStats {
        fn default() -> Self {
            fn off<T>() -> StatsState<T> {
                StatsState::Unavailable(StatsUnavailable::OnlineDisabled)
            }
            fn offline<T>() -> StatsState<T> {
                StatsState::Unavailable(StatsUnavailable::ClientOffline)
            }
            Self {
                online: false,
                recommended: offline(),
                build: off(),
                matchups: off(),
                profile: offline(),
                game_data: offline(),
            }
        }
    }

    impl HelperStats for FakeStats {
        fn online_enabled(&self) -> bool {
            self.online
        }
        fn recommended_build(&self, _: &StatsQuery) -> StatsState<ChampionBuild> {
            self.recommended.clone()
        }
        fn online_build(&self, _: &StatsQuery) -> StatsState<ChampionBuild> {
            self.build.clone()
        }
        fn matchups(&self, _: &StatsQuery) -> StatsState<MatchupList> {
            self.matchups.clone()
        }
        fn player_profile(&self) -> StatsState<PlayerProfile> {
            self.profile.clone()
        }
        fn game_data(&self) -> StatsState<Arc<GameData>> {
            self.game_data.clone()
        }
    }

    fn ahri_page() -> RuneSetup {
        RuneSetup {
            primary_style_id: 8100,
            sub_style_id: 8200,
            perk_ids: vec![8112, 8139, 8140, 8106, 8226, 8210],
            shard_ids: vec![5008, 5008, 5001],
        }
    }

    fn rates(win: f32, games: u32) -> Rates {
        Rates {
            win_rate: win,
            pick_rate: 0.1,
            ban_rate: 0.0,
            games,
        }
    }

    fn client_build() -> ChampionBuild {
        ChampionBuild {
            origin: StatsOrigin::RiotClient,
            runes: vec![(ahri_page(), Rates::default())],
            spells: vec![(
                SpellPair {
                    first: 4,
                    second: 14,
                },
                Rates::default(),
            )],
            ..ChampionBuild::default()
        }
    }

    fn online_build() -> ChampionBuild {
        let mut page = ahri_page();
        page.perk_ids[0] = 8229;
        ChampionBuild {
            origin: StatsOrigin::Online,
            rates: rates(0.512, 70_000),
            runes: vec![(page, rates(0.52, 30_000))],
            spells: vec![(
                SpellPair {
                    first: 4,
                    second: 12,
                },
                rates(0.51, 50_000),
            )],
            items: vec![
                ItemBlock {
                    stage: Some(ItemStage::Starting),
                    item_ids: vec![1056],
                    rates: rates(0.5, 1),
                },
                ItemBlock {
                    stage: Some(ItemStage::Core),
                    item_ids: vec![3118, 4645, 3089, 3157],
                    rates: rates(0.534, 20_000),
                },
            ],
            attribution: Some(Attribution {
                provider: "MetaBot.GG".into(),
                url: "https://metabot.gg/en/league/champion/Ahri/build".into(),
            }),
            ..ChampionBuild::default()
        }
    }

    /// The player (cell 2, Ahri mid) with four allies whose identities Riot hides, against
    /// Darius, Leona and Lux.
    fn draft(anonymized: bool) -> Value {
        let ally = |cell: i64, pos: &str, champ: i64| {
            if anonymized {
                json!({"cellId": cell, "assignedPosition": pos, "championId": champ, "summonerId": 0, "puuid": "", "nameVisibilityType": "HIDDEN"})
            } else {
                json!({"cellId": cell, "assignedPosition": pos, "championId": champ, "summonerId": 10 + cell, "puuid": format!("puuid-{cell}"), "gameName": format!("Mate{cell}"), "tagLine": "EUW"})
            }
        };
        json!({
            "localPlayerCellId": 2,
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 20_000},
            "myTeam": [
                ally(0, "TOP", 266),
                ally(1, "JUNGLE", 64),
                {"cellId": 2, "assignedPosition": "MIDDLE", "championId": 103, "summonerId": 1, "puuid": "p-self"},
                ally(3, "BOTTOM", 222),
                ally(4, "UTILITY", 0)
            ],
            "theirTeam": [
                {"cellId": 5, "championId": 122},
                {"cellId": 6, "championId": 89},
                {"cellId": 7, "championId": 99},
                {"cellId": 8, "championId": 0},
                {"cellId": 9, "championId": 0}
            ]
        })
    }

    fn project(cs: &Value, stats: Option<&dyn HelperStats>, lang: Lang) -> SummarySnapshot {
        project_in(420, cs, stats, lang)
    }

    /// Projects `cs` in the lobby's queue `queue_id`; the champ-select session carries none.
    fn project_in(
        queue_id: i64,
        cs: &Value,
        stats: Option<&dyn HelperStats>,
        lang: Lang,
    ) -> SummarySnapshot {
        let lobby = json!({"queueId": queue_id});
        project_match_with(
            &MatchInputs {
                lobby: Some(&lobby),
                champ_select: Some(cs),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            stats,
            lang,
        )
    }

    fn helper(snap: &SummarySnapshot) -> &ChampSelectHelper {
        &snap.champ_select.as_ref().expect("champ select").helper
    }

    #[test]
    fn an_empty_draft_is_neutral() {
        let cs = json!({
            "localPlayerCellId": 0,
            "myTeam": [
                {"cellId": 0, "assignedPosition": "", "championId": 0, "summonerId": 1, "puuid": "p-self"},
                {"cellId": 1, "assignedPosition": "", "championId": 0, "summonerId": 2, "puuid": "b"}
            ]
        });
        let full = FakeStats {
            online: true,
            recommended: StatsState::Ready(client_build()),
            build: StatsState::Ready(online_build()),
            ..FakeStats::default()
        };
        for stats in [None, Some(&full as &dyn HelperStats)] {
            let snap = project(&cs, stats, Lang::En);
            let h = helper(&snap);
            assert_eq!(h.champion_id, 0);
            assert_eq!(h.champion_name, "");
            assert_eq!(h.position, None);
            assert!(
                h.runes.is_none() && h.spells.is_none() && h.items.is_none() && h.lane.is_none()
            );
            assert_eq!(h.my_comp, HelperComp::default());
            assert_eq!(h.their_comp, HelperComp::default());
            assert!(h.tips.is_empty());
            assert_eq!(h.attribution, None);
        }
        // No draft at all: no helper either.
        let none = project_match(&MatchInputs::default(), &BuiltinChampionCatalog, Lang::En);
        assert!(none.champ_select.is_none());
    }

    #[test]
    fn placeholder_ids_never_count() {
        let cs = json!({
            "localPlayerCellId": 0,
            "myTeam": [
                {"cellId": 0, "assignedPosition": "MIDDLE", "championId": 0, "championName": "Selecting...", "summonerId": 1, "puuid": "p-self"},
                {"cellId": 1, "assignedPosition": "TOP", "championId": 0, "championName": "Sélection...", "summonerId": 2, "puuid": "b"},
                {"cellId": 2, "assignedPosition": "JUNGLE", "championId": -1, "summonerId": 3, "puuid": "c"}
            ],
            "theirTeam": [{"cellId": 5, "championId": 0}, {"cellId": 6, "championId": 0, "championName": "None"}]
        });
        let snap = project(&cs, Some(&FakeStats::default()), Lang::Fr);
        let h = helper(&snap);
        assert_eq!(h.champion_id, 0);
        assert_eq!(h.position, Some(Position::Mid));
        assert_eq!(h.my_comp.champions, 0);
        assert_eq!(h.their_comp.champions, 0);
        assert!(h.lane.is_none() && h.tips.is_empty());
        assert_eq!(h.online, HelperOnline::Off);
    }

    #[test]
    fn ranked_solo_allies_stay_anonymous() {
        let cs = draft(true);
        let snap = project(&cs, None, Lang::En);
        let p = snap.champ_select.as_ref().unwrap();
        assert!(p.is_ranked_solo_anonymized);
        let h = &p.helper;
        // Their champions still shape the comp...
        assert_eq!(h.my_comp.champions, 4);
        assert_eq!(h.their_comp.champions, 3);
        // ...but the helper names no player at all.
        let text = serde_json::to_string(h).unwrap();
        for ally in p.my_team.iter().filter(|a| !a.is_self) {
            assert!(ally.is_anonymized);
            if !ally.summoner_name.is_empty() {
                assert!(
                    !text.contains(&ally.summoner_name),
                    "{} leaked",
                    ally.summoner_name
                );
            }
        }
        assert!(!text.contains("puuid") && !text.contains("p-self"));

        // Revealed teammates do not leak into the helper either.
        let snap = project(&draft(false), None, Lang::En);
        let text = serde_json::to_string(helper(&snap)).unwrap();
        assert!(!text.contains("Mate") && !text.contains("puuid-"));
    }

    #[test]
    fn pairs_the_lane_and_reads_both_comps() {
        let snap = project(&draft(true), None, Lang::Fr);
        let h = helper(&snap);
        assert_eq!(
            (h.champion_id, h.champion_name.as_str(), h.locked),
            (103, "Ahri", true)
        );
        assert_eq!(h.position, Some(Position::Mid));
        // Leona takes support, so Lux is the mid laner.
        let lane = h.lane.as_ref().expect("lane opponent");
        assert_eq!(
            (
                lane.position,
                lane.enemy_champion_id,
                lane.enemy_champion_name.as_str()
            ),
            (Position::Mid, 99, "Lux")
        );
        assert_eq!(lane.win_rate, None);
        let sum = h.my_comp.physical + h.my_comp.magic + h.my_comp.true_damage;
        assert!((sum - 1.0).abs() < 0.01);
        assert!(h.my_comp.traits.iter().all(|t| !t.label.is_empty()));
        assert!(h.tips.len() <= crate::comp::MAX_TIPS);
        // Without the stats service nothing online is claimed.
        assert!(h.runes.is_none() && h.attribution.is_none());
    }

    #[test]
    fn client_page_first_then_online_items_with_the_provider_link() {
        let stats = FakeStats {
            online: true,
            recommended: StatsState::Ready(client_build()),
            build: StatsState::Ready(online_build()),
            matchups: StatsState::Ready(MatchupList {
                entries: vec![Matchup {
                    champion_id: 103,
                    opponent_id: 99,
                    position: Some(Position::Mid),
                    rates: rates(0.471, 3_200),
                    gold_diff_15: None,
                    origin: StatsOrigin::Online,
                }],
                origin: StatsOrigin::Online,
                ..MatchupList::default()
            }),
            game_data: StatsState::Ready(Arc::new(GameData {
                items: vec![GameDataEntry {
                    id: 3118,
                    name: "Malignance".into(),
                    ..Default::default()
                }],
                ..GameData::default()
            })),
            ..FakeStats::default()
        };
        let snap = project(&draft(true), Some(&stats), Lang::En);
        let h = helper(&snap);
        assert_eq!(h.online, HelperOnline::Ready);
        assert_eq!(h.win_rate, Some(0.512));

        let runes = h.runes.as_ref().unwrap();
        assert_eq!(
            runes.origin,
            StatsOrigin::RiotClient,
            "the client's page comes first"
        );
        assert_eq!(
            runes.keystone,
            HelperEntry {
                id: 8112,
                name: "Electrocute".into()
            }
        );
        assert_eq!(runes.sub_style.name, "Sorcery");
        assert_eq!(runes.selected_perk_ids.len(), 9);
        assert_eq!(h.spells.as_ref().unwrap().second.name, "Ignite");

        let items = h.items.as_ref().unwrap();
        assert_eq!(items.origin, StatsOrigin::Online);
        assert_eq!(
            items.items.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![3118, 4645, 3089]
        );
        assert_eq!(items.items[0].name, "Malignance");
        assert_eq!(items.win_rate, Some(0.534));

        let lane = h.lane.as_ref().unwrap();
        assert_eq!(
            (lane.win_rate, lane.games, lane.origin),
            (Some(0.471), 3_200, Some(StatsOrigin::Online))
        );
        assert!(h.shows_online());
        assert_eq!(h.attribution.as_ref().unwrap().provider, "MetaBot.GG");

        // The client page has not landed yet: the Online page fills in, still labelled Online.
        let stats = FakeStats {
            recommended: StatsState::Loading,
            ..stats
        };
        let snap = project(&draft(true), Some(&stats), Lang::En);
        let runes = helper(&snap).runes.clone().unwrap();
        assert_eq!(
            (runes.origin, runes.keystone.id),
            (StatsOrigin::Online, 8229)
        );
    }

    #[test]
    fn online_off_shows_only_local_data_and_no_link() {
        let game = |spells: (i64, i64)| PlayerGame {
            champion_id: 103,
            spells: SpellPair {
                first: spells.0,
                second: spells.1,
            },
            ..PlayerGame::default()
        };
        let profile = PlayerProfile {
            origin: StatsOrigin::Local,
            games: vec![game((4, 12)), game((4, 14)), game((4, 14))],
            champions: vec![PlayerChampionStats {
                champion_id: 103,
                runes: Some(ahri_page()),
                // Tier-1, tier-2 and tier-3 boots (3175 Spellslinger's Shoes) are all skipped.
                items: vec![1001, 3020, 3175, 3118, 4645, 3089, 3157],
                ..PlayerChampionStats::default()
            }],
        };
        let stats = FakeStats {
            profile: StatsState::Ready(profile),
            ..FakeStats::default()
        };
        let snap = project(&draft(true), Some(&stats), Lang::Fr);
        let h = helper(&snap);
        assert_eq!(h.online, HelperOnline::Off);
        assert_eq!(h.win_rate, None);
        let runes = h.runes.as_ref().unwrap();
        assert_eq!(
            (runes.origin, runes.keystone.name.as_str()),
            (StatsOrigin::Local, "Électrocution")
        );
        let spells = h.spells.as_ref().unwrap();
        assert_eq!(
            (spells.first.id, spells.second.id),
            (4, 14),
            "the most played pair"
        );
        let items = h.items.as_ref().unwrap();
        assert_eq!(items.origin, StatsOrigin::Local);
        assert_eq!(
            items.items.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![3118, 4645, 3089],
            "boots are not core"
        );
        assert!(!h.shows_online());
        assert_eq!(h.attribution, None);
        assert_eq!(h.lane.as_ref().unwrap().win_rate, None);

        // A failed fetch says why, in the active language.
        let stats = FakeStats {
            online: true,
            build: StatsState::Unavailable(StatsUnavailable::Failed("Hors ligne".into())),
            ..FakeStats::default()
        };
        let snap = project(&draft(true), Some(&stats), Lang::Fr);
        assert_eq!(
            helper(&snap).online,
            HelperOnline::Failed("Hors ligne".into())
        );
    }

    #[test]
    fn a_hovered_champion_drives_the_helper_before_lock_in() {
        let cs = json!({
            "localPlayerCellId": 0,
            "actions": [[
                {"id": 10, "actorCellId": 0, "type": "pick", "championId": 134, "completed": false, "isInProgress": true}
            ]],
            "myTeam": [{"cellId": 0, "assignedPosition": "MIDDLE", "championId": 0, "summonerId": 1, "puuid": "p-self"}]
        });
        let snap = project(&cs, None, Lang::En);
        let h = helper(&snap);
        assert_eq!(h.champion_id, 134);
        assert_eq!(h.champion_name, "Syndra");
        assert!(!h.locked);
    }

    #[test]
    fn a_declared_pick_intent_drives_the_helper_during_planning() {
        let cs = json!({
            "localPlayerCellId": 0,
            "timer": {"phase": "PLANNING"},
            "myTeam": [{"cellId": 0, "assignedPosition": "MIDDLE", "championId": 0, "championPickIntent": 134, "summonerId": 1, "puuid": "p-self"}]
        });
        let h = helper(&project(&cs, None, Lang::En)).clone();
        assert_eq!(
            (h.champion_id, h.champion_name.as_str(), h.locked),
            (134, "Syndra", false)
        );
    }

    #[test]
    fn aram_and_arena_have_no_lanes_and_say_why_nothing_comes_online() {
        let stats = FakeStats {
            online: true,
            build: StatsState::Unavailable(StatsUnavailable::NoData),
            matchups: StatsState::Unavailable(StatsUnavailable::NoData),
            ..FakeStats::default()
        };
        for queue_id in [450, 1700] {
            let snap = project_in(queue_id, &draft(false), Some(&stats), Lang::En);
            let h = helper(&snap);
            assert_eq!(h.champion_id, 103);
            assert_eq!(h.lane, None, "queue {queue_id} has no lanes");
            assert_eq!(h.online, HelperOnline::OtherMode);
            assert!(h.my_comp.champions > 0, "comps still read");
        }
        // Summoner's Rift with nothing online is plain NoData, and the lane stays.
        let snap = project_in(420, &draft(false), Some(&stats), Lang::En);
        assert_eq!(helper(&snap).online, HelperOnline::NoData);
        assert!(helper(&snap).lane.is_some());
    }

    #[test]
    fn the_clients_boots_list_wins_over_the_builtin_one() {
        let profile = PlayerProfile {
            origin: StatsOrigin::Local,
            champions: vec![PlayerChampionStats {
                champion_id: 103,
                items: vec![3020, 3118, 4645, 3089],
                ..PlayerChampionStats::default()
            }],
            ..PlayerProfile::default()
        };
        let stats = FakeStats {
            profile: StatsState::Ready(profile),
            game_data: StatsState::Ready(Arc::new(GameData {
                boots: vec![3118],
                ..GameData::default()
            })),
            ..FakeStats::default()
        };
        let snap = project(&draft(true), Some(&stats), Lang::En);
        let ids: Vec<i64> = helper(&snap)
            .items
            .as_ref()
            .unwrap()
            .items
            .iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids, vec![3020, 4645, 3089]);
    }

    #[test]
    fn a_page_without_both_trees_is_skipped() {
        let mut half = client_build();
        half.runes[0].0.sub_style_id = 0;
        let stats = FakeStats {
            online: true,
            recommended: StatsState::Ready(half),
            build: StatsState::Ready(online_build()),
            ..FakeStats::default()
        };
        let snap = project(&draft(true), Some(&stats), Lang::En);
        let runes = helper(&snap).runes.clone().unwrap();
        assert_eq!(
            (runes.origin, runes.sub_style.id),
            (StatsOrigin::Online, 8200)
        );
    }

    #[test]
    fn labels_read_in_both_languages() {
        for t in [
            CompTrait::Engage,
            CompTrait::Peel,
            CompTrait::Poke,
            CompTrait::Splitpush,
            CompTrait::Teamfight,
            CompTrait::Pick,
            CompTrait::Scaling,
            CompTrait::EarlyGame,
            CompTrait::Frontline,
            CompTrait::LacksFrontline,
            CompTrait::LacksEngage,
            CompTrait::AllPhysical,
            CompTrait::AllMagic,
        ] {
            assert!(
                !comp_trait_label(t, Lang::En).is_empty()
                    && !comp_trait_label(t, Lang::Fr).is_empty()
            );
        }
        for o in [
            StatsOrigin::Local,
            StatsOrigin::RiotClient,
            StatsOrigin::Online,
            StatsOrigin::Demo,
        ] {
            assert!(!stats_origin_label(o, Lang::Fr).is_empty());
        }
        assert_eq!(helper_queue(0, "ARAM_UNRANKED_5x5"), StatsQueue::Aram);
        assert_eq!(helper_queue(0, "RANKED_FLEX_SR"), StatsQueue::RankedSolo);
        assert_eq!(helper_queue(450, ""), StatsQueue::Aram);
        assert_eq!(helper_queue(1700, ""), StatsQueue::Arena);
        assert_eq!(helper_queue(440, ""), StatsQueue::RankedSolo);
        assert_eq!(helper_queue(0, ""), StatsQueue::RankedSolo);
    }
}
