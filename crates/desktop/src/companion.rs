//! Companion views (wait, champion select, live game, end of game) built from the Match
//! Projection and pushed to Slint only when the part they show actually changed.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use hypatia_core::art::{self, ClientArt as ArtPiece};
use hypatia_core::AppState;
use hypatia_shared::{
    comp_trait_label, spell_name_from_id_localized, stats_origin_label, ChampSelectHelper,
    ChampSelectSummary, EndOfGamePlayer, EndOfGameSummary, HelperComp, HelperEntry, HelperOnline,
    Lane, Lang, LiveBoard, LiveGameSummary, MapPlace, ObjectiveKind, Pit, Position, SourceId,
    SourceStatus, StatsOrigin, SummarySnapshot, SummonerSummary,
};
use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};

use crate::icons::{AppIconLookup, DesktopAssetLoader, IconLookup};
use crate::labels::{
    champ_initials, format_mm_ss, queue_type_label, rank_tier_label, sub_phase_label,
};
use crate::{
    ActiveLoadoutView, AllyRow, BanEntry, ChampSelectView, ClientArt, EndView, EnemyPick,
    EnemySighting, FeedLine, HelperChip, HelperIcon, HelperSuggestion, HelperTeam, HelperView,
    LiveView, MainWindow, MapAlly, ObjectiveChip, PartyRow, PitClock, PlayerLine, Screen, StatLine,
    TowerDot, WaitView,
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

fn build_ban_entries(names: &[String], ids: &[i64], icons: &mut impl IconLookup) -> Vec<BanEntry> {
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

/// Plan point of a fixed Rift place: towers as on the lane map, inhibitors between the
/// inhibitor tower and the nexus, pits in their river alcoves (as drawn by `LineMap`).
fn place_coordinates(place: MapPlace) -> (f32, f32) {
    match place {
        MapPlace::Turret {
            order_side,
            lane,
            tier,
        } => tower_coordinates(order_side, lane, tier),
        MapPlace::Inhibitor {
            order_side: true,
            lane: Lane::Top,
        } => (40.0, 308.0),
        MapPlace::Inhibitor {
            order_side: true,
            lane: Lane::Mid,
        } => (68.0, 320.0),
        MapPlace::Inhibitor {
            order_side: true,
            lane: _,
        } => (72.0, 348.0),
        MapPlace::Inhibitor {
            order_side: false,
            lane: Lane::Top,
        } => (326.0, 42.0),
        MapPlace::Inhibitor {
            order_side: false,
            lane: Lane::Mid,
        } => (330.0, 70.0),
        MapPlace::Inhibitor {
            order_side: false,
            lane: _,
        } => (358.0, 76.0),
        MapPlace::DragonPit => (266.0, 276.0),
        MapPlace::BaronPit => (132.0, 116.0),
    }
}

/// Where the k-th enemy sharing a place sits around it, so portraits never stack and the
/// tower or pit under them stays visible. Sides first: a pit's countdown and drake count sit
/// above and below it.
const SIGHTING_FAN: [(f32, f32); 5] = [
    (30.0, 0.0),
    (-30.0, 0.0),
    (30.0, -30.0),
    (-30.0, -30.0),
    (0.0, -34.0),
];

/// "Dragon pit", "Your mid outer tower", "Their bot inhibitor", from the player's side.
fn place_label(place: MapPlace, my_team_is_order: bool, lang: Lang) -> String {
    let lane = |lane: Lane| match lane {
        Lane::Top => "top",
        Lane::Jungle => "jungle",
        Lane::Mid => "mid",
        Lane::Bot => "bot",
    };
    match place {
        MapPlace::DragonPit => lang.pick("Dragon pit", "Antre du dragon").to_string(),
        MapPlace::BaronPit => lang.pick("Baron pit", "Antre du Baron").to_string(),
        MapPlace::Turret {
            order_side,
            lane: l,
            tier,
        } => {
            let ours = order_side == my_team_is_order;
            let (tier_en, tier_fr) = match tier {
                1 => ("outer", "extérieure"),
                2 => ("inner", "intérieure"),
                _ => ("inhibitor", "inhibitrice"),
            };
            if lang.is_fr() {
                format!(
                    "{} tourelle {} {}",
                    if ours { "Votre" } else { "Leur" },
                    tier_fr,
                    lane(l)
                )
            } else {
                format!(
                    "{} {} {} tower",
                    if ours { "Your" } else { "Their" },
                    lane(l),
                    tier_en
                )
            }
        }
        MapPlace::Inhibitor {
            order_side,
            lane: l,
        } => {
            let ours = order_side == my_team_is_order;
            if lang.is_fr() {
                format!(
                    "{} inhibiteur {}",
                    if ours { "Votre" } else { "Leur" },
                    lane(l)
                )
            } else {
                format!(
                    "{} {} inhibitor",
                    if ours { "Your" } else { "Their" },
                    lane(l)
                )
            }
        }
    }
}

/// "just now", "2 min ago" / "à l'instant", "il y a 2 min".
fn age_label(age_sec: f64, lang: Lang) -> String {
    let minutes = (age_sec.max(0.0) / 60.0).floor() as u32;
    match (minutes, lang.is_fr()) {
        (0, false) => "just now".to_string(),
        (0, true) => "à l'instant".to_string(),
        (m, false) => format!("{m} min ago"),
        (m, true) => format!("il y a {m} min"),
    }
}

/// A sighting older than this is drawn faded: the enemy has likely moved on.
const SIGHTING_STALE_SEC: f64 = 120.0;

/// Every enemy in roster order with their last known place, fanned around shared places.
fn build_sightings(
    lg: &LiveGameSummary,
    board: &LiveBoard,
    icons: &mut impl IconLookup,
    lang: Lang,
) -> Vec<EnemySighting> {
    let mut at_place: Vec<MapPlace> = Vec::new();
    lg.players
        .iter()
        .filter(|p| (p.team == "ORDER") != board.my_team_is_order)
        .map(|p| {
            let seen = board
                .sightings
                .iter()
                .find(|s| s.champion == p.champion_name);
            let (x, y) = match seen {
                Some(s) => {
                    let (x, y) = place_coordinates(s.place);
                    let k = at_place.iter().filter(|&&q| q == s.place).count();
                    at_place.push(s.place);
                    let (dx, dy) = SIGHTING_FAN[k % SIGHTING_FAN.len()];
                    (x + dx, y + dy)
                }
                None => (0.0, 0.0),
            };
            let age = seen.map_or(0.0, |s| lg.game_time_sec - s.time_sec);
            EnemySighting {
                x,
                y,
                champion: SharedString::from(p.champion_name.as_str()),
                initials: champ_initials(&p.champion_name),
                icon: icons.resolve_icon("champion", p.champion_id),
                seen: seen.is_some(),
                time: seen.map(|s| format_mm_ss(s.time_sec)).unwrap_or_default(),
                place: SharedString::from(match seen {
                    Some(s) => place_label(s.place, board.my_team_is_order, lang),
                    None => lang.pick("Not seen yet", "Pas encore repéré").to_string(),
                }),
                age: SharedString::from(if seen.is_some() {
                    age_label(age, lang)
                } else {
                    String::new()
                }),
                stale: age > SIGHTING_STALE_SEC,
                dead: p.is_dead,
            }
        })
        .collect()
}

fn objective_name(kind: ObjectiveKind, lang: Lang) -> &'static str {
    match kind {
        ObjectiveKind::Baron => "Baron Nashor",
        ObjectiveKind::Elder => lang.pick("Elder Dragon", "Dragon ancestral"),
        ObjectiveKind::Drake => lang.pick("Drake", "Dragon"),
        ObjectiveKind::Herald => lang.pick("Rift Herald", "Héraut de la Faille"),
        ObjectiveKind::Grubs => lang.pick("Void Grubs", "Larves du Néant"),
    }
}

fn build_pit_clocks(board: &LiveBoard, lang: Lang) -> Vec<PitClock> {
    board
        .pits
        .iter()
        .map(|p| {
            let (x, y) = place_coordinates(match p.pit {
                Pit::Dragon => MapPlace::DragonPit,
                Pit::Baron => MapPlace::BaronPit,
            });
            PitClock {
                x,
                y,
                kind: objective_kind_index(p.kind),
                label: SharedString::from(objective_name(p.kind, lang)),
                spawn_at: p.spawn_sec.ceil() as i32,
            }
        })
        .collect()
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
    sightings: Rc<VecModel<EnemySighting>>,
    pits: Rc<VecModel<PitClock>>,
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

fn clear_model<T: Clone + 'static>(model: &Rc<VecModel<T>>) {
    while model.row_count() > 0 {
        model.remove(model.row_count() - 1);
    }
}

impl DraftModels {
    fn clear(&self) {
        clear_model(&self.allies);
        clear_model(&self.map_allies);
        clear_model(&self.enemies);
        clear_model(&self.my_bans);
        clear_model(&self.their_bans);
    }
}

impl LiveModels {
    fn clear(&self) {
        clear_model(&self.my_players);
        clear_model(&self.enemy_players);
        clear_model(&self.towers);
        clear_model(&self.my_objectives);
        clear_model(&self.enemy_objectives);
        clear_model(&self.feed);
        clear_model(&self.sightings);
        clear_model(&self.pits);
    }
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
                    lang.pick(
                        "Your turn — ban a champion",
                        "Votre tour — bannissez un champion",
                    )
                    .to_string()
                } else {
                    lang.pick(
                        "Your turn — pick your champion",
                        "Votre tour — choisissez votre champion",
                    )
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
            icons.resolve_skin_art(
                art::BANNER_CATEGORY,
                ally.selected_skin_id,
                ally.champion_id,
            )
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
    let their_ban_list = build_ban_entries(&cs.their_team_bans, &cs.their_team_ban_ids, icons);
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
        loadout: build_active_loadout_view(cs, icons, lang),
        helper: build_helper_view(&cs.helper, icons, lang),
    }
}

