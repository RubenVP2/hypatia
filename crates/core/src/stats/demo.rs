//! Champion Stats fixtures for `--demo`: every champion of the demo draft and live game has a
//! build, a tier, lane matchups and game data; the demo player has a My Stats history. All of it
//! is labelled [`StatsOrigin::Demo`]. Numbers are illustrative, not real statistics.

use hypatia_shared::{
    ChampionBuild, GameData, GameDataEntry, ItemBlock, ItemStage, Lang, Matchup, MatchupList,
    PlayerGame, PlayerProfile, Position, RankBracket, Rates, RuneSetup, Skill, SkillOrder,
    SpellPair, StatsOrigin, StatsQuery, StatsQueue, StatsResult, StatsSource, StatsUnavailable,
    Tier, TierEntry, TierList,
};

use super::profile::build_profile;

/// Patch the demo numbers pretend to describe.
pub const DEMO_PATCH: &str = "26.22";

struct DemoChampion {
    id: i64,
    position: Position,
    tier: Tier,
    win: f32,
    pick: f32,
    ban: f32,
    games: u32,
    primary: i64,
    sub: i64,
    perks: [i64; 6],
    shards: [i64; 3],
    spells: (i64, i64),
    starting: &'static [i64],
    core: &'static [i64],
    boots: i64,
    situational: &'static [i64],
    /// Fifteen skill letters, level 1 first.
    skills: &'static str,
    priority: &'static str,
}

const AD: [i64; 3] = [5008, 5008, 5011];
const AP: [i64; 3] = [5008, 5008, 5001];
const MARKSMAN: [i64; 3] = [5005, 5008, 5011];
const TANK: [i64; 3] = [5007, 5001, 5011];

