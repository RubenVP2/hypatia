//! Companion views (wait, champion select, live game, end of game) built from the Match
//! Projection and pushed to Slint only when the part they show actually changed.

use std::sync::Arc;
use std::time::Instant;

use hypatia_core::AppState;
use hypatia_shared::{
    spell_name_from_id_localized, ChampSelectSummary, EndOfGameSummary, Lane, Lang, LiveBoard,
    LiveGameSummary, ObjectiveKind, Position, SourceId, SourceStatus, SummarySnapshot,
    SummonerSummary,
};
use slint::{Image, ModelRc, SharedString, VecModel};

use crate::icons::DesktopAssetLoader;
use crate::labels::{champ_initials, format_mm_ss, queue_type_label, rank_tier_label, sub_phase_label};
use crate::{
    AllyRow, BanEntry, ChampSelectView, EndView, EnemyPick, FeedLine, LiveView, MainWindow,
    MapAlly, MiniWidget, ObjectiveChip, PartyRow, PlayerLine, Screen, StatLine, TowerDot, WaitView,
};

fn objective_kind_index(kind: ObjectiveKind) -> i32 {
    match kind {
        ObjectiveKind::Baron => 0,
        ObjectiveKind::Elder => 1,
        ObjectiveKind::Drake => 2,
        ObjectiveKind::Herald => 3,
        ObjectiveKind::Grubs => 4,
    }
}

fn build_ban_entries(
    names: &[String],
    ids: &[i64],
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
) -> Vec<BanEntry> {
    names
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.trim().is_empty())
        .map(|(i, b)| {
            let id = ids.get(i).copied().filter(|&cid| cid > 0).unwrap_or(0);
            BanEntry {
                name: SharedString::from(b.as_str()),
                initials: champ_initials(b),
                icon: loader.resolve(app_state, "champion", id),
            }
        })
        .collect()
}

fn map_coord_for_lane(lane: i32) -> (f32, f32, i32) {
    match lane {
        0 => (40.0, 145.0, 0),  // Top line, label right
        1 => (132.0, 206.0, 2), // Jungle line, label above
        2 => (200.0, 194.0, 0), // Mid line, label right
        3 => (155.0, 348.0, 2), // Bot carry, label above track
        4 => (295.0, 348.0, 3), // Support on bot line, label below track
        _ => (120.0, 305.0, 0),
    }
}

fn tower_coordinates(order_side: bool, lane: Lane, tier: u8) -> (f32, f32) {
    match (order_side, lane, tier) {
        // Blue (Order) towers: outer (1), inner (2), inhib (3)
        (true, Lane::Top, 1) => (40.0, 130.0),
        (true, Lane::Top, 2) => (40.0, 205.0),
        (true, Lane::Top, 3) => (40.0, 280.0),
        (true, Lane::Mid, 1) => (170.0, 224.0),
        (true, Lane::Mid, 2) => (130.0, 262.0),
        (true, Lane::Mid, 3) => (90.0, 300.0),
        (true, Lane::Bot, 1) => (260.0, 348.0),
        (true, Lane::Bot, 2) => (180.0, 348.0),
        (true, Lane::Bot, 3) => (100.0, 348.0),
        // Red (Chaos) towers: outer (1), inner (2), inhib (3)
        (false, Lane::Top, 1) => (140.0, 42.0),
        (false, Lane::Top, 2) => (220.0, 42.0),
        (false, Lane::Top, 3) => (300.0, 42.0),
        (false, Lane::Mid, 1) => (230.0, 166.0),
        (false, Lane::Mid, 2) => (270.0, 128.0),
        (false, Lane::Mid, 3) => (310.0, 90.0),
        (false, Lane::Bot, 1) => (358.0, 255.0),
        (false, Lane::Bot, 2) => (358.0, 180.0),
        (false, Lane::Bot, 3) => (358.0, 105.0),
        _ => (200.0, 195.0),
    }
}