fn build_active_loadout_view(
    cs: &ChampSelectSummary,
    icons: &mut impl IconLookup,
    lang: Lang,
) -> ActiveLoadoutView {
    let self_ally = cs.my_team.iter().find(|m| m.is_self);
    let spell1_id = self_ally.map(|m| m.spell1_id).unwrap_or(0);
    let spell2_id = self_ally.map(|m| m.spell2_id).unwrap_or(0);
    let has_spells = spell1_id > 0 && spell2_id > 0;
    let spell1_str = if spell1_id > 0 {
        spell_name_from_id_localized(spell1_id, lang)
    } else {
        ""
    };
    let spell2_str = if spell2_id > 0 {
        spell_name_from_id_localized(spell2_id, lang)
    } else {
        ""
    };

    let is_jungle = self_ally
        .and_then(|m| Position::parse(&m.assigned_position))
        .is_some_and(|p| p == Position::Jungle)
        && !cs.bench_enabled;
    let smite_warning = is_jungle && has_spells && spell1_id != 11 && spell2_id != 11;

    let active_rune = cs
        .rune_pages
        .iter()
        .find(|p| p.current)
        .or_else(|| cs.recommended_rune_pages.iter().find(|p| p.current));
    let (has_runes, rune_page, keystone, keystone_initials, keystone_icon, styles_line) =
        if let Some(pg) = active_rune {
            let kid = pg.selected_perk_ids.first().copied().unwrap_or(0);
            let k_icon = if kid > 0 {
                icons.resolve_icon("perk", kid)
            } else {
                Image::default()
            };
            let styles = format!("{} + {}", pg.primary_style_name, pg.sub_style_name);
            (
                true,
                SharedString::from(pg.name.as_str()),
                SharedString::from(pg.keystone_name.as_str()),
                champ_initials(&pg.keystone_name),
                k_icon,
                SharedString::from(styles),
            )
        } else {
            (
                false,
                SharedString::default(),
                SharedString::default(),
                SharedString::default(),
                Image::default(),
                SharedString::default(),
            )
        };

    let has_champ = self_ally.is_some_and(|m| m.has_champion());
    let skin_name = if has_champ {
        SharedString::from(cs.selected_skin_name.as_str())
    } else {
        SharedString::default()
    };

    ActiveLoadoutView {
        shown: has_runes || has_spells || !skin_name.is_empty(),
        has_runes,
        rune_page,
        keystone,
        keystone_initials,
        keystone_icon,
        styles_line,
        has_spells,
        spell1_name: SharedString::from(spell1_str),
        spell1_initials: champ_initials(spell1_str),
        spell1_icon: if spell1_id > 0 {
            icons.resolve_icon("spell", spell1_id)
        } else {
            Image::default()
        },
        spell2_name: SharedString::from(spell2_str),
        spell2_initials: champ_initials(spell2_str),
        spell2_icon: if spell2_id > 0 {
            icons.resolve_icon("spell", spell2_id)
        } else {
            Image::default()
        },
        smite_warning,
        skin_name,
    }
}

/// `51.2%` / `51,2 %`.
fn format_percent(rate: f32, lang: Lang) -> String {
    let value = format_decimal(f64::from(rate) * 100.0, lang);
    if lang.is_fr() {
        format!("{value} %")
    } else {
        format!("{value}%")
    }
}

fn helper_icon(
    icons: &mut impl IconLookup,
    category: &'static str,
    entry: &HelperEntry,
) -> HelperIcon {
    HelperIcon {
        icon: icons.resolve_icon(category, entry.id),
        name: SharedString::from(entry.name.as_str()),
        initials: champ_initials(&entry.name),
    }
}

