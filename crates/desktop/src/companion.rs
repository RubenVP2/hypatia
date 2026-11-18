//! Companion views (wait, champion select, live game, end of game) built from the Match
//! Projection and pushed to Slint only when the part they show actually changed.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use hypatia_core::art::{self, ClientArt as ArtPiece};
use hypatia_core::AppState;
use hypatia_shared::{
    spell_name_from_id_localized, ChampSelectSummary, EndOfGamePlayer, EndOfGameSummary, Lane, Lang, LiveBoard,
    LiveGameSummary, ObjectiveKind, Position, SourceId, SourceStatus, SummarySnapshot,
    SummonerSummary,
};
use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};

use crate::icons::{AppIconLookup, DesktopAssetLoader, IconLookup};
use crate::labels::{champ_initials, format_mm_ss, queue_type_label, rank_tier_label, sub_phase_label};
use crate::{
    AllyRow, BanEntry, ChampSelectView, ClientArt, EndView, EnemyPick, FeedLine, LiveView, MainWindow,
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

fn canonical_rank_tier(raw: Option<&str>) -> SharedString {
    let Some(token) = raw.and_then(|s| s.split_whitespace().next()) else {
        return SharedString::default();
    };
    let upper = token.to_ascii_uppercase();
    match upper.as_str() {
        "IRON" | "BRONZE" | "SILVER" | "GOLD" | "PLATINUM" | "EMERALD" | "DIAMOND" | "MASTER"
        | "GRANDMASTER" | "CHALLENGER" => SharedString::from(upper),
        _ => SharedString::default(),
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
struct PitSummary {
    my_drakes: i32,
    enemy_drakes: i32,
    baron_owner: i32,
    herald_owner: i32,
}

fn summarize_pits(board: &LiveBoard) -> PitSummary {
    let mut out = PitSummary::default();
    for obj in &board.objectives {
        let mine = obj.by_order == board.my_team_is_order;
        match obj.kind {
            ObjectiveKind::Drake | ObjectiveKind::Elder => {
                if mine {
                    out.my_drakes += 1;
                } else {
                    out.enemy_drakes += 1;
                }
            }
            ObjectiveKind::Baron => out.baron_owner = if mine { 1 } else { 2 },
            ObjectiveKind::Herald => out.herald_owner = if mine { 1 } else { 2 },
            ObjectiveKind::Grubs => {}
        }
    }
    out
}

fn build_ban_entries(
    names: &[String],
    ids: &[i64],
    icons: &mut impl IconLookup,
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
                icon: icons.resolve_icon("champion", id),
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

/// Row models of the champion select view, kept for the whole session so Slint updates rows in
/// place. Only a row that did not exist before (a ban landing), or an element that newly
/// appears inside a row (a lock, a reveal), is created, and the one-shot draft animations play
/// on creation. A fresh model per update would rebuild every row and replay them all.
#[derive(Default)]
pub struct DraftModels {
    allies: Rc<VecModel<AllyRow>>,
    map_allies: Rc<VecModel<MapAlly>>,
    enemies: Rc<VecModel<EnemyPick>>,
    my_bans: Rc<VecModel<BanEntry>>,
    their_bans: Rc<VecModel<BanEntry>>,
}

/// Row models of the live game view, kept like `DraftModels` so a tower that falls or an
/// objective that is taken updates its own row and plays its moment once.
#[derive(Default)]
pub struct LiveModels {
    my_players: Rc<VecModel<PlayerLine>>,
    enemy_players: Rc<VecModel<PlayerLine>>,
    towers: Rc<VecModel<TowerDot>>,
    my_objectives: Rc<VecModel<ObjectiveChip>>,
    enemy_objectives: Rc<VecModel<ObjectiveChip>>,
    feed: Rc<VecModel<FeedLine>>,
}

/// Brings `model` to `rows`, touching only the rows that differ.
fn sync_rows<T: Clone + PartialEq + 'static>(model: &Rc<VecModel<T>>, rows: Vec<T>) -> ModelRc<T> {
    let keep = rows.len();
    for (i, row) in rows.into_iter().enumerate() {
        match model.row_data(i) {
            Some(old) if old == row => {}
            Some(_) => model.set_row_data(i, row),
            None => model.push(row),
        }
    }
    while model.row_count() > keep {
        model.remove(model.row_count() - 1);
    }
    ModelRc::from(model.clone())
}

pub fn build_champ_select_view(
    cs: &ChampSelectSummary,
    icons: &mut impl IconLookup,
    models: &DraftModels,
    lang: Lang,
) -> ChampSelectView {
    let mut allies = Vec::new();
    let mut map_allies = Vec::new();
    let mut acting_label = None;
    let mut my_turn = false;
    let mut self_splash: Option<(i64, i64)> = None;
    let mut fallback_splash: Option<(i64, i64)> = None;

    for ally in &cs.my_team {
        let lane = Position::parse(&ally.assigned_position).map_or(-1, Position::desktop_index);
        let has_champ = ally.has_champion();
        if has_champ {
            if ally.is_self {
                self_splash = Some((ally.selected_skin_id, ally.champion_id));
            } else if fallback_splash.is_none() {
                fallback_splash = Some((ally.selected_skin_id, ally.champion_id));
            }
        }
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
            icons.resolve_icon("champion", ally.champion_id)
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

        let banner = if has_champ {
            icons.resolve_skin_art(art::BANNER_CATEGORY, ally.selected_skin_id, ally.champion_id)
        } else {
            Image::default()
        };
        allies.push(AllyRow {
            lane,
            champion: SharedString::from(champ_str),
            initials: champ_initials(champ_str),
            icon: champ_icon.clone(),
            banner,
            locked: has_champ && ally.pick_locked,
            spell1_icon: icons.resolve_icon("spell", ally.spell1_id),
            spell1_name: SharedString::from(spell1_name),
            spell2_icon: icons.resolve_icon("spell", ally.spell2_id),
            spell2_name: SharedString::from(spell2_name),
            player: SharedString::from(ally.summoner_name.as_str()),
            rank: SharedString::from(
                ally.rank_label
                    .as_deref()
                    .unwrap_or(lang.pick("Unranked", "Non classé")),
            ),
            rank_tier: canonical_rank_tier(ally.rank_label.as_deref()),
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
            initials: champ_initials(champ_str),
            icon: champ_icon,
            is_self: ally.is_self,
            label_pos,
        });
    }

    // One stable slot per enemy cell, so a reveal fills its own slot instead of shifting rows.
    let mut enemies = Vec::new();
    let mut enemies_to_come = 0;
    for e in &cs.their_team {
        let picked = e.champion_id > 0 && !e.champion_name.is_empty();
        if !picked {
            enemies_to_come += 1;
            enemies.push(EnemyPick::default());
            continue;
        }
        enemies.push(EnemyPick {
            champion: SharedString::from(e.champion_name.as_str()),
            initials: champ_initials(&e.champion_name),
            icon: icons.resolve_icon("champion", e.champion_id),
            picked,
        });
    }
    if cs.their_team.is_empty() {
        enemies_to_come = 5;
        enemies.resize(5, EnemyPick::default());
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

    let my_ban_list = build_ban_entries(&cs.my_team_bans, &cs.my_team_ban_ids, icons);
    let their_ban_list =
        build_ban_entries(&cs.their_team_bans, &cs.their_team_ban_ids, icons);
    let (splash_skin, splash_champ) = self_splash.or(fallback_splash).unwrap_or((0, 0));

    ChampSelectView {
        title: SharedString::from(title),
        detail: SharedString::from(detail),
        timer: format_mm_ss(cs.time_left_sec as f64),
        my_turn,
        allies: sync_rows(&models.allies, allies),
        map_allies: sync_rows(&models.map_allies, map_allies),
        enemies: sync_rows(&models.enemies, enemies),
        enemies_to_come,
        my_bans: SharedString::from(cs.my_team_bans.join(", ")),
        their_bans: SharedString::from(cs.their_team_bans.join(", ")),
        my_ban_list: sync_rows(&models.my_bans, my_ban_list),
        their_ban_list: sync_rows(&models.their_bans, their_ban_list),
        anonymized: cs.is_ranked_solo_anonymized,
        splash_icon: icons.resolve_skin_art("splash", splash_skin, splash_champ),
    }
}

pub fn build_live_view(
    lg: &LiveGameSummary,
    board: Option<&LiveBoard>,
    icons: &mut impl IconLookup,
    models: &LiveModels,
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
    let mut self_skin_id = 0;

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
                self_skin_id = p.skin_id;
            }
        }
        let skin = if cid > 0 { cid * 1000 + p.skin_id } else { 0 };
        let row = PlayerLine {
            lane,
            champion: SharedString::from(p.champion_name.as_str()),
            initials: champ_initials(&p.champion_name),
            icon: icons.resolve_icon("champion", cid),
            banner: icons.resolve_skin_art(art::BANNER_CATEGORY, skin, cid),
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

    let respawn_secs = self_respawn.ceil() as i32;
    let (title, detail) = if self_dead {
        (
            if lang.is_fr() {
                format!("Réapparition dans {}s", respawn_secs)
            } else {
                format!("Respawn in {}s", respawn_secs)
            },
            format!("{} · {}", lg.active_champion, self_kda),
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
    // The skin number belongs to the player's row; it applies only to that same champion.
    let self_skin = if active_cid > 0 && active_cid == self_champion_id {
        active_cid * 1000 + self_skin_id
    } else {
        0
    };
    let pits = summarize_pits(b);

    LiveView {
        title: SharedString::from(title),
        detail: SharedString::from(detail),
        dead: self_dead,
        active_initials: champ_initials(&lg.active_champion),
        active_icon: icons.resolve_icon("champion", active_cid),
        splash_icon: icons.resolve_skin_art("splash", self_skin, active_cid),
        respawn_seconds: if self_dead { respawn_secs.max(0) } else { -1 },
        my_score: b.my_kills() as i32,
        enemy_score: b.enemy_kills() as i32,
        clock: format_mm_ss(lg.game_time_sec),
        self_kda: SharedString::from(self_kda),
        my_drakes: pits.my_drakes,
        enemy_drakes: pits.enemy_drakes,
        baron_owner: pits.baron_owner,
        herald_owner: pits.herald_owner,
        order_is_mine: b.my_team_is_order,
        my_players: sync_rows(&models.my_players, my_players),
        enemy_players: sync_rows(&models.enemy_players, enemy_players),
        towers: sync_rows(&models.towers, towers),
        my_objectives: sync_rows(&models.my_objectives, my_objectives),
        enemy_objectives: sync_rows(&models.enemy_objectives, enemy_objectives),
        feed: sync_rows(&models.feed, feed),
    }
}

/// One decimal, with the language's decimal separator (`11.5` / `11,5`).
fn format_decimal(value: f64, lang: Lang) -> String {
    let text = format!("{value:.1}");
    if lang.is_fr() { text.replace('.', ",") } else { text }
}

/// Thousands grouped the language's way (`14,950` / `14 950`).
fn format_thousands(value: u32, lang: Lang) -> String {
    let digits = value.to_string();
    let separator = if lang.is_fr() { ' ' } else { ',' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

pub fn build_end_view(
    eog: &EndOfGameSummary,
    icons: &mut impl IconLookup,
    lang: Lang,
) -> EndView {
    let victory = eog.result.eq_ignore_ascii_case("victory");
    let cid = eog.champion_id;
    let tile = |label: &str, value: String| StatLine {
        label: SharedString::from(label),
        value: SharedString::from(value),
    };
    let minutes = (eog.game_length_sec as f64 / 60.0).max(1.0);
    let kda_ratio = (eog.kills + eog.assists) as f64 / eog.deaths.max(1) as f64;
    let mut tiles = vec![tile("KDA", format_decimal(kda_ratio, lang))];
    if eog.creep_score > 0 {
        tiles.push(tile("CS/MIN", format_decimal(eog.creep_score as f64 / minutes, lang)));
    }
    tiles.push(tile(lang.pick("GOLD", "OR"), format_thousands(eog.total_gold, lang)));
    tiles.push(tile(lang.pick("DAMAGE", "DÉGÂTS"), format_thousands(eog.damage_to_champions, lang)));
    if eog.vision_score > 0 {
        tiles.push(tile("VISION", eog.vision_score.to_string()));
    }

    let mut rows = |players: &[EndOfGamePlayer]| -> Vec<PlayerLine> {
        let mut lines: Vec<PlayerLine> = players
            .iter()
            .map(|p| {
                let pcid = p.champion_id;
                PlayerLine {
                    lane: Position::parse(&p.position).map_or(-1, Position::desktop_index),
                    champion: SharedString::from(p.champion_name.as_str()),
                    initials: champ_initials(&p.champion_name),
                    icon: icons.resolve_icon("champion", pcid),
                    banner: icons.resolve_skin_art(art::BANNER_CATEGORY, p.skin_id, pcid),
                    level: p.level as i32,
                    kda: SharedString::from(format!("{} / {} / {}", p.kills, p.deaths, p.assists)),
                    cs: p.creep_score as i32,
                    is_self: p.is_self,
                    dead: false,
                    respawn: SharedString::default(),
                }
            })
            .collect();
        // Lane order, unknown positions last.
        lines.sort_by_key(|l| if l.lane < 0 { i32::MAX } else { l.lane });
        lines
    };
    let allies = eog
        .teams
        .iter()
        .find(|t| t.is_player_team)
        .map(|t| rows(&t.players))
        .unwrap_or_default();
    let enemies = eog
        .teams
        .iter()
        .find(|t| !t.is_player_team)
        .map(|t| rows(&t.players))
        .unwrap_or_default();

    EndView {
        title: SharedString::from(if victory {
            lang.pick("VICTORY", "VICTOIRE")
        } else {
            lang.pick("DEFEAT", "DÉFAITE")
        }),
        detail: SharedString::from(format!(
            "{} · {} / {} / {} · {}",
            eog.champion_name,
            eog.kills,
            eog.deaths,
            eog.assists,
            format_mm_ss(eog.game_length_sec as f64)
        )),
        victory,
        splash_icon: icons.resolve_skin_art("splash", eog.skin_id, cid),
        tiles: ModelRc::new(VecModel::from(tiles)),
        allies: ModelRc::new(VecModel::from(allies)),
        enemies: ModelRc::new(VecModel::from(enemies)),
    }
}
/// Everything besides the Match Projection that influences how views are rendered.
#[derive(Clone, Copy, PartialEq, Eq)]
struct RenderContext {
    lang: Lang,
    icons_enabled: bool,
    splash_enabled: bool,
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
    pub fn down_at(anchor: Instant, base_sec: f64, total_sec: i32) -> Self {
        Self { anchor, base_sec, counts_down: true, total_sec }
    }

    pub fn up_at(anchor: Instant, base_sec: f64) -> Self {
        Self { anchor, base_sec, counts_down: false, total_sec: 0 }
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

    /// The clock to show after a new snapshot. A snapshot is rebuilt for any event, but the
    /// clock reading inside it only changes when the Riot client sends a new one: re-anchoring
    /// an unchanged reading would rewind the clock to that older value on every unrelated
    /// event (and restart the drain-bar animation each time).
    fn follow(current: Option<Self>, next: Option<Self>) -> Option<Self> {
        match (current, next) {
            (Some(cur), Some(new))
                if cur.base_sec == new.base_sec
                    && cur.counts_down == new.counts_down
                    && cur.total_sec == new.total_sec =>
            {
                Some(cur)
            }
            _ => next,
        }
    }
}

/// Pure screen selection from [`SummarySnapshot`].
pub fn select_screen(snap: &SummarySnapshot) -> Screen {
    let lcu_live = snap
        .sources
        .iter()
        .find(|s| s.id == SourceId::LeagueClient)
        .is_some_and(|s| matches!(s.status, SourceStatus::Live | SourceStatus::Discovered));
    let phase_str = snap
        .phase
        .as_ref()
        .map(|p| p.phase.as_str())
        .unwrap_or("None");
    match phase_str {
        "Lobby" => Screen::Lobby,
        "Matchmaking" => Screen::Queue,
        "ReadyCheck" => Screen::ReadyCheck,
        "ChampSelect" => Screen::ChampSelect,
        "InProgress" => Screen::InGame,
        "EndOfGame" | "PreEndOfGame" => Screen::Results,
        _ if lcu_live || snap.summoner.is_some() => Screen::Idle,
        _ => Screen::Offline,
    }
}

/// Which clock, if any, the active view shows, anchored at `now`.
pub fn phase_clock_at(snap: &SummarySnapshot, view: Screen, now: Instant) -> Option<PhaseClock> {
    match view {
        Screen::Queue => snap
            .matchmaking
            .as_ref()
            .map(|m| PhaseClock::up_at(now, m.time_in_queue_sec)),
        Screen::ReadyCheck => snap
            .matchmaking
            .as_ref()
            .and_then(|m| m.ready_check_timer_sec)
            .map(|left| PhaseClock::down_at(now, f64::from(left), 10.max(left))),
        Screen::ChampSelect => snap.champ_select.as_ref().map(|cs| {
            let total = if cs.total_time_sec > 0 { cs.total_time_sec } else { cs.time_left_sec };
            PhaseClock::down_at(now, cs.time_left_sec as f64, total as i32)
        }),
        Screen::InGame => snap
            .live_game
            .as_ref()
            .map(|lg| PhaseClock::up_at(now, lg.game_time_sec)),
        _ => None,
    }
}

/// Which clock, if any, the active view shows.
fn phase_clock(snap: &SummarySnapshot, view: Screen) -> Option<PhaseClock> {
    phase_clock_at(snap, view, Instant::now())
}

/// Projects the wait-screen view (`Offline`, `Idle`, `Lobby`, `Queue`, `ReadyCheck`) and party rows.
pub fn build_wait_view(
    snap: &SummarySnapshot,
    view: Screen,
    icons: &mut impl IconLookup,
    lang: Lang,
) -> (WaitView, Vec<PartyRow>) {
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

    // The player's answer to the ready check: 0 pending, 1 accepted, 2 declined.
    let ready_state = match snap.matchmaking.as_ref().and_then(|m| m.player_response.as_deref()) {
        _ if view != Screen::ReadyCheck => 0,
        Some("Accepted") => 1,
        Some("Declined") => 2,
        _ => 0,
    };
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
            let (title, detail) = match ready_state {
                1 => (
                    lang.pick(
                        "Match Accepted — waiting on other players",
                        "Partie acceptée — en attente des autres joueurs",
                    ),
                    lang.pick(
                        "Champion select starts once all ten players accept.",
                        "La sélection des champions démarre quand les dix joueurs ont accepté.",
                    ),
                ),
                2 => (
                    lang.pick("Match Declined", "Partie refusée"),
                    lang.pick(
                        "You declined this match; the queue stops here.",
                        "Vous avez refusé cette partie ; la file s'arrête ici.",
                    ),
                ),
                _ => (
                    lang.pick("Match Found — accept now!", "Partie trouvée — acceptez maintenant !"),
                    lang.pick(
                        "Accept in the League client, or straight from your phone via Mobile Remote.",
                        "Acceptez dans le client League, ou directement depuis votre téléphone avec l'App Mobile.",
                    ),
                ),
            };
            (
                title.to_string(),
                detail.to_string(),
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
        .map(|s| icons.resolve_icon("profile", s.profile_icon_id))
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
        ready_state,
        summoner_name: summoner_header_name,
        summoner_initials,
        summoner_icon,
        rank_tier: canonical_rank_tier(snap.summoner.as_ref().and_then(|s| s.rank_tier.as_deref())),
        notes: ModelRc::new(VecModel::from(notes)),
    };

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

    (wait, party)
}

/// Keeps the companion windows in sync with the Match Projection, touching Slint only for the
/// sections whose data changed since the previous refresh.
#[derive(Default)]
pub struct Companion {
    loader: DesktopAssetLoader,
    shown: Option<Shown>,
    clock: Option<PhaseClock>,
    /// `(splash_enabled, icons_revision)` the client artwork was last resolved for.
    art_key: Option<(bool, u64)>,
    draft: DraftModels,
    live: LiveModels,
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
            splash_enabled: app_state.flags.splash_backdrops(),
            icons_revision: app_state.assets.image_revision(),
            mini_open,
        };
        let art_key = (context.splash_enabled, context.icons_revision);
        if self.art_key != Some(art_key) {
            apply_client_art(main_win, mini_win, app_state, &mut self.loader, context.splash_enabled);
            self.art_key = Some(art_key);
        }
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
            &self.draft,
            &self.live,
            &snapshot,
            self.shown.as_ref(),
            mini_open,
        );
        // Re-anchor only on a fresh reading of the same view's clock.
        let same_view = self.shown.as_ref().is_some_and(|shown| shown.view == view);
        let current = if same_view { self.clock } else { None };
        self.clock = PhaseClock::follow(current, phase_clock(&snapshot, view));
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

/// Resolves the prepared Summoner's Rift artwork (fetching what is missing) into both windows.
/// Runs only when the artwork switch or the icon cache changed, never per frame.
fn apply_client_art(
    main_win: &MainWindow,
    mini_win: &MiniWidget,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    enabled: bool,
) {
    let mut get = |piece: ArtPiece| {
        if enabled {
            loader.resolve(app_state, art::CATEGORY, piece.id())
        } else {
            Image::default()
        }
    };
    let map_south = get(ArtPiece::MapSouth);
    let map_north = get(ArtPiece::MapNorth);
    let ready_check = get(ArtPiece::ReadyCheck);
    let victory = get(ArtPiece::Victory);
    let defeat = get(ArtPiece::Defeat);
    let backdrop = get(ArtPiece::Backdrop);
    for target in [main_win.global::<ClientArt>(), mini_win.global::<ClientArt>()] {
        target.set_map_south(map_south.clone());
        target.set_map_north(map_north.clone());
        target.set_ready_check(ready_check.clone());
        target.set_victory(victory.clone());
        target.set_defeat(defeat.clone());
        target.set_backdrop(backdrop.clone());
    }
}

/// Pushes `snap` to the windows and returns the active screen. Sections equal to what
/// `previous` already displays are left untouched.
fn apply_summary(
    main_win: &MainWindow,
    mini_win: &MiniWidget,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    draft: &DraftModels,
    live: &LiveModels,
    snap: &SummarySnapshot,
    previous: Option<&Shown>,
    mini_open: bool,
) -> Screen {
    let lang = app_state.lang();
    let prev = previous.map(|shown| &*shown.snapshot);
    let mut icons = AppIconLookup {
        app: app_state,
        loader,
    };

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

    let view = select_screen(snap);
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
        let (wait, party) = build_wait_view(snap, view, &mut icons, lang);
        if mini_open {
            mini_win.set_wait(wait.clone());
        }
        main_win.set_wait(wait);
        main_win.set_party(ModelRc::new(VecModel::from(party)));
    }

    if let Some(ref cs) = snap.champ_select {
        if prev.is_none_or(|p| p.champ_select.as_ref() != Some(cs)) {
            let champ = build_champ_select_view(cs, &mut icons, draft, lang);
            // The mini widget only receives data while it is on screen.
            if mini_open {
                mini_win.set_champ(champ.clone());
            }
            main_win.set_champ(champ);
        }
    }

    if let Some(ref lg) = snap.live_game {
        if prev.is_none_or(|p| p.live_game.as_ref() != Some(lg) || p.board != snap.board) {
            let lv = build_live_view(lg, snap.board.as_ref(), &mut icons, live, lang);
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
            main_win.set_end(build_end_view(eog, &mut icons, lang));
        }
    }

    view
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_figures_follow_the_language() {
        assert_eq!(format_thousands(14950, Lang::En), "14,950");
        assert_eq!(format_thousands(14950, Lang::Fr), "14 950");
        assert_eq!(format_thousands(950, Lang::En), "950");
        assert_eq!(format_thousands(1234567, Lang::En), "1,234,567");
        assert_eq!(format_decimal(11.5, Lang::En), "11.5");
        assert_eq!(format_decimal(11.5, Lang::Fr), "11,5");
    }

    #[test]
    fn phase_clock_keeps_its_anchor_until_the_client_sends_a_new_reading() {
        let first = PhaseClock::down_at(Instant::now(), 30.0, 30);
        let later = Instant::now() + std::time::Duration::from_secs(5);
        // An unrelated event rebuilds the snapshot with the same 30 s reading: keep counting.
        let kept = PhaseClock::follow(Some(first), Some(PhaseClock::down_at(Instant::now(), 30.0, 30))).unwrap();
        assert_eq!(kept.seconds_now(later), first.seconds_now(later));
        // A new reading from the client re-anchors.
        let fresh = PhaseClock::follow(Some(first), Some(PhaseClock::down_at(Instant::now(), 29.0, 30))).unwrap();
        assert_eq!(fresh.base_sec, 29.0);
        assert!(PhaseClock::follow(Some(first), None).is_none());
        assert!(PhaseClock::follow(None, Some(first)).is_some());
    }

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

    #[test]
    fn extracts_rank_tiers_and_summarizes_live_pits() {
        assert_eq!(canonical_rank_tier(Some("EMERALD II (64 LP)")), "EMERALD");
        assert_eq!(canonical_rank_tier(Some("diamond")), "DIAMOND");
        assert_eq!(canonical_rank_tier(Some("Unranked")), "");

        let mut board = LiveBoard {
            my_team_is_order: false,
            ..LiveBoard::default()
        };
        board.objectives.push(hypatia_shared::ObjectiveTaken {
            kind: ObjectiveKind::Drake,
            label: "Drake".into(),
            by_order: false,
            time_sec: 400.0,
        });
        board.objectives.push(hypatia_shared::ObjectiveTaken {
            kind: ObjectiveKind::Drake,
            label: "Drake".into(),
            by_order: true,
            time_sec: 700.0,
        });
        board.objectives.push(hypatia_shared::ObjectiveTaken {
            kind: ObjectiveKind::Baron,
            label: "Baron".into(),
            by_order: false,
            time_sec: 1400.0,
        });
        board.objectives.push(hypatia_shared::ObjectiveTaken {
            kind: ObjectiveKind::Herald,
            label: "Herald".into(),
            by_order: true,
            time_sec: 900.0,
        });
        let pits = summarize_pits(&board);
        assert_eq!(pits.my_drakes, 1);
        assert_eq!(pits.enemy_drakes, 1);
        assert_eq!(pits.baron_owner, 1);
        assert_eq!(pits.herald_owner, 2);
    }

    #[test]
    fn projects_screens_clocks_and_companion_views_in_process() {
        use crate::icons::NoopIconLookup;
        use hypatia_shared::{
            project_match, BuiltinChampionCatalog, GamePhase, MatchInputs, SourceState,
        };
        use serde_json::json;
        use std::time::Duration;

        let mut icons = NoopIconLookup;

        let offline = SummarySnapshot::default();
        assert_eq!(select_screen(&offline), Screen::Offline);

        let idle = SummarySnapshot {
            sources: vec![SourceState {
                id: SourceId::LeagueClient,
                status: SourceStatus::Live,
                port: Some(12345),
                pid: Some(1),
                protocol: None,
                since: 0,
                events_total: 0,
                bytes_total: 0,
                credential: None,
            }],
            ..Default::default()
        };
        assert_eq!(select_screen(&idle), Screen::Idle);

        let cs = json!({
            "localPlayerCellId": 1,
            "queueType": "RANKED_SOLO_5x5",
            "timer": {"phase": "BAN_PICK", "adjustedTimeLeftInPhase": 28_500, "totalTimeInPhase": 30_000},
            "actions": [[{"id": 10, "actorCellId": 1, "type": "pick", "championId": 103, "completed": false, "isInProgress": true}]],
            "myTeam": [
                {"cellId": 0, "assignedPosition": "TOP", "championId": 266, "summonerId": 0, "puuid": ""},
                {"cellId": 1, "assignedPosition": "MIDDLE", "championId": 103, "spell1Id": 4, "spell2Id": 14, "summonerId": 1, "puuid": "p-self"}
            ],
            "bans": {"myTeamBans": [238], "theirTeamBans": [157]}
        });
        let lobby = json!({
            "queueId": 420,
            "members": [{"summonerName": "Self#EUW", "puuid": "p-self", "isLeader": true, "firstPositionPreference": "MIDDLE", "secondPositionPreference": "TOP"}]
        });
        let allgame = json!({
            "gameData": {"gameMode": "CLASSIC", "gameTime": 612.4},
            "activePlayer": {"riotId": "Self#EUW", "level": 11, "currentGold": 1450},
            "allPlayers": [
                {"riotId": "Self#EUW", "championName": "Ahri", "team": "ORDER", "position": "MIDDLE", "level": 11, "isDead": true, "respawnTimer": 8.2, "scores": {"kills": 5, "deaths": 2, "assists": 7, "creepScore": 140}}
            ]
        });
        let eog = json!({
            "gameResult": "Victory",
            "gameLength": 1805,
            "localPlayer": {"championId": 103, "stats": {"CHAMPIONS_KILLED": 8, "NUM_DEATHS": 2, "ASSISTS": 12, "GOLD_EARNED": 14000, "TOTAL_DAMAGE_DEALT_TO_CHAMPIONS": 28000}}
        });

        let snap = project_match(
            &MatchInputs {
                phase: Some(GamePhase {
                    phase: "ChampSelect".to_string(),
                    since: 0,
                }),
                lobby: Some(&lobby),
                champ_select: Some(&cs),
                live_allgame: Some(&allgame),
                end_of_game: Some(&eog),
                ..Default::default()
            },
            &BuiltinChampionCatalog,
            Lang::Fr,
        );
        assert_eq!(select_screen(&snap), Screen::ChampSelect);

        let anchor = Instant::now();
        let down_clock = phase_clock_at(&snap, Screen::ChampSelect, anchor).unwrap();
        assert!(down_clock.counts_down());
        assert_eq!(down_clock.total_sec, 30);
        assert_eq!(down_clock.seconds_now(anchor + Duration::from_millis(500)), 29);

        let up_clock = phase_clock_at(&snap, Screen::InGame, anchor).unwrap();
        assert!(!up_clock.counts_down());
        assert_eq!(up_clock.seconds_now(anchor + Duration::from_millis(900)), 613);

        let (wait, party) = build_wait_view(&snap, Screen::Lobby, &mut icons, Lang::Fr);
        assert!(wait.title.contains("Classée Solo/Duo"));
        assert_eq!(party.len(), 1);

        let cs_view = build_champ_select_view(
            snap.champ_select.as_ref().unwrap(),
            &mut icons,
            &DraftModels::default(),
            Lang::Fr,
        );
        assert!(cs_view.my_turn);
        assert!(cs_view.anonymized);
        assert!(cs_view.detail.contains("Classée Solo/Duo"));

        let live_view = build_live_view(
            snap.live_game.as_ref().unwrap(),
            snap.board.as_ref(),
            &mut icons,
            &LiveModels::default(),
            Lang::Fr,
        );
        assert!(live_view.dead);
        assert!(live_view.title.contains("9s"));

        let end_view = build_end_view(snap.end_of_game.as_ref().unwrap(), &mut icons, Lang::Fr);
        assert!(end_view.victory);
        assert_eq!(end_view.title.as_str(), "VICTOIRE");
    }
}