fn build_champ_select_view(
    cs: &ChampSelectSummary,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    lang: Lang,
) -> ChampSelectView {
    let mut allies = Vec::new();
    let mut map_allies = Vec::new();
    let mut acting_label = None;
    let mut my_turn = false;

    for ally in &cs.my_team {
        let lane = Position::parse(&ally.assigned_position).map_or(-1, Position::desktop_index);
        let has_champ = ally.has_champion();
        let champ_str = if has_champ {
            ally.champion_name.as_str()
        } else {
            ""
        };
        let picking = cs.acting_cell_id == Some(ally.cell_id);
        if picking && ally.is_self {
            my_turn = true;
        }
        if picking {
            let role_word = match lane {
                0 => "top laner",
                1 => "jungler",
                2 => "mid laner",
                3 => "bot laner",
                4 => "support",
                _ => lang.pick("teammate", "coéquipier"),
            };
            let is_ban = cs.acting_type.as_deref() == Some("ban");
            acting_label = Some(if ally.is_self {
                if is_ban {
                    lang.pick("Your turn — ban a champion", "Votre tour — bannissez un champion")
                        .to_string()
                } else {
                    lang.pick("Your turn — pick your champion", "Votre tour — choisissez votre champion")
                        .to_string()
                }
            } else if lang.is_fr() {
                let verb = if is_ban {
                    "en train de bannir"
                } else {
                    "en train de choisir"
                };
                format!("Votre {role_word} est {verb}")
            } else {
                let verb = if is_ban { "banning" } else { "picking" };
                format!("Your {role_word} is {verb}")
            });
        }

        let champ_icon = if has_champ {
            loader.resolve(app_state, "champion", ally.champion_id)
        } else {
            Image::default()
        };
        let spell1_name = if ally.spell1_id > 0 {
            spell_name_from_id_localized(ally.spell1_id, lang)
        } else {
            ""
        };
        let spell2_name = if ally.spell2_id > 0 {
            spell_name_from_id_localized(ally.spell2_id, lang)
        } else {
            ""
        };

        allies.push(AllyRow {
            lane,
            champion: SharedString::from(champ_str),
            initials: champ_initials(champ_str),
            icon: champ_icon,
            spell1_icon: loader.resolve(app_state, "spell", ally.spell1_id),
            spell1_name: SharedString::from(spell1_name),
            spell2_icon: loader.resolve(app_state, "spell", ally.spell2_id),
            spell2_name: SharedString::from(spell2_name),
            player: SharedString::from(ally.summoner_name.as_str()),
            rank: SharedString::from(
                ally.rank_label
                    .as_deref()
                    .unwrap_or(lang.pick("Unranked", "Non classé")),
            ),
            is_self: ally.is_self,
            is_premade: ally.is_premade,
            picking,
            hidden: ally.is_anonymized,
        });

        let (x, y, label_pos) = map_coord_for_lane(lane);
        let map_label = if !champ_str.is_empty() {
            if ally.is_self {
                format!("{champ_str} · {}", lang.pick("you", "vous"))
            } else {
                champ_str.to_string()
            }
        } else if ally.is_self {
            lang.pick("You", "Vous").to_string()
        } else {
            ally.summoner_name.clone()
        };
        map_allies.push(MapAlly {
            x,
            y,
            label: SharedString::from(map_label),
            is_self: ally.is_self,
            label_pos,
        });
    }

    let mut enemies = Vec::new();
    let mut enemies_to_come = 0;
    for e in &cs.their_team {
        if e.champion_id > 0 && !e.champion_name.is_empty() {
            enemies.push(EnemyPick {
                champion: SharedString::from(e.champion_name.as_str()),
                initials: champ_initials(&e.champion_name),
                icon: loader.resolve(app_state, "champion", e.champion_id),
            });
        } else {
            enemies_to_come += 1;
        }
    }
    if cs.their_team.is_empty() {
        enemies_to_come = 5;
    }

    let title = acting_label.unwrap_or_else(|| {
        if cs.sub_phase == "FINALIZATION" {
            lang.pick("Prepare your runes & spells", "Préparez vos runes et sorts")
                .to_string()
        } else {
            lang.pick(
                "Champion select in progress",
                "Sélection des champions en cours",
            )
            .to_string()
        }
    });
    let q_label = queue_type_label(&cs.queue_type, lang);
    let s_label = sub_phase_label(&cs.sub_phase, lang);
    let detail = match (q_label.is_empty(), s_label.is_empty()) {
        (true, true) => String::new(),
        (true, false) => s_label,
        (false, true) => q_label,
        (false, false) => format!("{q_label} · {s_label}"),
    };

    let my_ban_list = build_ban_entries(&cs.my_team_bans, &cs.my_team_ban_ids, app_state, loader);
    let their_ban_list =
        build_ban_entries(&cs.their_team_bans, &cs.their_team_ban_ids, app_state, loader);

    ChampSelectView {
        title: SharedString::from(title),
        detail: SharedString::from(detail),
        timer: format_mm_ss(cs.time_left_sec as f64),
        my_turn,
        allies: ModelRc::new(VecModel::from(allies)),
        map_allies: ModelRc::new(VecModel::from(map_allies)),
        enemies: ModelRc::new(VecModel::from(enemies)),
        enemies_to_come,
        my_bans: SharedString::from(cs.my_team_bans.join(", ")),
        their_bans: SharedString::from(cs.their_team_bans.join(", ")),
        my_ban_list: ModelRc::new(VecModel::from(my_ban_list)),
        their_ban_list: ModelRc::new(VecModel::from(their_ban_list)),
        anonymized: cs.is_ranked_solo_anonymized,
    }
}

