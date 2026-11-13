#![cfg_attr(all(not(test), not(debug_assertions)), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use hypatia_core::assets::{load_persisted_or_detect_lang, save_persisted_lang, GameAssetCache};
use hypatia_core::remote::{start_remote_server, QrCodeVisual, RemoteServerHandle};
use hypatia_core::{compute_json_diff_localized, AppState};
use hypatia_shared::{
    spell_name_from_id_localized, ChampSelectSummary, EndOfGameSummary, EventKind, Lane, Lang,
    LiveBoard, LiveGameSummary, ObjectiveKind, SourceId, SourceStatus, SummarySnapshot,
};
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer, TimerMode, VecModel};
use tracing::info;

slint::include_modules!();

#[derive(Parser, Debug, Clone)]
#[command(
    name = "hypatia",
    version,
    about = "Local League of Legends companion and telemetry observatory"
)]
struct Cli {
    /// Start in Demo mode with simulated event streams
    #[arg(long)]
    demo: bool,

    /// Demo events generated per second
    #[arg(long, default_value_t = 2.0)]
    demo_rate: f32,

    /// Interface language override (en or fr)
    #[arg(long, value_name = "LANG")]
    lang: Option<String>,

    /// Open Telemetry Lab observatory on launch
    #[arg(long)]
    telemetry: bool,

    /// Enable Mobile Remote server and show QR pairing drawer on launch
    #[arg(long)]
    remote: bool,

    /// Disable automatic startup of the Mobile Remote server
    #[arg(long)]
    no_remote: bool,

    /// Preferred LAN port for the Mobile Remote server
    #[arg(long, default_value_t = 7879)]
    remote_port: u16,

    /// Start in Light (Day) theme instead of Dark (Night) theme
    #[arg(long)]
    light: bool,

    /// Open the Always-on-Top Mini Widget on launch
    #[arg(long)]
    widget: bool,

    /// Pin a specific phase in Demo mode (lobby, queue, ready, champ-select, live, end)
    #[arg(long)]
    phase: Option<String>,
}

fn lane_index(position: &str) -> i32 {
    match position.trim().to_ascii_uppercase().as_str() {
        "TOP" => 0,
        "JUNGLE" => 1,
        "MIDDLE" | "MID" => 2,
        "BOTTOM" | "BOT" => 3,
        "UTILITY" | "SUPPORT" => 4,
        _ => -1,
    }
}

fn queue_type_label(queue_type: &str, lang: Lang) -> String {
    let trimmed = queue_type.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "RANKED_SOLO_5X5" | "RANKED SOLO 5X5" => {
            lang.pick("Ranked Solo/Duo", "Classée Solo/Duo").to_string()
        }
        "RANKED_FLEX_SR" | "RANKED FLEX SR" => {
            lang.pick("Ranked Flex", "Classée Flexible").to_string()
        }
        "NORMAL" | "NORMAL_DRAFT" | "DRAFT_PICK" => {
            lang.pick("Normal Draft", "Faille · Draft").to_string()
        }
        "QUICKPLAY" | "SWIFTPLAY" => lang.pick("Quickplay", "Partie rapide").to_string(),
        "ARAM" => "ARAM".to_string(),
        "URF" | "ARURF" => "URF".to_string(),
        "ARENA" | "CHERRY" => "Arena".to_string(),
        "PRACTICETOOL" | "PRACTICE_TOOL" => {
            lang.pick("Practice Tool", "Outil d'entraînement").to_string()
        }
        "CUSTOM" => lang.pick("Custom", "Partie personnalisée").to_string(),
        _ => trimmed.replace('_', " "),
    }
}

fn sub_phase_label(sub_phase: &str, lang: Lang) -> String {
    let trimmed = sub_phase.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "PLANNING" => lang.pick("Planning", "Planification").to_string(),
        "BAN_PICK" => lang.pick("Ban / Pick", "Bannissements & Choix").to_string(),
        "FINALIZATION" => lang.pick("Finalization", "Finalisation").to_string(),
        "GAME_STARTING" => lang.pick("Game Starting", "Lancement de la partie").to_string(),
        _ => trimmed.replace('_', " "),
    }
}

fn rank_tier_label(tier: &str, lang: Lang) -> &str {
    match tier.trim().to_ascii_uppercase().as_str() {
        "IRON" => lang.pick("Iron", "Fer"),
        "BRONZE" => "Bronze",
        "SILVER" => lang.pick("Silver", "Argent"),
        "GOLD" => lang.pick("Gold", "Or"),
        "PLATINUM" => lang.pick("Platinum", "Platine"),
        "EMERALD" => lang.pick("Emerald", "Émeraude"),
        "DIAMOND" => lang.pick("Diamond", "Diamant"),
        "MASTER" => lang.pick("Master", "Maître"),
        "GRANDMASTER" => lang.pick("Grandmaster", "Grand Maître"),
        "CHALLENGER" => "Challenger",
        _ => tier,
    }
}

fn champ_initials(name: &str) -> SharedString {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("Selecting")
        || trimmed.starts_with("Choosing")
        || trimmed.starts_with("Sélection")
    {
        return SharedString::default();
    }
    let words: Vec<&str> = trimmed
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let s: String = if words.len() >= 2 {
        words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect()
    } else {
        trimmed.chars().take(2).collect()
    };
    SharedString::from(s.to_uppercase())
}

fn format_mm_ss(seconds: f64) -> SharedString {
    let total = seconds.max(0.0) as u64;
    SharedString::from(format!("{}:{:02}", total / 60, total % 60))
}

fn qr_to_slint_image(qr: &QrCodeVisual) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&qr.rgba, qr.width, qr.height);
    Image::from_rgba8(buf)
}

#[derive(Default)]
struct DesktopAssetLoader {
    images: HashMap<String, Image>,
}

