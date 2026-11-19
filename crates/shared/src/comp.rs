//! Offline team comp and lane matchup analysis for the champ-select helper.
//!
//! Everything here is static knowledge shipped in the binary: a per-champion trait table
//! ([`CHAMPION_TRAITS`]) and pure functions over it. No I/O, no global win rates; those come
//! later from the opt-in online [`StatsSource`](crate::stats::StatsSource).
//!
//! Placeholder picks (champion ID `0`) and IDs missing from the table are ignored, so an empty
//! or half-filled draft yields neutral output instead of guesses.

use crate::champions::{builtin_champion_roles, Position};
use crate::i18n::Lang;
use crate::stats::{CompTrait, TeamCompProfile};

// Trait bits of the table. Kept private: callers read them as `CompTrait`.
const ENG: u16 = 1 << 0;
const PEEL: u16 = 1 << 1;
const POKE: u16 = 1 << 2;
const SPLIT: u16 = 1 << 3;
const FIGHT: u16 = 1 << 4;
const PICK: u16 = 1 << 5;
const LATE: u16 = 1 << 6;
const EARLY: u16 = 1 << 7;
const FRONT: u16 = 1 << 8;

/// The positive traits a champion can carry, with their bit in the table.
const TAG_TRAITS: [(u16, CompTrait); 9] = [
    (ENG, CompTrait::Engage),
    (PEEL, CompTrait::Peel),
    (POKE, CompTrait::Poke),
    (SPLIT, CompTrait::Splitpush),
    (FIGHT, CompTrait::Teamfight),
    (PICK, CompTrait::Pick),
    (LATE, CompTrait::Scaling),
    (EARLY, CompTrait::EarlyGame),
    (FRONT, CompTrait::Frontline),
];

/// Static profile of one champion: main damage split in percent and its comp traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChampionTraits {
    pub champion_id: i64,
    /// Physical / magic / true damage share in percent, summing to 100.
    pub physical: u8,
    pub magic: u8,
    pub true_damage: u8,
    tags: u16,
}

impl ChampionTraits {
    /// Whether the champion carries `t`. Gap traits (`Lacks*`, `All*`) describe teams, never
    /// one champion, so they are always `false` here.
    pub fn has(&self, t: CompTrait) -> bool {
        TAG_TRAITS
            .iter()
            .any(|&(bit, tr)| tr == t && self.tags & bit != 0)
    }

    /// The champion's traits in `CompTrait` declaration order.
    pub fn traits(&self) -> Vec<CompTrait> {
        TAG_TRAITS
            .iter()
            .filter(|&&(bit, _)| self.tags & bit != 0)
            .map(|&(_, t)| t)
            .collect()
    }
}

const fn c(
    champion_id: i64,
    physical: u8,
    magic: u8,
    true_damage: u8,
    tags: u16,
) -> ChampionTraits {
    ChampionTraits {
        champion_id,
        physical,
        magic,
        true_damage,
        tags,
    }
}