fn build_live_view(
    lg: &LiveGameSummary,
    board: Option<&LiveBoard>,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    lang: Lang,
) -> LiveView {
    let default_board = LiveBoard::default();
    let b = board.unwrap_or(&default_board);

    let mut my_players = Vec::new();
    let mut enemy_players = Vec::new();
    let mut self_dead = false;
    let mut self_respawn = 0.0;
    let mut self_kda = format!("{} / {} / {}", lg.kills, lg.deaths, lg.assists);
    let mut self_champion_id = 0;

    for p in &lg.players {
        let mine = (p.team == "ORDER") == b.my_team_is_order;
        let lane = Position::parse(&p.position).map_or(-1, Position::desktop_index);
        let kda_str = format!("{} / {} / {}", p.kills, p.deaths, p.assists);
        let cid = p.champion_id;
        if p.is_self {
            self_dead = p.is_dead;
            self_respawn = p.respawn_timer_sec;
            self_kda = kda_str.clone();
            if cid > 0 {
                self_champion_id = cid;
            }
        }
        let row = PlayerLine {
            lane,
            champion: SharedString::from(p.champion_name.as_str()),
            initials: champ_initials(&p.champion_name),
            icon: loader.resolve(app_state, "champion", cid),
            level: p.level as i32,
            kda: SharedString::from(kda_str),
            cs: p.creep_score as i32,
            is_self: p.is_self,
            dead: p.is_dead,
            respawn: SharedString::from(format!("{}s", p.respawn_timer_sec.ceil() as i32)),
        };
        if mine {
            my_players.push(row);
        } else {
            enemy_players.push(row);
        }
    }

    let towers: Vec<TowerDot> = b
        .towers
        .iter()
        .map(|t| {
            let (x, y) = tower_coordinates(t.order_side, t.lane, t.tier);
            TowerDot {
                x,
                y,
                mine: t.order_side == b.my_team_is_order,
                standing: t.standing,
            }
        })
        .collect();

    let mut my_objectives = Vec::new();
    let mut enemy_objectives = Vec::new();
    for obj in &b.objectives {
        let mine = obj.by_order == b.my_team_is_order;
        let chip = ObjectiveChip {
            kind: objective_kind_index(obj.kind),
            weight: obj.kind.weight() as i32,
            label: SharedString::from(obj.label.as_str()),
            mine,
        };
        if mine {
            my_objectives.push(chip);
        } else {
            enemy_objectives.push(chip);
        }
    }

    let feed: Vec<FeedLine> = b
        .feed
        .iter()
        .take(6)
        .map(|f| FeedLine {
            time: format_mm_ss(f.time_sec),
            text: SharedString::from(f.text.as_str()),
            lane: match f.lane {
                Some(Lane::Top) => 0,
                Some(Lane::Jungle) => 1,
                Some(Lane::Mid) => 2,
                Some(Lane::Bot) => 3,
                None => -1,
            },
            side: match f.by_my_team {
                Some(true) => 1,
                Some(false) => 0,
                None => -1,
            },
        })
        .collect();

    let (title, detail) = if self_dead {
        let respawn_secs = self_respawn.ceil() as i32;
        (
            if lang.is_fr() {
                format!("Réapparition dans {}s", respawn_secs)
            } else {
                format!("Respawn in {}s", respawn_secs)
            },
            format!("{} · {} {}", lg.active_champion, lang.pick("You", "Vous"), self_kda),
        )
    } else {
        (
            if lang.is_fr() {
                format!("Vous jouez {} · Niveau {}", lg.active_champion, lg.level)
            } else {
                format!("Playing {} · Level {}", lg.active_champion, lg.level)
            },
            if lang.is_fr() {
                format!(
                    "Vous {} · {} CS · {} or",
                    self_kda, lg.creep_score, lg.current_gold
                )
            } else {
                format!(
                    "You {} · {} CS · {} gold",
                    self_kda, lg.creep_score, lg.current_gold
                )
            },
        )
    };

    let active_cid = if lg.active_champion_id > 0 {
        lg.active_champion_id
    } else {
        self_champion_id
    };

    LiveView {
        title: SharedString::from(title),
        detail: SharedString::from(detail),
        dead: self_dead,
        active_initials: champ_initials(&lg.active_champion),
        active_icon: loader.resolve(app_state, "champion", active_cid),
        my_score: b.my_kills() as i32,
        enemy_score: b.enemy_kills() as i32,
        clock: format_mm_ss(lg.game_time_sec),
        self_kda: SharedString::from(self_kda),
        my_players: ModelRc::new(VecModel::from(my_players)),
        enemy_players: ModelRc::new(VecModel::from(enemy_players)),
        towers: ModelRc::new(VecModel::from(towers)),
        my_objectives: ModelRc::new(VecModel::from(my_objectives)),
        enemy_objectives: ModelRc::new(VecModel::from(enemy_objectives)),
        feed: ModelRc::new(VecModel::from(feed)),
    }
}

