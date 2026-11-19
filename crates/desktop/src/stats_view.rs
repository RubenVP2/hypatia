//! The Stats page (`ui/stats.slint`): Champion Build, Tier List, Matchups and My Stats, read from
//! Champion Stats (`AppState::stats`) and pushed into the `StatsBridge` global.
//!
//! Getters never block; `Loading` blocks are filled again when the Change Signal lands. Nothing
//! is read while the page is closed, and the page is repainted only when what it shows changed:
//! every refresh first compares the inputs it read with the last pushed ones.

use std::rc::Rc;
use std::sync::Arc;

use hypatia_core::AppState;
use hypatia_shared::{
    builtin_champion_roles, default_stats_position as default_position, normalize_champion_key,
    summarised_games, ChampionBuild, ChampionLookup, GameData, GameDataNames as Names, Lang,
    MatchupList, PlayerProfile, Position, RankBracket, Rates, RuneSetup, Skill, StatsOrigin,
    StatsQuery, StatsQueue, StatsState, StatsUnavailable, Tier, TierList,
};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use crate::icons::DesktopAssetLoader;
use crate::links;
use crate::{
    MainWindow, StatsBlock, StatsBridge, StatsChampOption, StatsChampionSummary, StatsGameRow,
    StatsIcon, StatsItemStage, StatsMatchupRow, StatsRates, StatsRunePage, StatsSkillOrder,
    StatsSpellRow, StatsTierRow, StatsTotals,
};

/// Champions shown in the picker: search matches, or the player's most played.
const PICKS: usize = 8;
/// Opponents per matchup column.
const MATCHUPS: usize = 8;

pub const TAB_BUILD: i32 = 0;
pub const TAB_TIERS: i32 = 1;
pub const TAB_MATCHUPS: i32 = 2;
pub const TAB_MY_STATS: i32 = 3;

/// What the page shows, chosen by the player.
#[derive(Debug, Clone, PartialEq)]
struct Selection {
    tab: i32,
    champion_id: i64,
    position: Option<Position>,
    /// 0 Ranked Solo/Duo, 1 ARAM, 2 Arena.
    tier_queue: i32,
    /// -1 all roles, else a desktop lane index.
    tier_role: i32,
    search: String,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            tab: TAB_BUILD,
            champion_id: 0,
            position: None,
            tier_queue: 0,
            tier_role: -1,
            search: String::new(),
        }
    }
}

/// Everything a push depends on. Equal inputs mean an identical page: nothing is pushed.
#[derive(Debug, PartialEq)]
struct Inputs {
    selection: Selection,
    lang: Lang,
    online: bool,
    icons: (bool, bool, u64, u64),
    game_data: Option<StatsState<Arc<GameData>>>,
    recommended: Option<StatsState<ChampionBuild>>,
    build: Option<StatsState<ChampionBuild>>,
    tiers: Option<StatsState<TierList>>,
    matchups: Option<StatsState<MatchupList>>,
    profile: Option<StatsState<PlayerProfile>>,
}

#[derive(Default)]
pub struct StatsView {
    selection: Selection,
    /// The champion was chosen once (by the player or from the game); never chosen again.
    seeded: bool,
    loader: DesktopAssetLoader,
    shown: Option<Inputs>,
}

impl StatsView {
    pub fn select_tab(&mut self, tab: i32) {
        self.selection.tab = tab.clamp(TAB_BUILD, TAB_MY_STATS);
    }

    pub fn select_tier_queue(&mut self, queue: i32) {
        self.selection.tier_queue = queue.clamp(0, 2);
    }

    pub fn select_tier_role(&mut self, role: i32) {
        self.selection.tier_role = role.clamp(-1, 4);
    }

    pub fn select_role(&mut self, lane: i32) {
        self.selection.position = position_from_lane(lane);
    }

    pub fn search_edited(&mut self, text: &str) {
        self.selection.search = text.to_string();
    }

    /// Enter in the search field: the first match.
    pub fn search_accepted(&mut self, app: &AppState) {
        if let Some(first) = search_catalog(app, &self.selection.search).first() {
            self.select_champion(app, first.id, None);
        }
    }

    /// Picks `champion_id` in `position` (or the role the player most likely wants).
    pub fn select_champion(
        &mut self,
        app: &AppState,
        champion_id: i64,
        position: Option<Position>,
    ) {
        let profile = app.stats.player_profile();
        self.selection.champion_id = champion_id;
        self.selection.position = position
            .or_else(|| default_position(champion_id, self.selection.position, profile.ready()));
        self.selection.search.clear();
        self.seeded = true;
    }

