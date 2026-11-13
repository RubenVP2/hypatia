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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LiveBoard {
    pub my_team_is_order: bool,
    pub order_kills: u32,
    pub chaos_kills: u32,
    pub towers: Vec<TowerState>,
    pub objectives: Vec<ObjectiveTaken>,
    /// Newest first.
    pub feed: Vec<FeedItem>,
}

impl LiveBoard {
    pub fn my_kills(&self) -> u32 {
        if self.my_team_is_order { self.order_kills } else { self.chaos_kills }
    }

    pub fn enemy_kills(&self) -> u32 {
        if self.my_team_is_order { self.chaos_kills } else { self.order_kills }
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

    fn team_of(&self, name: &str) -> Option<bool> {
        self.lookup(name)
            .map(|(order, ..)| order)
            .or_else(|| Self::structure_team(name))
    }
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
                towers.push(TowerState { order_side, lane, tier, standing: true });
            }
        }
    }

    let events = events_val
        .and_then(|ev| ev.get("Events"))
        .or_else(|| allgame_val.and_then(|ag| ag.get("events")).and_then(|e| e.get("Events")))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut objectives = Vec::new();
    let mut feed = Vec::new();
    let (mut ev_order_kills, mut ev_chaos_kills) = (0u32, 0u32);
    let mine = |order: bool| order == my_team_is_order;

    for ev in &events {
        let name = ev.get("EventName").and_then(|v| v.as_str()).unwrap_or("");
        let time_sec = ev.get("EventTime").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let killer = ev.get("KillerName").and_then(|v| v.as_str()).unwrap_or("");
        let killer_info = roster.lookup(killer);
        let killer_order = roster.team_of(killer);

        match name {
            "ChampionKill" => {
                let victim = ev.get("VictimName").and_then(|v| v.as_str()).unwrap_or("");
                let victim_info = roster.lookup(victim);
                match killer_order {
                    Some(true) => ev_order_kills += 1,
                    Some(false) => ev_chaos_kills += 1,
                    None => {}
                }
                let killer_is_self = matches!(&killer_info, Some((_, _, true, _)));
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
                            None => killer.to_string(),
                        };
                        format!("{who} a éliminé {whom}")
                    }
                } else {
                    let who = match &killer_info {
                        Some((_, _, true, _)) => "You".to_string(),
                        Some((.., champ)) => champ.clone(),
                        None => killer.to_string(),
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
                let turret = ev.get("TurretKilled").and_then(|v| v.as_str()).unwrap_or("");
                if let Some((order_side, lane, tier)) = parse_turret_id(turret) {
                    if let Some(t) = towers
                        .iter_mut()
                        .find(|t| t.order_side == order_side && t.lane == lane && t.tier == tier)
                    {
                        t.standing = false;
                    }
                    let text = if lang.is_fr() {
                        if mine(order_side) {
                            format!("Votre tourelle {} {} est tombée", tier_name(tier, lang), lane_name(lane))
                        } else {
                            format!("Votre équipe a détruit la tourelle {} {}", tier_name(tier, lang), lane_name(lane))
                        }
                    } else if mine(order_side) {
                        format!("Your {} {} tower fell", lane_name(lane), tier_name(tier, lang))
                    } else {
                        format!("Your team destroyed their {} {} tower", lane_name(lane), tier_name(tier, lang))
                    };
                    feed.push(FeedItem {
                        time_sec,
                        text,
                        lane: Some(lane),
                        by_my_team: Some(!mine(order_side)),
                    });
                }
            }
            "DragonKill" | "HeraldKill" | "BaronKill" | "HordeKill" => {
                let (kind, label) = match name {
                    "DragonKill" => {
                        let dt = ev.get("DragonType").and_then(|v| v.as_str()).unwrap_or("");
                        let kind = if dt == "Elder" { ObjectiveKind::Elder } else { ObjectiveKind::Drake };
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
                        let who = if mine(by_order) { "Votre équipe" } else { "L'ennemi" };
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
                    objectives.push(ObjectiveTaken { kind, label, by_order, time_sec });
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
                    if mine(order) { "Ace ! Votre équipe a éliminé l'ennemi" } else { "Ace : votre équipe a été éliminée" }
                } else if mine(order) {
                    "Ace! Your team wiped the enemy"
                } else {
                    "Ace: your team was wiped"
                };
                feed.push(FeedItem { time_sec, text: text.to_string(), lane: None, by_my_team: Some(mine(order)) });
            }
            _ => {}
        }
    }

    feed.sort_by(|a, b| b.time_sec.partial_cmp(&a.time_sec).unwrap_or(std::cmp::Ordering::Equal));
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

    LiveBoard { my_team_is_order, order_kills, chaos_kills, towers, objectives, feed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::champions::{BuiltinChampionCatalog, ChampionLookup};
    use crate::summary::LiveGamePlayerSummary;
    use serde_json::json;

    fn player(id: &str, champ: &str, team: &str, pos: &str, kills: u32, is_self: bool) -> LiveGamePlayerSummary {
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
            is_self,
            is_dead: false,
            respawn_timer_sec: 0.0,
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
            recent_events: vec![],
            players,
        }
    }

    #[test]
    fn parses_turret_ids() {
        assert_eq!(parse_turret_id("Turret_T1_L_03_A"), Some((true, Lane::Top, 1)));
        assert_eq!(parse_turret_id("Turret_T2_C_04_A"), Some((false, Lane::Mid, 2)));
        assert_eq!(parse_turret_id("Turret_T1_R_01_A"), Some((true, Lane::Bot, 3)));
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
        assert!(!board.towers.iter().find(|t| !t.order_side && t.lane == Lane::Bot && t.tier == 1).unwrap().standing);
        assert_eq!(board.objectives.len(), 1);
        assert_eq!(board.feed[0].text, "Your team took Infernal Drake");
        assert_eq!(board.feed[1].text, "Your bot outer tower fell");
        assert_eq!(board.feed[2].text, "You killed Zed");

        let board_fr = extract_live_board_localized(None, Some(&events), &l, Lang::Fr);
        assert_eq!(board_fr.feed[0].text, "Votre équipe a pris le Dragon infernal");
        assert_eq!(board_fr.feed[1].text, "Votre tourelle extérieure bot est tombée");
        assert_eq!(board_fr.feed[2].text, "Vous avez éliminé Zed");
    }
}