fn build_end_view(
    eog: &EndOfGameSummary,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    lang: Lang,
) -> EndView {
    let victory = eog.result.eq_ignore_ascii_case("victory");
    let cid = eog.champion_id;
    let stats = vec![
        StatLine {
            label: SharedString::from("Champion"),
            value: SharedString::from(eog.champion_name.as_str()),
        },
        StatLine {
            label: SharedString::from(lang.pick("Game Length", "Durée de la partie")),
            value: format_mm_ss(eog.game_length_sec as f64),
        },
        StatLine {
            label: SharedString::from(lang.pick("Total Gold", "Or accumulé")),
            value: SharedString::from(format!("{}", eog.total_gold)),
        },
        StatLine {
            label: SharedString::from(lang.pick("Damage to Champions", "Dégâts aux champions")),
            value: SharedString::from(format!("{}", eog.damage_to_champions)),
        },
    ];
    EndView {
        title: SharedString::from(if victory {
            lang.pick("Victory", "Victoire")
        } else {
            lang.pick("Defeat", "Défaite")
        }),
        detail: SharedString::from(format!(
            "{} · {}",
            eog.champion_name,
            format_mm_ss(eog.game_length_sec as f64)
        )),
        kda: SharedString::from(format!("{} / {} / {}", eog.kills, eog.deaths, eog.assists)),
        victory,
        champion_name: SharedString::from(eog.champion_name.as_str()),
        champion_initials: champ_initials(&eog.champion_name),
        champion_icon: loader.resolve(app_state, "champion", cid),
        stats: ModelRc::new(VecModel::from(stats)),
    }
}
/// Everything besides the Match Projection that influences how views are rendered.
#[derive(Clone, Copy, PartialEq, Eq)]
struct RenderContext {
    lang: Lang,
    icons_enabled: bool,
    /// Bumped by the asset cache each time a new icon lands on disk.
    icons_revision: u64,
    mini_open: bool,
}