    /// A tier row, a game or a matchup: open that champion's build. `lane` below 0 picks the
    /// role the player most likely wants.
    pub fn open_build(&mut self, app: &AppState, champion_id: i64, lane: i32) {
        self.selection.tab = TAB_BUILD;
        self.select_champion(app, champion_id, position_from_lane(lane));
    }

    /// Reads Champion Stats for the open tab and pushes what changed. Does nothing while the
    /// page is closed.
    pub fn refresh(&mut self, main: &MainWindow, app: &AppState) {
        if !main.get_stats_open() {
            self.shown = None;
            return;
        }
        if !self.seeded {
            self.seed(app);
        }
        let inputs = self.read(app);
        if self.shown.as_ref() == Some(&inputs) {
            return;
        }
        self.push(main, app, &inputs);
        self.shown = Some(inputs);
    }

    /// Starts on the champion of the current game, else the player's most played.
    fn seed(&mut self, app: &AppState) {
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
        if let Some((id, position)) = in_game {
            self.select_champion(app, id, position.filter(|p| *p != Position::Fill));
            return;
        }
        if let StatsState::Ready(profile) = app.stats.player_profile() {
            if let Some(top) = profile.champions.first() {
                self.select_champion(app, top.champion_id, None);
            } else {
                self.seeded = true;
            }
        }
    }

    fn query(&self) -> StatsQuery {
        StatsQuery {
            champion_id: self.selection.champion_id,
            position: self.selection.position,
            queue: StatsQueue::RankedSolo,
            bracket: RankBracket::default(),
            patch: None,
        }
    }

    fn read(&self, app: &AppState) -> Inputs {
        let stats = &app.stats;
        let tab = self.selection.tab;
        let has_champion = self.selection.champion_id > 0;
        let picker = matches!(tab, TAB_BUILD | TAB_MATCHUPS);
        let query = self.query();
        Inputs {
            selection: self.selection.clone(),
            lang: app.lang(),
            online: stats.online_enabled(),
            icons: (
                app.flags.game_icons(),
                app.flags.splash_backdrops(),
                app.assets.image_revision(),
                app.assets.metadata_revision(),
            ),
            game_data: matches!(tab, TAB_BUILD | TAB_MY_STATS).then(|| stats.game_data()),
            recommended: (tab == TAB_BUILD && has_champion)
                .then(|| stats.recommended_build(&query)),
            build: (tab == TAB_BUILD && has_champion).then(|| stats.build(&query)),
            tiers: (tab == TAB_TIERS).then(|| {
                stats.tier_list(
                    tier_queue(self.selection.tier_queue),
                    RankBracket::default(),
                )
            }),
            matchups: (tab == TAB_MATCHUPS && has_champion).then(|| stats.matchups(&query)),
            profile: (picker || tab == TAB_MY_STATS).then(|| stats.player_profile()),
        }
    }

