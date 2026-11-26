//! The Stats page (`ui/stats.slint`): Champion Build, Tier List, Matchups and My Stats, read from
//! Champion Stats (`AppState::stats`) and pushed into the `StatsBridge` global.
//!
//! Getters never block; `Loading` blocks are filled again when the Change Signal lands. Nothing
//! is read while the page is closed, and the page is repainted only when what it shows changed:
//! every refresh first compares the inputs it read with the last pushed ones.

use std::rc::Rc;
use std::sync::Arc;

use hypatia_core::stats::profile::group_sessions;
use hypatia_core::AppState;
use hypatia_shared::{
    aggregate_local_matchups, builtin_champion_roles, builtin_rune_style_slots, champion_traits,
    comp_trait_label, default_stats_position as default_position, normalize_champion_key,
    summarised_games, ChampionBuild, ChampionLookup, GameData, GameDataNames as Names, ItemStage,
    LaneVerdict, Lang, MatchBadge, MatchupList, PlayerGame, PlayerProfile, Position, RankBracket,
    Rates, RuneSetup, Skill, StatsOrigin, StatsQuery, StatsQueue, StatsState, StatsUnavailable,
    SummonerSummary, Tier, TierList, STAT_SHARD_ROWS,
};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use crate::icons::DesktopAssetLoader;
use crate::links;
use crate::{
    MainWindow, StatsBlock, StatsBridge, StatsChampOption, StatsChampProfile, StatsChampionSummary,
    StatsGameRow, StatsIcon, StatsItemStage, StatsLocalBuild, StatsLocalMatchupRow,
    StatsMatchupRow, StatsRankedCard, StatsRates, StatsRoleSummary, StatsRuneNode, StatsRunePage,
    StatsRuneRow, StatsSkillOrder, StatsSkillVariant, StatsSpellRow, StatsTierRow, StatsTotals,
};