impl DesktopAssetLoader {
    fn resolve(&mut self, app_state: &AppState, category: &'static str, id: i64) -> Image {
        if let Some(img) = self.load_cached_only(&app_state.assets, category, id) {
            return img;
        }
        if id > 0 || (category == "profile" && id >= 0) {
            app_state.assets.prefetch_if_missing(
                category,
                id,
                &app_state.discovery,
                &app_state.vault,
                &app_state.riot_client,
            );
        }
        Image::default()
    }

    fn load_cached_only(
        &mut self,
        cache: &GameAssetCache,
        category: &str,
        id: i64,
    ) -> Option<Image> {
        if id < 0 || (category != "profile" && id == 0) {
            return None;
        }
        let key = format!("{category}/{id}");
        if let Some(img) = self.images.get(&key) {
            return Some(img.clone());
        }
        let path = cache.cached_image_path(category, id)?;
        let img = Image::load_from_path(&path).ok()?;
        self.images.insert(key, img.clone());
        Some(img)
    }
}

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
            let id = ids
                .get(i)
                .copied()
                .filter(|&cid| cid > 0)
                .unwrap_or_else(|| app_state.assets.resolve_champion_id(b));
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

    for ally in &cs.my_team {
        let lane = lane_index(&ally.assigned_position);
        let has_champ = ally.champion_id > 0
            && !ally.champion_name.is_empty()
            && !ally.champion_name.starts_with("Selecting")
            && !ally.champion_name.starts_with("Sélection");
        let champ_str = if has_champ {
            ally.champion_name.as_str()
        } else {
            ""
        };
        let picking = cs.acting_cell_id == Some(ally.cell_id);
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
    let detail = format!(
        "{} · {}",
        queue_type_label(&cs.queue_type, lang),
        sub_phase_label(&cs.sub_phase, lang)
    );

    let my_ban_list = build_ban_entries(&cs.my_team_bans, &cs.my_team_ban_ids, app_state, loader);
    let their_ban_list =
        build_ban_entries(&cs.their_team_bans, &cs.their_team_ban_ids, app_state, loader);

    ChampSelectView {
        title: SharedString::from(title),
        detail: SharedString::from(detail),
        timer: format_mm_ss(cs.time_left_sec as f64),
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
        let lane = lane_index(&p.position);
        let kda_str = format!("{} / {} / {}", p.kills, p.deaths, p.assists);
        let cid = if p.champion_id > 0 {
            p.champion_id
        } else {
            app_state
                .assets
                .resolve_champion_id_with_alias(&p.champion_name, &p.champion_alias)
        };
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
    } else if self_champion_id > 0 {
        self_champion_id
    } else {
        app_state.assets.resolve_champion_id(&lg.active_champion)
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
    let cid = if eog.champion_id > 0 {
        eog.champion_id
    } else {
        app_state.assets.resolve_champion_id(&eog.champion_name)
    };
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

fn apply_summary_to_windows(
    main_win: &MainWindow,
    mini_win: &MiniWidget,
    app_state: &AppState,
    loader: &mut DesktopAssetLoader,
    snap: &SummarySnapshot,
) {
    let lang = app_state.lang();
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
        app_state.assets.trigger_metadata_sync(
            &app_state.discovery,
            &app_state.vault,
            &app_state.riot_client,
        );
    }

    // Determine active view index:
    // 0 League closed, 1 client idle, 2 lobby, 3 queue, 4 match found, 5 champ select, 6 in game, 7 results
    let view = match phase_str {
        "Lobby" => 2,
        "Matchmaking" => 3,
        "ReadyCheck" => 4,
        "ChampSelect" => 5,
        "InProgress" => 6,
        "EndOfGame" | "PreEndOfGame" => 7,
        _ => {
            if lcu_live || snap.summoner.is_some() {
                1
            } else {
                0
            }
        }
    };
    main_win.set_view(view);

    // Queue label in top bar
    let queue_label = snap
        .lobby
        .as_ref()
        .map(|l| l.queue_name.clone())
        .or_else(|| {
            snap.champ_select
                .as_ref()
                .map(|c| queue_type_label(&c.queue_type, lang))
        })
        .unwrap_or_default();
    main_win.set_queue_label(SharedString::from(queue_label));

    // Populate Lobby / Queue / Idle wait view
    if view <= 4 {
        let summoner_label = snap
            .summoner
            .as_ref()
            .map(|s| format!("{}#{}", s.display_name, s.tag_line))
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
            0 => (
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
            1 => (
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
            2 => {
                let qn = snap
                    .lobby
                    .as_ref()
                    .map(|l| l.queue_name.as_str())
                    .unwrap_or(lang.pick("Lobby", "Salon"));
                (
                    if lang.is_fr() {
                        format!("Dans le salon · {qn}")
                    } else {
                        format!("In Lobby · {qn}")
                    },
                    lang.pick(
                        "Select your primary & secondary lanes, or scan the Mobile Remote QR to queue from your phone.",
                        "Choisissez vos postes principal et secondaire, ou scannez le code QR de l'App Mobile pour lancer la file depuis votre téléphone.",
                    )
                    .to_string(),
                    SharedString::default(),
                )
            }
            3 => {
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
            .map(|s| SharedString::from(format!("{}#{}", s.display_name, s.tag_line)))
            .unwrap_or_default();

        main_win.set_wait(WaitView {
            title: SharedString::from(title),
            detail: SharedString::from(detail),
            specimen,
            summoner_name: summoner_header_name,
            summoner_initials,
            summoner_icon,
            notes: ModelRc::new(VecModel::from(notes)),
        });

        let self_puuid = snap
            .summoner
            .as_ref()
            .map(|s| s.puuid.as_str())
            .unwrap_or("");
        let party: Vec<PartyRow> = snap
            .lobby
            .as_ref()
            .map(|l| {
                l.members
                    .iter()
                    .enumerate()
                    .map(|(idx, m)| PartyRow {
                        name: SharedString::from(m.summoner_name.as_str()),
                        first_lane: lane_index(m.first_position.as_deref().unwrap_or("")),
                        second_lane: lane_index(m.second_position.as_deref().unwrap_or("")),
                        is_leader: m.is_leader,
                        is_self: m
                            .puuid
                            .as_deref()
                            .map(|p| !self_puuid.is_empty() && p == self_puuid)
                            .unwrap_or(idx == 0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        main_win.set_party(ModelRc::new(VecModel::from(party)));
    }

    if let Some(ref cs) = snap.champ_select {
        main_win.set_champ(build_champ_select_view(cs, app_state, loader, lang));
    }

    if let Some(ref lg) = snap.live_game {
        let lv = build_live_view(lg, snap.board.as_ref(), app_state, loader, lang);
        main_win.set_live(lv.clone());
        mini_win.set_in_game(true);
        mini_win.set_live(lv);
    } else {
        mini_win.set_in_game(false);
    }

    if let Some(ref eog) = snap.end_of_game {
        main_win.set_end(build_end_view(eog, app_state, loader, lang));
    }
}

const MAX_DISPLAYED_ENDPOINTS: usize = 150;

#[derive(Default, Clone)]
struct TelemetryUiState {
    selected_event_id: Option<u64>,
    secrets_revealed: bool,
    call_status: String,
    call_response: String,
    call_version: u64,
    endpoint_filter: String,
    cached_events: Vec<TelemetryEventItem>,
    last_events_sig: Option<(u64, usize, i32, i32, Lang)>,
    last_inspector_sig: Option<(u64, i32, bool, Lang)>,
    cached_inspector: (String, String, bool, String),
    cached_endpoints: Vec<TelemetryEndpointItem>,
    cached_endpoints_total: i32,
    cached_endpoints_summary: String,
    last_catalog_sig: Option<(usize, String, u64, Lang)>,
    last_render_sig: Option<u64>,
}

impl TelemetryUiState {
    fn invalidate_telemetry_cache(&mut self) {
        self.last_events_sig = None;
        self.last_inspector_sig = None;
        self.last_catalog_sig = None;
        self.last_render_sig = None;
    }
}

fn format_bytes(bytes: u64, lang: Lang) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} {}", bytes as f64 / 1_048_576.0, lang.pick("MB", "Mo"))
    } else if bytes >= 1024 {
        format!("{:.1} {}", bytes as f64 / 1024.0, lang.pick("KB", "Ko"))
    } else {
        format!("{} {}", bytes, lang.pick("B", "o"))
    }
}

fn format_ts_hms(ts_ms: i64) -> String {
    let total_secs = (ts_ms.max(0) / 1000) % 86_400;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn source_short_label(src: SourceId, lang: Lang) -> (&'static str, i32) {
    match src {
        SourceId::RiotClient => ("RC", 0),
        SourceId::LeagueClient => ("LCU", 1),
        SourceId::LiveGame => (lang.pick("GAME", "JEU"), 2),
        SourceId::System => ("SYS", 3),
    }
}

fn apply_telemetry_to_window(
    main_win: &MainWindow,
    app_state: &AppState,
    ui_state: &mut TelemetryUiState,
) {
    if !main_win.get_telemetry_enabled() {
        return;
    }

    let lang = app_state.lang();
    let snap = app_state.get_snapshot();
    let (rate, total_len) = {
        let guard = app_state.store.read().unwrap();
        (guard.current_rate(), guard.len())
    };
    let latest_id = snap.recent_events.last().map(|e| e.id).unwrap_or(0);
    let src_filter = main_win.get_telemetry_source_filter();
    let kind_filter = main_win.get_telemetry_kind_filter();
    let paused = main_win.get_telemetry_paused();
    let inspector_mode = main_win.get_inspector_mode();

    let catalog_len = app_state.catalog.read().unwrap().get_all().len();
    let raw_phase = snap
        .phase
        .as_ref()
        .map(|p| p.phase.as_str())
        .unwrap_or("None");
    let phase_label = match raw_phase {
        "Lobby" => lang.pick("Lobby", "Salon"),
        "Matchmaking" => lang.pick("Matchmaking", "File d'attente"),
        "ReadyCheck" => lang.pick("Ready Check", "Appel de partie"),
        "ChampSelect" => lang.pick("Champ Select", "Sélection"),
        "InProgress" => lang.pick("In Game", "En partie"),
        "EndOfGame" | "PreEndOfGame" => lang.pick("End of Game", "Fin de partie"),
        "WaitingForStats" => lang.pick("Waiting for Stats", "Attente des stats"),
        "Reconnect" => lang.pick("Reconnect", "Reconnexion"),
        "None" => lang.pick("None", "Aucune"),
        other => other,
    };

    // Fast signature check so idle 250ms ticks do zero allocations or Slint model rebuilds
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    lang.as_code().hash(&mut hasher);
    ((rate * 10.0).round() as u64).hash(&mut hasher);
    total_len.hash(&mut hasher);
    latest_id.hash(&mut hasher);
    phase_label.hash(&mut hasher);
    src_filter.hash(&mut hasher);
    kind_filter.hash(&mut hasher);
    paused.hash(&mut hasher);
    ui_state.selected_event_id.hash(&mut hasher);
    inspector_mode.hash(&mut hasher);
    ui_state.secrets_revealed.hash(&mut hasher);
    catalog_len.hash(&mut hasher);
    ui_state.endpoint_filter.hash(&mut hasher);
    ui_state.call_version.hash(&mut hasher);
    for s in &snap.sources {
        format!("{:?}", s.status).hash(&mut hasher);
        s.port.hash(&mut hasher);
        s.pid.hash(&mut hasher);
        s.events_total.hash(&mut hasher);
    }
    snap.system.processes.len().hash(&mut hasher);
    snap.system.sockets.len().hash(&mut hasher);
    snap.system.connections.len().hash(&mut hasher);
    if let Some(p0) = snap.system.processes.first() {
        (p0.cpu.round() as u64).hash(&mut hasher);
    }
    let render_sig = hasher.finish();
    if ui_state.last_render_sig == Some(render_sig) {
        return;
    }
    ui_state.last_render_sig = Some(render_sig);

    let sources: Vec<TelemetrySourceItem> = snap
        .sources
        .iter()
        .map(|s| {
            let name = match s.id {
                SourceId::RiotClient => lang.pick("Riot Client", "Client Riot"),
                SourceId::LeagueClient => lang.pick("League Client (LCU)", "Client League (LCU)"),
                SourceId::LiveGame => lang.pick("Live Game :2999", "Partie en direct :2999"),
                SourceId::System => lang.pick("System Monitor", "Moniteur système"),
            };
            let live = matches!(s.status, SourceStatus::Live | SourceStatus::Discovered);
            let status_str = match s.status {
                SourceStatus::Live => lang.pick("LIVE", "DIRECT"),
                SourceStatus::Discovered => lang.pick("READY", "PRÊT"),
                SourceStatus::Connecting => lang.pick("CONNECTING", "CONNEXION"),
                SourceStatus::Offline => lang.pick("OFFLINE", "HORS LIGNE"),
                SourceStatus::Degraded { .. } => lang.pick("DEGRADED", "DÉGRADÉ"),
            };
            let port_str = match s.port {
                Some(p) => format!(":{}", p),
                None => "local".to_string(),
            };
            let pid_str = match s.pid {
                Some(pid) => format!("PID {}", pid),
                None => String::new(),
            };
            let stats_str = if s.id == SourceId::System {
                if lang.is_fr() {
                    format!(
                        "{} proc. · {} ports",
                        snap.system.processes.len(),
                        snap.system.sockets.len()
                    )
                } else {
                    format!(
                        "{} procs · {} sockets",
                        snap.system.processes.len(),
                        snap.system.sockets.len()
                    )
                }
            } else {
                format!(
                    "{} {} · {}",
                    s.events_total,
                    lang.pick("evts", "évts"),
                    format_bytes(s.bytes_total, lang)
                )
            };
            TelemetrySourceItem {
                id: SharedString::from(s.id.to_string()),
                name: SharedString::from(name),
                status: SharedString::from(status_str),
                live,
                port: SharedString::from(port_str),
                pid: SharedString::from(pid_str),
                stats: SharedString::from(stats_str),
            }
        })
        .collect();

    let events_sig = (latest_id, total_len, src_filter, kind_filter, lang);
    if (!paused && ui_state.last_events_sig != Some(events_sig)) || ui_state.cached_events.is_empty()
    {
        let filtered: Vec<TelemetryEventItem> = snap
            .recent_events
            .iter()
            .rev()
            .filter(|ev| match src_filter {
                1 => ev.source == SourceId::LeagueClient,
                2 => ev.source == SourceId::RiotClient,
                3 => ev.source == SourceId::LiveGame,
                _ => true,
            })
            .filter(|ev| match kind_filter {
                1 => ev.kind == EventKind::Update,
                2 => ev.kind == EventKind::Create,
                3 => ev.kind == EventKind::Delete,
                4 => matches!(ev.kind, EventKind::Poll | EventKind::Call),
                _ => true,
            })
            .take(120)
            .map(|ev| {
                let (short_src, src_idx) = source_short_label(ev.source, lang);
                let kind_str = match ev.kind {
                    EventKind::Create => lang.pick("CREATE", "CRÉAT."),
                    EventKind::Update => lang.pick("UPD", "MÀJ"),
                    EventKind::Delete => lang.pick("DEL", "SUPPR."),
                    EventKind::Poll => lang.pick("POLL", "SONDE"),
                    EventKind::Call => lang.pick("CALL", "APPEL"),
                };
                TelemetryEventItem {
                    id: ev.id as i32,
                    time: SharedString::from(format_ts_hms(ev.ts)),
                    source: SharedString::from(short_src),
                    source_idx: src_idx,
                    kind: SharedString::from(kind_str),
                    uri: SharedString::from(ev.uri.as_str()),
                    size: SharedString::from(format_bytes(ev.size as u64, lang)),
                    preview: SharedString::from(ev.preview.as_str()),
                    has_secrets: ev.has_secrets,
                }
            })
            .collect();

        if ui_state.selected_event_id.is_none() {
            if let Some(first) = filtered.first() {
                ui_state.selected_event_id = Some(first.id as u64);
            }
        }
        ui_state.cached_events = filtered;
        ui_state.last_events_sig = Some(events_sig);
    }

    let desired_ins_sig = ui_state
        .selected_event_id
        .map(|id| (id, inspector_mode, ui_state.secrets_revealed, lang));
    if ui_state.last_inspector_sig != desired_ins_sig
        || (ui_state.selected_event_id.is_some() && ui_state.cached_inspector.0.is_empty())
    {
        ui_state.cached_inspector = if let Some(sel_id) = ui_state.selected_event_id {
            let stored_opt = app_state.store.read().unwrap().get_event(sel_id);
            if let Some(stored) = stored_opt {
                let (short_src, _) = source_short_label(stored.meta.source, lang);
                let title = format!("#{} · {} · {}", stored.meta.id, short_src, stored.meta.uri);
                let kind_lbl = match stored.meta.kind {
                    EventKind::Create => lang.pick("Create", "Création"),
                    EventKind::Update => lang.pick("Update", "Mise à jour"),
                    EventKind::Delete => lang.pick("Delete", "Suppression"),
                    EventKind::Poll => lang.pick("Poll", "Sondage"),
                    EventKind::Call => lang.pick("Call", "Appel"),
                };
                let meta = if lang.is_fr() {
                    format!(
                        "Type : {} · {} · Heure : {}",
                        kind_lbl,
                        format_bytes(stored.meta.size as u64, lang),
                        format_ts_hms(stored.meta.ts)
                    )
                } else {
                    format!(
                        "Kind: {} · {} · Time: {}",
                        kind_lbl,
                        format_bytes(stored.meta.size as u64, lang),
                        format_ts_hms(stored.meta.ts)
                    )
                };
                let curr_val = if ui_state.secrets_revealed {
                    app_state.reveal_secrets_in_value(&stored.payload)
                } else {
                    stored.payload.clone()
                };
                let prev_val = stored.previous_payload.as_ref().map(|p| {
                    if ui_state.secrets_revealed {
                        app_state.reveal_secrets_in_value(p)
                    } else {
                        p.clone()
                    }
                });
                let body = match inspector_mode {
                    1 => compute_json_diff_localized(prev_val.as_ref(), &curr_val, lang),
                    2 => match prev_val {
                        Some(ref pv) => {
                            serde_json::to_string_pretty(pv).unwrap_or_else(|_| "{}".to_string())
                        }
                        None => lang
                            .pick(
                                "(No previous state recorded for this URI)",
                                "(Aucun état précédent enregistré pour cette URI)",
                            )
                            .to_string(),
                    },
                    _ => serde_json::to_string_pretty(&curr_val)
                        .unwrap_or_else(|_| "{}".to_string()),
                };
                (title, meta, stored.meta.has_secrets, body)
            } else {
                (
                    if lang.is_fr() {
                        format!("Événement #{} éjecté du tampon", sel_id)
                    } else {
                        format!("Event #{} evicted from buffer", sel_id)
                    },
                    String::new(),
                    false,
                    String::new(),
                )
            }
        } else {
            (String::new(), String::new(), false, String::new())
        };
        ui_state.last_inspector_sig = desired_ins_sig;
    }
    let (ref inspector_title, ref inspector_meta, inspector_has_secrets, ref inspector_payload) =
        ui_state.cached_inspector;

    let catalog_sig = (
        catalog_len,
        ui_state.endpoint_filter.clone(),
        ui_state.call_version,
        lang,
    );
    if ui_state.last_catalog_sig.as_ref() != Some(&catalog_sig) {
        let filter_lc = ui_state.endpoint_filter.trim().to_ascii_lowercase();
        let guard = app_state.catalog.read().unwrap();
        let all = guard.get_all();
        let mut matched: Vec<_> = all
            .iter()
            .filter(|ep| {
                if filter_lc.is_empty() {
                    return true;
                }
                let (short_src, _) = source_short_label(ep.source, lang);
                ep.path_template.to_ascii_lowercase().contains(&filter_lc)
                    || ep.namespace.to_ascii_lowercase().contains(&filter_lc)
                    || short_src.to_ascii_lowercase().contains(&filter_lc)
                    || ep
                        .description
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&filter_lc)
            })
            .collect();
        // Prioritize endpoints with observed traffic, then alphabetical path
        matched.sort_by(|a, b| {
            (b.event_count > 0)
                .cmp(&(a.event_count > 0))
                .then_with(|| b.event_count.cmp(&a.event_count))
                .then_with(|| a.path_template.cmp(&b.path_template))
        });

        let matching_total = matched.len();
        let items: Vec<TelemetryEndpointItem> = matched
            .into_iter()
            .take(MAX_DISPLAYED_ENDPOINTS)
            .map(|ep| {
                let (short_src, _) = source_short_label(ep.source, lang);
                let call_src_idx = match ep.source {
                    SourceId::LeagueClient => 0,
                    SourceId::RiotClient => 1,
                    SourceId::LiveGame => 2,
                    SourceId::System => 0,
                };
                let origin_str = format!("{:?}", ep.origin);
                let stats_str = format!(
                    "{} {} · {}",
                    ep.event_count,
                    lang.pick("calls", "appels"),
                    format_bytes(ep.bytes_total, lang)
                );
                TelemetryEndpointItem {
                    source: SharedString::from(short_src),
                    source_idx: call_src_idx,
                    method: SharedString::from(ep.method.as_str()),
                    path: SharedString::from(ep.path_template.as_str()),
                    namespace: SharedString::from(ep.namespace.as_str()),
                    origin: SharedString::from(origin_str),
                    stats: SharedString::from(stats_str),
                    description: SharedString::from(ep.description.as_deref().unwrap_or("")),
                }
            })
            .collect();

        ui_state.cached_endpoints_summary = if matching_total > items.len() {
            if lang.is_fr() {
                format!(
                    "Affichage de {} sur {} ({} au total)",
                    items.len(),
                    matching_total,
                    all.len()
                )
            } else {
                format!(
                    "Showing {} of {} ({} total)",
                    items.len(),
                    matching_total,
                    all.len()
                )
            }
        } else if lang.is_fr() {
            format!("{} sur {} endpoints", matching_total, all.len())
        } else {
            format!("{} of {} endpoints", matching_total, all.len())
        };
        ui_state.cached_endpoints_total = all.len() as i32;
        ui_state.cached_endpoints = items;
        ui_state.last_catalog_sig = Some(catalog_sig);
    }

    let processes: Vec<TelemetryProcessItem> = snap
        .system
        .processes
        .iter()
        .map(|p| TelemetryProcessItem {
            pid: SharedString::from(p.pid.to_string()),
            name: SharedString::from(p.name.as_str()),
            source: SharedString::from(
                p.source
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "riot".to_string()),
            ),
            cpu: SharedString::from(format!("{:.1}% CPU", p.cpu)),
            mem: SharedString::from(format_bytes(p.mem_bytes, lang)),
            exe: SharedString::from(p.exe.as_deref().unwrap_or("")),
        })
        .collect();

    let sockets: Vec<TelemetrySocketItem> = snap
        .system
        .sockets
        .iter()
        .map(|s| TelemetrySocketItem {
            proto: SharedString::from(s.proto.as_str()),
            address: SharedString::from(format!("{}:{}", s.local, s.port)),
            pid: SharedString::from(s.pid.to_string()),
            state: SharedString::from(lang.pick("LISTEN", "ÉCOUTE")),
        })
        .collect();

    let connections: Vec<TelemetrySocketItem> = snap
        .system
        .connections
        .iter()
        .map(|c| TelemetrySocketItem {
            proto: SharedString::from("TCP"),
            address: SharedString::from(format!("{} -> {}", c.local, c.remote)),
            pid: SharedString::from(c.pid.to_string()),
            state: SharedString::from(c.state.as_str()),
        })
        .collect();

    main_win.set_telemetry(TelemetryView {
        rate: SharedString::from(if lang.is_fr() {
            format!("{:.1} évt/s", rate)
        } else {
            format!("{:.1} evt/s", rate)
        }),
        total_events: SharedString::from(if lang.is_fr() {
            format!("{} en mémoire", total_len)
        } else {
            format!("{} stored", total_len)
        }),
        phase: SharedString::from(phase_label),
        sources: ModelRc::new(VecModel::from(sources)),
        events: ModelRc::new(VecModel::from(ui_state.cached_events.clone())),
        selected_event_id: ui_state.selected_event_id.unwrap_or(0) as i32,
        inspector_title: SharedString::from(inspector_title.as_str()),
        inspector_meta: SharedString::from(inspector_meta.as_str()),
        inspector_has_secrets,
        secrets_revealed: ui_state.secrets_revealed,
        inspector_payload: SharedString::from(inspector_payload.as_str()),
        endpoints: ModelRc::new(VecModel::from(ui_state.cached_endpoints.clone())),
        endpoints_total: ui_state.cached_endpoints_total,
        endpoints_summary: SharedString::from(ui_state.cached_endpoints_summary.as_str()),
        call_status: SharedString::from(ui_state.call_status.as_str()),
        call_response: SharedString::from(ui_state.call_response.as_str()),
        processes: ModelRc::new(VecModel::from(processes)),
        sockets: ModelRc::new(VecModel::from(sockets)),
        connections: ModelRc::new(VecModel::from(connections)),
    });
}

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let cli = Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to start Tokio runtime");
    let _guard = rt.enter();

    let initial_lang = match cli.lang.as_deref() {
        Some(raw) => match Lang::from_code(raw) {
            Some(parsed) => parsed,
            None => {
                tracing::warn!("Unknown --lang value '{}', falling back to saved/OS locale", raw);
                load_persisted_or_detect_lang()
            }
        },
        None => load_persisted_or_detect_lang(),
    };
    let app_state = AppState::start_with_lang(cli.demo, cli.demo_rate, initial_lang);

    if let Some(ref pinned) = cli.phase {
        let normalized = match pinned.to_ascii_lowercase().as_str() {
            "lobby" => "Lobby",
            "queue" | "matchmaking" => "Matchmaking",
            "ready" | "readycheck" => "ReadyCheck",
            "champ" | "champ-select" | "champselect" => "ChampSelect",
            "live" | "inprogress" | "game" => "InProgress",
            "end" | "eog" | "endofgame" => "EndOfGame",
            _ => "ChampSelect",
        };
        let mut demo = app_state.demo_remote.write().unwrap();
        let _ = demo.apply_command(
            &app_state,
            &hypatia_shared::RemoteCommand::DemoSetPhase {
                phase: normalized.to_string(),
            },
        );
    }

    let main_win = MainWindow::new().expect("Failed to create MainWindow");
    let mini_win = MiniWidget::new().expect("Failed to create MiniWidget");

    main_win.set_demo(cli.demo);
    main_win.set_telemetry_enabled(cli.telemetry);
    if cli.light {
        Theme::get(&main_win).set_dark(false);
        Theme::get(&mini_win).set_dark(false);
    }
    Theme::get(&main_win).set_fr(initial_lang.is_fr());
    Theme::get(&mini_win).set_fr(initial_lang.is_fr());

    let widget_open = Rc::new(RefCell::new(false));
    if cli.widget {
        *widget_open.borrow_mut() = true;
        main_win.set_widget_open(true);
        let _ = mini_win.show();
    }

    let telemetry_ui_state = Rc::new(RefCell::new(TelemetryUiState::default()));
    let desktop_assets = Rc::new(RefCell::new(DesktopAssetLoader::default()));

    // Theme callback
    {
        let main_weak = main_win.as_weak();
        let mini_weak = mini_win.as_weak();
        main_win.on_set_dark(move |dark| {
            if let Some(m) = main_weak.upgrade() {
                Theme::get(&m).set_dark(dark);
            }
            if let Some(w) = mini_weak.upgrade() {
                Theme::get(&w).set_dark(dark);
            }
        });
    }

    {
        let main_weak = main_win.as_weak();
        let mini_weak = mini_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        let assets_loader = desktop_assets.clone();
        main_win.on_set_fr(move |fr| {
            let next = if fr { Lang::Fr } else { Lang::En };
            save_persisted_lang(next);
            app_state.set_lang(next);
            ui_state.borrow_mut().invalidate_telemetry_cache();
            if let Some(m) = main_weak.upgrade() {
                Theme::get(&m).set_fr(fr);
                if let Some(w) = mini_weak.upgrade() {
                    Theme::get(&w).set_fr(fr);
                    let snap = app_state.get_summary();
                    apply_summary_to_windows(
                        &m,
                        &w,
                        &app_state,
                        &mut assets_loader.borrow_mut(),
                        &snap,
                    );
                }
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
            }
        });
    }

    // Telemetry toggle callback
    {
        let main_weak = main_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        main_win.on_toggle_telemetry(move || {
            if let Some(m) = main_weak.upgrade() {
                let next = !m.get_telemetry_enabled();
                m.set_telemetry_enabled(next);
                if next {
                    let mut st = ui_state.borrow_mut();
                    st.last_render_sig = None;
                    apply_telemetry_to_window(&m, &app_state, &mut st);
                }
            }
        });
    }

    // Demo mode toggle callback
    {
        let main_weak = main_win.as_weak();
        let mini_weak = mini_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        let assets_loader = desktop_assets.clone();
        let demo_rate = cli.demo_rate;
        main_win.on_toggle_demo(move || {
            if let Some(m) = main_weak.upgrade() {
                let next = !m.get_demo();
                app_state.set_demo_active(next, demo_rate);
                m.set_demo(next);
                if let Some(w) = mini_weak.upgrade() {
                    let snap = app_state.get_summary();
                    apply_summary_to_windows(
                        &m,
                        &w,
                        &app_state,
                        &mut assets_loader.borrow_mut(),
                        &snap,
                    );
                }
                let mut st = ui_state.borrow_mut();
                st.invalidate_telemetry_cache();
                apply_telemetry_to_window(&m, &app_state, &mut st);
            }
        });
    }

    // Demo phase selector callback
    {
        let main_weak = main_win.as_weak();
        let mini_weak = mini_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        let assets_loader = desktop_assets.clone();
        main_win.on_select_demo_phase(move |phase| {
            {
                let mut demo = app_state.demo_remote.write().unwrap();
                let _ = demo.apply_command(
                    &app_state,
                    &hypatia_shared::RemoteCommand::DemoSetPhase {
                        phase: phase.to_string(),
                    },
                );
            }
            if let (Some(m), Some(w)) = (main_weak.upgrade(), mini_weak.upgrade()) {
                let snap = app_state.get_summary();
                apply_summary_to_windows(
                    &m,
                    &w,
                    &app_state,
                    &mut assets_loader.borrow_mut(),
                    &snap,
                );
                let mut st = ui_state.borrow_mut();
                st.invalidate_telemetry_cache();
                apply_telemetry_to_window(&m, &app_state, &mut st);
            }
        });
    }

    // Select telemetry event callback
    {
        let main_weak = main_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        main_win.on_select_telemetry_event(move |id| {
            if id > 0 {
                ui_state.borrow_mut().selected_event_id = Some(id as u64);
            }
            if let Some(m) = main_weak.upgrade() {
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
            }
        });
    }

    // Toggle secret reveal callback
    {
        let main_weak = main_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        main_win.on_toggle_reveal_secrets(move || {
            {
                let mut st = ui_state.borrow_mut();
                st.secrets_revealed = !st.secrets_revealed;
            }
            if let Some(m) = main_weak.upgrade() {
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
            }
        });
    }

    // Clear telemetry events callback
    {
        let main_weak = main_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        main_win.on_clear_telemetry_events(move || {
            app_state.clear_events();
            {
                let mut st = ui_state.borrow_mut();
                st.selected_event_id = None;
                st.cached_events.clear();
                st.cached_inspector = (String::new(), String::new(), false, String::new());
                st.invalidate_telemetry_cache();
            }
            if let Some(m) = main_weak.upgrade() {
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
            }
        });
    }

    // Endpoint catalog search filter callback
    {
        let main_weak = main_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        main_win.on_set_endpoint_filter(move |filter| {
            {
                let mut st = ui_state.borrow_mut();
                st.endpoint_filter = filter.to_string();
                st.last_render_sig = None;
            }
            if let Some(m) = main_weak.upgrade() {
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
            }
        });
    }

    // Execute GET call callback
    {
        let main_weak = main_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        let rt_handle = rt.handle().clone();
        main_win.on_execute_get_call(move |src_idx, path| {
            let source_id = match src_idx {
                1 => SourceId::RiotClient,
                2 => SourceId::LiveGame,
                _ => SourceId::LeagueClient,
            };
            let path_str = path.to_string();
            let res = rt_handle.block_on(app_state.call_endpoint(source_id, &path_str));
            {
                let mut st = ui_state.borrow_mut();
                st.call_version += 1;
                st.invalidate_telemetry_cache();
                match res {
                    Ok((status, body, ev_id)) => {
                        let lang = app_state.lang();
                        let pretty = serde_json::to_string_pretty(&body)
                            .unwrap_or_else(|_| "{}".to_string());
                        st.call_status = if lang.is_fr() {
                            format!(
                                "HTTP {} · {} · Événement #{}",
                                status,
                                format_bytes(pretty.len() as u64, lang),
                                ev_id
                            )
                        } else {
                            format!(
                                "HTTP {} · {} · Event #{}",
                                status,
                                format_bytes(pretty.len() as u64, lang),
                                ev_id
                            )
                        };
                        st.call_response = pretty;
                        st.selected_event_id = Some(ev_id);
                    }
                    Err(err) => {
                        let lang = app_state.lang();
                        st.call_status = lang
                            .pick("Request Failed", "Échec de la requête")
                            .to_string();
                        st.call_response = err;
                    }
                }
            }
            if let Some(m) = main_weak.upgrade() {
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
            }
        });
    }

    // Mini widget toggle callback
    {
        let main_weak = main_win.as_weak();
        let mini_weak = mini_win.as_weak();
        let open_flag = widget_open.clone();
        main_win.on_toggle_widget(move || {
            let mut open = open_flag.borrow_mut();
            *open = !*open;
            if let Some(w) = mini_weak.upgrade() {
                if *open {
                    let _ = w.show();
                } else {
                    let _ = w.hide();
                }
            }
            if let Some(m) = main_weak.upgrade() {
                m.set_widget_open(*open);
            }
        });
    }

    // Mobile Remote server state
    let remote_handle: Arc<Mutex<Option<RemoteServerHandle>>> = Arc::new(Mutex::new(None));
    let remote_starting = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let rt_handle = rt.handle().clone();

    let start_or_toggle_remote = {
        let remote_handle = remote_handle.clone();
        let remote_starting = remote_starting.clone();
        let app_state = app_state.clone();
        let main_weak = main_win.as_weak();
        let preferred_port = cli.remote_port;
        let rt_handle = rt_handle.clone();
        move |open_drawer: bool| {
            let has_handle = remote_handle.lock().unwrap().is_some();
            if has_handle {
                if let Some(m) = main_weak.upgrade() {
                    m.set_remote_panel_open(!m.get_remote_panel_open());
                }
                return;
            }
            if remote_starting.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let remote_handle_bg = remote_handle.clone();
            let remote_starting_bg = remote_starting.clone();
            let app_state_bg = app_state.clone();
            let main_weak_bg = main_weak.clone();
            rt_handle.spawn(async move {
                let res = start_remote_server(app_state_bg, preferred_port).await;
                remote_starting_bg.store(false, std::sync::atomic::Ordering::SeqCst);
                match res {
                    Ok(handle) => {
                        let qr_opt = handle.qr_visual().ok();
                        if let Some(ref qr) = qr_opt {
                            info!("Mobile Remote ready at {}", qr.url);
                        }
                        *remote_handle_bg.lock().unwrap() = Some(handle);
                        let _ = slint::invoke_from_event_loop(move || {
                            if let (Some(m), Some(qr)) = (main_weak_bg.upgrade(), qr_opt) {
                                m.set_remote_qr(qr_to_slint_image(&qr));
                                m.set_remote_url(SharedString::from(qr.url));
                                m.set_remote_enabled(true);
                                m.set_remote_panel_open(open_drawer);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to start Mobile Remote: {}", e);
                    }
                }
            });
        }
    };

    if !cli.no_remote {
        // Start remote server asynchronously so the desktop window appears immediately
        start_or_toggle_remote(cli.remote);
    }

    {
        let toggle_fn = start_or_toggle_remote.clone();
        main_win.on_toggle_remote_panel(move || {
            toggle_fn(true);
        });
    }

    {
        let remote_handle = remote_handle.clone();
        let main_weak = main_win.as_weak();
        main_win.on_rotate_remote_token(move || {
            let guard = remote_handle.lock().unwrap();
            if let Some(ref handle) = *guard {
                if let Ok(qr) = handle.rotate_token() {
                    if let Some(m) = main_weak.upgrade() {
                        m.set_remote_qr(qr_to_slint_image(&qr));
                        m.set_remote_url(SharedString::from(qr.url));
                    }
                }
            }
        });
    }

    {
        let remote_handle = remote_handle.clone();
        let main_weak = main_win.as_weak();
        main_win.on_disable_remote(move || {
            let mut guard = remote_handle.lock().unwrap();
            if let Some(mut handle) = guard.take() {
                handle.stop();
            }
            if let Some(m) = main_weak.upgrade() {
                m.set_remote_enabled(false);
                m.set_remote_panel_open(false);
                m.set_remote_url(SharedString::default());
            }
        });
    }

    // Initial state render + show window immediately + periodic 250ms UI refresh timer
    let initial_summary = app_state.get_summary();
    apply_summary_to_windows(
        &main_win,
        &mini_win,
        &app_state,
        &mut desktop_assets.borrow_mut(),
        &initial_summary,
    );
    apply_telemetry_to_window(&main_win, &app_state, &mut telemetry_ui_state.borrow_mut());
    main_win.show().expect("Failed to show MainWindow");

    let ui_timer = Timer::default();
    {
        let main_weak = main_win.as_weak();
        let mini_weak = mini_win.as_weak();
        let app_state = app_state.clone();
        let ui_state = telemetry_ui_state.clone();
        let assets_loader = desktop_assets.clone();
        let remote_handle_poll = remote_handle.clone();
        ui_timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            if let (Some(m), Some(w)) = (main_weak.upgrade(), mini_weak.upgrade()) {
                let snap = app_state.get_summary();
                apply_summary_to_windows(
                    &m,
                    &w,
                    &app_state,
                    &mut assets_loader.borrow_mut(),
                    &snap,
                );
                apply_telemetry_to_window(&m, &app_state, &mut ui_state.borrow_mut());
                let clients = remote_handle_poll
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|h| h.connected_clients() as i32)
                    .unwrap_or(0);
                m.set_remote_clients(clients);
            }
        });
    }

    main_win.run().expect("Slint event loop exited with error");
    rt.shutdown_background();
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
    fn desktop_asset_loader_loads_valid_png_from_disk_cache() {
        let dir = std::env::temp_dir().join(format!("hypatia_desktop_img_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("champion")).unwrap();
        // Minimal valid 1x1 RGBA PNG generated cleanly via Image::from_rgba8 or known PNG bytes:
        // Standard 1x1 transparent PNG (67 bytes):
        let png_1x1: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("champion").join("103.png"), png_1x1).unwrap();

        let cache = GameAssetCache::with_dir(dir.clone());
        let mut loader = DesktopAssetLoader::default();
        let img = loader.load_cached_only(&cache, "champion", 103).expect("should decode cached 1x1 PNG");
        assert_eq!(img.size().width, 1);
        assert_eq!(img.size().height, 1);
        assert!(loader.load_cached_only(&cache, "champion", 9999).is_none());

        let _ = std::fs::remove_dir_all(&dir);
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
    fn desktop_localization_labels_cover_both_languages() {
        assert_eq!(queue_type_label("RANKED_SOLO_5X5", Lang::En), "Ranked Solo/Duo");
        assert_eq!(queue_type_label("RANKED_SOLO_5X5", Lang::Fr), "Classée Solo/Duo");
        assert_eq!(sub_phase_label("BAN_PICK", Lang::En), "Ban / Pick");
        assert_eq!(sub_phase_label("BAN_PICK", Lang::Fr), "Bannissements & Choix");
        assert_eq!(rank_tier_label("EMERALD", Lang::En), "Emerald");
        assert_eq!(rank_tier_label("EMERALD", Lang::Fr), "Émeraude");
        assert_eq!(format_bytes(2048, Lang::En), "2.0 KB");
        assert_eq!(format_bytes(2048, Lang::Fr), "2.0 Ko");
        assert_eq!(format_bytes(512, Lang::En), "512 B");
        assert_eq!(format_bytes(512, Lang::Fr), "512 o");
        assert_eq!(source_short_label(SourceId::LiveGame, Lang::En), ("GAME", 2));
        assert_eq!(source_short_label(SourceId::LiveGame, Lang::Fr), ("JEU", 2));
    }
}
