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
    builtin_champion_roles, default_stats_position, summarised_games, Attribution, ChampionBuild,
    ChampionLookup, GameDataNames, ItemStage, Lang, Position, RankBracket, Rates, RuneSetup, Skill,
    StatsOrigin, StatsQuery, StatsQueue, StatsState, StatsUnavailable, Tier,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Champions offered under the search field: the player's most played.
const PICKS: usize = 8;
/// Opponents per matchup column.
const MATCHUPS: usize = 8;

/// What the phone asks for, all optional: the server picks the same defaults as the desktop.
#[derive(Debug, Deserialize, Default)]
pub struct StatsParams {
    pub champion: Option<i64>,
    /// Any Riot role token (`TOP`, `MIDDLE`, `UTILITY`, ...).
    pub position: Option<String>,
    /// Tier list queue: `ranked_solo` (default), `aram`, `arena`.
    pub queue: Option<String>,
    /// Tier list role filter (Ranked Solo/Duo only); absent or `ALL` for every role.
    pub role: Option<String>,
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

#[derive(Debug, Serialize)]
pub struct RunePageView {
    /// `popular` (the global page), `alternative`, or `client` (Riot's recommended page).
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

/// Champion Build: the global build (Online or Demo) beside the client's recommended page.
#[derive(Debug, Serialize)]
pub struct BuildView {
    pub champion: Option<ChampionRef>,
    pub position: Option<Position>,
    pub picks: Vec<ChampionRef>,
    pub dpm_url: Option<String>,
    pub build: Block,
    pub client: Block,
    pub rates: Option<Rates>,
    pub rune_pages: Vec<RunePageView>,
    pub spells: Vec<SpellRowView>,
    pub items: Vec<ItemStageView>,
    pub skills: Option<SkillView>,
}

#[derive(Debug, Serialize)]
pub struct TierRow {
    pub champion: ChampionRef,
    /// The role of the row (Ranked Solo/Duo only).
    pub position: Option<Position>,
    pub tier: Tier,
    pub rates: Rates,
}

#[derive(Debug, Serialize)]
pub struct TierView {
    pub queue: StatsQueue,
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
}

#[derive(Debug, Serialize)]
pub struct GameRow {
    pub champion: Named,
    pub win: bool,
    pub position: Option<Position>,
    pub queue: Option<StatsQueue>,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub cs: u32,
    pub damage_to_champions: u32,
    pub duration_secs: u32,
    pub started_at_ms: i64,
    /// Counted in the totals ([`summarised_games`]); ARAM, event modes and remakes are only listed.
    pub summarised: bool,
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

/// My Stats: the player's recent games and per-champion summary, from the local history.
#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub block: Block,
    pub games: Vec<GameRow>,
    pub champions: Vec<ChampionSummaryRow>,
}

/// Turns ids into names for one response.
struct Namer<'a> {
    app: &'a AppState,
    lang: Lang,
    names: GameDataNames,
}

impl<'a> Namer<'a> {
    fn new(app: &'a AppState, lang: Lang, with_game_data: bool) -> Self {
        let data = with_game_data.then(|| app.stats.game_data_in(lang));
        let names = GameDataNames::new(data.as_ref().and_then(|d| d.ready()).map(|g| g.as_ref()));
        Self { app, lang, names }
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

fn query(champion_id: i64, position: Option<Position>) -> StatsQuery {
    StatsQuery {
        champion_id,
        position,
        queue: StatsQueue::RankedSolo,
        bracket: RankBracket::default(),
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
    let (champion_id, position) = selection(app, params);
    let picks = picks(app, &namer);
    if champion_id <= 0 {
        return BuildView {
            champion: None,
            position: None,
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
            rune_pages: Vec::new(),
            spells: Vec::new(),
            items: Vec::new(),
            skills: None,
        };
    }
    let q = query(champion_id, position);
    let build_state = app.stats.build(&q);
    let client_state = app.stats.recommended_build(&q);
    let build_block = block(&build_state, |b: &ChampionBuild| {
        (b.origin, b.attribution.clone())
    });
    let client_block = block(&client_state, |b: &ChampionBuild| (b.origin, None));
    let build = build_state.ready();
    let client = client_state.ready();

    // Rune pages: the global ones first, then the client's own unless it is one of them.
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

    // Summoner spells: the global pair, and the client's when it differs.
    let mut spells = Vec::new();
    if let Some(b) = build {
        if let Some((pair, rates)) = b.spells.first() {
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
            let same = build
                .and_then(|b| b.spells.first())
                .is_some_and(|(p, _)| p == pair);
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

    let items = build
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

    BuildView {
        champion: Some(namer.champion(champion_id)),
        position,
        picks,
        dpm_url: links::dpm_build(champion_id),
        build: build_block,
        client: client_block,
        rates: build.and_then(|b| some_rates(&b.rates)),
        rune_pages,
        spells,
        items,
        skills,
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
    let lanes = queue == StatsQueue::RankedSolo;
    let role = if lanes {
        parse_position(params.role.as_deref())
    } else {
        None
    };
    let state = app.stats.tier_list(queue, RankBracket::default());
    let block = block(&state, |t| (t.origin, t.attribution.clone()));
    let (patch, entries) = match state.ready() {
        Some(list) => (
            list.patch.clone(),
            list.entries
                .iter()
                .filter(|e| role.is_none_or(|r| e.position == r))
                .map(|e| TierRow {
                    champion: namer.champion(e.champion_id),
                    position: lanes.then_some(e.position),
                    tier: e.tier,
                    rates: e.rates,
                })
                .collect(),
        ),
        None => (String::new(), Vec::new()),
    };
    TierView {
        queue,
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
        };
    }
    let state = app.stats.matchups(&query(champion_id, position));
    let block = block(&state, |m| (m.origin, m.attribution.clone()));
    let entries = state
        .ready()
        .map(|m| m.entries.as_slice())
        .unwrap_or_default();
    let row = |m: &hypatia_shared::Matchup| MatchupRow {
        opponent: namer.champion(m.opponent_id),
        rates: m.rates,
        gold_diff_15: m.gold_diff_15,
    };
    let mut hardest: Vec<_> = entries.iter().filter(|m| m.rates.win_rate < 0.5).collect();
    hardest.sort_by(|a, b| a.rates.win_rate.total_cmp(&b.rates.win_rate));
    let mut easiest: Vec<_> = entries.iter().filter(|m| m.rates.win_rate >= 0.5).collect();
    easiest.sort_by(|a, b| b.rates.win_rate.total_cmp(&a.rates.win_rate));
    MatchupView {
        champion: Some(namer.champion(champion_id)),
        position,
        picks,
        dpm_url: links::dpm_matchups(champion_id),
        block,
        hardest: hardest.into_iter().take(MATCHUPS).map(row).collect(),
        easiest: easiest.into_iter().take(MATCHUPS).map(row).collect(),
    }
}

pub fn profile_view(app: &AppState, params: &StatsParams) -> ProfileView {
    let lang = params.lang(app);
    let namer = Namer::new(app, lang, true);
    let state = app.stats.player_profile();
    let block = block(&state, |p| (p.origin, None));
    let Some(profile) = state.ready() else {
        return ProfileView {
            block,
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
    let games = profile
        .games
        .iter()
        .map(|g| GameRow {
            champion: champ(g.champion_id),
            win: g.win,
            position: g.position,
            queue: g.queue,
            kills: g.kills,
            deaths: g.deaths,
            assists: g.assists,
            cs: g.cs,
            damage_to_champions: g.damage_to_champions,
            duration_secs: g.duration_secs,
            started_at_ms: g.started_at_ms,
            summarised: counted.contains(&g.game_id),
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