/// Champions shown in the picker: search matches, or the player's most played.
const PICKS: usize = 8;
/// Opponents per matchup column when collapsed.
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
    /// Build tab queue: 0 Ranked Solo/Duo, 1 ARAM, 2 Arena.
    build_queue: i32,
    /// Build tab bracket: 0 All, 1 Emerald+, 2 Diamond+, 3 Master+.
    build_bracket: i32,
    /// Build tab item display: 0 by stage, 1 by slot (Item 1..5).
    item_view: i32,
    /// 0 Ranked Solo/Duo, 1 ARAM, 2 Arena.
    tier_queue: i32,
    /// Tier List bracket: 0 All, 1 Emerald+, 2 Diamond+, 3 Master+.
    tier_bracket: i32,
    /// -1 all roles, else a desktop lane index.
    tier_role: i32,
    /// Tier List sort column: 0 Tier, 1 Win, 2 Pick, 3 Ban, 4 Games.
    tier_sort_col: i32,
    tier_sort_desc: bool,
    tier_search: String,
    tier_off_meta: bool,
    /// Matchups search filter and full-list expansion.
    matchup_search: String,
    matchup_show_all: bool,
    /// My Stats queue filter: -1 All, 0 Ranked Solo, 1 Flex, 2 ARAM, 3 Arena.
    my_queue: i32,
    /// My Stats role filter: -1 All, 0..4 desktop lane index.
    my_role: i32,
    /// Game currently open in Match inspector (`My Stats`), if any.
    inspected_game_id: Option<i64>,
    search: String,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            tab: TAB_BUILD,
            champion_id: 0,
            position: None,
            build_queue: 0,
            build_bracket: 1,
            item_view: 0,
            tier_queue: 0,
            tier_bracket: 1,
            tier_role: -1,
            tier_sort_col: 0,
            tier_sort_desc: false,
            tier_search: String::new(),
            tier_off_meta: false,
            matchup_search: String::new(),
            matchup_show_all: false,
            my_queue: -1,
            my_role: -1,
            inspected_game_id: None,
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
    summoner: Option<SummonerSummary>,
    game_data: Option<StatsState<Arc<GameData>>>,
    recommended: Option<StatsState<ChampionBuild>>,
    build: Option<StatsState<ChampionBuild>>,
    tiers: Option<StatsState<TierList>>,
    matchups: Option<StatsState<MatchupList>>,
    profile: Option<StatsState<PlayerProfile>>,
    inspected_review: Option<hypatia_shared::GameReview>,
    llm_enabled: bool,
    inspected_debrief: hypatia_core::coach_llm::DebriefState,
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

    pub fn select_build_queue(&mut self, queue: i32) {
        self.selection.build_queue = queue.clamp(0, 2);
    }

    pub fn select_build_bracket(&mut self, bracket: i32) {
        self.selection.build_bracket = bracket.clamp(0, 3);
    }

    pub fn select_item_view(&mut self, mode: i32) {
        self.selection.item_view = mode.clamp(0, 1);
    }

    pub fn select_tier_queue(&mut self, queue: i32) {
        self.selection.tier_queue = queue.clamp(0, 2);
    }

    pub fn select_tier_bracket(&mut self, bracket: i32) {
        self.selection.tier_bracket = bracket.clamp(0, 3);
    }

    pub fn select_tier_role(&mut self, role: i32) {
        self.selection.tier_role = role.clamp(-1, 4);
    }

    pub fn sort_tier_column(&mut self, col: i32) {
        let col = col.clamp(0, 4);
        if self.selection.tier_sort_col == col {
            self.selection.tier_sort_desc = !self.selection.tier_sort_desc;
        } else {
            self.selection.tier_sort_col = col;
            // Tier defaults ascending (S+ first); numeric metrics default descending.
            self.selection.tier_sort_desc = col != 0;
        }
    }

    pub fn tier_search_edited(&mut self, text: &str) {
        self.selection.tier_search = text.to_string();
    }

    pub fn toggle_tier_off_meta(&mut self) {
        self.selection.tier_off_meta = !self.selection.tier_off_meta;
    }

    pub fn matchup_search_edited(&mut self, text: &str) {
        self.selection.matchup_search = text.to_string();
    }

    pub fn toggle_matchup_show_all(&mut self) {
        self.selection.matchup_show_all = !self.selection.matchup_show_all;
    }

    pub fn select_my_queue(&mut self, queue: i32) {
        self.selection.my_queue = queue.clamp(-1, 3);
    }

    pub fn select_my_role(&mut self, role: i32) {
        self.selection.my_role = role.clamp(-1, 4);
    }

    pub fn inspect_game(&mut self, game_id: &str) {
        self.selection.inspected_game_id = game_id.parse::<i64>().ok();
    }

    pub fn request_inspected_debrief(&self, app: &AppState) {
        let Some(gid) = self.selection.inspected_game_id else {
            return;
        };
        let lang = app.lang();
        let profile = app.stats.player_profile();
        let Some(prof) = profile.ready() else {
            return;
        };
        let Some(game) = prof.games.iter().find(|g| g.game_id == gid) else {
            return;
        };
        let personal = prof
            .champions
            .iter()
            .find(|c| c.champion_id == game.champion_id);
        if let Some(review) = app
            .reviews
            .review_for_player_game(game, personal, None, lang)
        {
            app.coach_llm.request_debrief(review, lang);
        }
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
        let queue = if self.selection.tab == TAB_BUILD {
            tier_queue(self.selection.build_queue)
        } else {
            StatsQueue::RankedSolo
        };
        let bracket = if self.selection.tab == TAB_BUILD {
            rank_bracket(self.selection.build_bracket)
        } else {
            RankBracket::default()
        };
        StatsQuery {
            champion_id: self.selection.champion_id,
            position: if queue.on_summoners_rift() {
                self.selection.position
            } else {
                None
            },
            queue,
            bracket,
            patch: None,
        }
    }

    fn read(&self, app: &AppState) -> Inputs {
        let stats = &app.stats;
        let tab = self.selection.tab;
        let has_champion = self.selection.champion_id > 0;
        let picker = matches!(tab, TAB_BUILD | TAB_MATCHUPS);
        let query = self.query();
        let lang = app.lang();
        let profile = (picker || tab == TAB_MY_STATS).then(|| stats.player_profile());
        let inspected_review = if tab == TAB_MY_STATS {
            self.selection.inspected_game_id.and_then(|gid| {
                let prof = profile.as_ref().and_then(|p| p.ready())?;
                let game = prof.games.iter().find(|g| g.game_id == gid)?;
                let personal = prof
                    .champions
                    .iter()
                    .find(|c| c.champion_id == game.champion_id);
                app.reviews
                    .review_for_player_game(game, personal, None, lang)
            })
        } else {
            None
        };
        let llm_enabled = app.coach_llm.is_available();
        let inspected_debrief = inspected_review
            .as_ref()
            .map(|r| app.coach_llm.state(r.game_id, lang))
            .unwrap_or_default();
        Inputs {
            selection: self.selection.clone(),
            lang,
            online: stats.online_enabled(),
            icons: (
                app.flags.game_icons(),
                app.flags.splash_backdrops(),
                app.assets.image_revision(),
                app.assets.metadata_revision(),
            ),
            summoner: (tab == TAB_MY_STATS)
                .then(|| app.get_summary().summoner.clone())
                .flatten(),
            game_data: matches!(tab, TAB_BUILD | TAB_MY_STATS).then(|| stats.game_data()),
            recommended: (tab == TAB_BUILD && has_champion)
                .then(|| stats.recommended_build(&query)),
            build: (tab == TAB_BUILD && has_champion).then(|| stats.build(&query)),
            tiers: (tab == TAB_TIERS).then(|| {
                stats.tier_list(
                    tier_queue(self.selection.tier_queue),
                    rank_bracket(self.selection.tier_bracket),
                )
            }),
            matchups: (tab == TAB_MATCHUPS && has_champion).then(|| stats.matchups(&query)),
            profile,
            inspected_review,
            llm_enabled,
            inspected_debrief,
        }
    }

    fn push(&mut self, main: &MainWindow, app: &AppState, inputs: &Inputs) {
        let bridge = main.global::<StatsBridge>();
        let lang = inputs.lang;
        let sel = &inputs.selection;
        let game_data = inputs.game_data.as_ref().and_then(|g| g.ready()).cloned();
        let names = Names::new(game_data.as_deref());
        let mut ctx = Ctx {
            app,
            lang,
            loader: &mut self.loader,
            names,
            game_data,
        };

        bridge.set_tab(sel.tab);
        bridge.set_online_enabled(inputs.online);
        bridge.set_role(sel.position.map_or(-1, |p| p.desktop_index()));
        bridge.set_build_queue(sel.build_queue);
        bridge.set_build_bracket(sel.build_bracket);
        bridge.set_item_view(sel.item_view);
        bridge.set_tier_queue(sel.tier_queue);
        bridge.set_tier_bracket(sel.tier_bracket);
        bridge.set_tier_role(sel.tier_role);
        bridge.set_tier_sort_col(sel.tier_sort_col);
        bridge.set_tier_sort_desc(sel.tier_sort_desc);
        bridge.set_tier_off_meta(sel.tier_off_meta);
        if bridge.get_tier_search().as_str() != sel.tier_search {
            bridge.set_tier_search(sel.tier_search.as_str().into());
        }
        bridge.set_matchup_show_all(sel.matchup_show_all);
        if bridge.get_matchup_search().as_str() != sel.matchup_search {
            bridge.set_matchup_search(sel.matchup_search.as_str().into());
        }
        bridge.set_my_queue(sel.my_queue);
        bridge.set_my_role(sel.my_role);
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
    bridge.on_select_build_queue(|q| with_ui(|ui| ui.update_stats(|s, _| s.select_build_queue(q))));
    bridge.on_select_build_bracket(|b| {
        with_ui(|ui| ui.update_stats(|s, _| s.select_build_bracket(b)))
    });
    bridge.on_select_item_view(|m| with_ui(|ui| ui.update_stats(|s, _| s.select_item_view(m))));
    bridge.on_search_edited(|text| with_ui(|ui| ui.update_stats(|s, _| s.search_edited(&text))));
    bridge.on_search_accepted(|| with_ui(|ui| ui.update_stats(|s, app| s.search_accepted(app))));
    bridge.on_select_tier_queue(|q| with_ui(|ui| ui.update_stats(|s, _| s.select_tier_queue(q))));
    bridge
        .on_select_tier_bracket(|b| with_ui(|ui| ui.update_stats(|s, _| s.select_tier_bracket(b))));
    bridge.on_select_tier_role(|r| with_ui(|ui| ui.update_stats(|s, _| s.select_tier_role(r))));
    bridge.on_sort_tier_column(|c| with_ui(|ui| ui.update_stats(|s, _| s.sort_tier_column(c))));
    bridge.on_tier_search_edited(|text| {
        with_ui(|ui| ui.update_stats(|s, _| s.tier_search_edited(&text)))
    });
    bridge
        .on_toggle_tier_off_meta(|| with_ui(|ui| ui.update_stats(|s, _| s.toggle_tier_off_meta())));
    bridge.on_matchup_search_edited(|text| {
        with_ui(|ui| ui.update_stats(|s, _| s.matchup_search_edited(&text)))
    });
    bridge.on_toggle_matchup_show_all(|| {
        with_ui(|ui| ui.update_stats(|s, _| s.toggle_matchup_show_all()))
    });
    bridge.on_select_my_queue(|q| with_ui(|ui| ui.update_stats(|s, _| s.select_my_queue(q))));
    bridge.on_select_my_role(|r| with_ui(|ui| ui.update_stats(|s, _| s.select_my_role(r))));
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
    bridge.on_inspect_game(|id| {
        with_ui(|ui| ui.update_stats(|s, _| s.inspect_game(&id)));
    });
    bridge.on_request_inspected_debrief(|| {
        with_ui(|ui| ui.update_stats(|s, app| s.request_inspected_debrief(app)));
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
    let profile = inputs.profile.as_ref().and_then(|p| p.ready());
    let link = |b: &ChampionBuild| attribution(b.origin, b.attribution.as_ref());

    // Role pickrate shares for the 5 lane chips.
    let mut shares = vec![SharedString::new(); 5];
    if let Some(b) = build {
        for &(pos, share) in &b.role_distribution {
            let idx = pos.desktop_index();
            if (0..5).contains(&idx) && share > 0.0 {
                let pct = (share * 100.0).round() as u32;
                shares[idx as usize] = format!("{pct}%").into();
            }
        }
    }
    bridge.set_role_shares(model(shares));

    bridge.set_rates(match build {
        Some(b) if b.rates.games > 0 || b.rates.win_rate > 0.0 => {
            let (tier, rank_label) = match b.tier_rank {
                Some((t, r, total)) => (
                    tier_index(t),
                    if lang.is_fr() {
                        format!("Rang {r}/{total}")
                    } else {
                        format!("Rank {r}/{total}")
                    },
                ),
                None => (-1, String::new()),
            };
            StatsRates {
                has: true,
                win: percent(b.rates.win_rate, lang).into(),
                pick: percent(b.rates.pick_rate, lang).into(),
                ban: percent(b.rates.ban_rate, lang).into(),
                games: thousands(b.rates.games, lang).into(),
                tier,
                rank_label: rank_label.into(),
            }
        }
        _ => StatsRates {
            tier: -1,
            ..StatsRates::default()
        },
    });

    // Player's own local stats on this champion.
    let local_champ = profile.and_then(|p| {
        p.champions
            .iter()
            .find(|c| c.champion_id == inputs.selection.champion_id)
            .map(|c| (p.origin, c))
    });
    bridge.set_local_build(match local_champ {
        Some((origin, c)) => StatsLocalBuild {
            has: true,
            origin: origin_index(origin),
            record: format!(
                "{} · {}",
                games_label(c.games, lang),
                record_label(c.wins, c.games.saturating_sub(c.wins), lang)
            )
            .into(),
            win_rate: percent(c.wins as f32 / c.games.max(1) as f32, lang).into(),
            kda: decimal(kda_ratio(c.kills, c.deaths, c.assists), lang).into(),
            cs_min: decimal(c.cs_per_min, lang).into(),
            dpm: thousands(c.dpm.round() as u32, lang).into(),
            keystone: c
                .runes
                .as_ref()
                .and_then(|r| r.perk_ids.first())
                .map(|&id| ctx.icon("perk", id))
                .unwrap_or_default(),
            spell_a: c
                .spells
                .map(|s| ctx.icon("spell", s.first))
                .unwrap_or_default(),
            spell_b: c
                .spells
                .map(|s| ctx.icon("spell", s.second))
                .unwrap_or_default(),
            items: model(
                c.items
                    .iter()
                    .take(6)
                    .map(|&id| ctx.icon("item", id))
                    .collect(),
            ),
        },
        None => StatsLocalBuild::default(),
    });

    // Rune pages: global pages first, then client recommended (if distinct), then player's own.
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
    if let Some((origin, c)) = local_champ {
        if let Some(page) = c.runes.as_ref() {
            let local_detail = format!(
                "{} · {}",
                games_label(c.games, lang),
                percent(c.wins as f32 / c.games.max(1) as f32, lang)
            );
            pages.push(ctx.rune_page(page, 3, origin, local_detail, false, "", ""));
        }
    }
    bridge.set_rune_pages(model(pages));

    // Summoner spells: top 2 global pairs, client's when it differs, else local player's most-used.
    let mut spells = Vec::new();
    if let Some(b) = build {
        for (pair, rates) in b.spells.iter().take(2) {
            spells.push(ctx.spell_row(pair.first, pair.second, b.origin, rates_line(rates, lang)));
        }
    }
    if let Some((pair, _)) = client.and_then(|c| c.spells.first()) {
        let same = build.is_some_and(|b| b.spells.iter().take(2).any(|(p, _)| p == pair));
        if !same {
            spells.push(ctx.spell_row(
                pair.first,
                pair.second,
                client.map_or(StatsOrigin::RiotClient, |c| c.origin),
                String::new(),
            ));
        }
    }
    if spells.is_empty() {
        if let Some((origin, c)) = local_champ {
            if let Some(pair) = c.spells {
                spells.push(ctx.spell_row(
                    pair.first,
                    pair.second,
                    origin,
                    games_label(c.games, lang),
                ));
            }
        }
    }
    bridge.set_spells(model(spells));

    let mut stage_items = Vec::new();
    let mut slot_items = Vec::new();
    let mut items_origin = build.map_or(0, |b| origin_index(b.origin));
    if let Some(b) = build {
        for blk in b.items.iter().filter(|blk| !blk.item_ids.is_empty()) {
            let row = StatsItemStage {
                stage: stage_index(blk.stage),
                items: model(
                    blk.item_ids
                        .iter()
                        .map(|&id| ctx.icon("item", id))
                        .collect(),
                ),
                rates: rates_line(&blk.rates, lang).into(),
            };
            if blk.stage.is_some_and(ItemStage::is_slot) {
                slot_items.push(row);
            } else {
                stage_items.push(row);
            }
        }
    }
    // Seamless fallback: if online/global items are absent, surface the player's own most-built items.
    if stage_items.is_empty() && slot_items.is_empty() {
        if let Some((origin, c)) = local_champ {
            if !c.items.is_empty() {
                items_origin = origin_index(origin);
                stage_items.push(StatsItemStage {
                    stage: stage_index(Some(ItemStage::Core)),
                    items: model(
                        c.items
                            .iter()
                            .take(6)
                            .map(|&id| ctx.icon("item", id))
                            .collect(),
                    ),
                    rates: games_label(c.games, lang).into(),
                });
            }
        }
    }
    bridge.set_items_origin(items_origin);
    bridge.set_items(model(stage_items));
    bridge.set_slot_items(model(slot_items));

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
    let alt_skills: Vec<StatsSkillVariant> = build
        .map(|b| {
            b.skills
                .iter()
                .skip(1)
                .take(2)
                .filter(|s| !s.priority.is_empty())
                .map(|s| StatsSkillVariant {
                    priority: model(
                        s.priority
                            .iter()
                            .map(|&k| SharedString::from(skill_letter(k)))
                            .collect(),
                    ),
                    rates: rates_line(&s.rates, lang).into(),
                })
                .collect()
        })
        .unwrap_or_default();
    bridge.set_alt_skills(model(alt_skills));

    // Champion Profile & Benchmarks (offline damage split + traits + local player averages).
    let champ_profile = match champion_traits(inputs.selection.champion_id) {
        Some(ct) => {
            let traits: Vec<SharedString> = ct
                .traits()
                .into_iter()
                .map(|t| SharedString::from(comp_trait_label(t, lang)))
                .collect();
            let (has_local, local_origin, kda, cs_min, dpm, gpm, vs_min) = match local_champ {
                Some((origin, c)) => (
                    true,
                    origin_index(origin),
                    decimal(kda_ratio(c.kills, c.deaths, c.assists), lang),
                    decimal(c.cs_per_min, lang),
                    thousands(c.dpm.round() as u32, lang),
                    thousands(c.gold_per_min.round() as u32, lang),
                    decimal(c.vision_per_min, lang),
                ),
                None => (
                    false,
                    0,
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            };
            StatsChampProfile {
                has: true,
                physical: f32::from(ct.physical) / 100.0,
                magic: f32::from(ct.magic) / 100.0,
                true_damage: f32::from(ct.true_damage) / 100.0,
                traits: model(traits),
                has_local,
                local_origin,
                kda: kda.into(),
                cs_min: cs_min.into(),
                dpm: dpm.into(),
                gpm: gpm.into(),
                vs_min: vs_min.into(),
            }
        }
        None => StatsChampProfile::default(),
    };
    bridge.set_champ_profile(champ_profile);
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
    let search_key = normalize_champion_key(&sel.tier_search);
    let mut filtered: Vec<(&hypatia_shared::TierEntry, StatsChampOption)> = list
        .entries
        .iter()
        .filter(|e| !lanes || sel.tier_role < 0 || e.position.desktop_index() == sel.tier_role)
        .filter(|e| sel.tier_off_meta || e.rates.pick_rate == 0.0 || e.rates.pick_rate >= 0.005)
        .filter_map(|e| {
            let champ = ctx.champion(e.champion_id);
            if !search_key.is_empty() && !normalize_champion_key(&champ.name).contains(&search_key)
            {
                return None;
            }
            Some((e, champ))
        })
        .collect();

    filtered.sort_by(|(a, _), (b, _)| {
        let ord = match sel.tier_sort_col {
            1 => a.rates.win_rate.total_cmp(&b.rates.win_rate),
            2 => a.rates.pick_rate.total_cmp(&b.rates.pick_rate),
            3 => a.rates.ban_rate.total_cmp(&b.rates.ban_rate),
            4 => a.rates.games.cmp(&b.rates.games),
            _ => a
                .tier
                .cmp(&b.tier)
                .then_with(|| b.rates.win_rate.total_cmp(&a.rates.win_rate)),
        };
        if sel.tier_sort_desc {
            ord.reverse()
        } else {
            ord
        }
    });

    let rows: Vec<StatsTierRow> = filtered
        .into_iter()
        .enumerate()
        .map(|(i, (e, champion))| {
            let (win_delta, win_delta_dir) = match e.win_rate_delta {
                Some(d) if d.abs() >= 0.0005 => {
                    let sign = if d > 0.0 { "+" } else { "−" };
                    let text = format!("{sign}{}", percent(d.abs(), lang));
                    let dir = if d > 0.0 { 1 } else { -1 };
                    (text, dir)
                }
                _ => (String::new(), 0),
            };
            StatsTierRow {
                rank: i as i32 + 1,
                tier: tier_index(e.tier),
                champion,
                lane: if lanes {
                    e.position.desktop_index()
                } else {
                    -1
                },
                win: percent(e.rates.win_rate, lang).into(),
                win_delta: win_delta.into(),
                win_delta_dir,
                pick: percent(e.rates.pick_rate, lang).into(),
                ban: percent(e.rates.ban_rate, lang).into(),
                games: thousands(e.rates.games, lang).into(),
            }
        })
        .collect();
    bridge.set_tiers(model(rows));
}

fn push_matchups(bridge: &StatsBridge<'_>, ctx: &mut Ctx<'_>, inputs: &Inputs) {
    let lang = ctx.lang;
    let sel = &inputs.selection;
    let empty = StatsState::Loading;
    let state = inputs.matchups.as_ref().unwrap_or(&empty);
    bridge.set_matchup_block(block(state, |m| (m.origin, m.attribution.clone())));

    let search_key = normalize_champion_key(&sel.matchup_search);
    let show_all = sel.matchup_show_all || !search_key.is_empty();

    let profile = inputs.profile.as_ref().and_then(|p| p.ready());
    let local_list = profile
        .map(|p| aggregate_local_matchups(&p.games, sel.champion_id))
        .unwrap_or_default();
    let local_by_opp: std::collections::HashMap<i64, hypatia_shared::LocalMatchup> = local_list
        .iter()
        .cloned()
        .map(|m| (m.opponent_id, m))
        .collect();
    bridge.set_local_matchups_origin(profile.map_or(0, |p| origin_index(p.origin)));
    let local_rows: Vec<StatsLocalMatchupRow> = local_list
        .into_iter()
        .filter_map(|m| {
            let opponent = ctx.champion(m.opponent_id);
            if !search_key.is_empty()
                && !normalize_champion_key(&opponent.name).contains(&search_key)
            {
                return None;
            }
            let win_value = m.wins as f32 / m.games.max(1) as f32;
            Some(StatsLocalMatchupRow {
                opponent,
                record: record_label(m.wins, m.games.saturating_sub(m.wins), lang).into(),
                win_rate: percent(win_value, lang).into(),
                kda: format!("{} KDA", decimal(m.kda, lang)).into(),
                win_value,
            })
        })
        .collect();
    bridge.set_local_matchups(model(local_rows));

    let entries = state
        .ready()
        .map(|m| m.entries.as_slice())
        .unwrap_or_default();
    let mut hardest_all: Vec<_> = entries.iter().filter(|m| m.rates.win_rate < 0.5).collect();
    hardest_all.sort_by(|a, b| a.rates.win_rate.total_cmp(&b.rates.win_rate));
    let mut easiest_all: Vec<_> = entries.iter().filter(|m| m.rates.win_rate >= 0.5).collect();
    easiest_all.sort_by(|a, b| b.rates.win_rate.total_cmp(&a.rates.win_rate));

    let has_more = hardest_all.len() > MATCHUPS || easiest_all.len() > MATCHUPS;
    bridge.set_matchup_has_more(has_more);

    let mut row = |m: &hypatia_shared::Matchup| {
        let opponent = ctx.champion(m.opponent_id);
        if !search_key.is_empty() && !normalize_champion_key(&opponent.name).contains(&search_key) {
            return None;
        }
        let verdict = match LaneVerdict::from_matchup(m.rates.win_rate, m.gold_diff_15) {
            LaneVerdict::Good => 0,
            LaneVerdict::Even => 1,
            LaneVerdict::Hard => 2,
        };
        let local_record = local_by_opp
            .get(&m.opponent_id)
            .map(|l| record_label(l.wins, l.games.saturating_sub(l.wins), lang))
            .unwrap_or_default();
        Some(StatsMatchupRow {
            opponent,
            win: percent(m.rates.win_rate, lang).into(),
            win_value: m.rates.win_rate,
            games: games_label(m.rates.games, lang).into(),
            gold: m
                .gold_diff_15
                .map(|g| gold_label(g, lang))
                .unwrap_or_default()
                .into(),
            verdict,
            local_record: local_record.into(),
        })
    };
    let limit = if show_all { usize::MAX } else { MATCHUPS };
    let hardest: Vec<_> = hardest_all
        .into_iter()
        .filter_map(&mut row)
        .take(limit)
        .collect();
    let easiest: Vec<_> = easiest_all
        .into_iter()
        .filter_map(&mut row)
        .take(limit)
        .collect();
    bridge.set_hardest(model(hardest));
    bridge.set_easiest(model(easiest));
}

fn compute_totals(profile_games: &[PlayerGame], lang: Lang) -> StatsTotals {
    let games = summarised_games(profile_games);
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
    StatsTotals {
        games: games.len().to_string().into(),
        win_rate: if games.is_empty() {
            "—".into()
        } else {
            percent(wins / n, lang).into()
        },
        kda: decimal(kda_ratio(k as f32, d as f32, a as f32), lang).into(),
        cs_min: decimal(cs as f32 / minutes, lang).into(),
        dpm: thousands((damage as f32 / minutes).round() as u32, lang).into(),
    }
}

fn filter_my_games(profile_games: &[PlayerGame], my_queue: i32, my_role: i32) -> Vec<&PlayerGame> {
    let queue_filter = my_queue_filter(my_queue);
    let role_filter = position_from_lane(my_role);
    profile_games
        .iter()
        .filter(|g| queue_filter.is_none_or(|q| g.queue == Some(q)))
        .filter(|g| role_filter.is_none_or(|r| g.position == Some(r)))
        .collect()
}

fn push_my_stats(bridge: &StatsBridge<'_>, ctx: &mut Ctx<'_>, inputs: &Inputs) {
    let lang = ctx.lang;
    let empty = StatsState::Loading;
    let state = inputs.profile.as_ref().unwrap_or(&empty);
    bridge.set_profile_block(block(state, |p| (p.origin, None)));

    // Ranked standing card from SummonerSummary.
    bridge.set_ranked_card(match inputs.summoner.as_ref() {
        Some(s) if s.rank_tier.as_deref().is_some_and(|t| !t.is_empty()) => {
            let tier = s.rank_tier.as_deref().unwrap_or_default();
            let div = s.rank_division.as_deref().unwrap_or_default();
            let tier_label = if div.is_empty() {
                tier.to_string()
            } else {
                format!("{tier} {div}")
            };
            let wins = s.rank_wins.unwrap_or(0);
            let losses = s.rank_losses.unwrap_or(0);
            let total = (wins + losses).max(1);
            StatsRankedCard {
                has: true,
                tier_label: tier_label.into(),
                lp: format!("{} LP", s.rank_lp.unwrap_or(0)).into(),
                record: record_label(wins, losses, lang).into(),
                win_rate: percent(wins as f32 / total as f32, lang).into(),
            }
        }
        _ => StatsRankedCard::default(),
    });

    let Some(profile) = state.ready() else {
        bridge.set_role_summaries(model(Vec::new()));
        bridge.set_my_champions(model(Vec::new()));
        bridge.set_games(model(Vec::new()));
        return;
    };

    // Totals cover the summarised games (Summoner's Rift when there are any); the list below
    // keeps every game in `profile.games` (filtered only by explicit user queue/role filters).
    bridge.set_totals(compute_totals(&profile.games, lang));

    let role_summaries: Vec<StatsRoleSummary> = profile
        .roles
        .iter()
        .map(|r| {
            let wr = r.wins as f32 / r.games.max(1) as f32;
            StatsRoleSummary {
                lane: r.position.desktop_index(),
                games: games_label(r.games, lang).into(),
                win_rate: percent(wr, lang).into(),
                kda: decimal(r.kda, lang).into(),
                win_value: wr,
            }
        })
        .collect();
    bridge.set_role_summaries(model(role_summaries));

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

    let counted_ids: Vec<i64> = summarised_games(&profile.games)
        .iter()
        .map(|g| g.game_id)
        .collect();
    let visible = filter_my_games(
        &profile.games,
        inputs.selection.my_queue,
        inputs.selection.my_role,
    );
    let sessions = group_sessions(&visible);
    let mut session_headers: Vec<(String, String)> =
        vec![(String::new(), String::new()); visible.len()];
    for s in &sessions {
        if let Some(&first_idx) = s.game_indices.first() {
            let header = if s.day_key > 0 {
                date_label(s.day_key * 86_400_000, lang)
            } else {
                lang.pick("Session", "Session").to_string()
            };
            let rec = record_label(s.wins, s.losses, lang);
            let summary = match s.avg_score {
                Some(avg) => format!(
                    "{rec} · {} {}",
                    lang.pick("Avg", "Moy."),
                    avg.round() as u32
                ),
                None => rec,
            };
            session_headers[first_idx] = (header, summary);
        }
    }

    let rows: Vec<StatsGameRow> = visible
        .iter()
        .enumerate()
        .map(|(idx, g)| {
            let minutes = (g.duration_secs as f32 / 60.0).max(1.0);
            let opponent = g
                .opponent_champion_id
                .filter(|id| *id > 0)
                .map(|id| ctx.champion(id))
                .unwrap_or_default();
            let keystone = g
                .runes
                .as_ref()
                .and_then(|r| r.perk_ids.first())
                .map(|&id| ctx.icon("perk", id))
                .unwrap_or_default();
            let spell_a = if g.spells.first > 0 {
                ctx.icon("spell", g.spells.first)
            } else {
                StatsIcon::default()
            };
            let spell_b = if g.spells.second > 0 {
                ctx.icon("spell", g.spells.second)
            } else {
                StatsIcon::default()
            };
            let items = model(
                g.items
                    .iter()
                    .take(6)
                    .map(|&id| ctx.icon("item", id))
                    .collect(),
            );
            let kp = g
                .kill_participation
                .map(|v| format!("{}% KP", (v * 100.0).round() as u32))
                .unwrap_or_default();
            let vs_min = format!("{} VS/m", decimal(g.vision_score as f32 / minutes, lang));
            let (score, score_value) = match g.performance_score {
                Some(s) => (s.to_string(), i32::from(s)),
                None => (String::new(), 0),
            };
            let (session_header, session_summary) = &session_headers[idx];
            StatsGameRow {
                game_id: g.game_id.to_string().into(),
                champion: ctx.champion(g.champion_id),
                win: g.win,
                remake: g.is_remake(),
                summarised: counted_ids.contains(&g.game_id),
                lane: g.position.map_or(-1, |p| p.desktop_index()),
                queue: g
                    .queue
                    .map(|q| queue_label(q, lang))
                    .unwrap_or_default()
                    .into(),
                kda: format!("{}/{}/{}", g.kills, g.deaths, g.assists).into(),
                ratio: if g.kill_participation.is_some() {
                    decimal(
                        kda_ratio(g.kills as f32, g.deaths as f32, g.assists as f32),
                        lang,
                    )
                } else {
                    format!(
                        "{} KDA",
                        decimal(
                            kda_ratio(g.kills as f32, g.deaths as f32, g.assists as f32),
                            lang
                        )
                    )
                }
                .into(),
                cs_min: format!("{} CS/m", decimal(g.cs as f32 / minutes, lang)).into(),
                dpm: thousands(
                    (g.damage_to_champions as f32 / minutes).round() as u32,
                    lang,
                )
                .into(),
                vs_min: vs_min.into(),
                kp: kp.into(),
                duration: format!("{}:{:02}", g.duration_secs / 60, g.duration_secs % 60).into(),
                when: date_label(g.started_at_ms, lang).into(),
                opponent,
                keystone,
                spell_a,
                spell_b,
                items,
                score: score.into(),
                score_value,
                badge: badge_index(g.badge),
                session_header: session_header.as_str().into(),
                session_summary: session_summary.as_str().into(),
            }
        })
        .collect();
    bridge.set_games(model(rows));
    bridge.set_inspected_review(crate::companion::build_review_view(
        inputs.inspected_review.as_ref(),
        inputs.llm_enabled,
        &inputs.inspected_debrief,
        lang,
    ));
}

/// What a push needs to turn ids into names and icons.
struct Ctx<'a> {
    app: &'a AppState,
    lang: Lang,
    loader: &'a mut DesktopAssetLoader,
    names: Names,
    game_data: Option<Arc<GameData>>,
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

    fn style_rows(&self, style_id: i64) -> Option<Vec<Vec<i64>>> {
        self.game_data
            .as_deref()
            .and_then(|g| g.style_rows(style_id))
            .or_else(|| builtin_rune_style_slots(style_id))
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
        let primary_rows: Vec<StatsRuneRow> = self
            .style_rows(page.primary_style_id)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| StatsRuneRow {
                        nodes: model(
                            row.into_iter()
                                .map(|id| StatsRuneNode {
                                    icon: self.icon("perk", id),
                                    active: perks.contains(&id),
                                })
                                .collect(),
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let secondary_rows: Vec<StatsRuneRow> = self
            .style_rows(page.sub_style_id)
            .map(|rows| {
                rows.into_iter()
                    .skip(1)
                    .map(|row| StatsRuneRow {
                        nodes: model(
                            row.into_iter()
                                .map(|id| StatsRuneNode {
                                    icon: self.icon("perk", id),
                                    active: perks.contains(&id),
                                })
                                .collect(),
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let shard_rows: Vec<StatsRuneRow> = if page.shard_ids.is_empty() {
            Vec::new()
        } else {
            STAT_SHARD_ROWS
                .iter()
                .enumerate()
                .map(|(row_idx, row)| {
                    let chosen = page.shard_ids.get(row_idx).copied();
                    StatsRuneRow {
                        nodes: model(
                            row.iter()
                                .map(|&id| StatsRuneNode {
                                    icon: self.icon("perk", id),
                                    active: chosen == Some(id),
                                })
                                .collect(),
                        ),
                    }
                })
                .collect()
        };
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
            primary_rows: model(primary_rows),
            secondary_rows: model(secondary_rows),
            shard_rows: model(shard_rows),
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
    let catalog = app.assets.champion_catalog(app.lang());
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

fn my_queue_filter(index: i32) -> Option<StatsQueue> {
    match index {
        0 => Some(StatsQueue::RankedSolo),
        1 => Some(StatsQueue::RankedFlex),
        2 => Some(StatsQueue::Aram),
        3 => Some(StatsQueue::Arena),
        _ => None,
    }
}

fn rank_bracket(index: i32) -> RankBracket {
    match index {
        0 => RankBracket::All,
        2 => RankBracket::DiamondPlus,
        3 => RankBracket::MasterPlus,
        _ => RankBracket::EmeraldPlus,
    }
}

fn stage_index(stage: Option<ItemStage>) -> i32 {
    match stage {
        Some(ItemStage::Starting) => 0,
        Some(ItemStage::Core) => 1,
        Some(ItemStage::Boots) => 2,
        Some(ItemStage::Situational) => 3,
        Some(ItemStage::Slot1) => 5,
        Some(ItemStage::Slot2) => 6,
        Some(ItemStage::Slot3) => 7,
        Some(ItemStage::Slot4) => 8,
        Some(ItemStage::Slot5) => 9,
        None => 4,
    }
}

fn badge_index(badge: Option<MatchBadge>) -> i32 {
    match badge {
        Some(MatchBadge::Mvp) => 1,
        Some(MatchBadge::Ace) => 2,
        Some(MatchBadge::Multikill(3)) => 3,
        Some(MatchBadge::Multikill(4)) => 4,
        Some(MatchBadge::Multikill(_)) => 5,
        None => 0,
    }
}

fn tier_index(tier: Tier) -> i32 {
    match tier {
        Tier::SPlus => 0,
        Tier::S => 1,
        Tier::A => 2,
        Tier::B => 3,
        Tier::C => 4,
        Tier::D => 5,
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

    #[test]
    fn my_stats_totals_count_rift_while_list_keeps_every_game() {
        let mk =
            |id: i64, queue: Option<StatsQueue>, pos: Option<Position>, secs: u32, win: bool| {
                PlayerGame {
                    game_id: id,
                    champion_id: 103,
                    position: pos,
                    queue,
                    win,
                    duration_secs: secs,
                    kills: 6,
                    deaths: 2,
                    assists: 4,
                    cs: 180,
                    damage_to_champions: 24_000,
                    ..PlayerGame::default()
                }
            };
        let games = vec![
            mk(
                1,
                Some(StatsQueue::RankedSolo),
                Some(Position::Mid),
                1800,
                true,
            ),
            mk(2, Some(StatsQueue::Aram), None, 1200, false),
            mk(3, Some(StatsQueue::Arena), None, 900, true),
            mk(
                4,
                Some(StatsQueue::RankedSolo),
                Some(Position::Mid),
                200,
                false,
            ),
        ];
        // Totals strictly reflect the 1 valid Ranked Solo game (100% WR, 1 game).
        let totals = compute_totals(&games, Lang::En);
        assert_eq!(totals.games.as_str(), "1");
        assert_eq!(totals.win_rate.as_str(), "100.0%");

        // Unfiltered list keeps all 4 games (Ranked Solo, ARAM, Arena, and Remake).
        let all = filter_my_games(&games, -1, -1);
        assert_eq!(all.len(), 4);
        // Queue filter narrows cleanly to ARAM.
        let aram = filter_my_games(&games, 2, -1);
        assert_eq!(aram.len(), 1);
        assert_eq!(aram[0].game_id, 2);
        // Role filter narrows cleanly to Mid.
        let mid = filter_my_games(&games, -1, 2);
        assert_eq!(mid.len(), 2);
    }
}