/// What the companion windows currently display.
struct Shown {
    snapshot: Arc<SummarySnapshot>,
    view: Screen,
    context: RenderContext,
}

/// A phase clock read from the Riot clients and extrapolated locally, so the displayed time
/// keeps moving between two LCU events instead of freezing and then jumping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseClock {
    anchor: Instant,
    base_sec: f64,
    counts_down: bool,
    /// Full length of a countdown phase in seconds; 0 when unknown (counting up, or no total).
    pub total_sec: i32,
}

impl PhaseClock {
    fn down(base_sec: f64, total_sec: i32) -> Self {
        Self { anchor: Instant::now(), base_sec, counts_down: true, total_sec }
    }

    fn up(base_sec: f64) -> Self {
        Self { anchor: Instant::now(), base_sec, counts_down: false, total_sec: 0 }
    }

    /// Whole seconds to display right now: rounded up while counting down (a countdown shows
    /// 0 only once the phase is over), rounded down while counting up.
    pub fn seconds_now(&self, now: Instant) -> i32 {
        let elapsed = now.saturating_duration_since(self.anchor).as_secs_f64();
        if self.counts_down {
            (self.base_sec - elapsed).max(0.0).ceil() as i32
        } else {
            (self.base_sec + elapsed).max(0.0).floor() as i32
        }
    }

    pub fn counts_down(&self) -> bool {
        self.counts_down
    }
}

/// Which clock, if any, the active view shows.
fn phase_clock(snap: &SummarySnapshot, view: Screen) -> Option<PhaseClock> {
    match view {
        Screen::Queue => snap.matchmaking.as_ref().map(|m| PhaseClock::up(m.time_in_queue_sec)),
        Screen::ReadyCheck => snap
            .matchmaking
            .as_ref()
            .and_then(|m| m.ready_check_timer_sec)
            .map(|left| PhaseClock::down(f64::from(left), 10.max(left))),
        Screen::ChampSelect => snap.champ_select.as_ref().map(|cs| {
            let total = if cs.total_time_sec > 0 { cs.total_time_sec } else { cs.time_left_sec };
            PhaseClock::down(cs.time_left_sec as f64, total as i32)
        }),
        Screen::InGame => snap.live_game.as_ref().map(|lg| PhaseClock::up(lg.game_time_sec)),
        _ => None,
    }
}

/// Keeps the companion windows in sync with the Match Projection, touching Slint only for the
/// sections whose data changed since the previous refresh.
#[derive(Default)]
pub struct Companion {
    loader: DesktopAssetLoader,
    shown: Option<Shown>,
    clock: Option<PhaseClock>,
}

impl Companion {
    pub fn refresh(
        &mut self,
        main_win: &MainWindow,
        mini_win: &MiniWidget,
        app_state: &AppState,
        mini_open: bool,
    ) {
        let snapshot = app_state.get_summary();
        let context = RenderContext {
            lang: app_state.lang(),
            icons_enabled: app_state.flags.game_icons(),
            icons_revision: app_state.assets.image_revision(),
            mini_open,
        };
        // A different context invalidates every section; an identical projection none.
        let previous = self.shown.take().filter(|shown| shown.context == context);
        if let Some(shown) = previous {
            if Arc::ptr_eq(&shown.snapshot, &snapshot) {
                self.shown = Some(shown);
                return;
            }
            self.shown = Some(shown);
        }
        let view = apply_summary(
            main_win,
            mini_win,
            app_state,
            &mut self.loader,
            &snapshot,
            self.shown.as_ref(),
            mini_open,
        );
        // Re-anchor only on a new snapshot: each LCU event carries a fresh reading.
        self.clock = phase_clock(&snapshot, view);
        self.shown = Some(Shown {
            snapshot,
            view,
            context,
        });
    }