    fn push(&mut self, main: &MainWindow, app: &AppState, inputs: &Inputs) {
        let bridge = main.global::<StatsBridge>();
        let lang = inputs.lang;
        let sel = &inputs.selection;
        let names = Names::new(
            inputs
                .game_data
                .as_ref()
                .and_then(|g| g.ready())
                .map(|g| g.as_ref()),
        );
        let mut ctx = Ctx {
            app,
            lang,
            loader: &mut self.loader,
            names,
        };

        bridge.set_tab(sel.tab);
        bridge.set_online_enabled(inputs.online);
        bridge.set_role(sel.position.map_or(-1, |p| p.desktop_index()));
        bridge.set_tier_queue(sel.tier_queue);
        bridge.set_tier_role(sel.tier_role);
        if bridge.get_search().as_str() != sel.search {
            bridge.set_search(sel.search.as_str().into());
        }

        let profile = inputs.profile.as_ref().and_then(|p| p.ready());
        if matches!(sel.tab, TAB_BUILD | TAB_MATCHUPS) {
            let champion = if sel.champion_id > 0 {
                ctx.champion(sel.champion_id)
            } else {
                StatsChampOption::default()
            };
            bridge.set_champion(champion);
            bridge.set_splash(ctx.splash(sel.champion_id));
            let searching = !sel.search.trim().is_empty();
            let picks: Vec<i64> = if searching {
                search_catalog(app, &sel.search)
                    .iter()
                    .map(|c| c.id)
                    .collect()
            } else {
                profile
                    .map(|p| {
                        p.champions
                            .iter()
                            .map(|c| c.champion_id)
                            .take(PICKS)
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let picks: Vec<_> = picks.into_iter().map(|id| ctx.champion(id)).collect();
            bridge.set_results(model(picks));
            bridge.set_results_are_search(searching);
        }

        // The same champion on dpm.lol, opened in the browser on the player's click.
        let dpm = match sel.tab {
            TAB_BUILD => links::dpm_build(sel.champion_id),
            TAB_MATCHUPS => links::dpm_matchups(sel.champion_id),
            TAB_TIERS => Some(links::DPM_TIER_LIST.to_string()),
            _ => None,
        };
        bridge.set_dpm_url(dpm.unwrap_or_default().into());

        match sel.tab {
            TAB_BUILD => push_build(&bridge, &mut ctx, inputs),
            TAB_TIERS => push_tiers(&bridge, &mut ctx, inputs),
            TAB_MATCHUPS => push_matchups(&bridge, &mut ctx, inputs),
            _ => push_my_stats(&bridge, &mut ctx, inputs),
        }
    }
}

/// Connects the page's callbacks to the [`StatsView`] held by the UI.
pub fn wire(bridge: &StatsBridge<'_>) {
    use crate::with_ui;
    bridge.on_select_tab(|tab| with_ui(|ui| ui.update_stats(|s, _| s.select_tab(tab))));
    bridge.on_select_champion(|id| {
        with_ui(|ui| ui.update_stats(|s, app| s.select_champion(app, id.into(), None)))
    });
    bridge.on_select_role(|lane| with_ui(|ui| ui.update_stats(|s, _| s.select_role(lane))));
    bridge.on_search_edited(|text| with_ui(|ui| ui.update_stats(|s, _| s.search_edited(&text))));
    bridge.on_search_accepted(|| with_ui(|ui| ui.update_stats(|s, app| s.search_accepted(app))));
    bridge.on_select_tier_queue(|q| with_ui(|ui| ui.update_stats(|s, _| s.select_tier_queue(q))));
    bridge.on_select_tier_role(|r| with_ui(|ui| ui.update_stats(|s, _| s.select_tier_role(r))));
    bridge.on_open_build(|id, lane| {
        with_ui(|ui| ui.update_stats(|s, app| s.open_build(app, id.into(), lane)))
    });
    bridge.on_open_link(|url| {
        links::open_external(&url);
    });
    bridge.on_refresh_profile(|| {
        with_ui(|ui| {
            ui.app.stats.refresh_player_profile();
            ui.update_stats(|_, _| {});
        })
    });
    bridge.on_enable_online(|| {
        with_ui(|ui| {
            if !ui.settings.borrow().online_stats {
                ui.toggle_setting("online_stats");
            }
        })
    });
}

fn push_build(bridge: &StatsBridge<'_>, ctx: &mut Ctx<'_>, inputs: &Inputs) {
    let lang = ctx.lang;
    let empty = StatsState::Loading;
    let build_state = inputs.build.as_ref().unwrap_or(&empty);
    let client_state = inputs.recommended.as_ref().unwrap_or(&empty);
    bridge.set_build_block(block(build_state, |b| (b.origin, b.attribution.clone())));
    bridge.set_client_block(block(client_state, |b| (b.origin, None)));
    let build = build_state.ready();
    let client = client_state.ready();
    let link = |b: &ChampionBuild| attribution(b.origin, b.attribution.as_ref());

    bridge.set_rates(match build {
        Some(b) if b.rates.games > 0 || b.rates.win_rate > 0.0 => StatsRates {
            has: true,
            win: percent(b.rates.win_rate, lang).into(),
            pick: percent(b.rates.pick_rate, lang).into(),
            ban: percent(b.rates.ban_rate, lang).into(),
            games: thousands(b.rates.games, lang).into(),
        },
        _ => StatsRates::default(),
    });

    // Rune pages: the global ones first, then the client's own unless it is one of them.
    let client_page = client.and_then(|c| c.runes.first().map(|(page, _)| page));
    let mut pages = Vec::new();
    if let Some(b) = build {
        let (link_label, link_url) = link(b);
        for (kind, (page, rates)) in b.runes.iter().take(2).enumerate() {
            pages.push(ctx.rune_page(
                page,
                kind as i32,
                b.origin,
                rates_line(rates, lang),
                client_page == Some(page),
                &link_label,
                &link_url,
            ));
        }
    }
    if let (Some(c), Some(page)) = (client, client_page) {
        let shown = build.is_some_and(|b| b.runes.iter().take(2).any(|(p, _)| p == page));
        if !shown {
            pages.push(ctx.rune_page(page, 2, c.origin, String::new(), false, "", ""));
        }
    }
    bridge.set_rune_pages(model(pages));

    // Summoner spells: the global pair, and the client's when it differs.
    let mut spells = Vec::new();
    if let Some((pair, rates)) = build.and_then(|b| b.spells.first()) {
        spells.push(ctx.spell_row(
            pair.first,
            pair.second,
            build.map_or(StatsOrigin::Local, |b| b.origin),
            rates_line(rates, lang),
        ));
    }
    if let Some((pair, _)) = client.and_then(|c| c.spells.first()) {
        let same = build
            .and_then(|b| b.spells.first())
            .is_some_and(|(p, _)| p == pair);
        if !same {
            spells.push(ctx.spell_row(
                pair.first,
                pair.second,
                client.map_or(StatsOrigin::RiotClient, |c| c.origin),
                String::new(),
            ));
        }
    }
    bridge.set_spells(model(spells));

    let items: Vec<StatsItemStage> = build
        .map(|b| {
            b.items
                .iter()
                .filter(|block| !block.item_ids.is_empty())
                .map(|block| StatsItemStage {
                    stage: block.stage.map_or(4, |s| s as i32),
                    items: model(
                        block
                            .item_ids
                            .iter()
                            .map(|&id| ctx.icon("item", id))
                            .collect(),
                    ),
                    rates: rates_line(&block.rates, lang).into(),
                })
                .collect()
        })
        .unwrap_or_default();
    bridge.set_items(model(items));

    let skills = build
        .and_then(|b| b.skills.first().map(|s| (s, b.origin)))
        .or_else(|| client.and_then(|c| c.skills.first().map(|s| (s, c.origin))));
    bridge.set_has_skills(skills.is_some_and(|(s, _)| !s.levels.is_empty()));
    if let Some((order, origin)) = skills {
        bridge.set_skills(StatsSkillOrder {
            levels: model(order.levels.iter().map(|&s| skill_index(s)).collect()),
            priority: model(
                order
                    .priority
                    .iter()
                    .map(|&s| SharedString::from(skill_letter(s)))
                    .collect(),
            ),
            rates: rates_line(&order.rates, lang).into(),
        });
        bridge.set_skills_origin(origin_index(origin));
    }
}

fn push_tiers(bridge: &StatsBridge<'_>, ctx: &mut Ctx<'_>, inputs: &Inputs) {
    let lang = ctx.lang;
    let empty = StatsState::Loading;
    let state = inputs.tiers.as_ref().unwrap_or(&empty);
    bridge.set_tier_block(block(state, |t| (t.origin, t.attribution.clone())));
    let sel = &inputs.selection;
    let Some(list) = state.ready() else {
        bridge.set_tier_patch(SharedString::new());
        bridge.set_tiers(model(Vec::new()));
        return;
    };
    bridge.set_tier_patch(list.patch.as_str().into());
    let lanes = sel.tier_queue == 0;
    let rows: Vec<StatsTierRow> = list
        .entries
        .iter()
        .filter(|e| !lanes || sel.tier_role < 0 || e.position.desktop_index() == sel.tier_role)
        .enumerate()
        .map(|(i, e)| StatsTierRow {
            rank: i as i32 + 1,
            tier: tier_index(e.tier),
            champion: ctx.champion(e.champion_id),
            lane: if lanes {
                e.position.desktop_index()
            } else {
                -1
            },
            win: percent(e.rates.win_rate, lang).into(),
            pick: percent(e.rates.pick_rate, lang).into(),
            ban: percent(e.rates.ban_rate, lang).into(),
            games: thousands(e.rates.games, lang).into(),
        })
        .collect();
    bridge.set_tiers(model(rows));
}

fn push_matchups(bridge: &StatsBridge<'_>, ctx: &mut Ctx<'_>, inputs: &Inputs) {
    let lang = ctx.lang;
    let empty = StatsState::Loading;
    let state = inputs.matchups.as_ref().unwrap_or(&empty);
    bridge.set_matchup_block(block(state, |m| (m.origin, m.attribution.clone())));
    let entries = state
        .ready()
        .map(|m| m.entries.as_slice())
        .unwrap_or_default();
    let mut row = |m: &hypatia_shared::Matchup| StatsMatchupRow {
        opponent: ctx.champion(m.opponent_id),
        win: percent(m.rates.win_rate, lang).into(),
        win_value: m.rates.win_rate,
        games: games_label(m.rates.games, lang).into(),
        gold: m
            .gold_diff_15
            .map(|g| gold_label(g, lang))
            .unwrap_or_default()
            .into(),
    };
    let mut hardest: Vec<_> = entries.iter().filter(|m| m.rates.win_rate < 0.5).collect();
    hardest.sort_by(|a, b| a.rates.win_rate.total_cmp(&b.rates.win_rate));
    let mut easiest: Vec<_> = entries.iter().filter(|m| m.rates.win_rate >= 0.5).collect();
    easiest.sort_by(|a, b| b.rates.win_rate.total_cmp(&a.rates.win_rate));
    let hardest: Vec<_> = hardest.into_iter().take(MATCHUPS).map(&mut row).collect();
    let easiest: Vec<_> = easiest.into_iter().take(MATCHUPS).map(&mut row).collect();
    bridge.set_hardest(model(hardest));
    bridge.set_easiest(model(easiest));
}

fn push_my_stats(bridge: &StatsBridge<'_>, ctx: &mut Ctx<'_>, inputs: &Inputs) {
    let lang = ctx.lang;
    let empty = StatsState::Loading;
    let state = inputs.profile.as_ref().unwrap_or(&empty);
    bridge.set_profile_block(block(state, |p| (p.origin, None)));
    let Some(profile) = state.ready() else {
        bridge.set_my_champions(model(Vec::new()));
        bridge.set_games(model(Vec::new()));
        return;
    };

    // Totals cover the summarised games (Summoner's Rift when there are any); the list below
    // keeps every game.
    let games = summarised_games(&profile.games);
    let n = games.len().max(1) as f32;
    let wins = games.iter().filter(|g| g.win).count() as f32;
    let (k, d, a) = games.iter().fold((0u32, 0u32, 0u32), |t, g| {
        (t.0 + g.kills, t.1 + g.deaths, t.2 + g.assists)
    });
    let minutes: f32 = games
        .iter()
        .map(|g| g.duration_secs as f32 / 60.0)
        .sum::<f32>()
        .max(1.0);
    let cs: u32 = games.iter().map(|g| g.cs).sum();
    let damage: u32 = games.iter().map(|g| g.damage_to_champions).sum();
    bridge.set_totals(StatsTotals {
        games: games.len().to_string().into(),
        win_rate: if games.is_empty() {
            "—".into()
        } else {
            percent(wins / n, lang).into()
        },
        kda: decimal(kda_ratio(k as f32, d as f32, a as f32), lang).into(),
        cs_min: decimal(cs as f32 / minutes, lang).into(),
        dpm: thousands((damage as f32 / minutes).round() as u32, lang).into(),
    });

    let champions: Vec<StatsChampionSummary> = profile
        .champions
        .iter()
        .map(|c| {
            let keystone = c
                .runes
                .as_ref()
                .and_then(|r| r.perk_ids.first())
                .map(|&id| ctx.icon("perk", id))
                .unwrap_or_default();
            StatsChampionSummary {
                champion: ctx.champion(c.champion_id),
                games: games_label(c.games, lang).into(),
                record: record_label(c.wins, c.games.saturating_sub(c.wins), lang).into(),
                win_rate: percent(c.wins as f32 / c.games.max(1) as f32, lang).into(),
                kda: decimal(kda_ratio(c.kills, c.deaths, c.assists), lang).into(),
                dpm: thousands(c.dpm.round() as u32, lang).into(),
                cs_min: decimal(c.cs_per_min, lang).into(),
                keystone,
                items: model(
                    c.items
                        .iter()
                        .take(6)
                        .map(|&id| ctx.icon("item", id))
                        .collect(),
                ),
            }
        })
        .collect();
    bridge.set_my_champions(model(champions));

    let rows: Vec<StatsGameRow> = games
        .iter()
        .map(|g| {
            let minutes = (g.duration_secs as f32 / 60.0).max(1.0);
            StatsGameRow {
                champion: ctx.champion(g.champion_id),
                win: g.win,
                lane: g.position.map_or(-1, |p| p.desktop_index()),
                queue: g
                    .queue
                    .map(|q| queue_label(q, lang))
                    .unwrap_or_default()
                    .into(),
                kda: format!("{}/{}/{}", g.kills, g.deaths, g.assists).into(),
                ratio: format!(
                    "{} KDA",
                    decimal(
                        kda_ratio(g.kills as f32, g.deaths as f32, g.assists as f32),
                        lang
                    )
                )
                .into(),
                cs_min: decimal(g.cs as f32 / minutes, lang).into(),
                dpm: thousands(
                    (g.damage_to_champions as f32 / minutes).round() as u32,
                    lang,
                )
                .into(),
                duration: format!("{}:{:02}", g.duration_secs / 60, g.duration_secs % 60).into(),
                when: date_label(g.started_at_ms, lang).into(),
            }
        })
        .collect();
    bridge.set_games(model(rows));
}

/// What a push needs to turn ids into names and icons.
struct Ctx<'a> {
    app: &'a AppState,
    lang: Lang,
    loader: &'a mut DesktopAssetLoader,
    names: Names,
}

impl Ctx<'_> {
    fn icon(&mut self, category: &'static str, id: i64) -> StatsIcon {
        let name = self.names.name(category, id, self.lang);
        StatsIcon {
            initials: initials(&name).into(),
            name: name.into(),
            icon: self.loader.resolve(self.app, category, id),
        }
    }

    fn champion(&mut self, id: i64) -> StatsChampOption {
        let name = self
            .app
            .assets
            .resolve_champion_name_localized(id, self.lang);
        let roles: Vec<&str> = builtin_champion_roles(id)
            .iter()
            .filter_map(|r| Position::parse(r))
            .map(|p| p.display_label(self.lang))
            .collect();
        StatsChampOption {
            id: id as i32,
            initials: initials(&name).into(),
            name: name.into(),
            icon: self.loader.resolve(self.app, "champion", id),
            detail: roles.join(" · ").into(),
        }
    }

    fn splash(&mut self, champion_id: i64) -> Image {
        if champion_id <= 0 || !self.app.flags.splash_backdrops() {
            return Image::default();
        }
        self.loader.resolve(self.app, "splash", champion_id * 1000)
    }

    #[allow(clippy::too_many_arguments)]
    fn rune_page(
        &mut self,
        page: &RuneSetup,
        kind: i32,
        origin: StatsOrigin,
        rates: String,
        matches_client: bool,
        link_label: &str,
        link_url: &str,
    ) -> StatsRunePage {
        let perks = &page.perk_ids;
        let slice =
            |from: usize, to: usize| perks.get(from..to.min(perks.len())).unwrap_or_default();
        StatsRunePage {
            kind,
            origin: origin_index(origin),
            link_label: link_label.into(),
            link_url: link_url.into(),
            rates: rates.into(),
            matches_client,
            primary: self.icon("style", page.primary_style_id),
            secondary: self.icon("style", page.sub_style_id),
            keystone: perks
                .first()
                .map(|&id| self.icon("perk", id))
                .unwrap_or_default(),
            primary_perks: model(
                slice(1, 4)
                    .iter()
                    .map(|&id| self.icon("perk", id))
                    .collect(),
            ),
            secondary_perks: model(
                slice(4, 6)
                    .iter()
                    .map(|&id| self.icon("perk", id))
                    .collect(),
            ),
            shards: model(
                page.shard_ids
                    .iter()
                    .map(|&id| self.icon("perk", id))
                    .collect(),
            ),
        }
    }

    fn spell_row(
        &mut self,
        first: i64,
        second: i64,
        origin: StatsOrigin,
        rates: String,
    ) -> StatsSpellRow {
        StatsSpellRow {
            origin: origin_index(origin),
            first: self.icon("spell", first),
            second: self.icon("spell", second),
            rates: rates.into(),
        }
    }
}

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

/// A block's state for `StatsBlock`, with origin and provider link when it is ready.
fn block<T>(
    state: &StatsState<T>,
    meta: impl Fn(&T) -> (StatsOrigin, Option<hypatia_shared::Attribution>),
) -> StatsBlock {
    match state {
        StatsState::Ready(value) => {
            let (origin, attr) = meta(value);
            let (link_label, link_url) = attribution(origin, attr.as_ref());
            StatsBlock {
                state: 0,
                message: SharedString::new(),
                origin: origin_index(origin),
                link_label: link_label.into(),
                link_url: link_url.into(),
            }
        }
        StatsState::Loading => StatsBlock {
            state: 1,
            ..StatsBlock::default()
        },
        StatsState::Unavailable(why) => {
            let (state, message) = match why {
                StatsUnavailable::OnlineDisabled => (2, String::new()),
                StatsUnavailable::ClientOffline => (3, String::new()),
                StatsUnavailable::NoData => (4, String::new()),
                StatsUnavailable::Failed(message) => (5, message.clone()),
            };
            StatsBlock {
                state,
                message: message.into(),
                ..StatsBlock::default()
            }
        }
    }
}

/// The provider credit shown next to Online data: its name and the page, only for `https`.
fn attribution(
    origin: StatsOrigin,
    attr: Option<&hypatia_shared::Attribution>,
) -> (String, String) {
    match attr {
        Some(a) if origin == StatsOrigin::Online && links::is_allowed(&a.url) => {
            (a.provider.clone(), a.url.clone())
        }
        _ => (String::new(), String::new()),
    }
}

fn search_catalog(app: &AppState, text: &str) -> Vec<hypatia_shared::RemoteChampionOption> {
    let key = normalize_champion_key(text);
    if key.is_empty() {
        return Vec::new();
    }
    let catalog = app.assets.champion_catalog();
    let mut starts: Vec<_> = Vec::new();
    let mut contains: Vec<_> = Vec::new();
    for c in catalog {
        let name = normalize_champion_key(&c.name);
        if name.starts_with(&key) {
            starts.push(c);
        } else if name.contains(&key) {
            contains.push(c);
        }
    }
    starts.extend(contains);
    starts.truncate(PICKS);
    starts
}

fn position_from_lane(lane: i32) -> Option<Position> {
    match lane {
        0 => Some(Position::Top),
        1 => Some(Position::Jungle),
        2 => Some(Position::Mid),
        3 => Some(Position::Bot),
        4 => Some(Position::Support),
        _ => None,
    }
}

fn tier_queue(index: i32) -> StatsQueue {
    match index {
        1 => StatsQueue::Aram,
        2 => StatsQueue::Arena,
        _ => StatsQueue::RankedSolo,
    }
}

fn tier_index(tier: Tier) -> i32 {
    match tier {
        Tier::S => 0,
        Tier::A => 1,
        Tier::B => 2,
        Tier::C => 3,
        Tier::D => 4,
    }
}

fn origin_index(origin: StatsOrigin) -> i32 {
    match origin {
        StatsOrigin::Local => 0,
        StatsOrigin::RiotClient => 1,
        StatsOrigin::Online => 2,
        StatsOrigin::Demo => 3,
    }
}

fn skill_index(skill: Skill) -> i32 {
    match skill {
        Skill::Q => 0,
        Skill::W => 1,
        Skill::E => 2,
        Skill::R => 3,
    }
}

fn skill_letter(skill: Skill) -> &'static str {
    ["Q", "W", "E", "R"][skill_index(skill) as usize]
}

