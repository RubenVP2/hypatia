//! The Mobile Remote's Stats tab: read-only views of Champion Stats for the paired phone.
//!
//! Each view reads [`crate::stats::ChampionStats`] through its non-blocking getters and turns the
//! ids into names, the way the desktop Stats page does. Nothing here writes: no `RemoteCommand`,
//! no Riot call beyond what the getters already make with `GET`, and no settings change (the
//! phone cannot turn Online stats on). A `Loading` block is filled again when the fetch lands:
//! the Change Signal bumps, the broadcaster pushes a snapshot with a new `stats_revision`, and
//! the phone asks again.

use std::collections::HashMap;

use hypatia_shared::links::{self, DPM_TIER_LIST};
use hypatia_shared::{
    aggregate_local_matchups, builtin_champion_roles, builtin_rune_style_slots, champion_traits,
    comp_trait_label, default_stats_position, normalize_champion_key, summarised_games,
    Attribution, ChampionBuild, ChampionLookup, GameData, GameDataNames, ItemStage, LaneVerdict,
    Lang, LocalMatchup, MatchBadge, Position, RankBracket, Rates, RoleSummary, RuneSetup, Skill,
    StatsOrigin, StatsQuery, StatsQueue, StatsState, StatsUnavailable, Tier, STAT_SHARD_ROWS,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Champions offered under the search field: the player's most played.
const PICKS: usize = 8;
/// Opponents per matchup column when collapsed.
const MATCHUPS: usize = 8;

/// What the phone asks for, all optional: the server picks the same defaults as the desktop.
#[derive(Debug, Deserialize, Default)]
pub struct StatsParams {
    pub champion: Option<i64>,
    /// Any Riot role token (`TOP`, `MIDDLE`, `UTILITY`, ...).
    pub position: Option<String>,
    /// Queue: `ranked_solo`, `ranked_flex`, `aram`, `arena` (or `all` for profile filter).
    pub queue: Option<String>,
    /// Rank bracket: `all`, `emerald_plus`, `diamond_plus`, `master_plus`.
    pub bracket: Option<String>,
    /// Role filter (Tier list or My Stats); absent or `ALL` for every role.
    pub role: Option<String>,
    /// Text filter for Tier List or Matchups.
    pub search: Option<String>,
    /// Tier List sort column (`tier`, `win`, `pick`, `ban`, `games`).
    pub sort: Option<String>,
    /// Sort direction (`asc` or `desc`).
    pub dir: Option<String>,
    /// Include low-pickrate off-meta entries in Tier List.
    pub off_meta: Option<bool>,
    /// Expand all matchups beyond the default 8 per column.
    pub all: Option<bool>,
    /// `en` or `fr`, for champion names and built-in fallbacks; the app language otherwise.
    pub lang: Option<String>,
}

impl StatsParams {
    fn lang(&self, app: &AppState) -> Lang {
        self.lang
            .as_deref()
            .and_then(Lang::from_code)
            .unwrap_or_else(|| app.lang())
    }
}

/// One block's state, with its origin and, for Online data only, the provider credit.
#[derive(Debug, Serialize, PartialEq)]
pub struct Block {
    /// `Ready`, `Loading`, `OnlineDisabled`, `ClientOffline`, `NoData` or `Failed`.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub origin: Option<StatsOrigin>,
    pub attribution: Option<Attribution>,
}

fn block<T>(
    state: &StatsState<T>,
    meta: impl Fn(&T) -> (StatsOrigin, Option<Attribution>),
) -> Block {
    match state {
        StatsState::Ready(value) => {
            let (origin, attr) = meta(value);
            let attribution =
                attr.filter(|a| origin == StatsOrigin::Online && links::is_allowed(&a.url));
            Block {
                state: "Ready",
                message: None,
                origin: Some(origin),
                attribution,
            }
        }
        StatsState::Loading => Block {
            state: "Loading",
            message: None,
            origin: None,
            attribution: None,
        },
        StatsState::Unavailable(why) => {
            let (state, message) = match why {
                StatsUnavailable::OnlineDisabled => ("OnlineDisabled", None),
                StatsUnavailable::ClientOffline => ("ClientOffline", None),
                StatsUnavailable::NoData => ("NoData", None),
                StatsUnavailable::Failed(message) => ("Failed", Some(message.clone())),
            };
            Block {
                state,
                message,
                origin: None,
                attribution: None,
            }
        }
    }
}

/// An id with its display name (champion, item, rune, spell).
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Named {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ChampionRef {
    pub id: i64,
    pub name: String,
    pub roles: Vec<Position>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct RuneNodeView {
    pub id: i64,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct RunePageView {
    /// `popular`, `alternative`, `client` (Riot's recommended page), or `local` (player's own).
    pub kind: &'static str,
    pub origin: StatsOrigin,
    pub rates: Option<Rates>,
    /// The client's own recommended page is this one.
    pub matches_client: bool,
    pub primary: Named,
    pub secondary: Named,
    pub keystone: Option<Named>,
    pub primary_perks: Vec<Named>,
    pub secondary_perks: Vec<Named>,
    pub shards: Vec<Named>,
    /// Spatial primary tree rows (4 rows: keystone + 3 perk rows), when style slots are known.
    pub primary_rows: Vec<Vec<RuneNodeView>>,
    /// Spatial secondary tree rows (3 regular perk rows), when style slots are known.
    pub secondary_rows: Vec<Vec<RuneNodeView>>,
    /// Spatial 3x3 stat shard rows.
    pub shard_rows: Vec<Vec<RuneNodeView>>,
}

#[derive(Debug, Serialize)]
pub struct SpellRowView {
    pub origin: StatsOrigin,
    pub first: Named,
    pub second: Named,
    pub rates: Option<Rates>,
}

#[derive(Debug, Serialize)]
pub struct ItemStageView {
    pub stage: Option<ItemStage>,
    pub items: Vec<Named>,
    pub rates: Option<Rates>,
}

#[derive(Debug, Serialize)]
pub struct SkillView {
    pub origin: StatsOrigin,
    pub levels: Vec<Skill>,
    pub priority: Vec<Skill>,
    pub rates: Option<Rates>,
}

/// Player's own local stats & most-used kit on the selected champion.
#[derive(Debug, Serialize)]
pub struct LocalBuildView {
    pub origin: StatsOrigin,
    pub games: u32,
    pub wins: u32,
    pub kills: f32,
    pub deaths: f32,
    pub assists: f32,
    pub cs_per_min: f32,
    pub dpm: f32,
    pub keystone: Option<Named>,
    pub spells: Option<[Named; 2]>,
    pub items: Vec<Named>,
}

#[derive(Debug, Serialize)]
pub struct ChampProfileView {
    pub physical: f32,
    pub magic: f32,
    pub true_damage: f32,
    pub traits: Vec<String>,
    pub local_origin: Option<StatsOrigin>,
    pub gpm: Option<f32>,
    pub vision_per_min: Option<f32>,
}

/// Champion Build: the global build (Online or Demo) beside the client's recommended page and
/// the player's own local games on that champion.
#[derive(Debug, Serialize)]
pub struct BuildView {
    pub champion: Option<ChampionRef>,
    pub position: Option<Position>,
    pub queue: StatsQueue,
    pub bracket: RankBracket,
    pub picks: Vec<ChampionRef>,
    pub dpm_url: Option<String>,
    pub build: Block,
    pub client: Block,
    pub rates: Option<Rates>,
    pub role_distribution: Vec<(Position, f32)>,
    pub tier_rank: Option<(Tier, u32, u32)>,
    pub local_build: Option<LocalBuildView>,
    pub champ_profile: Option<ChampProfileView>,
    pub rune_pages: Vec<RunePageView>,
    pub spells: Vec<SpellRowView>,
    pub items: Vec<ItemStageView>,
    pub items_origin: Option<StatsOrigin>,
    pub skills: Option<SkillView>,
    pub alt_skills: Vec<SkillView>,
}

#[derive(Debug, Serialize)]
pub struct TierRow {
    pub champion: ChampionRef,
    /// The role of the row (Ranked Solo/Duo only).
    pub position: Option<Position>,
    pub tier: Tier,
    pub rates: Rates,
    pub win_rate_delta: Option<f32>,
    pub lane_presence: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct TierView {
    pub queue: StatsQueue,
    pub bracket: RankBracket,
    pub role: Option<Position>,
    pub patch: String,
    pub dpm_url: String,
    pub block: Block,
    pub entries: Vec<TierRow>,
}

#[derive(Debug, Serialize)]
pub struct MatchupRow {
    pub opponent: ChampionRef,
    pub rates: Rates,
    pub gold_diff_15: Option<f32>,
    pub verdict: LaneVerdict,
    pub local: Option<LocalMatchup>,
}

#[derive(Debug, Serialize)]
pub struct LocalMatchupView {
    pub opponent: ChampionRef,
    pub games: u32,
    pub wins: u32,
    pub kda: f32,
}

#[derive(Debug, Serialize)]
pub struct MatchupView {
    pub champion: Option<ChampionRef>,
    pub position: Option<Position>,
    pub picks: Vec<ChampionRef>,
    pub dpm_url: Option<String>,
    pub block: Block,
    pub hardest: Vec<MatchupRow>,
    pub easiest: Vec<MatchupRow>,
    pub total_entries: usize,
    pub local_origin: Option<StatsOrigin>,
    pub local_matchups: Vec<LocalMatchupView>,
}

#[derive(Debug, Serialize)]
pub struct RankedStanding {
    pub tier: String,
    pub division: String,
    pub lp: i32,
    pub wins: u32,
    pub losses: u32,
}

#[derive(Debug, Serialize)]
pub struct GameRow {
    pub champion: Named,
    pub win: bool,
    pub remake: bool,
    pub position: Option<Position>,
    pub queue: Option<StatsQueue>,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub cs: u32,
    pub damage_to_champions: u32,
    pub vision_score: u32,
    pub duration_secs: u32,
    pub started_at_ms: i64,
    /// Counted in the totals ([`summarised_games`]); ARAM, event modes and remakes are only listed.
    pub summarised: bool,
    pub opponent: Option<Named>,
    pub keystone: Option<Named>,
    pub spells: Option<[Named; 2]>,
    pub items: Vec<Named>,
    pub kill_participation: Option<f32>,
    pub damage_share: Option<f32>,
    pub performance_score: Option<u8>,
    pub placement: Option<u8>,
    pub badge: Option<MatchBadge>,
}

#[derive(Debug, Serialize)]
pub struct ChampionSummaryRow {
    pub champion: Named,
    pub games: u32,
    pub wins: u32,
    pub kills: f32,
    pub deaths: f32,
    pub assists: f32,
    pub dpm: f32,
    pub cs_per_min: f32,
    pub keystone: Option<Named>,
    pub items: Vec<Named>,
}

/// My Stats: the player's ranked standing, role performance, recent games and per-champion summary.
#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub block: Block,
    pub rank: Option<RankedStanding>,
    pub roles: Vec<RoleSummary>,
    pub games: Vec<GameRow>,
    pub champions: Vec<ChampionSummaryRow>,
}

/// Turns ids into names for one response.
struct Namer<'a> {
    app: &'a AppState,
    lang: Lang,
    names: GameDataNames,
    game_data: Option<std::sync::Arc<GameData>>,
}

impl<'a> Namer<'a> {
    fn new(app: &'a AppState, lang: Lang, with_game_data: bool) -> Self {
        let data = with_game_data.then(|| app.stats.game_data_in(lang));
        let game_data = data.and_then(|d| d.ready().cloned());
        let names = GameDataNames::new(game_data.as_deref());
        Self {
            app,
            lang,
            names,
            game_data,
        }
    }

    fn named(&self, category: &str, id: i64) -> Named {
        Named {
            id,
            name: self.names.name(category, id, self.lang),
        }
    }

    fn champion_name(&self, id: i64) -> Named {
        Named {
            id,
            name: self
                .app
                .assets
                .resolve_champion_name_localized(id, self.lang),
        }
    }

    fn champion(&self, id: i64) -> ChampionRef {
        ChampionRef {
            id,
            name: self
                .app
                .assets
                .resolve_champion_name_localized(id, self.lang),
            roles: catalog_roles(id),
        }
    }

    fn style_rows(&self, style_id: i64) -> Option<Vec<Vec<i64>>> {
        self.game_data
            .as_deref()
            .and_then(|g| g.style_rows(style_id))
            .or_else(|| builtin_rune_style_slots(style_id))
    }

    fn rune_page(
        &self,
        kind: &'static str,
        page: &RuneSetup,
        origin: StatsOrigin,
        rates: &Rates,
        matches_client: bool,
    ) -> RunePageView {
        let perks = &page.perk_ids;
        let slice =
            |from: usize, to: usize| perks.get(from..to.min(perks.len())).unwrap_or_default();
        let primary_rows = self
            .style_rows(page.primary_style_id)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|id| RuneNodeView {
                                id,
                                name: self.names.name("perk", id, self.lang),
                                active: perks.contains(&id),
                            })
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let secondary_rows = self
            .style_rows(page.sub_style_id)
            .map(|rows| {
                rows.into_iter()
                    .skip(1)
                    .map(|row| {
                        row.into_iter()
                            .map(|id| RuneNodeView {
                                id,
                                name: self.names.name("perk", id, self.lang),
                                active: perks.contains(&id),
                            })
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let shard_rows = if page.shard_ids.is_empty() {
            Vec::new()
        } else {
            STAT_SHARD_ROWS
                .iter()
                .enumerate()
                .map(|(row_idx, row)| {
                    let chosen = page.shard_ids.get(row_idx).copied();
                    row.iter()
                        .map(|&id| RuneNodeView {
                            id,
                            name: self.names.name("perk", id, self.lang),
                            active: chosen == Some(id),
                        })
                        .collect()
                })
                .collect()
        };
        RunePageView {
            kind,
            origin,
            rates: some_rates(rates),
            matches_client,
            primary: self.named("style", page.primary_style_id),
            secondary: self.named("style", page.sub_style_id),
            keystone: perks.first().map(|&id| self.named("perk", id)),
            primary_perks: slice(1, 4)
                .iter()
                .map(|&id| self.named("perk", id))
                .collect(),
            secondary_perks: slice(4, 6)
                .iter()
                .map(|&id| self.named("perk", id))
                .collect(),
            shards: page
                .shard_ids
                .iter()
                .map(|&id| self.named("perk", id))
                .collect(),
            primary_rows,
            secondary_rows,
            shard_rows,
        }
    }
}

fn catalog_roles(id: i64) -> Vec<Position> {
    builtin_champion_roles(id)
        .iter()
        .filter_map(|r| Position::parse(r))
        .collect()
}

/// Rates worth showing, or `None` when the source gave none (the client's pages).
fn some_rates(rates: &Rates) -> Option<Rates> {
    (rates.games > 0 || rates.pick_rate > 0.0 || rates.win_rate > 0.0).then_some(*rates)
}

fn parse_position(raw: Option<&str>) -> Option<Position> {
    raw.and_then(Position::parse)
        .filter(|p| *p != Position::Fill)
}

fn parse_queue(raw: Option<&str>) -> StatsQueue {
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("ranked_flex") | Some("flex") => StatsQueue::RankedFlex,
        Some("normal") => StatsQueue::Normal,
        Some("aram") => StatsQueue::Aram,
        Some("arena") => StatsQueue::Arena,
        _ => StatsQueue::RankedSolo,
    }
}

fn parse_bracket(raw: Option<&str>) -> RankBracket {
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("all") => RankBracket::All,
        Some("diamond_plus") | Some("diamond") => RankBracket::DiamondPlus,
        Some("master_plus") | Some("master") => RankBracket::MasterPlus,
        _ => RankBracket::EmeraldPlus,
    }
}

/// The champion and role a Build or Matchups request is about: the asked one, else the
/// champion of the current game, else the player's most played (as the desktop page starts).
fn selection(app: &AppState, params: &StatsParams) -> (i64, Option<Position>) {
    let profile = app.stats.player_profile();
    let asked = parse_position(params.position.as_deref());
    let (id, current) = match params.champion.filter(|id| *id > 0) {
        Some(id) => (id, asked),
        None => {
            let snapshot = app.get_summary();
            let in_game = snapshot
                .champ_select
                .as_ref()
                .and_then(|cs| cs.my_team.iter().find(|a| a.is_self))
                .filter(|me| me.champion_id > 0)
                .map(|me| (me.champion_id, Position::parse(&me.assigned_position)))
                .or_else(|| {
                    snapshot
                        .live_game
                        .as_ref()
                        .filter(|g| g.active_champion_id > 0)
                        .map(|g| (g.active_champion_id, None))
                });
            match in_game {
                Some((id, pos)) => (id, asked.or(pos.filter(|p| *p != Position::Fill))),
                None => (
                    profile
                        .ready()
                        .and_then(|p| p.champions.first())
                        .map_or(0, |c| c.champion_id),
                    asked,
                ),
            }
        }
    };
    if id <= 0 {
        return (0, None);
    }
    // An asked role is kept even off-catalog; otherwise the role the player most likely wants.
    let position = asked.or_else(|| default_stats_position(id, current, profile.ready()));
    (id, position)
}

fn picks(app: &AppState, namer: &Namer<'_>) -> Vec<ChampionRef> {
    match app.stats.player_profile() {
        StatsState::Ready(p) => p
            .champions
            .iter()
            .take(PICKS)
            .map(|c| namer.champion(c.champion_id))
            .collect(),
        _ => Vec::new(),
    }
}

fn query(
    champion_id: i64,
    position: Option<Position>,
    queue: StatsQueue,
    bracket: RankBracket,
) -> StatsQuery {
    StatsQuery {
        champion_id,
        position,
        queue,
        bracket,
        patch: None,
    }
}

/// The champion roster for the search field, named in `lang`.
pub fn champions_view(app: &AppState, params: &StatsParams) -> Vec<ChampionRef> {
    let lang = params.lang(app);
    let mut out: Vec<ChampionRef> = app
        .assets
        .champion_catalog(lang)
        .into_iter()
        .filter(|c| c.id > 0)
        .map(|c| ChampionRef {
            id: c.id,
            name: app.assets.resolve_champion_name_localized(c.id, lang),
            roles: c
                .roles
                .iter()
                .filter_map(|r| Position::parse(r))
                .filter(|p| *p != Position::Fill)
                .collect(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn build_view(app: &AppState, params: &StatsParams) -> BuildView {
    let lang = params.lang(app);
    let namer = Namer::new(app, lang, true);
    let queue = parse_queue(params.queue.as_deref());
    let bracket = parse_bracket(params.bracket.as_deref());
    let (champion_id, position) = selection(app, params);
    let picks = picks(app, &namer);
    if champion_id <= 0 {
        return BuildView {
            champion: None,
            position: None,
            queue,
            bracket,
            picks,
            dpm_url: None,
            build: block(
                &StatsState::<()>::Unavailable(StatsUnavailable::NoData),
                |_| (StatsOrigin::Local, None),
            ),
            client: block(
                &StatsState::<()>::Unavailable(StatsUnavailable::NoData),
                |_| (StatsOrigin::Local, None),
            ),
            rates: None,
            role_distribution: Vec::new(),
            tier_rank: None,
            local_build: None,
            champ_profile: None,
            rune_pages: Vec::new(),
            spells: Vec::new(),
            items: Vec::new(),
            items_origin: None,
            skills: None,
            alt_skills: Vec::new(),
        };
    }
    let q = query(champion_id, position, queue, bracket);
    let build_state = app.stats.build(&q);
    let client_state = app.stats.recommended_build(&q);
    let profile_state = app.stats.player_profile();
    let build_block = block(&build_state, |b: &ChampionBuild| {
        (b.origin, b.attribution.clone())
    });
    let client_block = block(&client_state, |b: &ChampionBuild| (b.origin, None));
    let build = build_state.ready();
    let client = client_state.ready();
    let profile = profile_state.ready();

    // Local player build for this champion, if they have played it.
    let local_champ = profile.and_then(|p| {
        p.champions
            .iter()
            .find(|c| c.champion_id == champion_id)
            .map(|c| (p.origin, c))
    });
    let local_build = local_champ.map(|(origin, c)| LocalBuildView {
        origin,
        games: c.games,
        wins: c.wins,
        kills: c.kills,
        deaths: c.deaths,
        assists: c.assists,
        cs_per_min: c.cs_per_min,
        dpm: c.dpm,
        keystone: c
            .runes
            .as_ref()
            .and_then(|r| r.perk_ids.first())
            .map(|&id| namer.named("perk", id)),
        spells: c.spells.map(|s| {
            [
                namer.named("spell", s.first),
                namer.named("spell", s.second),
            ]
        }),
        items: c
            .items
            .iter()
            .take(6)
            .map(|&id| namer.named("item", id))
            .collect(),
    });

    // Rune pages: the global ones first, then the client's own, then the player's own if distinct.
    let client_page = client.and_then(|c| c.runes.first().map(|(page, _)| page));
    let mut rune_pages = Vec::new();
    if let Some(b) = build {
        for (i, (page, rates)) in b.runes.iter().take(2).enumerate() {
            let kind = if i == 0 { "popular" } else { "alternative" };
            rune_pages.push(namer.rune_page(
                kind,
                page,
                b.origin,
                rates,
                client_page == Some(page),
            ));
        }
    }
    if let (Some(c), Some(page)) = (client, client_page) {
        let shown = build.is_some_and(|b| b.runes.iter().take(2).any(|(p, _)| p == page));
        if !shown {
            rune_pages.push(namer.rune_page("client", page, c.origin, &Rates::default(), false));
        }
    }
    if let Some((origin, c)) = local_champ {
        if let Some(page) = c.runes.as_ref() {
            let local_rates = Rates {
                win_rate: c.wins as f32 / c.games.max(1) as f32,
                pick_rate: 0.0,
                ban_rate: 0.0,
                games: c.games,
            };
            rune_pages.push(namer.rune_page("local", page, origin, &local_rates, false));
        }
    }

    // Summoner spells: top 2 global pairs, then client's when distinct.
    let mut spells = Vec::new();
    if let Some(b) = build {
        for (pair, rates) in b.spells.iter().take(2) {
            spells.push(SpellRowView {
                origin: b.origin,
                first: namer.named("spell", pair.first),
                second: namer.named("spell", pair.second),
                rates: some_rates(rates),
            });
        }
    }
    if let Some(c) = client {
        if let Some((pair, _)) = c.spells.first() {
            let same = build.is_some_and(|b| b.spells.iter().take(2).any(|(p, _)| p == pair));
            if !same {
                spells.push(SpellRowView {
                    origin: c.origin,
                    first: namer.named("spell", pair.first),
                    second: namer.named("spell", pair.second),
                    rates: None,
                });
            }
        }
    }

    if spells.is_empty() {
        if let Some((origin, c)) = local_champ {
            if let Some(pair) = c.spells {
                spells.push(SpellRowView {
                    origin,
                    first: namer.named("spell", pair.first),
                    second: namer.named("spell", pair.second),
                    rates: None,
                });
            }
        }
    }

    let mut items_origin = build.map(|b| b.origin);
    let mut items: Vec<ItemStageView> = build
        .map(|b| {
            b.items
                .iter()
                .filter(|block| !block.item_ids.is_empty())
                .map(|block| ItemStageView {
                    stage: block.stage,
                    items: block
                        .item_ids
                        .iter()
                        .map(|&id| namer.named("item", id))
                        .collect(),
                    rates: some_rates(&block.rates),
                })
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        if let Some((origin, c)) = local_champ {
            if !c.items.is_empty() {
                items_origin = Some(origin);
                items.push(ItemStageView {
                    stage: Some(ItemStage::Core),
                    items: c
                        .items
                        .iter()
                        .take(6)
                        .map(|&id| namer.named("item", id))
                        .collect(),
                    rates: None,
                });
            }
        }
    }

    let skills = build
        .and_then(|b| b.skills.first().map(|s| (s, b.origin)))
        .or_else(|| client.and_then(|c| c.skills.first().map(|s| (s, c.origin))))
        .filter(|(s, _)| !s.levels.is_empty())
        .map(|(s, origin)| SkillView {
            origin,
            levels: s.levels.clone(),
            priority: s.priority.clone(),
            rates: some_rates(&s.rates),
        });
    let alt_skills = build
        .map(|b| {
            b.skills
                .iter()
                .skip(1)
                .take(2)
                .filter(|s| !s.priority.is_empty())
                .map(|s| SkillView {
                    origin: b.origin,
                    levels: s.levels.clone(),
                    priority: s.priority.clone(),
                    rates: some_rates(&s.rates),
                })
                .collect()
        })
        .unwrap_or_default();

    let champ_profile = champion_traits(champion_id).map(|ct| ChampProfileView {
        physical: f32::from(ct.physical) / 100.0,
        magic: f32::from(ct.magic) / 100.0,
        true_damage: f32::from(ct.true_damage) / 100.0,
        traits: ct
            .traits()
            .into_iter()
            .map(|t| comp_trait_label(t, lang).to_string())
            .collect(),
        local_origin: local_champ.map(|(o, _)| o),
        gpm: local_champ.map(|(_, c)| c.gold_per_min),
        vision_per_min: local_champ.map(|(_, c)| c.vision_per_min),
    });

    BuildView {
        champion: Some(namer.champion(champion_id)),
        position,
        queue,
        bracket,
        picks,
        dpm_url: links::dpm_build(champion_id),
        build: build_block,
        client: client_block,
        rates: build.and_then(|b| some_rates(&b.rates)),
        role_distribution: build
            .map(|b| b.role_distribution.clone())
            .unwrap_or_default(),
        tier_rank: build.and_then(|b| b.tier_rank),
        local_build,
        champ_profile,
        rune_pages,
        spells,
        items,
        items_origin,
        skills,
        alt_skills,
    }
}

pub fn tiers_view(app: &AppState, params: &StatsParams) -> TierView {
    let lang = params.lang(app);
    let namer = Namer::new(app, lang, false);
    let queue = match params
        .queue
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("aram") => StatsQueue::Aram,
        Some("arena") => StatsQueue::Arena,
        _ => StatsQueue::RankedSolo,
    };
    let bracket = parse_bracket(params.bracket.as_deref());
    let lanes = queue == StatsQueue::RankedSolo;
    let role = if lanes {
        parse_position(params.role.as_deref())
    } else {
        None
    };
    let search_key = params
        .search
        .as_deref()
        .map(normalize_champion_key)
        .unwrap_or_default();
    let include_off_meta = params.off_meta.unwrap_or(false);
    let sort_col = params.sort.as_deref().unwrap_or("tier");
    let sort_desc = match params.dir.as_deref() {
        Some("asc") => false,
        Some("desc") => true,
        _ => sort_col != "tier",
    };

    let state = app.stats.tier_list(queue, bracket);
    let block = block(&state, |t| (t.origin, t.attribution.clone()));
    let (patch, mut entries) = match state.ready() {
        Some(list) => (
            list.patch.clone(),
            list.entries
                .iter()
                .filter(|e| role.is_none_or(|r| e.position == r))
                .filter(|e| {
                    include_off_meta || e.rates.pick_rate == 0.0 || e.rates.pick_rate >= 0.005
                })
                .filter_map(|e| {
                    let champion = namer.champion(e.champion_id);
                    if !search_key.is_empty()
                        && !normalize_champion_key(&champion.name).contains(&search_key)
                    {
                        return None;
                    }
                    Some(TierRow {
                        champion,
                        position: lanes.then_some(e.position),
                        tier: e.tier,
                        rates: e.rates,
                        win_rate_delta: e.win_rate_delta,
                        lane_presence: e.lane_presence,
                    })
                })
                .collect(),
        ),
        None => (String::new(), Vec::new()),
    };

    entries.sort_by(|a, b| {
        let ord = match sort_col {
            "win" => a.rates.win_rate.total_cmp(&b.rates.win_rate),
            "pick" => a.rates.pick_rate.total_cmp(&b.rates.pick_rate),
            "ban" => a.rates.ban_rate.total_cmp(&b.rates.ban_rate),
            "games" => a.rates.games.cmp(&b.rates.games),
            _ => a
                .tier
                .cmp(&b.tier)
                .then_with(|| b.rates.win_rate.total_cmp(&a.rates.win_rate)),
        };
        if sort_col == "tier" {
            if sort_desc {
                ord.reverse()
            } else {
                ord
            }
        } else if sort_desc {
            ord.reverse()
        } else {
            ord
        }
    });

    TierView {
        queue,
        bracket,
        role,
        patch,
        dpm_url: DPM_TIER_LIST.to_string(),
        block,
        entries,
    }
}

pub fn matchups_view(app: &AppState, params: &StatsParams) -> MatchupView {
    let lang = params.lang(app);
    let namer = Namer::new(app, lang, false);
    let (champion_id, position) = selection(app, params);
    let picks = picks(app, &namer);
    if champion_id <= 0 {
        return MatchupView {
            champion: None,
            position: None,
            picks,
            dpm_url: None,
            block: block(
                &StatsState::<()>::Unavailable(StatsUnavailable::NoData),
                |_| (StatsOrigin::Local, None),
            ),
            hardest: Vec::new(),
            easiest: Vec::new(),
            total_entries: 0,
            local_origin: None,
            local_matchups: Vec::new(),
        };
    }
    let profile_state = app.stats.player_profile();
    let local_origin = profile_state.ready().map(|p| p.origin);
    let local_list = profile_state
        .ready()
        .map(|p| aggregate_local_matchups(&p.games, champion_id))
        .unwrap_or_default();
    let local_by_opp: HashMap<i64, LocalMatchup> = local_list
        .iter()
        .cloned()
        .map(|m| (m.opponent_id, m))
        .collect();

    let search_key = params
        .search
        .as_deref()
        .map(normalize_champion_key)
        .unwrap_or_default();
    let show_all = params.all.unwrap_or(false) || !search_key.is_empty();

    let local_matchups: Vec<LocalMatchupView> = local_list
        .into_iter()
        .filter_map(|m| {
            let opponent = namer.champion(m.opponent_id);
            if !search_key.is_empty()
                && !normalize_champion_key(&opponent.name).contains(&search_key)
            {
                return None;
            }
            Some(LocalMatchupView {
                opponent,
                games: m.games,
                wins: m.wins,
                kda: m.kda,
            })
        })
        .collect();

    let state = app.stats.matchups(&query(
        champion_id,
        position,
        StatsQueue::RankedSolo,
        RankBracket::default(),
    ));
    let block = block(&state, |m| (m.origin, m.attribution.clone()));
    let entries = state
        .ready()
        .map(|m| m.entries.as_slice())
        .unwrap_or_default();
    let total_entries = entries.len();
    let row = |m: &hypatia_shared::Matchup| {
        let opponent = namer.champion(m.opponent_id);
        if !search_key.is_empty() && !normalize_champion_key(&opponent.name).contains(&search_key) {
            return None;
        }
        Some(MatchupRow {
            opponent,
            rates: m.rates,
            gold_diff_15: m.gold_diff_15,
            verdict: LaneVerdict::from_matchup(m.rates.win_rate, m.gold_diff_15),
            local: local_by_opp.get(&m.opponent_id).cloned(),
        })
    };
    let mut hardest: Vec<_> = entries.iter().filter(|m| m.rates.win_rate < 0.5).collect();
    hardest.sort_by(|a, b| a.rates.win_rate.total_cmp(&b.rates.win_rate));
    let mut easiest: Vec<_> = entries.iter().filter(|m| m.rates.win_rate >= 0.5).collect();
    easiest.sort_by(|a, b| b.rates.win_rate.total_cmp(&a.rates.win_rate));
    let limit = if show_all { usize::MAX } else { MATCHUPS };
    MatchupView {
        champion: Some(namer.champion(champion_id)),
        position,
        picks,
        dpm_url: links::dpm_matchups(champion_id),
        block,
        hardest: hardest.into_iter().filter_map(row).take(limit).collect(),
        easiest: easiest.into_iter().filter_map(row).take(limit).collect(),
        total_entries,
        local_origin,
        local_matchups,
    }
}

pub fn profile_view(app: &AppState, params: &StatsParams) -> ProfileView {
    let lang = params.lang(app);
    let namer = Namer::new(app, lang, true);
    let state = app.stats.player_profile();
    let block = block(&state, |p| (p.origin, None));
    let rank = app.get_summary().summoner.as_ref().and_then(|s| {
        let tier = s
            .rank_tier
            .as_deref()
            .filter(|t| !t.is_empty())?
            .to_string();
        Some(RankedStanding {
            tier,
            division: s.rank_division.clone().unwrap_or_default(),
            lp: s.rank_lp.unwrap_or(0),
            wins: s.rank_wins.unwrap_or(0),
            losses: s.rank_losses.unwrap_or(0),
        })
    });
    let Some(profile) = state.ready() else {
        return ProfileView {
            block,
            rank,
            roles: Vec::new(),
            games: Vec::new(),
            champions: Vec::new(),
        };
    };
    let mut champion_names: HashMap<i64, Named> = HashMap::new();
    let mut champ = |id: i64| {
        champion_names
            .entry(id)
            .or_insert_with(|| namer.champion_name(id))
            .clone()
    };
    let counted: Vec<i64> = summarised_games(&profile.games)
        .iter()
        .map(|g| g.game_id)
        .collect();
    let queue_filter = match params
        .queue
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("ranked_solo") => Some(StatsQueue::RankedSolo),
        Some("ranked_flex") => Some(StatsQueue::RankedFlex),
        Some("normal") => Some(StatsQueue::Normal),
        Some("aram") => Some(StatsQueue::Aram),
        Some("arena") => Some(StatsQueue::Arena),
        _ => None,
    };
    let role_filter = parse_position(params.role.as_deref());
    let games = profile
        .games
        .iter()
        .filter(|g| queue_filter.is_none_or(|q| g.queue == Some(q)))
        .filter(|g| role_filter.is_none_or(|r| g.position == Some(r)))
        .map(|g| GameRow {
            champion: champ(g.champion_id),
            win: g.win,
            remake: g.is_remake(),
            position: g.position,
            queue: g.queue,
            kills: g.kills,
            deaths: g.deaths,
            assists: g.assists,
            cs: g.cs,
            damage_to_champions: g.damage_to_champions,
            vision_score: g.vision_score,
            duration_secs: g.duration_secs,
            started_at_ms: g.started_at_ms,
            summarised: counted.contains(&g.game_id),
            opponent: g.opponent_champion_id.map(&mut champ),
            keystone: g
                .runes
                .as_ref()
                .and_then(|r| r.perk_ids.first())
                .map(|&id| namer.named("perk", id)),
            spells: (g.spells.first > 0 && g.spells.second > 0).then(|| {
                [
                    namer.named("spell", g.spells.first),
                    namer.named("spell", g.spells.second),
                ]
            }),
            items: g
                .items
                .iter()
                .take(6)
                .map(|&id| namer.named("item", id))
                .collect(),
            kill_participation: g.kill_participation,
            damage_share: g.damage_share,
            performance_score: g.performance_score,
            placement: g.placement,
            badge: g.badge,
        })
        .collect();
    let champions = profile
        .champions
        .iter()
        .map(|c| ChampionSummaryRow {
            champion: champ(c.champion_id),
            games: c.games,
            wins: c.wins,
            kills: c.kills,
            deaths: c.deaths,
            assists: c.assists,
            dpm: c.dpm,
            cs_per_min: c.cs_per_min,
            keystone: c
                .runes
                .as_ref()
                .and_then(|r| r.perk_ids.first())
                .map(|&id| namer.named("perk", id)),
            items: c
                .items
                .iter()
                .take(6)
                .map(|&id| namer.named("item", id))
                .collect(),
        })
        .collect();
    ProfileView {
        block,
        rank,
        roles: profile.roles.clone(),
        games,
        champions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_name_every_state_and_credit_only_online_https_pages() {
        let credit = Attribution {
            provider: "MetaBot.GG".into(),
            url: "https://metabot.gg/en/lol/champions/ahri".into(),
        };
        let online = block(&StatsState::Ready(()), |_| {
            (StatsOrigin::Online, Some(credit.clone()))
        });
        assert_eq!(online.state, "Ready");
        assert_eq!(online.origin, Some(StatsOrigin::Online));
        assert_eq!(online.attribution, Some(credit.clone()));

        let demo = block(&StatsState::Ready(()), |_| {
            (StatsOrigin::Demo, Some(credit.clone()))
        });
        assert_eq!(
            demo.attribution, None,
            "only Online data carries the credit"
        );
        let foreign = Attribution {
            provider: "x".into(),
            url: "javascript:alert(1)".into(),
        };
        let bad = block(&StatsState::Ready(()), |_| {
            (StatsOrigin::Online, Some(foreign.clone()))
        });
        assert_eq!(
            bad.attribution, None,
            "links outside the allowlist are dropped"
        );

        let cases: [(StatsState<()>, &str); 5] = [
            (StatsState::Loading, "Loading"),
            (
                StatsState::Unavailable(StatsUnavailable::OnlineDisabled),
                "OnlineDisabled",
            ),
            (
                StatsState::Unavailable(StatsUnavailable::ClientOffline),
                "ClientOffline",
            ),
            (StatsState::Unavailable(StatsUnavailable::NoData), "NoData"),
            (
                StatsState::Unavailable(StatsUnavailable::Failed("boom".into())),
                "Failed",
            ),
        ];
        for (state, name) in cases {
            let b = block(&state, |_| (StatsOrigin::Online, None));
            assert_eq!(b.state, name);
            assert_eq!(b.origin, None);
        }
        let failed = block(
            &StatsState::<()>::Unavailable(StatsUnavailable::Failed("boom".into())),
            |_| (StatsOrigin::Online, None),
        );
        assert_eq!(failed.message.as_deref(), Some("boom"));
    }

    #[test]
    fn rates_without_a_sample_are_hidden() {
        assert_eq!(some_rates(&Rates::default()), None);
        let r = Rates {
            win_rate: 0.52,
            ..Rates::default()
        };
        assert_eq!(some_rates(&r), Some(r));
        assert_eq!(parse_position(Some("FILL")), None);
        assert_eq!(parse_position(Some("utility")), Some(Position::Support));
    }
}