    /// The clock the active view shows, if it has one.
    pub fn clock(&self) -> Option<PhaseClock> {
        self.clock
    }
}

/// Pushes `snap` to the windows and returns the active screen. Sections equal to what
/// `previous` already displays are left untouched.
fn apply_summary(
    main_win: &MainWindow,
    mini_win: &MiniWidget,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    snap: &SummarySnapshot,
    previous: Option<&Shown>,
    mini_open: bool,
) -> Screen {
    let lang = app_state.lang();
    let prev = previous.map(|shown| &*shown.snapshot);
    let phase_str = snap
        .phase
        .as_ref()
        .map(|p| p.phase.as_str())
        .unwrap_or("None");

    let source_info = |id: SourceId| -> (bool, SharedString) {
        snap.sources
            .iter()
            .find(|s| s.id == id)
            .map(|s| {
                let live = matches!(s.status, SourceStatus::Live | SourceStatus::Discovered);
                let port = s
                    .port
                    .map(|p| SharedString::from(format!(":{p}")))
                    .unwrap_or_default();
                (live, port)
            })
            .unwrap_or((false, SharedString::default()))
    };

    let (lcu_live, lcu_port) = source_info(SourceId::LeagueClient);
    let (rc_live, rc_port) = source_info(SourceId::RiotClient);
    let (game_live, game_port) = source_info(SourceId::LiveGame);
    main_win.set_lcu_live(lcu_live);
    main_win.set_lcu_port(lcu_port);
    main_win.set_rc_live(rc_live);
    main_win.set_rc_port(rc_port);
    main_win.set_game_live(game_live);
    main_win.set_game_port(game_port);

    if (lcu_live || game_live) && !app_state.is_demo_active() {
        app_state
            .assets
            .trigger_metadata_sync(&app_state.discovery, &app_state.riot_client);
    }

    let view = match phase_str {
        "Lobby" => Screen::Lobby,
        "Matchmaking" => Screen::Queue,
        "ReadyCheck" => Screen::ReadyCheck,
        "ChampSelect" => Screen::ChampSelect,
        "InProgress" => Screen::InGame,
        "EndOfGame" | "PreEndOfGame" => Screen::Results,
        _ if lcu_live || snap.summoner.is_some() => Screen::Idle,
        _ => Screen::Offline,
    };
    main_win.set_view(view);
    mini_win.set_view(view);

    // Queue label in top bar
    let queue_label = snap
        .lobby
        .as_ref()
        .map(|l| l.queue_name.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            snap.champ_select
                .as_ref()
                .map(|c| queue_type_label(&c.queue_type, lang))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();
    main_win.set_queue_label(SharedString::from(queue_label));

    // Populate Lobby / Queue / Idle wait view
    let wait_changed = previous.is_none_or(|shown| shown.view != view)
        || prev.is_none_or(|p| {
            p.summoner != snap.summoner || p.lobby != snap.lobby || p.matchmaking != snap.matchmaking
        });
    let waiting = !matches!(view, Screen::ChampSelect | Screen::InGame | Screen::Results);
    if waiting && wait_changed {
        let summoner_label = snap
            .summoner
            .as_ref()
            .map(SummonerSummary::riot_id)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| lang.pick("Not signed in", "Non connecté").to_string());
        let rank_label = snap
            .summoner
            .as_ref()
            .and_then(|s| {
                s.rank_tier.as_ref().map(|t| {
                    format!(
                        "{} {} · {} LP",
                        rank_tier_label(t, lang),
                        s.rank_division.as_deref().unwrap_or(""),
                        s.rank_lp.unwrap_or(0)
                    )
                })
            })
            .unwrap_or_else(|| lang.pick("Unranked", "Non classé").to_string());

        let (title, detail, specimen) = match view {
            Screen::Offline => (
                lang.pick(
                    "Waiting for League of Legends",
                    "En attente de League of Legends",
                )
                .to_string(),
                lang.pick(
                    "Open the Riot Client or League of Legends — Hypatia connects automatically.",
                    "Lancez le Client Riot ou League of Legends — Hypatia se connecte automatiquement.",
                )
                .to_string(),
                SharedString::default(),
            ),
            Screen::Idle => (
                if lang.is_fr() {
                    format!("Connecté en tant que {summoner_label}")
                } else {
                    format!("Signed in as {summoner_label}")
                },
                lang.pick(
                    "Open a lobby or queue up to see your party and lane map.",
                    "Ouvrez un salon ou lancez une file pour voir votre équipe et la carte des postes.",
                )
                .to_string(),
                SharedString::default(),
            ),
            Screen::Lobby => {
                let qn = snap
                    .lobby
                    .as_ref()
                    .map(|l| l.queue_name.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(lang.pick("Lobby", "Salon"));
                (
                    if lang.is_fr() {
                        format!("Dans le salon · {qn}")
                    } else {
                        format!("In Lobby · {qn}")
                    },
                    lang.pick(
                        "Your party and lanes appear here. Pick lanes and queue in League, or from your phone with Mobile Remote.",
                        "Votre groupe et vos postes s'affichent ici. Choisissez vos postes et lancez la file dans League, ou depuis votre téléphone avec l'App Mobile.",
                    )
                    .to_string(),
                    SharedString::default(),
                )
            }
            Screen::Queue => {
                let (elapsed, est) = snap
                    .matchmaking
                    .as_ref()
                    .map(|m| (m.time_in_queue_sec, m.estimated_queue_time_sec))
                    .unwrap_or((0.0, 60.0));
                (
                    lang.pick("In Queue", "Recherche d'une partie").to_string(),
                    if lang.is_fr() {
                        format!("Temps estimé {}", format_mm_ss(est))
                    } else {
                        format!("Est. queue time {}", format_mm_ss(est))
                    },
                    format_mm_ss(elapsed),
                )
            }
            _ => {
                let resp = snap
                    .matchmaking
                    .as_ref()
                    .and_then(|m| m.player_response.as_deref())
                    .unwrap_or("None");
                (
                    if resp == "Accepted" {
                        lang.pick(
                            "Match Accepted — waiting on other players",
                            "Partie acceptée — en attente des autres joueurs",
                        )
                        .to_string()
                    } else {
                        lang.pick(
                            "Match Found — accept now!",
                            "Partie trouvée — acceptez maintenant !",
                        )
                        .to_string()
                    },
                    lang.pick(
                        "You can accept straight from your phone via Mobile Remote.",
                        "Vous pouvez accepter directement depuis votre téléphone avec l'App Mobile.",
                    )
                    .to_string(),
                    SharedString::from(lang.pick("READY", "PRÊT")),
                )
            }
        };

        let mut notes = vec![
            StatLine {
                label: SharedString::from(lang.pick("Summoner", "Invocateur")),
                value: SharedString::from(summoner_label),
            },
            StatLine {
                label: SharedString::from(lang.pick("Rank", "Classement")),
                value: SharedString::from(rank_label),
            },
        ];
        if let Some(ref s) = snap.summoner {
            if s.summoner_level > 0 {
                notes.push(StatLine {
                    label: SharedString::from(lang.pick("Summoner Level", "Niveau d'invocateur")),
                    value: SharedString::from(if lang.is_fr() {
                        format!("Niveau {}", s.summoner_level)
                    } else {
                        format!("Level {}", s.summoner_level)
                    }),
                });
            }
        }

        let summoner_icon = snap
            .summoner
            .as_ref()
            .map(|s| loader.resolve(app_state, "profile", s.profile_icon_id))
            .unwrap_or_default();
        let summoner_initials = snap
            .summoner
            .as_ref()
            .map(|s| champ_initials(&s.display_name))
            .unwrap_or_default();
        let summoner_header_name = snap
            .summoner
            .as_ref()
            .map(SummonerSummary::riot_id)
            .map(SharedString::from)
            .unwrap_or_default();

        let wait = WaitView {
            title: SharedString::from(title),
            detail: SharedString::from(detail),
            specimen,
            summoner_name: summoner_header_name,
            summoner_initials,
            summoner_icon,
            notes: ModelRc::new(VecModel::from(notes)),
        };
        if mini_open {
            mini_win.set_wait(wait.clone());
        }
        main_win.set_wait(wait);

        let party: Vec<PartyRow> = snap
            .lobby
            .as_ref()
            .map(|l| {
                l.members
                    .iter()
                    .map(|m| PartyRow {
                        name: SharedString::from(m.summoner_name.as_str()),
                        first_lane: Position::parse(m.first_position.as_deref().unwrap_or(""))
                            .map_or(-1, Position::desktop_index),
                        second_lane: Position::parse(m.second_position.as_deref().unwrap_or(""))
                            .map_or(-1, Position::desktop_index),
                        is_leader: m.is_leader,
                        is_self: m.is_self,
                    })
                    .collect()
            })
            .unwrap_or_default();
        main_win.set_party(ModelRc::new(VecModel::from(party)));
    }

    if let Some(ref cs) = snap.champ_select {
        if prev.is_none_or(|p| p.champ_select.as_ref() != Some(cs)) {
            let champ = build_champ_select_view(cs, app_state, loader, lang);
            // The mini widget only receives data while it is on screen.
            if mini_open {
                mini_win.set_champ(champ.clone());
            }
            main_win.set_champ(champ);
        }
    }

    if let Some(ref lg) = snap.live_game {
        if prev.is_none_or(|p| p.live_game.as_ref() != Some(lg) || p.board != snap.board) {
            let lv = build_live_view(lg, snap.board.as_ref(), app_state, loader, lang);
            // The mini widget only receives data while it is on screen.
            if mini_open {
                mini_win.set_live(lv.clone());
            }
            main_win.set_live(lv);
        }
    }
    mini_win.set_in_game(snap.live_game.is_some());

    if let Some(ref eog) = snap.end_of_game {
        if prev.is_none_or(|p| p.end_of_game.as_ref() != Some(eog)) {
            main_win.set_end(build_end_view(eog, app_state, loader, lang));
        }
    }

    view
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_objective_kinds_to_distinct_icon_indices() {
        assert_eq!(objective_kind_index(ObjectiveKind::Baron), 0);
        assert_eq!(objective_kind_index(ObjectiveKind::Elder), 1);
        assert_eq!(objective_kind_index(ObjectiveKind::Drake), 2);
        assert_eq!(objective_kind_index(ObjectiveKind::Herald), 3);
        assert_eq!(objective_kind_index(ObjectiveKind::Grubs), 4);
    }

    #[test]
    fn bot_and_support_map_labels_stay_separated() {
        let (bx, by, b_pos) = map_coord_for_lane(3);
        let (sx, sy, s_pos) = map_coord_for_lane(4);
        assert_ne!(b_pos, s_pos, "Bot and Support labels must use opposite sides of track");
        assert!((sx - bx).abs() >= 120.0, "Bot and Support nodes must have horizontal spacing");
        assert!((by - sy).abs() < 1.0);
    }

    #[test]
    fn tower_coordinates_fit_within_track_canvas() {
        for order in [true, false] {
            for lane in [Lane::Top, Lane::Mid, Lane::Bot] {
                for tier in 1..=3 {
                    let (x, y) = tower_coordinates(order, lane, tier);
                    assert!((40.0..=360.0).contains(&x), "tower x out of canvas: {x}");
                    assert!((40.0..=360.0).contains(&y), "tower y out of canvas: {y}");
                }
            }
        }
    }
}