/// Trait chips per row, and rows, one team has room for.
const HELPER_CHIPS_PER_ROW: usize = 2;
const HELPER_CHIP_ROWS: usize = 2;

fn helper_team(comp: &HelperComp) -> HelperTeam {
    // Gaps first: what a team lacks matters more than what it has.
    let mut traits: Vec<_> = comp.traits.iter().collect();
    traits.sort_by_key(|t| !t.gap);
    traits.truncate(HELPER_CHIPS_PER_ROW * HELPER_CHIP_ROWS);
    let chips = |row: &[&hypatia_shared::HelperTrait]| {
        ModelRc::new(VecModel::from(
            row.iter()
                .map(|t| HelperChip {
                    label: SharedString::from(t.label.as_str()),
                    gap: t.gap,
                })
                .collect::<Vec<_>>(),
        ))
    };
    let split = traits.len().min(HELPER_CHIPS_PER_ROW);
    HelperTeam {
        physical: comp.physical,
        magic: comp.magic,
        true_damage: comp.true_damage,
        known: comp.champions > 0,
        traits: chips(&traits[..split]),
        traits_more: chips(&traits[split..]),
    }
}

/// The champ-select helper panel (`helper.slint`) from the projected helper block.
pub fn build_helper_view(
    h: &ChampSelectHelper,
    icons: &mut impl IconLookup,
    lang: Lang,
) -> HelperView {
    let origin = |o: StatsOrigin| SharedString::from(stats_origin_label(o, lang));
    let rate_tag = |rate: Option<f32>| {
        rate.map(|r| {
            let p = format_percent(r, lang);
            if lang.is_fr() {
                format!("{p} vict.")
            } else {
                format!("{p} WR")
            }
        })
        .unwrap_or_default()
    };
    let mut view = HelperView {
        shown: true,
        has_champion: h.champion_id > 0,
        champion: SharedString::from(h.champion_name.as_str()),
        position: SharedString::from(
            h.position
                .map(|p| p.display_label(lang))
                .unwrap_or_default(),
        ),
        lane: h.position.map_or(-1, Position::desktop_index),
        win_rate: SharedString::from(
            h.win_rate
                .map(|r| {
                    let p = format_percent(r, lang);
                    if lang.is_fr() {
                        format!("{p} de victoires")
                    } else {
                        format!("{p} win rate")
                    }
                })
                .unwrap_or_default(),
        ),
        empty_note: SharedString::from(if h.champion_id > 0 {
            lang.pick(
                "No build for this champion yet.",
                "Pas encore de build pour ce champion.",
            )
        } else {
            lang.pick(
                "Hover a champion to see its runes, spells and items.",
                "Survolez un champion pour voir ses runes, sorts et objets.",
            )
        }),
        mine: helper_team(&h.my_comp),
        theirs: helper_team(&h.their_comp),
        tips: ModelRc::new(VecModel::from(
            h.tips
                .iter()
                .map(|t| SharedString::from(t.as_str()))
                .collect::<Vec<_>>(),
        )),
        suggestions: ModelRc::new(VecModel::from(
            h.suggested_picks
                .iter()
                .map(|s| {
                    let mut parts = Vec::new();
                    if s.personal_games > 0 {
                        if let Some(wr) = s.personal_win_rate {
                            parts.push(format!(
                                "{}g {}",
                                s.personal_games,
                                format_percent(wr, lang)
                            ));
                        } else {
                            parts.push(format!("{}g", s.personal_games));
                        }
                    } else if let Some(wr) = s.meta_win_rate {
                        parts.push(format_percent(wr, lang));
                    }
                    if let Some(tr) = s.fills_trait {
                        parts.push(comp_trait_label(tr, lang).to_string());
                    }
                    HelperSuggestion {
                        champion: helper_icon(
                            icons,
                            "champion",
                            &HelperEntry {
                                id: s.champion_id,
                                name: s.champion_name.clone(),
                            },
                        ),
                        tier: SharedString::from(s.tier.map(|t| t.label()).unwrap_or("")),
                        detail: SharedString::from(parts.join(" · ")),
                    }
                })
                .collect::<Vec<_>>(),
        )),
        enemy_choices: ModelRc::new(VecModel::from(
            h.enemy_choices
                .iter()
                .map(|e| helper_icon(icons, "champion", e))
                .collect::<Vec<_>>(),
        )),
        ..HelperView::default()
    };
    if let Some(r) = &h.runes {
        view.has_runes = true;
        view.keystone = helper_icon(icons, "perk", &r.keystone);
        view.primary_style = helper_icon(icons, "style", &r.primary_style);
        view.sub_style = helper_icon(icons, "style", &r.sub_style);
        view.runes_origin = origin(r.origin);
        view.runes_online = r.origin == StatsOrigin::Online;
        view.runes_rate = SharedString::from(rate_tag(r.win_rate));
    }
    if let Some(sp) = &h.spells {
        view.has_spells = true;
        view.spell1 = helper_icon(icons, "spell", &sp.first);
        view.spell2 = helper_icon(icons, "spell", &sp.second);
        view.spells_origin = origin(sp.origin);
        view.spells_online = sp.origin == StatsOrigin::Online;
    }
    if let Some(it) = &h.items {
        view.items = ModelRc::new(VecModel::from(
            it.items
                .iter()
                .map(|e| helper_icon(icons, "item", e))
                .collect::<Vec<_>>(),
        ));
        view.items_origin = origin(it.origin);
        view.items_online = it.origin == StatsOrigin::Online;
        view.items_rate = SharedString::from(rate_tag(it.win_rate));
    }
    if let Some(lane) = &h.lane {
        view.has_lane = true;
        view.lane = lane.position.desktop_index();
        view.lane_enemy = helper_icon(
            icons,
            "champion",
            &HelperEntry {
                id: lane.enemy_champion_id,
                name: lane.enemy_champion_name.clone(),
            },
        );
        view.lane_line = SharedString::from(format!(
            "{} {} {}",
            lane.position.display_label(lang),
            lang.pick("vs", "contre"),
            lane.enemy_champion_name
        ));
        view.lane_rate = SharedString::from(
            lane.win_rate
                .map(|r| {
                    let p = format_percent(r, lang);
                    let games = format_thousands(lane.games, lang);
                    if lang.is_fr() {
                        format!("{p} de victoires · {games} parties")
                    } else {
                        format!("{p} win rate · {games} games")
                    }
                })
                .unwrap_or_default(),
        );
        if let Some(o) = lane.origin {
            view.lane_origin = origin(o);
            view.lane_online = o == StatsOrigin::Online;
        }
    }
    let demo = [
        h.runes.as_ref().map(|r| r.origin),
        h.items.as_ref().map(|i| i.origin),
    ]
    .contains(&Some(StatsOrigin::Demo));
    view.hint = SharedString::from(match &h.online {
        _ if demo => lang
            .pick("Demo figures, not real statistics.", "Chiffres de démo, pas de vraies statistiques.")
            .to_string(),
        HelperOnline::Off => lang
            .pick(
                "Local data only. Turn on Online stats in Settings for global builds and matchup win rates.",
                "Données locales uniquement. Activez les statistiques en ligne dans les Paramètres pour les builds mondiaux et les taux de victoire par affrontement.",
            )
            .to_string(),
        HelperOnline::Loading if h.champion_id > 0 => lang
            .pick("Loading online stats…", "Chargement des statistiques en ligne…")
            .to_string(),
        HelperOnline::NoData if h.champion_id > 0 => lang
            .pick("No online stats for this champion yet.", "Pas encore de statistiques en ligne pour ce champion.")
            .to_string(),
        HelperOnline::OtherMode if h.champion_id > 0 => lang
            .pick(
                "Online stats cover ranked Summoner's Rift games, not this mode.",
                "Les statistiques en ligne couvrent les parties classées de la Faille, pas ce mode.",
            )
            .to_string(),
        HelperOnline::Failed(message) => message.clone(),
        _ => String::new(),
    });
    // The mini widget has no room for a chip per line: one line names every origin shown.
    let mut sources: Vec<&str> = Vec::new();
    for o in [
        h.runes.as_ref().map(|r| r.origin),
        h.spells.as_ref().map(|s| s.origin),
        h.items.as_ref().map(|i| i.origin),
        h.lane.as_ref().and_then(|l| l.origin),
    ]
    .into_iter()
    .flatten()
    {
        let label = stats_origin_label(o, lang);
        if !sources.contains(&label) {
            sources.push(label);
        }
    }
    view.sources = SharedString::from(sources.join(" · "));
    if let Some(a) = h.attribution.as_ref().filter(|_| h.shows_online()) {
        view.attribution = SharedString::from(format!(
            "{} {}",
            lang.pick("Data:", "Données :"),
            a.provider
        ));
        view.attribution_url = SharedString::from(a.url.as_str());
    }
    view
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
        let mut sub_parts = Vec::new();
        if p.cs_per_min > 0.0 {
            sub_parts.push(format!(
                "{} CS/m",
                format_decimal(f64::from(p.cs_per_min), lang)
            ));
        }
        if let Some(diff) = p.cs_diff_vs_lane {
            if diff > 0 {
                sub_parts.push(format!("+{diff}"));
            } else if diff < 0 {
                sub_parts.push(format!("−{}", diff.abs()));
            } else {
                sub_parts.push("±0".to_string());
            }
        }
        let row = PlayerLine {
            lane,
            champion: SharedString::from(p.champion_name.as_str()),
            initials: champ_initials(&p.champion_name),
            icon: icons.resolve_icon("champion", cid),
            banner: icons.resolve_skin_art(art::BANNER_CATEGORY, skin, cid),
            level: p.level as i32,
            kda: SharedString::from(kda_str),
            cs: p.creep_score as i32,
            sub_detail: SharedString::from(sub_parts.join(" · ")),
            badge: SharedString::default(),
            score: SharedString::default(),
            bar_ratio: 0.0,
            bar_label: SharedString::default(),
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
    for buff in &b.active_buffs {
        let mine = buff.by_order == b.my_team_is_order;
        let name = match buff.kind {
            ObjectiveKind::Baron => "Baron",
            ObjectiveKind::Elder => lang.pick("Elder", "Ancestral"),
            _ => continue,
        };
        let rem = format_mm_ss(buff.remaining_sec);
        let chip = ObjectiveChip {
            kind: objective_kind_index(buff.kind),
            weight: buff.kind.weight() as i32 + 1,
            label: SharedString::from(format!("{name} {rem} · {}/5", buff.holders_alive)),
            mine,
        };
        if mine {
            my_objectives.push(chip);
        } else {
            enemy_objectives.push(chip);
        }
    }
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
        let mut extra = String::new();
        if lg.self_cs_per_min > 0.0 {
            extra.push_str(&format!(
                " ({} CS/m",
                format_decimal(f64::from(lg.self_cs_per_min), lang)
            ));
            if let Some(diff) = lg.lane_cs_diff {
                if diff > 0 {
                    extra.push_str(&format!(" · +{diff} CS"));
                } else if diff < 0 {
                    extra.push_str(&format!(" · −{} CS", diff.abs()));
                }
            }
            if let Some(avg) = lg.personal_avg_cs_per_min.filter(|&a| a > 0.0) {
                extra.push_str(&format!(
                    " · {} {}",
                    lang.pick("avg", "moy."),
                    format_decimal(f64::from(avg), lang)
                ));
            }
            extra.push(')');
        }
        (
            if lang.is_fr() {
                format!("Vous jouez {} · Niveau {}", lg.active_champion, lg.level)
            } else {
                format!("Playing {} · Level {}", lg.active_champion, lg.level)
            },
            if lang.is_fr() {
                format!(
                    "Vous {} · {} CS{extra} · {} or",
                    self_kda, lg.creep_score, lg.current_gold
                )
            } else {
                format!(
                    "You {} · {} CS{extra} · {} gold",
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
    let sightings = build_sightings(lg, b, icons, lang);
    let pit_clocks = build_pit_clocks(b, lang);

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
        sightings: sync_rows(&models.sightings, sightings),
        pits: sync_rows(&models.pits, pit_clocks),
        game_seconds: lg.game_time_sec.floor() as i32,
    }
}

/// One decimal, with the language's decimal separator (`11.5` / `11,5`).
fn format_decimal(value: f64, lang: Lang) -> String {
    let text = format!("{value:.1}");
    if lang.is_fr() {
        text.replace('.', ",")
    } else {
        text
    }
}

/// Thousands grouped the language's way (`14,950` / `14 950`).
fn format_thousands(value: u32, lang: Lang) -> String {
    let digits = value.to_string();
    let separator = if lang.is_fr() { ' ' } else { ',' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

/// Converts an optional Layer 1 [`hypatia_shared::GameReview`] and Layer 2 [`hypatia_core::coach_llm::DebriefState`]
/// into the Slint [`CoachReviewView`].
pub fn build_review_view(
    review: Option<&hypatia_shared::GameReview>,
    llm_enabled: bool,
    debrief: &hypatia_core::coach_llm::DebriefState,
    lang: Lang,
) -> crate::CoachReviewView {
    let Some(rev) = review.filter(|r| r.has_findings()) else {
        return crate::CoachReviewView {
            has: false,
            coverage: SharedString::default(),
            archetype: SharedString::default(),
            habit_pattern: SharedString::default(),
            good_choices: ModelRc::default(),
            improvements: ModelRc::default(),
            turning_points: ModelRc::default(),
            llm_enabled: false,
            debrief_state: 0,
            debrief_focus: 0,
            debrief_text: SharedString::default(),
            debrief_meta: SharedString::default(),
            quick_questions: ModelRc::default(),
            turns: ModelRc::default(),
            followup_loading: false,
            followup_error: SharedString::default(),
        };
    };
    let to_rows = |items: &[hypatia_shared::ReviewFinding]| -> ModelRc<crate::ReviewItemRow> {
        let rows: Vec<crate::ReviewItemRow> = items
            .iter()
            .map(|f| crate::ReviewItemRow {
                pillar: SharedString::from(f.pillar.label(lang)),
                timestamp: f
                    .timestamp_sec
                    .map(|s| SharedString::from(hypatia_shared::format_mm_ss(s)))
                    .unwrap_or_default(),
                title: SharedString::from(f.title.as_str()),
                detail: SharedString::from(f.detail.as_str()),
            })
            .collect();
        ModelRc::new(VecModel::from(rows))
    };
    let turning: Vec<crate::TurningPointRow> = rev
        .turning_points
        .iter()
        .map(|tp| crate::TurningPointRow {
            timestamp: SharedString::from(hypatia_shared::format_mm_ss(tp.time_sec)),
            pillar: SharedString::from(tp.pillar.label(lang)),
            label: SharedString::from(tp.label.as_str()),
            good: tp.polarity == hypatia_shared::FindingPolarity::GoodChoice,
        })
        .collect();
    let (
        debrief_state,
        debrief_focus,
        debrief_text,
        debrief_meta,
        turns,
        followup_loading,
        followup_error,
    ) = match debrief {
        hypatia_core::coach_llm::DebriefState::Idle => (
            0,
            0,
            SharedString::default(),
            SharedString::default(),
            ModelRc::default(),
            false,
            SharedString::default(),
        ),
        hypatia_core::coach_llm::DebriefState::Loading(focus) => (
            1,
            focus.as_index(),
            SharedString::default(),
            SharedString::default(),
            ModelRc::default(),
            false,
            SharedString::default(),
        ),
        hypatia_core::coach_llm::DebriefState::Ready {
            text,
            model,
            focus,
            turns,
            followup_loading,
            followup_error,
            ..
        } => {
            let turn_rows: Vec<crate::CoachTurnRow> = turns
                .iter()
                .map(|t| crate::CoachTurnRow {
                    question: SharedString::from(t.question.as_str()),
                    answer: SharedString::from(t.answer.as_str()),
                })
                .collect();
            (
                2,
                focus.as_index(),
                SharedString::from(text.as_str()),
                SharedString::from(model.as_str()),
                ModelRc::new(VecModel::from(turn_rows)),
                *followup_loading,
                followup_error
                    .as_deref()
                    .map(SharedString::from)
                    .unwrap_or_default(),
            )
        }
        hypatia_core::coach_llm::DebriefState::Failed { message, focus } => (
            3,
            focus.as_index(),
            SharedString::from(message.as_str()),
            SharedString::default(),
            ModelRc::default(),
            false,
            SharedString::default(),
        ),
    };
    let quick_questions: Vec<SharedString> = hypatia_shared::suggested_coach_questions(rev, lang)
        .into_iter()
        .map(SharedString::from)
        .collect();
    let habit_pattern = rev
        .context
        .habit_profile
        .as_ref()
        .and_then(|h| h.primary_pattern(lang))
        .map(SharedString::from)
        .unwrap_or_default();
    crate::CoachReviewView {
        has: true,
        coverage: SharedString::from(rev.coverage.label(lang)),
        archetype: SharedString::from(rev.context.archetype.label(lang)),
        habit_pattern,
        good_choices: to_rows(&rev.good_choices),
        improvements: to_rows(&rev.improvements),
        turning_points: ModelRc::new(VecModel::from(turning)),
        llm_enabled,
        debrief_state,
        debrief_focus,
        debrief_text,
        debrief_meta,
        quick_questions: ModelRc::new(VecModel::from(quick_questions)),
        turns,
        followup_loading,
        followup_error,
    }
}

pub fn build_end_view(
    eog: &EndOfGameSummary,
    review: Option<&hypatia_shared::GameReview>,
    llm_enabled: bool,
    debrief: &hypatia_core::coach_llm::DebriefState,
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
        tiles.push(tile(
            "CS/MIN",
            format_decimal(eog.creep_score as f64 / minutes, lang),
        ));
    }
    tiles.push(tile(
        lang.pick("GOLD", "OR"),
        format_thousands(eog.total_gold, lang),
    ));
    tiles.push(tile(
        lang.pick("DAMAGE", "DÉGÂTS"),
        format_thousands(eog.damage_to_champions, lang),
    ));
    if eog.vision_score > 0 {
        tiles.push(tile("VISION", eog.vision_score.to_string()));
    }

    let max_dmg = eog
        .teams
        .iter()
        .flat_map(|t| t.players.iter())
        .map(|p| p.damage_to_champions)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut rows = |players: &[EndOfGamePlayer]| -> Vec<PlayerLine> {
        let mut lines: Vec<PlayerLine> = players
            .iter()
            .map(|p| {
                let pcid = p.champion_id;
                let mut sub = Vec::new();
                if p.kill_participation > 0.0 {
                    sub.push(format!(
                        "{}% KP",
                        (p.kill_participation * 100.0).round() as u32
                    ));
                }
                if p.gold_earned > 0 {
                    sub.push(format!(
                        "{} {}",
                        format_thousands(p.gold_earned, lang),
                        lang.pick("g", "or")
                    ));
                }
                if p.damage_to_champions > 0 {
                    sub.push(format!(
                        "{} {}",
                        format_thousands(p.damage_to_champions, lang),
                        lang.pick("dmg", "dég.")
                    ));
                }
                let badge = if p.is_mvp {
                    "MVP"
                } else if p.is_ace {
                    "ACE"
                } else {
                    ""
                };
                let score = if p.performance_score > 0 {
                    format!("{} · #{}", p.performance_score, p.placement)
                } else {
                    String::new()
                };
                let bar_ratio = p.damage_to_champions as f32 / max_dmg as f32;
                let bar_label = format!(
                    "{} {} ({}%) · {} {}",
                    format_thousands(p.damage_to_champions, lang),
                    lang.pick("dmg", "dég."),
                    (p.damage_share * 100.0).round() as u32,
                    format_thousands(p.gold_earned, lang),
                    lang.pick("gold", "or")
                );
                PlayerLine {
                    lane: Position::parse(&p.position).map_or(-1, Position::desktop_index),
                    champion: SharedString::from(p.champion_name.as_str()),
                    initials: champ_initials(&p.champion_name),
                    icon: icons.resolve_icon("champion", pcid),
                    banner: icons.resolve_skin_art(art::BANNER_CATEGORY, p.skin_id, pcid),
                    level: p.level as i32,
                    kda: SharedString::from(format!("{} / {} / {}", p.kills, p.deaths, p.assists)),
                    cs: p.creep_score as i32,
                    sub_detail: SharedString::from(sub.join(" · ")),
                    badge: SharedString::from(badge),
                    score: SharedString::from(score),
                    bar_ratio,
                    bar_label: SharedString::from(bar_label),
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
    let team_totals = |t: &hypatia_shared::EndOfGameTeam| -> SharedString {
        if t.total_kills == 0 && t.total_gold == 0 {
            return SharedString::default();
        }
        SharedString::from(format!(
            "{} {} · {} {} · {} {}",
            t.total_kills,
            lang.pick("kills", "élim."),
            format_thousands(t.total_gold, lang),
            lang.pick("g", "or"),
            format_thousands(t.total_damage, lang),
            lang.pick("dmg", "dég.")
        ))
    };
    let ally_team = eog.teams.iter().find(|t| t.is_player_team);
    let enemy_team = eog.teams.iter().find(|t| !t.is_player_team);
    let allies = ally_team.map(|t| rows(&t.players)).unwrap_or_default();
    let enemies = enemy_team.map(|t| rows(&t.players)).unwrap_or_default();

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
        ally_totals: ally_team.map(team_totals).unwrap_or_default(),
        enemy_totals: enemy_team.map(team_totals).unwrap_or_default(),
        allies: ModelRc::new(VecModel::from(allies)),
        enemies: ModelRc::new(VecModel::from(enemies)),
        review: build_review_view(review, llm_enabled, debrief, lang),
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
    coach_llm_enabled: bool,
    coach_llm_revision: u64,
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
        Self {
            anchor,
            base_sec,
            counts_down: true,
            total_sec,
        }
    }

    pub fn up_at(anchor: Instant, base_sec: f64) -> Self {
        Self {
            anchor,
            base_sec,
            counts_down: false,
            total_sec: 0,
        }
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
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let left = cs.time_left_sec_at(now_ms);
            let total = if cs.total_time_sec > 0 {
                cs.total_time_sec
            } else {
                left
            };
            PhaseClock::down_at(now, left as f64, total as i32)
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
    let ready_state = match snap
        .matchmaking
        .as_ref()
        .and_then(|m| m.player_response.as_deref())
    {
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

/// Keeps the companion window in sync with the Match Projection, touching Slint only for the
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

fn reset_client_art(main_win: &MainWindow) {
    let target = main_win.global::<ClientArt>();
    target.set_map_south(Image::default());
    target.set_map_north(Image::default());
    target.set_ready_check(Image::default());
    target.set_victory(Image::default());
    target.set_defeat(Image::default());
    target.set_backdrop(Image::default());
}

fn reset_window_views(main_win: &MainWindow) {
    main_win.set_wait(WaitView::default());
    main_win.set_champ(ChampSelectView::default());
    main_win.set_live(LiveView::default());
    main_win.set_end(EndView::default());
}

impl Companion {
    pub fn refresh(&mut self, main_win: &MainWindow, app_state: &AppState) {
        let snapshot = app_state.get_summary();
        let context = RenderContext {
            lang: app_state.lang(),
            icons_enabled: app_state.flags.game_icons(),
            splash_enabled: app_state.flags.splash_backdrops(),
            icons_revision: app_state.assets.image_revision(),
            coach_llm_enabled: app_state.coach_llm.is_available(),
            coach_llm_revision: app_state.coach_llm.revision(),
        };

        let prev_context = self.shown.as_ref().map(|shown| shown.context);
        let prev_view = self.shown.as_ref().map(|shown| shown.view);
        let icons_turned_off =
            prev_context.is_some_and(|prev| prev.icons_enabled && !context.icons_enabled);
        let splashes_turned_off =
            prev_context.is_some_and(|prev| prev.splash_enabled && !context.splash_enabled);

        if context.icons_enabled {
            self.loader
                .set_max_entries(app_state.flags.asset_cache_budget().max_entries());
        } else {
            self.loader.set_max_entries(0);
        }
        if !context.splash_enabled {
            self.loader
                .evict_categories(&["splash", art::CATEGORY, art::BANNER_CATEGORY]);
        }
        if snapshot.champ_select.is_none() || icons_turned_off {
            self.draft.clear();
        }
        if snapshot.live_game.is_none() || icons_turned_off {
            self.live.clear();
        }
        if icons_turned_off || splashes_turned_off {
            reset_window_views(main_win);
        }

        let art_key = (context.splash_enabled, context.icons_revision);
        if self.art_key != Some(art_key) {
            apply_client_art(
                main_win,
                app_state,
                &mut self.loader,
                context.splash_enabled,
            );
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
            app_state,
            &mut self.loader,
            &self.draft,
            &self.live,
            &snapshot,
            self.shown.as_ref(),
        );
        // Re-anchor only on a fresh reading of the same view's clock.
        let same_view = prev_view == Some(view);
        let current = if same_view { self.clock } else { None };
        self.clock = PhaseClock::follow(current, phase_clock(&snapshot, view));
        self.shown = Some(Shown {
            snapshot,
            view,
            context,
        });
    }

    /// Returns `(decoded_image_count, approx_rgba_bytes)` currently held in desktop memory.
    pub fn loader_stats(&self) -> (usize, u64) {
        self.loader.stats()
    }

    /// Forces the next `refresh` to repopulate client art and active views.
    pub fn invalidate_visuals(&mut self) {
        self.art_key = None;
        self.shown = None;
    }

    /// Releases every decoded Slint texture from memory and clears cached window views.
    pub fn clear_icon_cache(&mut self, main_win: &MainWindow) {
        self.loader.clear();
        reset_client_art(main_win);
        reset_window_views(main_win);
        self.draft.clear();
        self.live.clear();
        self.invalidate_visuals();
    }

    /// The clock the active view shows, if it has one.
    pub fn clock(&self) -> Option<PhaseClock> {
        self.clock
    }
}

/// Resolves the prepared Summoner's Rift artwork (fetching what is missing) into the window.
/// Runs only when the artwork switch or the icon cache changed, never per frame.
fn apply_client_art(
    main_win: &MainWindow,
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
    let target = main_win.global::<ClientArt>();
    target.set_map_south(get(ArtPiece::MapSouth));
    target.set_map_north(get(ArtPiece::MapNorth));
    target.set_ready_check(get(ArtPiece::ReadyCheck));
    target.set_victory(get(ArtPiece::Victory));
    target.set_defeat(get(ArtPiece::Defeat));
    target.set_backdrop(get(ArtPiece::Backdrop));
}

/// Pushes `snap` to the window and returns the active screen. Sections equal to what
/// `previous` already displays are left untouched.
fn apply_summary(
    main_win: &MainWindow,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    draft: &DraftModels,
    live: &LiveModels,
    snap: &SummarySnapshot,
    previous: Option<&Shown>,
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
            p.summoner != snap.summoner
                || p.lobby != snap.lobby
                || p.matchmaking != snap.matchmaking
        });
    let waiting = !matches!(view, Screen::ChampSelect | Screen::InGame | Screen::Results);
    if waiting && wait_changed {
        let (wait, party) = build_wait_view(snap, view, &mut icons, lang);
        main_win.set_wait(wait);
        main_win.set_party(ModelRc::new(VecModel::from(party)));
    }

    if let Some(ref cs) = snap.champ_select {
        if prev.is_none_or(|p| p.champ_select.as_ref() != Some(cs)) {
            let champ = build_champ_select_view(cs, &mut icons, draft, lang);
            main_win.set_champ(champ);
        }
    }

    if let Some(ref lg) = snap.live_game {
        if prev.is_none_or(|p| p.live_game.as_ref() != Some(lg) || p.board != snap.board) {
            let lv = build_live_view(lg, snap.board.as_ref(), &mut icons, live, lang);
            main_win.set_live(lv);
        }
    }

    if let Some(ref eog) = snap.end_of_game {
        let game_id = app_state.current_eog_game_id();
        let profile_state = app_state.stats.player_profile();
        let prof = profile_state.ready();
        let personal_champ = prof.and_then(|p| {
            p.champions
                .iter()
                .find(|c| c.champion_id == eog.champion_id)
        });
        let recent_games = prof.map(|p| p.games.as_slice()).unwrap_or(&[]);
        let review = app_state.reviews.review_for_end_of_game(
            eog,
            game_id,
            personal_champ,
            None,
            recent_games,
            lang,
        );
        let llm_enabled = app_state.coach_llm.is_available();
        let debrief = review
            .as_ref()
            .map(|r| app_state.coach_llm.state(r.game_id, lang))
            .unwrap_or_default();
        let end = build_end_view(
            eog,
            review.as_ref(),
            llm_enabled,
            &debrief,
            &mut icons,
            lang,
        );
        main_win.set_end(end);
    }

    view
}

/// Triggers an on-demand Layer 2 coach debrief for the active End-of-Game screen under `focus`.
pub fn request_end_debrief(app_state: &AppState, focus: hypatia_shared::CoachFocus) {
    let snap = app_state.get_summary();
    let Some(ref eog) = snap.end_of_game else {
        return;
    };
    let lang = app_state.lang();
    let game_id = app_state.current_eog_game_id();
    let profile_state = app_state.stats.player_profile();
    let prof = profile_state.ready();
    let personal_champ = prof.and_then(|p| {
        p.champions
            .iter()
            .find(|c| c.champion_id == eog.champion_id)
    });
    let recent_games = prof.map(|p| p.games.as_slice()).unwrap_or(&[]);
    if let Some(review) = app_state.reviews.review_for_end_of_game(
        eog,
        game_id,
        personal_champ,
        None,
        recent_games,
        lang,
    ) {
        app_state
            .coach_llm
            .request_debrief_with_focus(review, focus, lang);
    }
}

/// Sends a grounded follow-up question for the active End-of-Game screen's coach session.
pub fn ask_end_coach(app_state: &AppState, question: String) {
    let snap = app_state.get_summary();
    let Some(ref eog) = snap.end_of_game else {
        return;
    };
    let lang = app_state.lang();
    let game_id = app_state.current_eog_game_id();
    let profile_state = app_state.stats.player_profile();
    let prof = profile_state.ready();
    let personal_champ = prof.and_then(|p| {
        p.champions
            .iter()
            .find(|c| c.champion_id == eog.champion_id)
    });
    let recent_games = prof.map(|p| p.games.as_slice()).unwrap_or(&[]);
    if let Some(review) = app_state.reviews.review_for_end_of_game(
        eog,
        game_id,
        personal_champ,
        None,
        recent_games,
        lang,
    ) {
        app_state.coach_llm.ask_followup(review, question, lang);
    }
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
        let kept = PhaseClock::follow(
            Some(first),
            Some(PhaseClock::down_at(Instant::now(), 30.0, 30)),
        )
        .unwrap();
        assert_eq!(kept.seconds_now(later), first.seconds_now(later));
        // A new reading from the client re-anchors.
        let fresh = PhaseClock::follow(
            Some(first),
            Some(PhaseClock::down_at(Instant::now(), 29.0, 30)),
        )
        .unwrap();
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
        assert_ne!(
            b_pos, s_pos,
            "Bot and Support labels must use opposite sides of track"
        );
        assert!(
            (sx - bx).abs() >= 120.0,
            "Bot and Support nodes must have horizontal spacing"
        );
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
    fn sighting_places_fit_within_track_canvas() {
        let mut places = vec![MapPlace::DragonPit, MapPlace::BaronPit];
        for order_side in [true, false] {
            for lane in [Lane::Top, Lane::Mid, Lane::Bot] {
                places.push(MapPlace::Inhibitor { order_side, lane });
                for tier in 1..=3 {
                    places.push(MapPlace::Turret {
                        order_side,
                        lane,
                        tier,
                    });
                }
            }
        }
        for place in places {
            let (x, y) = place_coordinates(place);
            for (dx, dy) in SIGHTING_FAN {
                assert!(
                    (0.0..=400.0).contains(&(x + dx)),
                    "{place:?} x out of canvas"
                );
                assert!(
                    (0.0..=400.0).contains(&(y + dy)),
                    "{place:?} y out of canvas"
                );
            }
        }
    }

    #[test]
    fn lists_every_enemy_and_fans_out_those_sharing_a_place() {
        use hypatia_shared::{LiveGamePlayerSummary, Sighting};
        let player = |champ: &str, team: &str| LiveGamePlayerSummary {
            riot_id: format!("{champ}#EUW"),
            champion_id: 0,
            champion_name: champ.to_string(),
            champion_alias: String::new(),
            team: team.to_string(),
            position: String::new(),
            level: 1,
            kills: 0,
            deaths: 0,
            assists: 0,
            creep_score: 0,
            cs_per_min: 0.0,
            cs_diff_vs_lane: None,
            is_self: team == "ORDER",
            is_dead: false,
            respawn_timer_sec: 0.0,
            skin_id: 0,
        };
        let lg = LiveGameSummary {
            game_mode: "CLASSIC".into(),
            map_name: String::new(),
            game_time_sec: 1000.0,
            active_champion_id: 0,
            active_champion: String::new(),
            level: 1,
            current_gold: 0,
            kills: 0,
            deaths: 0,
            assists: 0,
            creep_score: 0,
            self_cs_per_min: 0.0,
            lane_cs_diff: None,
            personal_avg_cs_per_min: None,
            recent_events: vec![],
            players: vec![
                player("Ahri", "ORDER"),
                player("Viego", "CHAOS"),
                player("Zed", "CHAOS"),
                player("Nautilus", "CHAOS"),
            ],
        };
        let seen = |champion: &str, time_sec: f64| Sighting {
            champion: champion.to_string(),
            champion_id: 0,
            place: MapPlace::DragonPit,
            time_sec,
        };
        let board = LiveBoard {
            my_team_is_order: true,
            sightings: vec![seen("Viego", 950.0), seen("Nautilus", 700.0)],
            ..LiveBoard::default()
        };
        let rows = build_sightings(&lg, &board, &mut crate::icons::NoopIconLookup, Lang::En);
        let names: Vec<&str> = rows.iter().map(|r| r.champion.as_str()).collect();
        assert_eq!(names, ["Viego", "Zed", "Nautilus"]);
        assert!(rows[0].seen && !rows[1].seen && rows[2].seen);
        assert_eq!(rows[0].place, "Dragon pit");
        assert_eq!(rows[0].age, "just now");
        assert_eq!(rows[1].place, "Not seen yet");
        assert_eq!(rows[2].age, "5 min ago");
        assert!(!rows[0].stale && rows[2].stale);
        assert_ne!(
            (rows[0].x, rows[0].y),
            (rows[2].x, rows[2].y),
            "shared place fans out"
        );
    }

    #[test]
    fn helper_view_labels_origins_and_links_only_online_values() {
        use hypatia_shared::{
            Attribution, CompTrait, HelperItems, HelperLane, HelperRunes, HelperTrait,
        };
        let entry = |id, name: &str| HelperEntry {
            id,
            name: name.into(),
        };
        let mut h = ChampSelectHelper {
            champion_id: 103,
            champion_name: "Ahri".into(),
            position: Some(Position::Mid),
            win_rate: Some(0.512),
            runes: Some(HelperRunes {
                origin: StatsOrigin::RiotClient,
                keystone: entry(8112, "Electrocute"),
                primary_style: entry(8100, "Domination"),
                sub_style: entry(8200, "Sorcery"),
                selected_perk_ids: vec![8112, 8139, 8140, 8106, 8226, 8210, 5008, 5008, 5001],
                win_rate: None,
            }),
            items: Some(HelperItems {
                origin: StatsOrigin::Online,
                items: vec![entry(3118, "Malignance")],
                win_rate: Some(0.53),
            }),
            lane: Some(HelperLane {
                position: Position::Mid,
                enemy_champion_id: 99,
                enemy_champion_name: "Lux".into(),
                win_rate: Some(0.471),
                games: 3200,
                origin: Some(StatsOrigin::Online),
            }),
            my_comp: HelperComp {
                champions: 3,
                physical: 0.6,
                magic: 0.4,
                true_damage: 0.0,
                traits: [
                    CompTrait::Engage,
                    CompTrait::Peel,
                    CompTrait::Poke,
                    CompTrait::LacksFrontline,
                    CompTrait::Pick,
                ]
                .into_iter()
                .map(|kind| HelperTrait {
                    kind,
                    label: format!("{kind:?}"),
                    gap: kind == CompTrait::LacksFrontline,
                })
                .collect(),
            },
            online: HelperOnline::Ready,
            attribution: Some(Attribution {
                provider: "MetaBot.GG".into(),
                url: "https://metabot.gg/en/league/champion/Ahri/build".into(),
            }),
            ..ChampSelectHelper::default()
        };
        let mut icons = crate::icons::NoopIconLookup;
        let v = build_helper_view(&h, &mut icons, Lang::Fr);
        assert!(v.shown && v.has_champion && v.has_runes && !v.has_spells);
        assert_eq!(v.position, "Mid");
        assert_eq!(v.win_rate, "51,2 % de victoires");
        assert_eq!(
            (v.runes_origin.as_str(), v.runes_online),
            ("Client Riot", false)
        );
        assert_eq!(
            (v.items_origin.as_str(), v.items_online),
            ("En ligne", true)
        );
        assert_eq!(v.lane_line, "Mid contre Lux");
        assert_eq!(v.lane_rate, "47,1 % de victoires · 3 200 parties");
        assert_eq!(v.attribution, "Données : MetaBot.GG");
        assert_eq!(v.hint, "");
        assert_eq!(
            v.sources, "Client Riot · En ligne",
            "the mini widget names every origin once"
        );
        // Gaps first, two chips per row, four at most.
        let first: Vec<_> = v
            .mine
            .traits
            .iter()
            .map(|c| (c.label.to_string(), c.gap))
            .collect();
        assert_eq!(
            first,
            [
                ("LacksFrontline".to_string(), true),
                ("Engage".to_string(), false)
            ]
        );
        assert_eq!(v.mine.traits_more.row_count(), 2);

        // Online off: no link, the hint says how to turn it on.
        h.items.as_mut().unwrap().origin = StatsOrigin::Local;
        h.lane.as_mut().unwrap().origin = None;
        h.online = HelperOnline::Off;
        let v = build_helper_view(&h, &mut icons, Lang::En);
        assert_eq!(v.attribution, "");
        assert!(v.hint.contains("Online stats"));
        assert_eq!(v.sources, "Riot client · Your games");

        // ARAM: the hint says the mode has no online builds.
        h.online = HelperOnline::OtherMode;
        assert!(build_helper_view(&h, &mut icons, Lang::Fr)
            .hint
            .contains("pas ce mode"));

        // Nothing hovered yet: a neutral note, nothing invented.
        let v = build_helper_view(&ChampSelectHelper::default(), &mut icons, Lang::En);
        assert!(!v.has_champion && !v.has_runes && !v.has_lane && v.items.row_count() == 0);
        assert_eq!(
            v.empty_note,
            "Hover a champion to see its runes, spells and items."
        );
    }

    #[test]
    fn names_places_from_the_players_side() {
        let tower = MapPlace::Turret {
            order_side: false,
            lane: Lane::Mid,
            tier: 2,
        };
        assert_eq!(place_label(tower, true, Lang::En), "Their mid inner tower");
        assert_eq!(
            place_label(tower, false, Lang::Fr),
            "Votre tourelle intérieure mid"
        );
        let inhib = MapPlace::Inhibitor {
            order_side: true,
            lane: Lane::Bot,
        };
        assert_eq!(place_label(inhib, true, Lang::En), "Your bot inhibitor");
        assert_eq!(age_label(150.0, Lang::Fr), "il y a 2 min");
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
        assert_eq!(
            down_clock.seconds_now(anchor + Duration::from_millis(500)),
            29
        );

        let up_clock = phase_clock_at(&snap, Screen::InGame, anchor).unwrap();
        assert!(!up_clock.counts_down());
        assert_eq!(
            up_clock.seconds_now(anchor + Duration::from_millis(900)),
            613
        );

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

        let end_view = build_end_view(
            snap.end_of_game.as_ref().unwrap(),
            None,
            false,
            &hypatia_core::coach_llm::DebriefState::Idle,
            &mut icons,
            Lang::Fr,
        );
        assert!(end_view.victory);
        assert_eq!(end_view.title.as_str(), "VICTOIRE");
    }
}