/// The demo draft (allies 266 64 103 222 412, enemies 122 234 89 81 99, ban 238) and the demo
/// live game (86 64 103 222 412 / 122 234 238 145 111).
const CHAMPIONS: &[DemoChampion] = &[
    DemoChampion {
        id: 266,
        position: Position::Top,
        tier: Tier::A,
        win: 0.503,
        pick: 0.071,
        ban: 0.122,
        games: 48_210,
        primary: 8000,
        sub: 8400,
        perks: [8010, 9111, 9105, 8299, 8444, 8242],
        shards: AD,
        spells: (4, 12),
        starting: &[1055, 2003],
        core: &[6692, 3053, 3071],
        boots: 3047,
        situational: &[6333, 3026, 6694],
        skills: "QEWQQRQEQERWEEW",
        priority: "QEW",
    },
    DemoChampion {
        id: 64,
        position: Position::Jungle,
        tier: Tier::B,
        win: 0.488,
        pick: 0.112,
        ban: 0.096,
        games: 61_200,
        primary: 8000,
        sub: 8100,
        perks: [8010, 9111, 9104, 8299, 8143, 8106],
        shards: AD,
        spells: (4, 11),
        starting: &[1101, 2031],
        core: &[6692, 3071, 3053],
        boots: 3158,
        situational: &[3156, 6333, 3026],
        skills: "QWEQQRQWQWRWWEE",
        priority: "QWE",
    },
    DemoChampion {
        id: 103,
        position: Position::Mid,
        tier: Tier::A,
        win: 0.508,
        pick: 0.063,
        ban: 0.041,
        games: 73_900,
        primary: 8100,
        sub: 8200,
        perks: [8112, 8139, 8140, 8106, 8226, 8210],
        shards: AP,
        spells: (4, 14),
        starting: &[1056, 2003],
        core: &[3118, 4645, 3089],
        boots: 3020,
        situational: &[3157, 3135, 3102],
        skills: "WQEQQRQWQWRWWEE",
        priority: "QWE",
    },
    DemoChampion {
        id: 222,
        position: Position::Bot,
        tier: Tier::S,
        win: 0.521,
        pick: 0.142,
        ban: 0.087,
        games: 112_400,
        primary: 8000,
        sub: 8300,
        perks: [8008, 9101, 9104, 8014, 8345, 8347],
        shards: MARKSMAN,
        spells: (4, 7),
        starting: &[1055, 2003],
        core: &[6672, 3031, 3085],
        boots: 3006,
        situational: &[3036, 3072, 3026],
        skills: "QWEQQRQWQWRWWEE",
        priority: "QWE",
    },
    DemoChampion {
        id: 412,
        position: Position::Support,
        tier: Tier::A,
        win: 0.505,
        pick: 0.098,
        ban: 0.058,
        games: 77_800,
        primary: 8400,
        sub: 8300,
        perks: [8439, 8463, 8473, 8453, 8345, 8347],
        shards: TANK,
        spells: (4, 14),
        starting: &[3865, 2003],
        core: &[3190, 3109, 3050],
        boots: 3117,
        situational: &[3107, 3222, 3075],
        skills: "QEWEEREWEWRWWQQ",
        priority: "EWQ",
    },
    DemoChampion {
        id: 122,
        position: Position::Top,
        tier: Tier::A,
        win: 0.511,
        pick: 0.084,
        ban: 0.198,
        games: 55_600,
        primary: 8000,
        sub: 8400,
        perks: [8010, 9111, 9105, 8299, 8444, 8242],
        shards: AD,
        spells: (4, 6),
        starting: &[1054, 2003],
        core: &[3078, 3053, 3742],
        boots: 3047,
        situational: &[3065, 3075, 6333],
        skills: "QEWQQRQWQWRWWEE",
        priority: "QWE",
    },
    DemoChampion {
        id: 234,
        position: Position::Jungle,
        tier: Tier::B,
        win: 0.492,
        pick: 0.088,
        ban: 0.071,
        games: 49_800,
        primary: 8000,
        sub: 8100,
        perks: [8010, 9111, 9104, 8299, 8143, 8135],
        shards: MARKSMAN,
        spells: (4, 11),
        starting: &[1101, 2031],
        core: &[3153, 6672, 3053],
        boots: 3006,
        situational: &[6333, 3026, 3072],
        skills: "QWEQQRQEQEREEWW",
        priority: "QEW",
    },
    DemoChampion {
        id: 89,
        position: Position::Support,
        tier: Tier::B,
        win: 0.497,
        pick: 0.061,
        ban: 0.034,
        games: 41_800,
        primary: 8400,
        sub: 8300,
        perks: [8439, 8463, 8473, 8242, 8345, 8347],
        shards: TANK,
        spells: (4, 14),
        starting: &[3865, 2003],
        core: &[3190, 3109, 3050],
        boots: 3047,
        situational: &[3075, 3065, 3193],
        skills: "QWEWWRWEWEREEQQ",
        priority: "WEQ",
    },
    DemoChampion {
        id: 81,
        position: Position::Bot,
        tier: Tier::C,
        win: 0.481,
        pick: 0.154,
        ban: 0.032,
        games: 121_000,
        primary: 8000,
        sub: 8300,
        perks: [8010, 9111, 9104, 8299, 8304, 8347],
        shards: AD,
        spells: (4, 7),
        starting: &[1055, 2003],
        core: &[3078, 3004, 3508],
        boots: 3158,
        situational: &[6694, 3036, 3026],
        skills: "QEWQQRQWQWRWWEE",
        priority: "QWE",
    },
    DemoChampion {
        id: 99,
        position: Position::Mid,
        tier: Tier::A,
        win: 0.517,
        pick: 0.052,
        ban: 0.046,
        games: 47_400,
        primary: 8200,
        sub: 8300,
        perks: [8229, 8226, 8210, 8237, 8345, 8347],
        shards: AP,
        spells: (4, 14),
        starting: &[1056, 2003],
        core: &[6655, 4645, 3089],
        boots: 3020,
        situational: &[3135, 3157, 4628],
        skills: "EQWEEREQEQRQQWW",
        priority: "EQW",
    },
    DemoChampion {
        id: 238,
        position: Position::Mid,
        tier: Tier::B,
        win: 0.498,
        pick: 0.081,
        ban: 0.211,
        games: 63_100,
        primary: 8100,
        sub: 8000,
        perks: [8112, 8143, 8140, 8106, 9111, 8014],
        shards: AD,
        spells: (4, 14),
        starting: &[1036, 2003],
        core: &[3142, 6701, 6694],
        boots: 3158,
        situational: &[3814, 6333, 3026],
        skills: "QEWQQRQEQEREEWW",
        priority: "QEW",
    },
    DemoChampion {
        id: 86,
        position: Position::Top,
        tier: Tier::B,
        win: 0.506,
        pick: 0.059,
        ban: 0.047,
        games: 39_500,
        primary: 8000,
        sub: 8400,
        perks: [8010, 9111, 9105, 8299, 8444, 8242],
        shards: AD,
        spells: (4, 14),
        starting: &[1055, 2003],
        core: &[6672, 3053, 3742],
        boots: 3047,
        situational: &[3065, 3026, 6333],
        skills: "QEWEEREQEQRQQWW",
        priority: "EQW",
    },
    DemoChampion {
        id: 145,
        position: Position::Bot,
        tier: Tier::A,
        win: 0.502,
        pick: 0.168,
        ban: 0.074,
        games: 132_000,
        primary: 8000,
        sub: 8100,
        perks: [8005, 9111, 9104, 8014, 8139, 8135],
        shards: MARKSMAN,
        spells: (4, 7),
        starting: &[1055, 2003],
        core: &[6672, 3124, 3115],
        boots: 3006,
        situational: &[3089, 3157, 3026],
        skills: "QEWQQRQEQEREEWW",
        priority: "QEW",
    },
    DemoChampion {
        id: 111,
        position: Position::Support,
        tier: Tier::S,
        win: 0.523,
        pick: 0.087,
        ban: 0.064,
        games: 58_800,
        primary: 8400,
        sub: 8300,
        perks: [8439, 8401, 8473, 8242, 8345, 8347],
        shards: TANK,
        spells: (4, 14),
        starting: &[3865, 2003],
        core: &[3190, 3050, 3109],
        boots: 3047,
        situational: &[3075, 3065, 3222],
        skills: "QEWEEREWEWRWWQQ",
        priority: "EWQ",
    },
];