/// One entry per champion of [`crate::champions::BUILTIN_CHAMPIONS`], same order.
/// Columns: ID, physical %, magic %, true %, traits.
pub const CHAMPION_TRAITS: &[ChampionTraits] = &[
    c(1, 0, 100, 0, ENG | FIGHT | PICK),          // Annie
    c(2, 80, 0, 20, EARLY | FRONT),               // Olaf
    c(3, 0, 100, 0, ENG | PEEL | FIGHT | FRONT),  // Galio
    c(4, 15, 85, 0, PICK | SPLIT),                // Twisted Fate
    c(5, 90, 10, 0, ENG | EARLY),                 // Xin Zhao
    c(6, 95, 0, 5, FRONT | FIGHT),                // Urgot
    c(7, 0, 100, 0, PICK | EARLY),                // LeBlanc
    c(8, 0, 100, 0, FIGHT | LATE),                // Vladimir
    c(9, 0, 100, 0, ENG | FIGHT),                 // Fiddlesticks
    c(10, 40, 50, 10, LATE | FIGHT),              // Kayle
    c(11, 70, 0, 30, LATE | SPLIT),               // Master Yi
    c(12, 0, 100, 0, ENG | PEEL | FIGHT | FRONT), // Alistar
    c(13, 0, 100, 0, LATE | FIGHT),               // Ryze
    c(14, 60, 40, 0, ENG | FRONT | SPLIT),        // Sion
    c(15, 100, 0, 0, FIGHT | LATE),               // Sivir
    c(16, 0, 100, 0, PEEL | POKE),                // Soraka
    c(17, 30, 70, 0, SPLIT | POKE),               // Teemo
    c(18, 85, 15, 0, LATE | SPLIT),               // Tristana
    c(19, 50, 50, 0, EARLY | PICK),               // Warwick
    c(20, 0, 100, 0, ENG | FRONT | PICK),         // Nunu & Willump
    c(21, 100, 0, 0, FIGHT | EARLY),              // Miss Fortune
    c(22, 100, 0, 0, ENG | PICK | POKE),          // Ashe
    c(23, 100, 0, 0, SPLIT | LATE),               // Tryndamere
    c(24, 60, 40, 0, SPLIT | LATE),               // Jax
    c(25, 0, 100, 0, PEEL | PICK),                // Morgana
    c(26, 0, 100, 0, PEEL | POKE),                // Zilean
    c(27, 0, 100, 0, FRONT | SPLIT),              // Singed
    c(28, 0, 100, 0, PICK),                       // Evelynn
    c(29, 80, 0, 20, LATE | FIGHT),               // Twitch
    c(30, 0, 100, 0, LATE | FIGHT),               // Karthus
    c(31, 0, 80, 20, FRONT | LATE | FIGHT),       // Cho'Gath
    c(32, 0, 100, 0, ENG | FRONT | FIGHT),        // Amumu
    c(33, 20, 80, 0, ENG | FRONT | PICK),         // Rammus
    c(34, 0, 100, 0, FIGHT | LATE),               // Anivia
    c(35, 60, 40, 0, PICK | EARLY),               // Shaco
    c(36, 50, 50, 0, FRONT | LATE),               // Dr. Mundo
    c(37, 0, 100, 0, FIGHT | LATE | PEEL),        // Sona
    c(38, 0, 100, 0, LATE | PICK),                // Kassadin
    c(39, 90, 10, 0, SPLIT | FIGHT),              // Irelia
    c(40, 0, 100, 0, PEEL),                       // Janna
    c(41, 70, 20, 10, LATE | SPLIT | FIGHT),      // Gangplank
    c(42, 40, 60, 0, POKE | LATE),                // Corki
    c(43, 0, 100, 0, POKE | PEEL | EARLY),        // Karma
    c(44, 20, 80, 0, PEEL | FRONT | FIGHT),       // Taric
    c(45, 0, 100, 0, LATE | PICK),                // Veigar
    c(48, 100, 0, 0, SPLIT | EARLY),              // Trundle
    c(50, 0, 100, 0, FRONT | FIGHT),              // Swain
    c(51, 100, 0, 0, POKE | EARLY),               // Caitlyn
    c(53, 0, 100, 0, PICK | ENG),                 // Blitzcrank
    c(54, 20, 80, 0, ENG | FRONT | FIGHT),        // Malphite
    c(55, 10, 90, 0, FIGHT | PICK),               // Katarina
    c(56, 100, 0, 0, ENG | PICK),                 // Nocturne
    c(57, 0, 100, 0, ENG | PEEL | FRONT | FIGHT), // Maokai
    c(58, 100, 0, 0, EARLY | FRONT),              // Renekton
    c(59, 100, 0, 0, ENG | FIGHT | EARLY),        // Jarvan IV
    c(60, 10, 90, 0, PICK | EARLY),               // Elise
    c(61, 0, 100, 0, FIGHT | PEEL),               // Orianna
    c(62, 100, 0, 0, ENG | FIGHT),                // Wukong
    c(63, 0, 100, 0, FIGHT | POKE),               // Brand
    c(64, 100, 0, 0, EARLY | PICK),               // Lee Sin
    c(67, 70, 0, 30, LATE | SPLIT),               // Vayne
    c(68, 0, 100, 0, FIGHT | EARLY),              // Rumble
    c(69, 0, 100, 0, LATE | FIGHT),               // Cassiopeia
    c(72, 70, 30, 0, ENG | FRONT | PICK),         // Skarner
    c(74, 0, 100, 0, POKE | PEEL),                // Heimerdinger
    c(75, 90, 10, 0, LATE | SPLIT),               // Nasus
    c(76, 10, 90, 0, POKE | EARLY),               // Nidalee
    c(77, 50, 50, 0, FRONT | EARLY),              // Udyr
    c(78, 100, 0, 0, PEEL | FRONT),               // Poppy
    c(79, 0, 100, 0, ENG | FRONT),                // Gragas
    c(80, 100, 0, 0, EARLY | PICK),               // Pantheon
    c(81, 80, 20, 0, POKE),                       // Ezreal
    c(82, 0, 100, 0, FRONT | SPLIT),              // Mordekaiser
    c(83, 70, 30, 0, SPLIT),                      // Yorick
    c(84, 10, 90, 0, PICK),                       // Akali
    c(85, 0, 100, 0, ENG | FIGHT),                // Kennen
    c(86, 80, 0, 20, FRONT | EARLY),              // Garen
    c(89, 0, 100, 0, ENG | FRONT | PICK),         // Leona
    c(90, 0, 100, 0, PICK),                       // Malzahar
    c(91, 100, 0, 0, PICK | EARLY),               // Talon
    c(92, 100, 0, 0, SPLIT | EARLY),              // Riven
    c(96, 50, 50, 0, LATE | POKE),                // Kog'Maw
    c(98, 40, 40, 20, FRONT | PEEL | SPLIT),      // Shen
    c(99, 0, 100, 0, POKE | PICK),                // Lux
    c(101, 0, 100, 0, POKE),                      // Xerath
    c(102, 50, 50, 0, FRONT | LATE),              // Shyvana
    c(103, 0, 100, 0, PICK),                      // Ahri
    c(104, 100, 0, 0, EARLY),                     // Graves
    c(105, 10, 90, 0, PICK),                      // Fizz
    c(106, 40, 60, 0, ENG | FRONT | EARLY),       // Volibear
    c(107, 100, 0, 0, PICK | EARLY),              // Rengar
    c(110, 70, 30, 0, POKE | PICK),               // Varus
    c(111, 0, 100, 0, ENG | FRONT | PICK),        // Nautilus
    c(112, 0, 100, 0, LATE | FIGHT),              // Viktor
    c(113, 0, 100, 0, ENG | FRONT | FIGHT),       // Sejuani
    c(114, 70, 0, 30, SPLIT | LATE),              // Fiora
    c(117, 0, 100, 0, PEEL),                      // Lulu
    c(119, 100, 0, 0, EARLY),                     // Draven
    c(120, 100, 0, 0, ENG | FIGHT),               // Hecarim
    c(121, 100, 0, 0, PICK),                      // Kha'Zix
    c(122, 80, 0, 20, FRONT | EARLY),             // Darius
    c(126, 100, 0, 0, POKE | EARLY),              // Jayce
    c(127, 0, 100, 0, ENG | FIGHT),               // Lissandra
    c(131, 0, 100, 0, ENG | FIGHT),               // Diana
    c(133, 100, 0, 0, SPLIT | EARLY),             // Quinn
    c(134, 0, 100, 0, PICK | POKE),               // Syndra
    c(136, 0, 100, 0, LATE | FIGHT),              // Aurelion Sol
    c(141, 100, 0, 0, PICK),                      // Kayn
    c(142, 0, 100, 0, POKE | PICK),               // Zoe
    c(143, 0, 100, 0, POKE | FIGHT),              // Zyra
    c(145, 60, 40, 0, LATE | PICK),               // Kai'Sa
    c(147, 0, 100, 0, FIGHT | POKE),              // Seraphine
    c(150, 100, 0, 0, FRONT | FIGHT),             // Gnar
    c(154, 0, 100, 0, ENG | FRONT | FIGHT),       // Zac
    c(157, 100, 0, 0, FIGHT | LATE),              // Yasuo
    c(161, 0, 70, 30, POKE),                      // Vel'Koz
    c(163, 0, 100, 0, PICK | POKE),               // Taliyah
    c(164, 80, 0, 20, SPLIT | PICK),              // Camille
    c(200, 80, 0, 20, LATE | SPLIT),              // Bel'Veth
    c(201, 0, 100, 0, PEEL | FRONT),              // Braum
    c(202, 100, 0, 0, POKE | PICK),               // Jhin
    c(203, 100, 0, 0, LATE),                      // Kindred
    c(221, 90, 10, 0, LATE | FIGHT),              // Zeri
    c(222, 100, 0, 0, LATE | FIGHT),              // Jinx
    c(223, 0, 100, 0, PEEL | FRONT),              // Tahm Kench
    c(233, 100, 0, 0, ENG | EARLY),               // Briar
    c(234, 100, 0, 0, FIGHT),                     // Viego
    c(235, 100, 0, 0, LATE | POKE),               // Senna
    c(236, 100, 0, 0, EARLY),                     // Lucian
    c(238, 100, 0, 0, PICK | SPLIT),              // Zed
    c(240, 100, 0, 0, ENG | EARLY),               // Kled
    c(245, 0, 100, 0, PICK),                      // Ekko
    c(246, 100, 0, 0, PICK | FIGHT),              // Qiyana
    c(254, 100, 0, 0, ENG | PICK),                // Vi
    c(266, 100, 0, 0, FIGHT | FRONT),             // Aatrox
    c(267, 0, 100, 0, PEEL | FIGHT),              // Nami
    c(268, 0, 100, 0, LATE | FIGHT),              // Azir
    c(350, 0, 100, 0, PEEL | LATE),               // Yuumi
    c(360, 100, 0, 0, FIGHT),                     // Samira
    c(412, 0, 100, 0, PICK | PEEL | ENG),         // Thresh
    c(420, 100, 0, 0, SPLIT | FRONT),             // Illaoi
    c(421, 100, 0, 0, EARLY | PICK),              // Rek'Sai
    c(427, 0, 100, 0, PEEL),                      // Ivern
    c(429, 100, 0, 0, EARLY),                     // Kalista
    c(432, 0, 100, 0, PICK),                      // Bard
    c(497, 0, 100, 0, ENG | PEEL),                // Rakan
    c(498, 100, 0, 0, LATE),                      // Xayah
    c(516, 50, 50, 0, ENG | FRONT | FIGHT),       // Ornn
    c(517, 0, 100, 0, FIGHT),                     // Sylas
    c(518, 0, 100, 0, ENG | FIGHT),               // Neeko
    c(523, 100, 0, 0, LATE | FIGHT),              // Aphelios
    c(526, 0, 100, 0, ENG | FRONT | FIGHT),       // Rell
    c(555, 100, 0, 0, PICK | EARLY),              // Pyke
    c(711, 0, 100, 0, PICK | FIGHT),              // Vex
    c(777, 50, 50, 0, FIGHT | LATE),              // Yone
    c(799, 100, 0, 0, ENG | EARLY),               // Ambessa
    c(800, 0, 100, 0, POKE | LATE),               // Mel
    c(804, 80, 20, 0, LATE),                      // Yunara
    c(805, 100, 0, 0, FRONT | SPLIT),             // Zaahen
    c(875, 80, 0, 20, FRONT | ENG),               // Sett
    c(876, 0, 100, 0, FIGHT | LATE),              // Lillia
    c(887, 0, 80, 20, SPLIT | LATE),              // Gwen
    c(888, 0, 100, 0, PEEL | FIGHT),              // Renata Glasc
    c(893, 0, 100, 0, PICK),                      // Aurora
    c(895, 100, 0, 0, FIGHT | EARLY),             // Nilah
    c(897, 90, 0, 10, FRONT | ENG),               // K'Sante
    c(901, 80, 10, 10, LATE | POKE),              // Smolder
    c(902, 0, 100, 0, PEEL),                      // Milio
    c(910, 0, 100, 0, POKE | FIGHT),              // Hwei
    c(950, 100, 0, 0, PICK | EARLY),              // Naafiri
];