fn queue_label(queue: StatsQueue, lang: Lang) -> String {
    match queue {
        StatsQueue::RankedSolo => lang.pick("Ranked Solo/Duo", "Classée Solo/Duo"),
        StatsQueue::RankedFlex => lang.pick("Ranked Flex", "Classée Flexible"),
        StatsQueue::Normal => lang.pick("Normal", "Normale"),
        StatsQueue::Aram => "ARAM",
        StatsQueue::Arena => lang.pick("Arena", "Arène"),
    }
    .to_string()
}

/// Two letters for an icon that is not cached: the initials of two words, else the first two.
fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '\'')
        .filter(|w| !w.is_empty())
        .collect();
    let out: String = if words.len() >= 2 {
        words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect()
    } else {
        name.chars()
            .filter(|c| c.is_alphanumeric())
            .take(2)
            .collect()
    };
    out.to_uppercase()
}

fn kda_ratio(kills: f32, deaths: f32, assists: f32) -> f32 {
    (kills + assists) / deaths.max(1.0)
}

/// One decimal, with the language's decimal separator.
fn decimal(value: f32, lang: Lang) -> String {
    let text = format!("{value:.1}");
    if lang.is_fr() {
        text.replace('.', ",")
    } else {
        text
    }
}

/// A rate in `0..=1` as a percentage: `52.3%` / `52,3 %`.
fn percent(rate: f32, lang: Lang) -> String {
    let value = decimal(rate * 100.0, lang);
    if lang.is_fr() {
        format!("{value}\u{a0}%")
    } else {
        format!("{value}%")
    }
}