/// Every champion with demo Champion Stats.
pub fn demo_champion_ids() -> impl Iterator<Item = i64> {
    CHAMPIONS.iter().map(|c| c.id)
}

fn find(champion_id: i64) -> Option<&'static DemoChampion> {
    CHAMPIONS.iter().find(|c| c.id == champion_id)
}

fn skills(letters: &str) -> Vec<Skill> {
    letters
        .chars()
        .filter_map(|c| match c {
            'Q' => Some(Skill::Q),
            'W' => Some(Skill::W),
            'E' => Some(Skill::E),
            'R' => Some(Skill::R),
            _ => None,
        })
        .collect()
}

fn rates(win: f32, pick: f32, games: u32) -> Rates {
    Rates {
        win_rate: win,
        pick_rate: pick,
        ban_rate: 0.0,
        games,
    }
}

fn runes(c: &DemoChampion) -> RuneSetup {
    RuneSetup {
        primary_style_id: c.primary,
        sub_style_id: c.sub,
        perk_ids: c.perks.to_vec(),
        shard_ids: c.shards.to_vec(),
    }
}

fn demo_query(c: &DemoChampion, query: &StatsQuery) -> StatsQuery {
    StatsQuery {
        champion_id: c.id,
        position: Some(c.position),
        queue: query.queue,
        bracket: query.bracket,
        patch: Some(DEMO_PATCH.to_string()),
    }
}

/// Demo stand-in for every Champion Stats source.
#[derive(Debug, Default, Clone, Copy)]
pub struct DemoStats;

impl DemoStats {
    /// What the client's recommended pages would give: the champion's page and spells.
    pub fn recommended_build(&self, query: &StatsQuery) -> StatsResult<ChampionBuild> {
        let c = find(query.champion_id).ok_or(StatsUnavailable::NoData)?;
        Ok(ChampionBuild {
            query: demo_query(c, query),
            origin: StatsOrigin::Demo,
            runes: vec![(runes(c), Rates::default())],
            spells: vec![(
                SpellPair {
                    first: c.spells.0,
                    second: c.spells.1,
                },
                Rates::default(),
            )],
            ..ChampionBuild::default()
        })
    }

    pub fn matchup_list(&self, query: &StatsQuery) -> StatsResult<MatchupList> {
        let c = find(query.champion_id).ok_or(StatsUnavailable::NoData)?;
        Ok(MatchupList {
            query: demo_query(c, query),
            origin: StatsOrigin::Demo,
            entries: self.matchups(query)?,
            attribution: None,
        })
    }