/// Static traits of `champion_id`, or `None` for placeholders (`0`) and unknown IDs.
pub fn champion_traits(champion_id: i64) -> Option<&'static ChampionTraits> {
    if champion_id <= 0 {
        return None;
    }
    CHAMPION_TRAITS
        .iter()
        .find(|t| t.champion_id == champion_id)
}

/// A team needs this many known champions before gaps (`Lacks*`, `All*`) are judged and before
/// [`comp_advice`] says anything about it: two picks say little about a comp.
pub const MIN_PICKS_FOR_VERDICT: usize = 3;

/// Damage share above which a team counts as one-sided (`AllPhysical`, `AllMagic`).
pub const ONE_SIDED_DAMAGE: f32 = 0.8;

/// Traits that become a team identity only when at least two champions bring them; one poke
/// champion does not make a poke comp. The rest count from a single champion.
const NEEDS_TWO: [CompTrait; 4] = [
    CompTrait::Poke,
    CompTrait::Teamfight,
    CompTrait::Scaling,
    CompTrait::EarlyGame,
];

fn known(champion_ids: &[i64]) -> Vec<&'static ChampionTraits> {
    champion_ids
        .iter()
        .filter_map(|&id| champion_traits(id))
        .collect()
}

/// Damage split and comp traits of a team. Placeholders and unknown IDs are skipped; with no
/// known champion the profile is all zeros with no traits.
pub fn team_profile(champion_ids: &[i64]) -> TeamCompProfile {
    let champs = known(champion_ids);
    if champs.is_empty() {
        return TeamCompProfile::default();
    }
    let n = champs.len() as f32;
    let share = |f: fn(&ChampionTraits) -> u8| {
        champs.iter().map(|c| f32::from(f(c))).sum::<f32>() / (100.0 * n)
    };
    let physical = share(|c| c.physical);
    let magic = share(|c| c.magic);
    let true_damage = share(|c| c.true_damage);

    let mut traits: Vec<CompTrait> = TAG_TRAITS
        .iter()
        .map(|&(_, t)| t)
        .filter(|&t| {
            let count = champs.iter().filter(|c| c.has(t)).count();
            count >= if NEEDS_TWO.contains(&t) { 2 } else { 1 }
        })
        .collect();

    if champs.len() >= MIN_PICKS_FOR_VERDICT {
        if !traits.contains(&CompTrait::Frontline) {
            traits.push(CompTrait::LacksFrontline);
        }
        if !traits.contains(&CompTrait::Engage) {
            traits.push(CompTrait::LacksEngage);
        }
        if physical >= ONE_SIDED_DAMAGE {
            traits.push(CompTrait::AllPhysical);
        }
        if magic >= ONE_SIDED_DAMAGE {
            traits.push(CompTrait::AllMagic);
        }
    }

    TeamCompProfile {
        physical,
        magic,
        true_damage,
        traits,
    }
}

