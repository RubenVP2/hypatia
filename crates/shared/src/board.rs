//! Player-facing live game board: team score, tower status, objectives and a
//! plain-language feed, derived from Live Client Data (`/liveclientdata/*`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::champions::Position;
use crate::i18n::Lang;
use crate::summary::LiveGameSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lane {
    Top,
    Jungle,
    Mid,
    Bot,
}

impl Lane {
    /// Maps a Riot position (`TOP`, `JUNGLE`, `MIDDLE`, `BOTTOM`, `UTILITY`) to a lane.
    pub fn from_position(position: &str) -> Option<Lane> {
        match Position::parse(position)? {
            Position::Top => Some(Lane::Top),
            Position::Jungle => Some(Lane::Jungle),
            Position::Mid => Some(Lane::Mid),
            Position::Bot | Position::Support => Some(Lane::Bot),
            Position::Fill => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TowerState {
    /// True when the tower belongs to ORDER (blue side).
    pub order_side: bool,
    pub lane: Lane,
    /// 1 = outer, 2 = inner, 3 = inhibitor tower.
    pub tier: u8,
    pub standing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectiveKind {
    Baron,
    Elder,
    Drake,
    Herald,
    Grubs,
}

impl ObjectiveKind {
    /// Fixed importance ramp: Baron > Elder > Drake > Herald > Grubs.
    pub fn weight(self) -> u8 {
        match self {
            ObjectiveKind::Baron => 5,
            ObjectiveKind::Elder => 4,
            ObjectiveKind::Drake => 3,
            ObjectiveKind::Herald => 2,
            ObjectiveKind::Grubs => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveTaken {
    pub kind: ObjectiveKind,
    /// Human name, e.g. "Infernal Drake".
    pub label: String,
    pub by_order: bool,
    pub time_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedItem {
    pub time_sec: f64,
    pub text: String,
    pub lane: Option<Lane>,
    /// Some(true) when the player's team did it, Some(false) for the enemy.
    pub by_my_team: Option<bool>,
}

/// A fixed spot on Summoner's Rift where a Live Client event can only have happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapPlace {
    Turret {
        order_side: bool,
        lane: Lane,
        tier: u8,
    },
    Inhibitor {
        order_side: bool,
        lane: Lane,
    },
    DragonPit,
    /// The top-side pit shared by Void Grubs, Rift Herald and Baron Nashor.
    BaronPit,
}

/// The last place an enemy champion was spotted: the latest event that names them (killer or
/// assister) and happens at a fixed spot. Live Client Data carries no coordinates, so a
/// champion kill alone places no one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub champion: String,
    pub champion_id: i64,
    pub place: MapPlace,
    pub time_sec: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pit {
    Dragon,
    Baron,
}

/// The epic monster next due in a pit and the game time it spawns, from the fixed
/// Summoner's Rift schedule and the kills seen so far. Smaller camps raise no event, so their
/// timers cannot be known.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PitTimer {
    pub pit: Pit,
    pub kind: ObjectiveKind,
    pub spawn_sec: f64,
}

/// Summoner's Rift epic monster schedule (patch 26.1): game seconds.
pub mod rift_schedule {
    pub const DRAKE_FIRST: f64 = 300.0;
    pub const DRAKE_RESPAWN: f64 = 300.0;
    /// Elder spawns this long after a soul is claimed, and respawns as long after a kill.
    pub const ELDER_RESPAWN: f64 = 360.0;
    pub const DRAKES_FOR_SOUL: usize = 4;
    pub const GRUBS_SPAWN: f64 = 480.0;
    pub const GRUBS_LEAVE: f64 = 885.0;
    pub const HERALD_SPAWN: f64 = 900.0;
    pub const HERALD_LEAVE: f64 = 1185.0;
    pub const BARON_FIRST: f64 = 1200.0;
    pub const BARON_RESPAWN: f64 = 360.0;
    pub const BARON_BUFF_SEC: f64 = 180.0;
    pub const ELDER_BUFF_SEC: f64 = 150.0;
}

/// Active Baron Nashor (180s) or Elder Dragon (150s) buff timer with surviving holders count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveBuff {
    pub kind: ObjectiveKind,
    pub by_order: bool,
    pub expires_at_sec: f64,
    pub remaining_sec: f64,
    pub holders_alive: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LiveBoard {
    pub my_team_is_order: bool,
    pub order_kills: u32,
    pub chaos_kills: u32,
    pub towers: Vec<TowerState>,
    pub objectives: Vec<ObjectiveTaken>,
    /// Newest first.
    pub feed: Vec<FeedItem>,
    /// One per enemy champion spotted so far, in enemy roster order.
    pub sightings: Vec<Sighting>,
    /// The next monster of each pit; empty outside Summoner's Rift.
    pub pits: Vec<PitTimer>,
    /// Active Baron / Elder buffs currently ticking on Summoner's Rift.
    #[serde(default)]
    pub active_buffs: Vec<ActiveBuff>,
}

impl LiveBoard {
    pub fn my_kills(&self) -> u32 {
        if self.my_team_is_order {
            self.order_kills
        } else {
            self.chaos_kills
        }
    }

    pub fn enemy_kills(&self) -> u32 {
        if self.my_team_is_order {
            self.chaos_kills
        } else {
            self.order_kills
        }
    }
}

struct Roster<'a> {
    live: &'a LiveGameSummary,
}

impl Roster<'_> {
    /// Resolves an event actor name to (is_order, lane, is_self, champion).
    fn lookup(&self, name: &str) -> Option<(bool, Option<Lane>, bool, String)> {
        let p = self.live.players.iter().find(|p| {
            p.riot_id == name
                || p.riot_id.split('#').next() == Some(name)
                || p.champion_name == name
        })?;
        Some((
            p.team == "ORDER",
            Lane::from_position(&p.position),
            p.is_self,
            p.champion_name.clone(),
        ))
    }

    /// Team of a non-player actor (minions, turrets): T100 / T1 = ORDER, T200 / T2 = CHAOS.
    fn structure_team(name: &str) -> Option<bool> {
        if name.contains("T100") || name.contains("_T1_") {
            Some(true)
        } else if name.contains("T200") || name.contains("_T2_") {
            Some(false)
        } else {
            None
        }
    }

    /// Index into the roster of a champion actor, for sightings.
    fn index_of(&self, name: &str) -> Option<usize> {
        self.live.players.iter().position(|p| {
            p.riot_id == name
                || p.riot_id.split('#').next() == Some(name)
                || p.champion_name == name
        })
    }

    fn team_of(&self, name: &str) -> Option<bool> {
        self.lookup(name)
            .map(|(order, ..)| order)
            .or_else(|| Self::structure_team(name))
    }
}

/// Parses `Barracks_T1_L1` into (order_side, lane).
pub fn parse_inhibitor_id(id: &str) -> Option<(bool, Lane)> {
    let parts: Vec<&str> = id.split('_').collect();
    if parts.len() < 3 || parts[0] != "Barracks" {
        return None;
    }
    let order_side = match parts[1] {
        "T1" => true,
        "T2" => false,
        _ => return None,
    };
    let lane = match parts[2].chars().next()? {
        'L' => Lane::Top,
        'C' => Lane::Mid,
        'R' => Lane::Bot,
        _ => return None,
    };
    Some((order_side, lane))
}

/// Next monster of each pit at `now`, from the epic monsters taken so far (in time order).
pub fn pit_timers(objectives: &[ObjectiveTaken], grubs_taken: bool, now: f64) -> Vec<PitTimer> {
    use rift_schedule::*;

    let mut drakes = [0usize; 2];
    let mut soul_at = None;
    let mut last_drake = None;
    let mut last_elder = None;
    let mut last_baron = None;
    let mut herald_taken = false;
    for obj in objectives {
        match obj.kind {
            ObjectiveKind::Drake => {
                last_drake = Some(obj.time_sec);
                let team = &mut drakes[usize::from(!obj.by_order)];
                *team += 1;
                if *team == DRAKES_FOR_SOUL && soul_at.is_none() {
                    soul_at = Some(obj.time_sec);
                }
            }
            ObjectiveKind::Elder => last_elder = Some(obj.time_sec),
            ObjectiveKind::Baron => last_baron = Some(obj.time_sec),
            ObjectiveKind::Herald => herald_taken = true,
            ObjectiveKind::Grubs => {}
        }
    }

    let dragon = match soul_at {
        Some(soul) => PitTimer {
            pit: Pit::Dragon,
            kind: ObjectiveKind::Elder,
            spawn_sec: last_elder.unwrap_or(soul) + ELDER_RESPAWN,
        },
        None => PitTimer {
            pit: Pit::Dragon,
            kind: ObjectiveKind::Drake,
            spawn_sec: last_drake.map_or(DRAKE_FIRST, |t| t + DRAKE_RESPAWN),
        },
    };
    let baron = if !grubs_taken && now < GRUBS_LEAVE {
        PitTimer {
            pit: Pit::Baron,
            kind: ObjectiveKind::Grubs,
            spawn_sec: GRUBS_SPAWN,
        }
    } else if !herald_taken && now < HERALD_LEAVE {
        PitTimer {
            pit: Pit::Baron,
            kind: ObjectiveKind::Herald,
            spawn_sec: HERALD_SPAWN,
        }
    } else {
        PitTimer {
            pit: Pit::Baron,
            kind: ObjectiveKind::Baron,
            spawn_sec: last_baron.map_or(BARON_FIRST, |t| t + BARON_RESPAWN),
        }
    };
    vec![dragon, baron]
}

/// Parses `Turret_T1_L_03_A` into (order_side, lane, tier). Nexus towers return None.
pub fn parse_turret_id(id: &str) -> Option<(bool, Lane, u8)> {
    let parts: Vec<&str> = id.split('_').collect();
    if parts.len() < 4 || parts[0] != "Turret" {
        return None;
    }
    let order_side = match parts[1] {
        "T1" => true,
        "T2" => false,
        _ => return None,
    };
    let num: u8 = parts[3].parse().ok()?;
    let (lane, tier) = match parts[2] {
        "L" => (Lane::Top, 4u8.checked_sub(num)?),
        "R" => (Lane::Bot, 4u8.checked_sub(num)?),
        "C" => (Lane::Mid, 6u8.checked_sub(num)?),
        _ => return None,
    };
    (1..=3).contains(&tier).then_some((order_side, lane, tier))
}

fn tier_name(tier: u8, lang: Lang) -> &'static str {
    match (tier, lang) {
        (1, Lang::Fr) => "extérieure",
        (2, Lang::Fr) => "intérieure",
        (_, Lang::Fr) => "inhibitrice",
        (1, Lang::En) => "outer",
        (2, Lang::En) => "inner",
        (_, Lang::En) => "inhibitor",
    }
}

fn lane_name(lane: Lane) -> &'static str {
    match lane {
        Lane::Top => "top",
        Lane::Jungle => "jungle",
        Lane::Mid => "mid",
        Lane::Bot => "bot",
    }
}

fn drake_label(dragon_type: &str, lang: Lang) -> String {
    match (dragon_type, lang) {
        ("Fire", Lang::Fr) => "Dragon infernal",
        ("Water", Lang::Fr) => "Dragon des océans",
        ("Earth", Lang::Fr) => "Dragon des montagnes",
        ("Air", Lang::Fr) => "Dragon des nuages",
        ("Hextech", Lang::Fr) => "Dragon Hextech",
        ("Chemtech", Lang::Fr) => "Dragon Techno-chimique",
        ("Elder", Lang::Fr) => "Dragon ancestral",
        (_, Lang::Fr) => "Dragon",
        ("Fire", Lang::En) => "Infernal Drake",
        ("Water", Lang::En) => "Ocean Drake",
        ("Earth", Lang::En) => "Mountain Drake",
        ("Air", Lang::En) => "Cloud Drake",
        ("Hextech", Lang::En) => "Hextech Drake",
        ("Chemtech", Lang::En) => "Chemtech Drake",
        ("Elder", Lang::En) => "Elder Dragon",
        (_, Lang::En) => "Drake",
    }
    .to_string()
}

/// A killer that is not a champion, in words: `Turret_T1_C_05_A` is "A tower", a minion wave
/// "Minions". Anything else keeps Riot's name.
fn structure_killer(name: &str, lang: Lang) -> String {
    if name.starts_with("Turret") {
        lang.pick("A tower", "Une tourelle").to_string()
    } else if name.starts_with("Minion") {
        lang.pick("Minions", "Des sbires").to_string()
    } else {
        name.to_string()
    }
}

fn objective_article(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::Grubs => "les",
        _ => "le",
    }
}

/// Builds the board from the raw Live Client payloads and the extracted summary in `lang`.
pub fn extract_live_board_localized(
    allgame_val: Option<&Value>,
    events_val: Option<&Value>,
    live: &LiveGameSummary,
    lang: Lang,
) -> LiveBoard {
    let roster = Roster { live };
    let my_team_is_order = live
        .players
        .iter()
        .find(|p| p.is_self)
        .map(|p| p.team == "ORDER")
        .unwrap_or(true);

    let mut towers = Vec::new();
    for order_side in [true, false] {
        for lane in [Lane::Top, Lane::Mid, Lane::Bot] {
            for tier in 1..=3 {
                towers.push(TowerState {
                    order_side,
                    lane,
                    tier,
                    standing: true,
                });
            }
        }
    }

    let events = events_val
        .and_then(|ev| ev.get("Events"))
        .or_else(|| {
            allgame_val
                .and_then(|ag| ag.get("events"))
                .and_then(|e| e.get("Events"))
        })
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut objectives = Vec::new();
    let mut feed = Vec::new();
    let (mut ev_order_kills, mut ev_chaos_kills) = (0u32, 0u32);
    let mine = |order: bool| order == my_team_is_order;
    // Only Summoner's Rift has these places and this monster schedule.
    let on_rift = live.game_mode == "CLASSIC";
    // Latest sighting per roster slot.
    let mut spotted: Vec<Option<(MapPlace, f64)>> = vec![None; live.players.len()];
    let mut grubs_taken = false;

    for ev in &events {
        let name = ev.get("EventName").and_then(|v| v.as_str()).unwrap_or("");
        let time_sec = ev.get("EventTime").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let killer = ev.get("KillerName").and_then(|v| v.as_str()).unwrap_or("");
        let killer_info = roster.lookup(killer);
        let killer_order = roster.team_of(killer);
        // Everyone the event credits: the killer, then the assisters.
        let mut spot = |place: MapPlace, names: &mut dyn Iterator<Item = &str>| {
            if !on_rift {
                return;
            }
            for actor in names {
                if let Some(i) = roster.index_of(actor) {
                    let newer = spotted[i].is_none_or(|(_, t)| time_sec >= t);
                    if newer {
                        spotted[i] = Some((place, time_sec));
                    }
                }
            }
        };
        let assisters: Vec<&str> = ev
            .get("Assisters")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let mut credited = std::iter::once(killer).chain(assisters.iter().copied());

        match name {
            "ChampionKill" => {
                let victim = ev.get("VictimName").and_then(|v| v.as_str()).unwrap_or("");
                let victim_info = roster.lookup(victim);
                // Killed by a turret: the victim stood under it.
                if let Some((order_side, lane, tier)) = parse_turret_id(killer) {
                    spot(
                        MapPlace::Turret {
                            order_side,
                            lane,
                            tier,
                        },
                        &mut std::iter::once(victim),
                    );
                }
                match killer_order {
                    Some(true) => ev_order_kills += 1,
                    Some(false) => ev_chaos_kills += 1,
                    None => {}
                }
                let killer_is_self = matches!(&killer_info, Some((_, _, true, _)));
                let victim_is_self = matches!(&victim_info, Some((_, _, true, _)));
                let whom = match &victim_info {
                    Some((_, _, true, _)) => lang.pick("you", "vous").to_string(),
                    Some((.., champ)) => champ.clone(),
                    None => victim.to_string(),
                };
                let text = if lang.is_fr() {
                    if killer_is_self {
                        format!("Vous avez éliminé {whom}")
                    } else {
                        let who = match &killer_info {
                            Some((.., champ)) => champ.clone(),
                            None => structure_killer(killer, lang),
                        };
                        // French puts the object pronoun before the verb.
                        let verb = if killer.starts_with("Minion") {
                            "ont"
                        } else {
                            "a"
                        };
                        if victim_is_self {
                            format!("{who} vous {verb} éliminé")
                        } else {
                            format!("{who} {verb} éliminé {whom}")
                        }
                    }
                } else {
                    let who = match &killer_info {
                        Some((_, _, true, _)) => "You".to_string(),
                        Some((.., champ)) => champ.clone(),
                        None => structure_killer(killer, lang),
                    };
                    format!("{who} killed {whom}")
                };
                feed.push(FeedItem {
                    time_sec,
                    text,
                    lane: killer_info.as_ref().and_then(|k| k.1),
                    by_my_team: killer_order.map(mine),
                });
            }
            "TurretKilled" => {
                let turret = ev
                    .get("TurretKilled")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some((order_side, lane, tier)) = parse_turret_id(turret) {
                    spot(
                        MapPlace::Turret {
                            order_side,
                            lane,
                            tier,
                        },
                        &mut credited,
                    );
                    if let Some(t) = towers
                        .iter_mut()
                        .find(|t| t.order_side == order_side && t.lane == lane && t.tier == tier)
                    {
                        t.standing = false;
                    }
                    let text = if lang.is_fr() {
                        if mine(order_side) {
                            format!(
                                "Votre tourelle {} {} est tombée",
                                tier_name(tier, lang),
                                lane_name(lane)
                            )
                        } else {
                            format!(
                                "Votre équipe a détruit la tourelle {} {}",
                                tier_name(tier, lang),
                                lane_name(lane)
                            )
                        }
                    } else if mine(order_side) {
                        format!(
                            "Your {} {} tower fell",
                            lane_name(lane),
                            tier_name(tier, lang)
                        )
                    } else {
                        format!(
                            "Your team destroyed their {} {} tower",
                            lane_name(lane),
                            tier_name(tier, lang)
                        )
                    };
                    feed.push(FeedItem {
                        time_sec,
                        text,
                        lane: Some(lane),
                        by_my_team: Some(!mine(order_side)),
                    });
                }
            }
            "InhibKilled" => {
                let inhib = ev.get("InhibKilled").and_then(|v| v.as_str()).unwrap_or("");
                if let Some((order_side, lane)) = parse_inhibitor_id(inhib) {
                    spot(MapPlace::Inhibitor { order_side, lane }, &mut credited);
                }
            }
            "DragonKill" | "HeraldKill" | "BaronKill" | "HordeKill" => {
                let pit = if name == "DragonKill" {
                    MapPlace::DragonPit
                } else {
                    MapPlace::BaronPit
                };
                spot(pit, &mut credited);
                grubs_taken |= name == "HordeKill";
                let (kind, label) = match name {
                    "DragonKill" => {
                        let dt = ev.get("DragonType").and_then(|v| v.as_str()).unwrap_or("");
                        let kind = if dt == "Elder" {
                            ObjectiveKind::Elder
                        } else {
                            ObjectiveKind::Drake
                        };
                        (kind, drake_label(dt, lang))
                    }
                    "HeraldKill" => (
                        ObjectiveKind::Herald,
                        lang.pick("Rift Herald", "Héraut de la Faille").to_string(),
                    ),
                    "BaronKill" => (ObjectiveKind::Baron, "Baron Nashor".to_string()),
                    _ => (
                        ObjectiveKind::Grubs,
                        lang.pick("Void Grubs", "Larves du Néant").to_string(),
                    ),
                };
                if let Some(by_order) = killer_order {
                    let stolen = ev.get("Stolen").and_then(|v| v.as_str()) == Some("True");
                    let text = if lang.is_fr() {
                        let who = if mine(by_order) {
                            "Votre équipe"
                        } else {
                            "L'ennemi"
                        };
                        let verb = if stolen { "a volé" } else { "a pris" };
                        let article = objective_article(kind);
                        format!("{who} {verb} {article} {label}")
                    } else {
                        let who = if mine(by_order) { "Your team" } else { "Enemy" };
                        let verb = if stolen { "stole" } else { "took" };
                        format!("{who} {verb} {label}")
                    };
                    feed.push(FeedItem {
                        time_sec,
                        text,
                        lane: Some(Lane::Jungle),
                        by_my_team: Some(mine(by_order)),
                    });
                    objectives.push(ObjectiveTaken {
                        kind,
                        label,
                        by_order,
                        time_sec,
                    });
                }
            }
            "FirstBlood" => {
                let recipient = ev.get("Recipient").and_then(|v| v.as_str()).unwrap_or("");
                if let Some((order, lane, is_self, champ)) = roster.lookup(recipient) {
                    let who = if is_self {
                        lang.pick("You", "Vous").to_string()
                    } else {
                        champ
                    };
                    let text = if lang.is_fr() {
                        format!("Premier sang : {who}")
                    } else {
                        format!("First blood: {who}")
                    };
                    feed.push(FeedItem {
                        time_sec,
                        text,
                        lane,
                        by_my_team: Some(mine(order)),
                    });
                }
            }
            "Ace" => {
                let acing = ev.get("AcingTeam").and_then(|v| v.as_str()).unwrap_or("");
                let order = acing == "ORDER";
                let text = if lang.is_fr() {
                    if mine(order) {
                        "Ace ! Votre équipe a éliminé l'ennemi"
                    } else {
                        "Ace : votre équipe a été éliminée"
                    }
                } else if mine(order) {
                    "Ace! Your team wiped the enemy"
                } else {
                    "Ace: your team was wiped"
                };
                feed.push(FeedItem {
                    time_sec,
                    text: text.to_string(),
                    lane: None,
                    by_my_team: Some(mine(order)),
                });
            }
            _ => {}
        }
    }

    feed.sort_by(|a, b| {
        b.time_sec
            .partial_cmp(&a.time_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    feed.truncate(12);

    let (mut order_kills, mut chaos_kills) = (0u32, 0u32);
    for p in &live.players {
        if p.team == "ORDER" {
            order_kills += p.kills;
        } else {
            chaos_kills += p.kills;
        }
    }
    if live.players.is_empty() {
        order_kills = ev_order_kills;
        chaos_kills = ev_chaos_kills;
    }

    let sightings = live
        .players
        .iter()
        .zip(spotted)
        .filter(|(p, _)| (p.team == "ORDER") != my_team_is_order)
        .filter_map(|(p, seen)| {
            let (place, time_sec) = seen?;
            Some(Sighting {
                champion: p.champion_name.clone(),
                champion_id: p.champion_id,
                place,
                time_sec,
            })
        })
        .collect();

    let pits = if on_rift {
        let mut taken = objectives.clone();
        taken.sort_by(|a, b| {
            a.time_sec
                .partial_cmp(&b.time_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        pit_timers(&taken, grubs_taken, live.game_time_sec)
    } else {
        Vec::new()
    };

    let active_buffs = if on_rift {
        compute_active_buffs(&objectives, &events, live)
    } else {
        Vec::new()
    };

    LiveBoard {
        my_team_is_order,
        order_kills,
        chaos_kills,
        towers,
        objectives,
        feed,
        sightings,
        pits,
        active_buffs,
    }
}

/// Computes active Baron (180s) and Elder (150s) buffs on Summoner's Rift, excluding holders
/// who were dead when the objective fell or died afterward.
fn compute_active_buffs(
    objectives: &[ObjectiveTaken],
    events: &[Value],
    live: &LiveGameSummary,
) -> Vec<ActiveBuff> {
    let now = live.game_time_sec;
    let mut out = Vec::new();
    for kind in [ObjectiveKind::Baron, ObjectiveKind::Elder] {
        let duration = match kind {
            ObjectiveKind::Baron => rift_schedule::BARON_BUFF_SEC,
            ObjectiveKind::Elder => rift_schedule::ELDER_BUFF_SEC,
            _ => continue,
        };
        let Some(latest) = objectives
            .iter()
            .filter(|o| o.kind == kind && o.time_sec <= now && now < o.time_sec + duration)
            .max_by(|a, b| {
                a.time_sec
                    .partial_cmp(&b.time_sec)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            continue;
        };
        let taken_at = latest.time_sec;
        let expires_at_sec = taken_at + duration;
        let remaining_sec = (expires_at_sec - now).max(0.0);

        let team_players: Vec<&crate::summary::LiveGamePlayerSummary> = live
            .players
            .iter()
            .filter(|p| (p.team == "ORDER") == latest.by_order)
            .collect();
        let holders_alive = if team_players.is_empty() {
            5u8
        } else {
            let mut alive = 0u8;
            for p in team_players {
                let matches_victim = |victim: &str| {
                    p.riot_id == victim
                        || p.riot_id.split('#').next() == Some(victim)
                        || p.champion_name == victim
                };
                // Died after the objective was taken -> lost the buff.
                let died_after = events.iter().any(|ev| {
                    ev.get("EventName").and_then(|v| v.as_str()) == Some("ChampionKill")
                        && ev
                            .get("EventTime")
                            .and_then(|v| v.as_f64())
                            .is_some_and(|t| t >= taken_at && t <= now)
                        && ev
                            .get("VictimName")
                            .and_then(|v| v.as_str())
                            .is_some_and(matches_victim)
                });
                // Died shortly before the objective was taken (~25s respawn window) -> never got it.
                let dead_at_kill = events.iter().any(|ev| {
                    ev.get("EventName").and_then(|v| v.as_str()) == Some("ChampionKill")
                        && ev
                            .get("EventTime")
                            .and_then(|v| v.as_f64())
                            .is_some_and(|t| t < taken_at && taken_at - t <= 25.0)
                        && ev
                            .get("VictimName")
                            .and_then(|v| v.as_str())
                            .is_some_and(matches_victim)
                });
                if !died_after && !dead_at_kill {
                    alive = alive.saturating_add(1);
                }
            }
            alive
        };

        if holders_alive > 0 {
            out.push(ActiveBuff {
                kind,
                by_order: latest.by_order,
                expires_at_sec,
                remaining_sec,
                holders_alive,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::champions::{BuiltinChampionCatalog, ChampionLookup};
    use crate::summary::LiveGamePlayerSummary;
    use serde_json::json;

    fn player(
        id: &str,
        champ: &str,
        team: &str,
        pos: &str,
        kills: u32,
        is_self: bool,
    ) -> LiveGamePlayerSummary {
        LiveGamePlayerSummary {
            riot_id: id.to_string(),
            champion_id: BuiltinChampionCatalog.resolve_champion_id(champ),
            champion_name: champ.to_string(),
            champion_alias: String::new(),
            team: team.to_string(),
            position: pos.to_string(),
            level: 10,
            kills,
            deaths: 0,
            assists: 0,
            creep_score: 0,
            cs_per_min: 0.0,
            cs_diff_vs_lane: None,
            is_self,
            is_dead: false,
            respawn_timer_sec: 0.0,
            skin_id: 0,
        }
    }

    fn live(players: Vec<LiveGamePlayerSummary>) -> LiveGameSummary {
        LiveGameSummary {
            game_mode: "CLASSIC".into(),
            map_name: "Summoner's Rift".into(),
            game_time_sec: 900.0,
            active_champion_id: BuiltinChampionCatalog.resolve_champion_id("Ahri"),
            active_champion: "Ahri".into(),
            level: 10,
            current_gold: 0,
            kills: 0,
            deaths: 0,
            assists: 0,
            creep_score: 0,
            self_cs_per_min: 0.0,
            lane_cs_diff: None,
            personal_avg_cs_per_min: None,
            recent_events: vec![],
            players,
        }
    }

    #[test]
    fn parses_turret_ids() {
        assert_eq!(
            parse_turret_id("Turret_T1_L_03_A"),
            Some((true, Lane::Top, 1))
        );
        assert_eq!(
            parse_turret_id("Turret_T2_C_04_A"),
            Some((false, Lane::Mid, 2))
        );
        assert_eq!(
            parse_turret_id("Turret_T1_R_01_A"),
            Some((true, Lane::Bot, 3))
        );
        assert_eq!(parse_turret_id("Turret_T2_C_01_A"), None);
    }

    #[test]
    fn builds_board_from_my_teams_perspective() {
        let l = live(vec![
            player("Ezo#EUW", "Ahri", "CHAOS", "MIDDLE", 4, true),
            player("Foe#EUW", "Zed", "ORDER", "MIDDLE", 2, false),
        ]);
        let events = json!({"Events": [
            {"EventName": "ChampionKill", "EventTime": 600.0, "KillerName": "Ezo", "VictimName": "Foe"},
            {"EventName": "TurretKilled", "EventTime": 700.0, "TurretKilled": "Turret_T2_R_03_A", "KillerName": "Minion_T100L0S1N0001"},
            {"EventName": "DragonKill", "EventTime": 800.0, "DragonType": "Fire", "KillerName": "Ezo", "Stolen": "False"}
        ]});
        let board = extract_live_board_localized(None, Some(&events), &l, Lang::En);
        assert!(!board.my_team_is_order);
        assert_eq!(board.my_kills(), 4);
        assert_eq!(board.enemy_kills(), 2);
        assert!(
            !board
                .towers
                .iter()
                .find(|t| !t.order_side && t.lane == Lane::Bot && t.tier == 1)
                .unwrap()
                .standing
        );
        assert_eq!(board.objectives.len(), 1);
        assert_eq!(board.feed[0].text, "Your team took Infernal Drake");
        assert_eq!(board.feed[1].text, "Your bot outer tower fell");
        assert_eq!(board.feed[2].text, "You killed Zed");

        let board_fr = extract_live_board_localized(None, Some(&events), &l, Lang::Fr);
        assert_eq!(
            board_fr.feed[0].text,
            "Votre équipe a pris le Dragon infernal"
        );
        assert_eq!(
            board_fr.feed[1].text,
            "Votre tourelle extérieure bot est tombée"
        );
        assert_eq!(board_fr.feed[2].text, "Vous avez éliminé Zed");
    }

    #[test]
    fn a_kill_on_the_player_reads_naturally_in_french() {
        let l = live(vec![
            player("Ezo#EUW", "Ahri", "CHAOS", "MIDDLE", 0, true),
            player("Foe#EUW", "Zed", "ORDER", "MIDDLE", 1, false),
        ]);
        let events = json!({"Events": [
            {"EventName": "ChampionKill", "EventTime": 600.0, "KillerName": "Foe", "VictimName": "Ezo"}
        ]});
        let fr = extract_live_board_localized(None, Some(&events), &l, Lang::Fr);
        assert_eq!(fr.feed[0].text, "Zed vous a éliminé");
        let en = extract_live_board_localized(None, Some(&events), &l, Lang::En);
        assert_eq!(en.feed[0].text, "Zed killed you");
    }
    #[test]
    fn a_tower_or_minion_kill_reads_as_words_not_ids() {
        let l = live(vec![
            player("Ezo#EUW", "Ahri", "ORDER", "MIDDLE", 0, true),
            player("Foe#EUW", "Zed", "CHAOS", "MIDDLE", 0, false),
        ]);
        let events = json!({"Events": [
            {"EventName": "ChampionKill", "EventTime": 600.0, "KillerName": "Turret_T1_C_05_A", "VictimName": "Foe"},
            {"EventName": "ChampionKill", "EventTime": 700.0, "KillerName": "Minion_T200L1S04N0007", "VictimName": "Ezo"}
        ]});
        let en = extract_live_board_localized(None, Some(&events), &l, Lang::En);
        assert_eq!(en.feed[0].text, "Minions killed you");
        assert_eq!(en.feed[1].text, "A tower killed Zed");
        let fr = extract_live_board_localized(None, Some(&events), &l, Lang::Fr);
        assert_eq!(fr.feed[0].text, "Des sbires vous ont éliminé");
        assert_eq!(fr.feed[1].text, "Une tourelle a éliminé Zed");
    }

    #[test]
    fn parses_inhibitor_ids() {
        assert_eq!(
            parse_inhibitor_id("Barracks_T1_L1"),
            Some((true, Lane::Top))
        );
        assert_eq!(
            parse_inhibitor_id("Barracks_T2_C1"),
            Some((false, Lane::Mid))
        );
        assert_eq!(
            parse_inhibitor_id("Barracks_T2_R1"),
            Some((false, Lane::Bot))
        );
        assert_eq!(parse_inhibitor_id("Turret_T1_L_03_A"), None);
    }

    #[test]
    fn spots_each_enemy_at_the_latest_fixed_place_they_were_credited() {
        let l = live(vec![
            player("Ezo#EUW", "Ahri", "ORDER", "MIDDLE", 0, true),
            player("Ally#EUW", "Garen", "ORDER", "TOP", 0, false),
            player("Foe#EUW", "Zed", "CHAOS", "MIDDLE", 0, false),
            player("Jg#EUW", "Viego", "CHAOS", "JUNGLE", 0, false),
            player("Adc#EUW", "Jinx", "CHAOS", "BOTTOM", 0, false),
            player("Top#EUW", "Darius", "CHAOS", "TOP", 0, false),
        ]);
        let events = json!({"Events": [
            {"EventName": "DragonKill", "EventTime": 400.0, "DragonType": "Fire", "KillerName": "Jg", "Assisters": ["Foe"]},
            {"EventName": "TurretKilled", "EventTime": 500.0, "TurretKilled": "Turret_T1_C_05_A", "KillerName": "Foe", "Assisters": []},
            {"EventName": "ChampionKill", "EventTime": 520.0, "KillerName": "Turret_T1_R_03_A", "VictimName": "Adc"},
            {"EventName": "ChampionKill", "EventTime": 530.0, "KillerName": "Top", "VictimName": "Ally"},
            {"EventName": "HeraldKill", "EventTime": 540.0, "KillerName": "Ally", "Assisters": ["Ezo"]}
        ]});
        let board = extract_live_board_localized(None, Some(&events), &l, Lang::En);
        let seen: Vec<(&str, MapPlace, f64)> = board
            .sightings
            .iter()
            .map(|s| (s.champion.as_str(), s.place, s.time_sec))
            .collect();
        assert_eq!(
            seen,
            vec![
                (
                    "Zed",
                    MapPlace::Turret {
                        order_side: true,
                        lane: Lane::Mid,
                        tier: 1
                    },
                    500.0
                ),
                ("Viego", MapPlace::DragonPit, 400.0),
                (
                    "Jinx",
                    MapPlace::Turret {
                        order_side: true,
                        lane: Lane::Bot,
                        tier: 1
                    },
                    520.0
                ),
            ],
            "a kill places no one, allies are never listed, the latest place wins"
        );
    }

    #[test]
    fn no_sightings_or_timers_off_summoners_rift() {
        let mut l = live(vec![
            player("Ezo#EUW", "Ahri", "ORDER", "MIDDLE", 0, true),
            player("Jg#EUW", "Viego", "CHAOS", "JUNGLE", 0, false),
        ]);
        l.game_mode = "ARAM".into();
        let events = json!({"Events": [
            {"EventName": "TurretKilled", "EventTime": 500.0, "TurretKilled": "Turret_T1_C_05_A", "KillerName": "Jg"}
        ]});
        let board = extract_live_board_localized(None, Some(&events), &l, Lang::En);
        assert!(board.sightings.is_empty());
        assert!(board.pits.is_empty());
    }

    fn taken(kind: ObjectiveKind, by_order: bool, time_sec: f64) -> ObjectiveTaken {
        ObjectiveTaken {
            kind,
            label: String::new(),
            by_order,
            time_sec,
        }
    }

    #[test]
    fn pit_timers_follow_the_rift_schedule() {
        use ObjectiveKind::*;
        let at = |objs: &[ObjectiveTaken], grubs: bool, now: f64| {
            pit_timers(objs, grubs, now)
                .iter()
                .map(|p| (p.kind, p.spawn_sec))
                .collect::<Vec<_>>()
        };
        assert_eq!(at(&[], false, 60.0), vec![(Drake, 300.0), (Grubs, 480.0)]);
        // Grubs taken, or gone at 14:45: Herald is next in the top pit.
        assert_eq!(at(&[], true, 600.0)[1], (Herald, 900.0));
        assert_eq!(at(&[], false, 885.0)[1], (Herald, 900.0));
        // Herald taken, or gone at 19:45: Baron at 20:00, then 6 minutes after each kill.
        assert_eq!(
            at(&[taken(Herald, true, 950.0)], true, 960.0)[1],
            (Baron, 1200.0)
        );
        assert_eq!(at(&[], true, 1185.0)[1], (Baron, 1200.0));
        assert_eq!(
            at(&[taken(Baron, false, 1300.0)], true, 1310.0)[1],
            (Baron, 1660.0)
        );
        // A drake respawns 5 minutes after the last one.
        assert_eq!(
            at(&[taken(Drake, true, 330.0)], false, 340.0)[0],
            (Drake, 630.0)
        );
        // The fourth drake of one team claims the soul: Elder 6 minutes later, and after each kill.
        let soul = [
            taken(Drake, true, 300.0),
            taken(Drake, false, 600.0),
            taken(Drake, true, 900.0),
            taken(Drake, true, 1200.0),
            taken(Drake, true, 1500.0),
        ];
        assert_eq!(at(&soul, true, 1510.0)[0], (Elder, 1860.0));
        let mut elder = soul.to_vec();
        elder.push(taken(Elder, false, 1900.0));
        assert_eq!(at(&elder, true, 1910.0)[0], (Elder, 2260.0));
    }

    #[test]
    fn active_buffs_track_baron_elder_and_post_kill_deaths() {
        let mut live = live(vec![
            player("Me#EUW", "Ahri", "ORDER", "MIDDLE", 4, true),
            player("AllyTop#EUW", "Aatrox", "ORDER", "TOP", 2, false),
            player("EnemyMid#EUW", "Syndra", "CHAOS", "MIDDLE", 3, false),
            player("EnemyTop#EUW", "Darius", "CHAOS", "TOP", 1, false),
        ]);
        live.game_time_sec = 1400.0;
        let events = json!({
            "Events": [
                { "EventName": "BaronKill", "EventTime": 1300.0, "KillerName": "Me#EUW" },
                { "EventName": "ChampionKill", "EventTime": 1350.0, "KillerName": "EnemyMid#EUW", "VictimName": "Me#EUW" },
                { "EventName": "DragonKill", "EventTime": 1310.0, "DragonType": "Elder", "KillerName": "EnemyMid#EUW" }
            ]
        });
        let board = extract_live_board_localized(None, Some(&events), &live, Lang::En);
        assert_eq!(board.active_buffs.len(), 2);
        let baron = board
            .active_buffs
            .iter()
            .find(|b| b.kind == ObjectiveKind::Baron)
            .expect("baron buff active");
        assert!(baron.by_order);
        assert!((baron.remaining_sec - 80.0).abs() < 1e-3);
        let order_count = live.players.iter().filter(|p| p.team == "ORDER").count() as u8;
        assert_eq!(
            baron.holders_alive,
            order_count.saturating_sub(1),
            "post-baron death removes holder"
        );
        let elder = board
            .active_buffs
            .iter()
            .find(|b| b.kind == ObjectiveKind::Elder)
            .expect("elder buff active");
        assert!(!elder.by_order);
        assert!((elder.remaining_sec - 60.0).abs() < 1e-3);
    }
}