    /// Game data covering every id the demo uses, named in `lang`.
    pub fn game_data(&self, lang: Lang) -> GameData {
        let entries = |table: &[(i64, &'static str, &'static str)]| {
            table
                .iter()
                .map(|(id, en, fr)| GameDataEntry {
                    id: *id,
                    name: lang.pick(en, fr).to_string(),
                    ..GameDataEntry::default()
                })
                .collect()
        };
        GameData {
            items: entries(ITEMS),
            perks: entries(PERKS),
            perk_styles: entries(STYLES),
            spells: entries(SPELLS),
            boots: Vec::new(),
        }
    }

    /// Twenty recent games of the demo player (mostly Ahri, the demo pick), aggregated.
    pub fn player_profile(&self) -> PlayerProfile {
        const SCHEDULE: [i64; 20] = [
            103, 103, 99, 103, 238, 103, 222, 99, 103, 238, 103, 99, 222, 103, 238, 99, 103, 222,
            238, 99,
        ];
        const BASE_MS: i64 = 1_795_420_800_000;
        let games = SCHEDULE
            .iter()
            .enumerate()
            .map(|(i, &champion_id)| {
                let c = find(champion_id).expect("demo schedule uses demo champions");
                let n = i as u32;
                let win = (n * 7 + champion_id as u32) % 5 < 3;
                let duration_secs = 1500 + (n * 97) % 900;
                let minutes = duration_secs / 60;
                PlayerGame {
                    game_id: 7_100_000_000 + i as i64,
                    champion_id,
                    position: Some(c.position),
                    queue: Some(if i % 4 == 3 {
                        StatsQueue::RankedFlex
                    } else {
                        StatsQueue::RankedSolo
                    }),
                    win,
                    duration_secs,
                    started_at_ms: BASE_MS - i as i64 * 5 * 3_600_000,
                    kills: if win { 7 + n % 5 } else { 3 + n % 3 },
                    deaths: if win { 2 + n % 3 } else { 5 + n % 3 },
                    assists: 6 + (n * 3) % 7,
                    damage_to_champions: minutes * (720 + (n * 37) % 260),
                    cs: minutes * (6 + n % 3) + minutes / 2,
                    gold: minutes * (380 + (n * 11) % 70),
                    vision_score: minutes * 9 / 10 + n % 6,
                    items: c
                        .core
                        .iter()
                        .chain(&[c.boots])
                        .chain(c.situational.iter().take((n % 3) as usize))
                        .copied()
                        .collect(),
                    runes: Some(runes(c)),
                    spells: SpellPair {
                        first: c.spells.0,
                        second: c.spells.1,
                    },
                }
            })
            .collect();
        build_profile(games, StatsOrigin::Demo)
    }
}

impl StatsSource for DemoStats {
    fn build(&self, query: &StatsQuery) -> StatsResult<ChampionBuild> {
        let c = find(query.champion_id).ok_or(StatsUnavailable::NoData)?;
        let total = c.games;
        let block = |stage, ids: &[i64], share: f32, win: f32| ItemBlock {
            stage: Some(stage),
            item_ids: ids.to_vec(),
            rates: rates(win, share, (total as f32 * share) as u32),
        };
        let alt_runes = RuneSetup {
            perk_ids: {
                let mut p = c.perks.to_vec();
                p.swap(4, 5);
                p
            },
            ..runes(c)
        };
        Ok(ChampionBuild {
            query: demo_query(c, query),
            origin: StatsOrigin::Demo,
            rates: Rates {
                win_rate: c.win,
                pick_rate: c.pick,
                ban_rate: c.ban,
                games: total,
            },
            runes: vec![
                (runes(c), rates(c.win + 0.008, 0.46, total * 46 / 100)),
                (alt_runes, rates(c.win - 0.006, 0.18, total * 18 / 100)),
            ],
            spells: vec![(
                SpellPair {
                    first: c.spells.0,
                    second: c.spells.1,
                },
                rates(c.win + 0.002, 0.81, total * 81 / 100),
            )],
            items: vec![
                block(ItemStage::Starting, c.starting, 0.74, c.win),
                block(ItemStage::Core, c.core, 0.31, c.win + 0.024),
                block(ItemStage::Boots, &[c.boots], 0.62, c.win + 0.006),
                block(ItemStage::Situational, c.situational, 0.12, c.win + 0.011),
            ],
            skills: vec![SkillOrder {
                levels: skills(c.skills),
                priority: skills(c.priority),
                rates: rates(c.win + 0.004, 0.58, total * 58 / 100),
            }],
            attribution: None,
        })
    }