/// The five Summoner's Rift lanes in board order.
const LANES: [Position; 5] = [
    Position::Top,
    Position::Jungle,
    Position::Mid,
    Position::Bot,
    Position::Support,
];

/// Cost of playing a champion off every role the built-in table lists for it.
const OFF_ROLE: usize = LANES.len();

fn lane_cost(champion_id: i64, lane: Position) -> usize {
    builtin_champion_roles(champion_id)
        .iter()
        .position(|r| Position::parse(r) == Some(lane))
        .unwrap_or(OFF_ROLE)
}

/// Seats a team in the five lanes. Known positions (not `Fill`) are kept, first come first
/// served; the other picks fill the open lanes with the cheapest assignment by built-in role
/// order (a champion's first listed role costs 0). Placeholders (`0`) take no lane.
fn seat_team(picks: &[(i64, Option<Position>)]) -> [Option<i64>; 5] {
    let mut seats = [None; 5];
    let mut unseated = Vec::new();
    for &(id, pos) in picks.iter().filter(|(id, _)| *id > 0) {
        let slot = pos.and_then(|p| LANES.iter().position(|&l| l == p));
        match slot {
            Some(i) if seats[i].is_none() => seats[i] = Some(id),
            _ => unseated.push(id),
        }
    }
    let open: Vec<usize> = (0..LANES.len()).filter(|&i| seats[i].is_none()).collect();
    unseated.truncate(open.len());

    // At most 5! orders, so an exhaustive search is cheap and stays deterministic: lanes are
    // tried in board order and only a strictly cheaper assignment replaces the current best.
    fn search(
        unseated: &[i64],
        open: &[usize],
        used: &mut Vec<bool>,
        current: &mut Vec<usize>,
        cost: usize,
        best: &mut (usize, Vec<usize>),
    ) {
        if cost >= best.0 {
            return;
        }
        let Some(&id) = unseated.get(current.len()) else {
            *best = (cost, current.clone());
            return;
        };
        for (k, &lane) in open.iter().enumerate() {
            if used[k] {
                continue;
            }
            used[k] = true;
            current.push(lane);
            search(
                unseated,
                open,
                used,
                current,
                cost + lane_cost(id, LANES[lane]),
                best,
            );
            current.pop();
            used[k] = false;
        }
    }
    let mut best = (usize::MAX, Vec::new());
    search(
        &unseated,
        &open,
        &mut vec![false; open.len()],
        &mut Vec::new(),
        0,
        &mut best,
    );
    for (&id, &lane) in unseated.iter().zip(&best.1) {
        seats[lane] = Some(id);
    }
    seats
}