/// Thousands grouped the language's way (`14,950` / `14 950`).
fn thousands(value: u32, lang: Lang) -> String {
    let digits = value.to_string();
    let separator = if lang.is_fr() { '\u{a0}' } else { ',' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

fn games_label(games: u32, lang: Lang) -> String {
    let n = thousands(games, lang);
    match (lang.is_fr(), games) {
        (false, 1) => format!("{n} game"),
        (false, _) => format!("{n} games"),
        (true, 0 | 1) => format!("{n} partie"),
        (true, _) => format!("{n} parties"),
    }
}

fn record_label(wins: u32, losses: u32, lang: Lang) -> String {
    if lang.is_fr() {
        format!("{wins}V {losses}D")
    } else {
        format!("{wins}W {losses}L")
    }
}

fn gold_label(diff: f32, lang: Lang) -> String {
    let rounded = diff.round() as i32;
    let sign = if rounded > 0 {
        "+"
    } else if rounded < 0 {
        "−"
    } else {
        "±"
    };
    let value = thousands(rounded.unsigned_abs(), lang);
    if lang.is_fr() {
        format!("{sign}{value} or à 15 min")
    } else {
        format!("{sign}{value} gold at 15 min")
    }
}

/// Share of games and win rate (`46.0% of games · 53.1% wins`), or nothing without rates.
fn rates_line(rates: &Rates, lang: Lang) -> String {
    if rates.games == 0 && rates.pick_rate == 0.0 && rates.win_rate == 0.0 {
        return String::new();
    }
    let pick = percent(rates.pick_rate, lang);
    let win = percent(rates.win_rate, lang);
    if lang.is_fr() {
        format!("{pick} des parties · {win} de victoires")
    } else {
        format!("{pick} of games · {win} wins")
    }
}

/// The day a game started (UTC), `Nov 21` / `21 nov.`.
fn date_label(epoch_ms: i64, lang: Lang) -> String {
    if epoch_ms <= 0 {
        return String::new();
    }
    let days = epoch_ms.div_euclid(86_400_000);
    let (_, month, day) = civil_from_days(days);
    const EN: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const FR: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    let m = (month - 1) as usize;
    if lang.is_fr() {
        format!("{day} {}", FR[m])
    } else {
        format!("{} {day}", EN[m])
    }
}

/// Gregorian date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_the_language() {
        assert_eq!(percent(0.5234, Lang::En), "52.3%");
        assert_eq!(percent(0.5234, Lang::Fr), "52,3\u{a0}%");
        assert_eq!(thousands(14950, Lang::En), "14,950");
        assert_eq!(games_label(1, Lang::En), "1 game");
        assert_eq!(games_label(3, Lang::Fr), "3 parties");
        assert_eq!(record_label(7, 3, Lang::Fr), "7V 3D");
        assert_eq!(gold_label(-352.4, Lang::En), "−352 gold at 15 min");
        assert_eq!(rates_line(&Rates::default(), Lang::En), "");
    }

    #[test]
    fn dates_are_calendar_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_412), (2025, 11, 20));
        assert_eq!(date_label(1_763_596_800_000, Lang::En), "Nov 20");
        assert_eq!(date_label(1_763_596_800_000, Lang::Fr), "20 nov.");
    }

    #[test]
    fn initials_fit_a_badge() {
        assert_eq!(initials("Flash"), "FL");
        assert_eq!(initials("Press the Attack"), "PT");
        assert_eq!(initials("Kai'Sa"), "KS");
    }

    #[test]
    fn default_role_prefers_the_current_then_the_catalog() {
        // Ahri is a mid laner in the catalog.
        assert_eq!(default_position(103, None, None), Some(Position::Mid));
        assert_eq!(
            default_position(103, Some(Position::Top), None),
            Some(Position::Mid)
        );
        assert_eq!(position_from_lane(-2), None);
    }

    #[test]
    fn blocks_carry_state_origin_and_only_online_links() {
        let attr = hypatia_shared::Attribution {
            provider: "MetaBot.GG".into(),
            url: "https://metabot.gg/x".into(),
        };
        let online = block(&StatsState::Ready(()), |_| {
            (StatsOrigin::Online, Some(attr.clone()))
        });
        assert_eq!((online.state, online.origin), (0, 2));
        assert_eq!(online.link_url.as_str(), "https://metabot.gg/x");
        let demo = block(&StatsState::Ready(()), |_| {
            (StatsOrigin::Demo, Some(attr.clone()))
        });
        assert_eq!(demo.link_url.as_str(), "");
        let off: StatsState<()> = StatsState::Unavailable(StatsUnavailable::OnlineDisabled);
        assert_eq!(block(&off, |_| (StatsOrigin::Online, None)).state, 2);
        let failed: StatsState<()> = StatsState::Unavailable(StatsUnavailable::Failed("x".into()));
        assert_eq!(
            block(&failed, |_| (StatsOrigin::Online, None))
                .message
                .as_str(),
            "x"
        );
    }
}