    fn tier_list(&self, queue: StatsQueue, bracket: RankBracket) -> StatsResult<TierList> {
        let lanes = matches!(
            queue,
            StatsQueue::RankedSolo | StatsQueue::RankedFlex | StatsQueue::Normal
        );
        let mut entries: Vec<TierEntry> = CHAMPIONS
            .iter()
            .map(|c| TierEntry {
                champion_id: c.id,
                position: if lanes { c.position } else { Position::Fill },
                tier: c.tier,
                rates: Rates {
                    win_rate: c.win,
                    pick_rate: c.pick,
                    ban_rate: c.ban,
                    games: c.games,
                },
            })
            .collect();
        entries.sort_by(|a, b| {
            a.tier
                .cmp(&b.tier)
                .then(b.rates.win_rate.total_cmp(&a.rates.win_rate))
        });
        Ok(TierList {
            queue,
            bracket,
            patch: DEMO_PATCH.to_string(),
            origin: StatsOrigin::Demo,
            entries,
            attribution: None,
        })
    }

    fn matchups(&self, query: &StatsQuery) -> StatsResult<Vec<Matchup>> {
        let c = find(query.champion_id).ok_or(StatsUnavailable::NoData)?;
        let mut list: Vec<Matchup> = CHAMPIONS
            .iter()
            .filter(|o| o.id != c.id && o.position == c.position)
            .map(|o| {
                let edge = lane_edge(c.id, o.id);
                Matchup {
                    champion_id: c.id,
                    opponent_id: o.id,
                    position: Some(c.position),
                    rates: Rates {
                        win_rate: (0.5 + (c.win - o.win) * 0.8 + edge).clamp(0.3, 0.7),
                        pick_rate: 0.0,
                        ban_rate: 0.0,
                        games: 300 + ((c.id + o.id) * 37 % 900) as u32,
                    },
                    gold_diff_15: Some(((c.win - o.win) * 1500.0 + edge * 4000.0).round()),
                    origin: StatsOrigin::Demo,
                }
            })
            .collect();
        list.sort_by(|a, b| b.rates.win_rate.total_cmp(&a.rates.win_rate));
        Ok(list)
    }
}

/// A small, antisymmetric lane edge so both sides of a matchup agree.
fn lane_edge(a: i64, b: i64) -> f32 {
    let (lo, hi) = (a.min(b), a.max(b));
    let raw = ((lo * 31 + hi * 17) % 9 - 4) as f32 * 0.006;
    if a == lo {
        raw
    } else {
        -raw
    }
}

const ITEMS: &[(i64, &str, &str)] = &[
    (1036, "Long Sword", "Épée longue"),
    (1054, "Doran's Shield", "Bouclier de Doran"),
    (1055, "Doran's Blade", "Lame de Doran"),
    (1056, "Doran's Ring", "Anneau de Doran"),
    (1101, "Scorchclaw Pup", "Chiot Griffe-ardente"),
    (2003, "Health Potion", "Potion de soin"),
    (2031, "Refillable Potion", "Potion rechargeable"),
    (3865, "World Atlas", "Atlas mondial"),
    (3004, "Manamune", "Manamune"),
    (3006, "Berserker's Greaves", "Jambières du berserker"),
    (3020, "Sorcerer's Shoes", "Chaussures du sorcier"),
    (3026, "Guardian Angel", "Ange gardien"),
    (3031, "Infinity Edge", "Lame d'infini"),
    (
        3036,
        "Lord Dominik's Regards",
        "Salutations de Lord Dominik",
    ),
    (3047, "Plated Steelcaps", "Coques en acier renforcé"),
    (3050, "Zeke's Convergence", "Convergence de Zeke"),
    (3053, "Sterak's Gage", "Gage de Sterak"),
    (3065, "Spirit Visage", "Visage spirituel"),
    (3071, "Black Cleaver", "Couperet noir"),
    (3072, "Bloodthirster", "Soif-de-sang"),
    (3075, "Thornmail", "Cotte épineuse"),
    (3078, "Trinity Force", "Force de la trinité"),
    (3085, "Runaan's Hurricane", "Ouragan de Runaan"),
    (3089, "Rabadon's Deathcap", "Coiffe de Rabadon"),
    (3102, "Banshee's Veil", "Voile de la banshee"),
    (3107, "Redemption", "Rédemption"),
    (3109, "Knight's Vow", "Serment du chevalier"),
    (3115, "Nashor's Tooth", "Dent de Nashor"),
    (3117, "Mobility Boots", "Bottes de mobilité"),
    (3118, "Malignance", "Malignance"),
    (3124, "Guinsoo's Rageblade", "Lame enragée de Guinsoo"),
    (3135, "Void Staff", "Bâton du vide"),
    (3142, "Youmuu's Ghostblade", "Lame spectrale de Youmuu"),
    (3153, "Blade of The Ruined King", "Lame du roi déchu"),
    (3156, "Maw of Malmortius", "Gueule de Malmortius"),
    (3157, "Zhonya's Hourglass", "Sablier de Zhonya"),
    (
        3158,
        "Ionian Boots of Lucidity",
        "Bottes ioniennes de lucidité",
    ),
    (
        3190,
        "Locket of the Iron Solari",
        "Médaillon de l'Iron Solari",
    ),
    (
        3193,
        "Gargoyle Stoneplate",
        "Plaque de pierre de gargouille",
    ),
    (3222, "Mikael's Blessing", "Bénédiction de Mikael"),
    (3508, "Essence Reaver", "Voleur d'essence"),
    (3742, "Dead Man's Plate", "Plaque du mort"),
    (3814, "Edge of Night", "Lisière de la nuit"),
    (4628, "Horizon Focus", "Focaliseur d'horizon"),
    (4645, "Shadowflame", "Flamme-ombre"),
    (6333, "Death's Dance", "Danse de la mort"),
    (6655, "Luden's Companion", "Compagnon de Luden"),
    (6672, "Kraken Slayer", "Tueur de krakens"),
    (6692, "Eclipse", "Éclipse"),
    (6694, "Serylda's Grudge", "Rancune de Serylda"),
    (6701, "Opportunity", "Opportunité"),
];