/// Pairs each player with their lane opponent, in board order (Top, Jungle, Mid, Bot,
/// Support), as `(ally champion, enemy champion, lane)`.
///
/// Positions that are `None` or `Fill` (enemy positions are hidden in champ select) are
/// inferred from the built-in role table. Placeholders (`0`) are never paired; a lane appears
/// only when both sides have a champion in it.
pub fn lane_opponents(
    ally: &[(i64, Option<Position>)],
    enemy: &[(i64, Option<Position>)],
) -> Vec<(i64, i64, Position)> {
    let a = seat_team(ally);
    let e = seat_team(enemy);
    LANES
        .iter()
        .enumerate()
        .filter_map(|(i, &lane)| Some((a[i]?, e[i]?, lane)))
        .collect()
}

/// Most tips [`comp_advice`] returns.
pub const MAX_TIPS: usize = 4;

/// Short, plain-language tips for the draft, most important first, at most [`MAX_TIPS`].
/// A side says nothing until it has [`MIN_PICKS_FOR_VERDICT`] known champions.
pub fn comp_advice(ally: &[i64], enemy: &[i64], lang: Lang) -> Vec<String> {
    let ours = verdict(ally);
    let theirs = verdict(enemy);
    let we = |t| ours.as_ref().is_some_and(|p| p.traits.contains(&t));
    let they = |t| theirs.as_ref().is_some_and(|p| p.traits.contains(&t));

    use CompTrait::*;
    let rules: [(bool, &str, &str); 11] = [
        (
            they(AllMagic),
            "The enemy deals almost only magic damage: magic resist is worth it.",
            "L'ennemi fait presque que des dégâts magiques : la résistance magique vaut le coup.",
        ),
        (
            they(AllPhysical),
            "The enemy deals almost only physical damage: armor is worth it.",
            "L'ennemi fait presque que des dégâts physiques : l'armure vaut le coup.",
        ),
        (
            we(LacksFrontline),
            "Your team has no frontline: someone needs to build tanky.",
            "Votre équipe n'a pas de ligne avant : quelqu'un doit jouer plus résistant.",
        ),
        (
            we(LacksEngage),
            "Your team struggles to start fights: let the enemy come to you.",
            "Votre équipe a du mal à lancer les combats : laissez l'ennemi venir à vous.",
        ),
        (
            we(AllPhysical),
            "Your damage is almost all physical: the enemy only needs armor.",
            "Vos dégâts sont presque tous physiques : l'ennemi n'a besoin que d'armure.",
        ),
        (
            we(AllMagic),
            "Your damage is almost all magic: the enemy only needs magic resist.",
            "Vos dégâts sont presque tous magiques : l'ennemi n'a besoin que de résistance magique.",
        ),
        (
            we(EarlyGame) && they(Scaling),
            "You are stronger early: take fights and objectives before they catch up.",
            "Vous êtes plus forts en début de partie : prenez combats et objectifs avant qu'ils ne rattrapent.",
        ),
        (
            we(Scaling) && they(EarlyGame),
            "They are stronger early: play safe, your team gets stronger later.",
            "Ils sont plus forts en début de partie : jouez prudent, votre équipe monte en puissance.",
        ),
        (
            they(Pick),
            "The enemy can catch players alone: move together and stay in sight of your team.",
            "L'ennemi peut attraper les joueurs isolés : déplacez-vous ensemble.",
        ),
        (
            they(Poke),
            "The enemy hits from long range: don't wait under their fire, go in or back off.",
            "L'ennemi frappe de loin : ne restez pas sous le feu, engagez ou reculez.",
        ),
        (
            they(Splitpush),
            "An enemy will push a side lane alone: send one player to answer, not three.",
            "Un ennemi poussera seul une voie : envoyez un joueur pour répondre, pas trois.",
        ),
    ];
    rules
        .iter()
        .filter(|(fires, _, _)| *fires)
        .take(MAX_TIPS)
        .map(|&(_, en, fr)| lang.pick(en, fr).to_string())
        .collect()
}