const PERKS: &[(i64, &str, &str)] = &[
    (8005, "Press the Attack", "Attaque soutenue"),
    (8008, "Lethal Tempo", "Tempo mortel"),
    (8010, "Conqueror", "Conquérant"),
    (9101, "Absorb Life", "Absorption de vie"),
    (9111, "Triumph", "Triomphe"),
    (9104, "Legend: Alacrity", "Légende : alacrité"),
    (9105, "Legend: Haste", "Légende : hâte"),
    (8014, "Coup de Grace", "Coup de grâce"),
    (8299, "Last Stand", "Baroud d'honneur"),
    (8112, "Electrocute", "Électrocution"),
    (8139, "Taste of Blood", "Goût du sang"),
    (8143, "Sudden Impact", "Impact soudain"),
    (8140, "Grisly Mementos", "Souvenirs macabres"),
    (8135, "Treasure Hunter", "Chasseur de trésors"),
    (8106, "Ultimate Hunter", "Chasseur ultime"),
    (8229, "Arcane Comet", "Comète arcanique"),
    (8226, "Manaflow Band", "Bande de flux de mana"),
    (8210, "Transcendence", "Transcendance"),
    (8237, "Scorch", "Brûlure"),
    (8439, "Aftershock", "Répercussion"),
    (8463, "Font of Life", "Source de vie"),
    (8401, "Shield Bash", "Coup de bouclier"),
    (8444, "Second Wind", "Second souffle"),
    (8473, "Bone Plating", "Plaque osseuse"),
    (8453, "Revitalize", "Revitalisation"),
    (8242, "Unflinching", "Inébranlable"),
    (8304, "Magical Footwear", "Chaussures magiques"),
    (8345, "Biscuit Delivery", "Livraison de biscuits"),
    (8347, "Cosmic Insight", "Perspicacité cosmique"),
    (5001, "Health Scaling", "PV croissants"),
    (5005, "Attack Speed", "Vitesse d'attaque"),
    (5007, "Ability Haste", "Accélération de compétence"),
    (5008, "Adaptive Force", "Force adaptative"),
    (5011, "Health", "PV"),
];

const STYLES: &[(i64, &str, &str)] = &[
    (8000, "Precision", "Précision"),
    (8100, "Domination", "Domination"),
    (8200, "Sorcery", "Sorcellerie"),
    (8300, "Inspiration", "Inspiration"),
    (8400, "Resolve", "Volonté"),
];

const SPELLS: &[(i64, &str, &str)] = &[
    (4, "Flash", "Saut éclair"),
    (6, "Ghost", "Fantôme"),
    (7, "Heal", "Soins"),
    (11, "Smite", "Châtiment"),
    (12, "Teleport", "Téléportation"),
    (14, "Ignite", "Embrasement"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Champions of the demo draft (`fixtures.rs` allies, enemies and ban) and live game.
    const DEMO_DRAFT_AND_LIVE: [i64; 15] = [
        266, 64, 103, 222, 412, 122, 234, 89, 81, 99, 238, 86, 145, 111, 64,
    ];

    #[test]
    fn covers_every_demo_champion() {
        let demo = DemoStats;
        for id in DEMO_DRAFT_AND_LIVE {
            let query = StatsQuery {
                champion_id: id,
                ..StatsQuery::default()
            };
            let build = demo.build(&query).unwrap();
            assert_eq!(build.origin, StatsOrigin::Demo);
            assert_eq!(build.runes[0].0.perk_ids.len(), 6, "{id}");
            assert_eq!(build.skills[0].levels.len(), 15, "{id} skill order");
            assert_eq!(build.items.len(), 4);
            assert!(
                !demo.matchups(&query).unwrap().is_empty(),
                "{id} has lane opponents"
            );
            assert_eq!(
                demo.recommended_build(&query).unwrap().origin,
                StatsOrigin::Demo
            );
        }
        assert_eq!(
            demo.build(&StatsQuery {
                champion_id: 1,
                ..StatsQuery::default()
            }),
            Err(StatsUnavailable::NoData)
        );
    }

    #[test]
    fn game_data_names_every_demo_id_in_both_languages() {
        let demo = DemoStats;
        let (en, fr) = (demo.game_data(Lang::En), demo.game_data(Lang::Fr));
        let has = |list: &[GameDataEntry], id: i64| list.iter().any(|e| e.id == id);
        for c in CHAMPIONS {
            for id in c
                .starting
                .iter()
                .chain(c.core)
                .chain(c.situational)
                .chain([&c.boots])
            {
                assert!(has(&en.items, *id), "item {id}");
            }
            for id in c.perks.iter().chain(&c.shards) {
                assert!(has(&en.perks, *id), "perk {id}");
            }
            assert!(has(&en.perk_styles, c.primary) && has(&en.perk_styles, c.sub));
            assert!(has(&en.spells, c.spells.0) && has(&en.spells, c.spells.1));
        }
        assert_eq!(
            fr.spells.iter().find(|s| s.id == 4).unwrap().name,
            "Saut éclair"
        );
        assert_eq!(en.spells.iter().find(|s| s.id == 4).unwrap().name, "Flash");
    }

    #[test]
    fn matchups_agree_from_both_sides() {
        let demo = DemoStats;
        let ahri_vs = demo
            .matchups(&StatsQuery {
                champion_id: 103,
                ..StatsQuery::default()
            })
            .unwrap();
        let zed_vs = demo
            .matchups(&StatsQuery {
                champion_id: 238,
                ..StatsQuery::default()
            })
            .unwrap();
        let a = ahri_vs
            .iter()
            .find(|m| m.opponent_id == 238)
            .unwrap()
            .rates
            .win_rate;
        let z = zed_vs
            .iter()
            .find(|m| m.opponent_id == 103)
            .unwrap()
            .rates
            .win_rate;
        assert!((a + z - 1.0).abs() < 1e-4);
        assert!(ahri_vs
            .windows(2)
            .all(|w| w[0].rates.win_rate >= w[1].rates.win_rate));
    }

    #[test]
    fn profile_and_tier_list_are_demo_labelled() {
        let profile = DemoStats.player_profile();
        assert_eq!(profile.origin, StatsOrigin::Demo);
        assert_eq!(profile.games.len(), 20);
        assert_eq!(
            profile.champions[0].champion_id, 103,
            "Ahri is the demo main"
        );
        let tiers = DemoStats
            .tier_list(StatsQueue::RankedSolo, RankBracket::EmeraldPlus)
            .unwrap();
        assert_eq!(tiers.origin, StatsOrigin::Demo);
        assert_eq!(tiers.entries.len(), CHAMPIONS.len());
        assert!(tiers.entries.windows(2).all(|w| w[0].tier <= w[1].tier));
        let aram = DemoStats
            .tier_list(StatsQueue::Aram, RankBracket::All)
            .unwrap();
        assert!(aram.entries.iter().all(|e| e.position == Position::Fill));
    }
}