/// The team's profile once it has enough known champions to be judged.
fn verdict(champion_ids: &[i64]) -> Option<TeamCompProfile> {
    (known(champion_ids).len() >= MIN_PICKS_FOR_VERDICT).then(|| team_profile(champion_ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::champions::BUILTIN_CHAMPIONS;
    use std::collections::HashSet;
    use Position::*;

    // Champion IDs used across tests.
    const MALPHITE: i64 = 54;
    const AHRI: i64 = 103;
    const SYNDRA: i64 = 134;
    const LUX: i64 = 99;
    const ZYRA: i64 = 143;
    const BRAND: i64 = 63;
    const XERATH: i64 = 101;
    const DRAVEN: i64 = 119;
    const GRAVES: i64 = 104;
    const RIVEN: i64 = 92;
    const TALON: i64 = 91;
    const LUCIAN: i64 = 236;
    const LEE_SIN: i64 = 64;
    const JINX: i64 = 222;
    const THRESH: i64 = 412;
    const LEONA: i64 = 89;
    const DARIUS: i64 = 122;
    const NASUS: i64 = 75;
    const KASSADIN: i64 = 38;
    const SENNA: i64 = 235;
    const YUUMI: i64 = 350;

    #[test]
    fn every_builtin_champion_has_traits() {
        let ids: HashSet<i64> = CHAMPION_TRAITS.iter().map(|t| t.champion_id).collect();
        assert_eq!(
            ids.len(),
            CHAMPION_TRAITS.len(),
            "duplicate champion in trait table"
        );
        for champ in BUILTIN_CHAMPIONS {
            assert!(
                ids.contains(&champ.id),
                "{} ({}) has no traits",
                champ.name,
                champ.id
            );
        }
        assert_eq!(
            CHAMPION_TRAITS.len(),
            BUILTIN_CHAMPIONS.len(),
            "trait for unknown champion"
        );
    }

    #[test]
    fn damage_splits_sum_to_100_and_every_champion_has_a_trait() {
        for t in CHAMPION_TRAITS {
            let sum = u16::from(t.physical) + u16::from(t.magic) + u16::from(t.true_damage);
            assert_eq!(sum, 100, "champion {}", t.champion_id);
            assert!(
                !t.traits().is_empty(),
                "champion {} has no trait",
                t.champion_id
            );
            assert!(!t.has(CompTrait::LacksFrontline));
        }
    }

    #[test]
    fn champion_traits_ignores_placeholders() {
        assert!(champion_traits(0).is_none());
        assert!(champion_traits(-1).is_none());
        assert!(champion_traits(99_999).is_none());
        assert!(champion_traits(MALPHITE).unwrap().has(CompTrait::Frontline));
    }

    #[test]
    fn empty_or_placeholder_team_profile_is_neutral() {
        assert_eq!(team_profile(&[]), TeamCompProfile::default());
        assert_eq!(team_profile(&[0, 0, 0, 0, 0]), TeamCompProfile::default());
        assert_eq!(team_profile(&[0, 99_999]), TeamCompProfile::default());
    }

    #[test]
    fn team_profile_averages_damage_and_skips_placeholders() {
        let p = team_profile(&[AHRI, DRAVEN, 0]);
        assert!((p.physical - 0.5).abs() < 1e-6);
        assert!((p.magic - 0.5).abs() < 1e-6);
        assert_eq!(p.true_damage, 0.0);
        assert!((p.physical + p.magic + p.true_damage - 1.0).abs() < 1e-6);
        // Two known picks: no verdict on gaps yet.
        assert!(!p.traits.contains(&CompTrait::LacksFrontline));
    }

    #[test]
    fn team_profile_flags_gaps_once_the_team_is_formed() {
        let p = team_profile(&[AHRI, SYNDRA, LUX, ZYRA, BRAND]);
        assert!(p.magic > 0.99);
        for t in [
            CompTrait::LacksFrontline,
            CompTrait::LacksEngage,
            CompTrait::AllMagic,
        ] {
            assert!(p.traits.contains(&t), "missing {t:?}");
        }
        assert!(!p.traits.contains(&CompTrait::AllPhysical));
        assert!(p.traits.contains(&CompTrait::Poke));

        let p = team_profile(&[DARIUS, LEE_SIN, TALON, LUCIAN, LEONA]);
        assert!(p.traits.contains(&CompTrait::Frontline));
        assert!(p.traits.contains(&CompTrait::Engage));
        assert!(p.traits.contains(&CompTrait::EarlyGame));
        assert!(!p.traits.contains(&CompTrait::LacksFrontline));
        assert!(!p.traits.contains(&CompTrait::LacksEngage));
        assert!(!p.traits.contains(&CompTrait::AllMagic));
    }

    #[test]
    fn team_identity_traits_need_two_champions() {
        // One poke champion is not a poke comp.
        let p = team_profile(&[XERATH, DARIUS, LEONA]);
        assert!(!p.traits.contains(&CompTrait::Poke));
        assert!(p.traits.contains(&CompTrait::Engage));
    }

    #[test]
    fn lane_opponents_keeps_known_positions() {
        let ally = [
            (MALPHITE, Some(Top)),
            (LEE_SIN, Some(Jungle)),
            (AHRI, Some(Mid)),
        ];
        let enemy = [
            (DARIUS, Some(Top)),
            (GRAVES, Some(Jungle)),
            (SYNDRA, Some(Mid)),
        ];
        assert_eq!(
            lane_opponents(&ally, &enemy),
            vec![
                (MALPHITE, DARIUS, Top),
                (LEE_SIN, GRAVES, Jungle),
                (AHRI, SYNDRA, Mid)
            ]
        );
    }

    #[test]
    fn lane_opponents_infers_hidden_enemy_positions_from_roles() {
        let ally = [
            (MALPHITE, Some(Top)),
            (LEE_SIN, Some(Jungle)),
            (AHRI, Some(Mid)),
            (JINX, Some(Bot)),
            (LEONA, Some(Support)),
        ];
        // Champ select order, positions unknown (some reported as Fill).
        let enemy = [
            (THRESH, None),
            (KASSADIN, Some(Fill)),
            (GRAVES, None),
            (DRAVEN, None),
            (RIVEN, None),
        ];
        assert_eq!(
            lane_opponents(&ally, &enemy),
            vec![
                (MALPHITE, RIVEN, Top),
                (LEE_SIN, GRAVES, Jungle),
                (AHRI, KASSADIN, Mid),
                (JINX, DRAVEN, Bot),
                (LEONA, THRESH, Support),
            ]
        );
    }

    #[test]
    fn lane_opponents_resolves_flex_picks_around_known_lanes() {
        // Senna lists Support before Bot; with Yuumi in the team she still ends up Bot,
        // because Yuumi only plays Support.
        let enemy = [(SENNA, None), (YUUMI, None), (NASUS, None)];
        let ally = [
            (JINX, Some(Bot)),
            (LEONA, Some(Support)),
            (DARIUS, Some(Top)),
        ];
        assert_eq!(
            lane_opponents(&ally, &enemy),
            vec![
                (DARIUS, NASUS, Top),
                (JINX, SENNA, Bot),
                (LEONA, YUUMI, Support)
            ]
        );
    }

    #[test]
    fn lane_opponents_skips_placeholders_and_unpaired_lanes() {
        assert!(lane_opponents(&[], &[]).is_empty());
        assert!(lane_opponents(&[(0, Some(Mid))], &[(AHRI, Some(Mid))]).is_empty());
        assert!(lane_opponents(&[(AHRI, Some(Mid))], &[(0, None), (0, None)]).is_empty());
        // A duplicate claim on a lane falls back to the role table.
        let ally = [(AHRI, Some(Mid)), (DARIUS, Some(Mid))];
        let enemy = [(NASUS, Some(Top)), (SYNDRA, Some(Mid))];
        assert_eq!(
            lane_opponents(&ally, &enemy),
            vec![(DARIUS, NASUS, Top), (AHRI, SYNDRA, Mid)]
        );
    }

    #[test]
    fn comp_advice_is_empty_for_empty_or_partial_drafts() {
        assert!(comp_advice(&[], &[], Lang::En).is_empty());
        assert!(comp_advice(&[0; 5], &[0; 5], Lang::Fr).is_empty());
        // Two picks per side: too early to judge.
        assert!(comp_advice(&[AHRI, SYNDRA], &[LUX, ZYRA], Lang::En).is_empty());
    }

    #[test]
    fn comp_advice_orders_tips_and_caps_them() {
        let ally = [AHRI, SYNDRA, LUX, ZYRA, BRAND];
        let enemy = [AHRI, SYNDRA, LUX, XERATH, BRAND];
        let tips = comp_advice(&ally, &enemy, Lang::En);
        assert_eq!(tips.len(), MAX_TIPS);
        assert!(tips[0].contains("magic resist is worth it"), "{tips:?}");
        assert!(tips[1].contains("no frontline"), "{tips:?}");
        assert!(tips[2].contains("start fights"), "{tips:?}");
        assert!(tips[3].contains("almost all magic"), "{tips:?}");
    }

    #[test]
    fn comp_advice_is_localized_and_balanced_teams_get_few_tips() {
        let ally = [AHRI, SYNDRA, LUX, ZYRA, BRAND];
        let enemy = [DRAVEN, GRAVES, RIVEN, TALON, LUCIAN];
        let en = comp_advice(&ally, &enemy, Lang::En);
        let fr = comp_advice(&ally, &enemy, Lang::Fr);
        assert_eq!(en.len(), fr.len());
        assert!(en[0].contains("armor"), "{en:?}");
        assert!(fr[0].contains("armure"), "{fr:?}");
        assert_ne!(en, fr);

        let balanced = [DARIUS, LEE_SIN, AHRI, JINX, LEONA];
        assert!(comp_advice(&balanced, &[0; 5], Lang::En).is_empty());
    }
}
