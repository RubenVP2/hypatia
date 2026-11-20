//! My Stats: pure aggregation of the player's own games into a [`PlayerProfile`].

use std::collections::HashMap;

use hypatia_shared::{
    summarised_games, PlayerChampionStats, PlayerGame, PlayerProfile, Position, RoleSummary,
    RuneSetup, SessionGroup, SpellPair, StatsOrigin,
};

pub use hypatia_shared::REMAKE_SECS;

/// Items that say nothing about a build: trinkets, potions, wards, elixirs.
const NON_BUILD_ITEMS: &[i64] = &[
    2003, 2010, 2031, 2033, 2055, 2138, 2139, 2140, 3330, 3340, 3363, 3364, 3513,
];

/// How many of a champion's most frequent items are kept.
const TOP_ITEMS: usize = 6;

/// Sorts `games` newest first and summarizes them per champion and per role.
pub fn build_profile(mut games: Vec<PlayerGame>, origin: StatsOrigin) -> PlayerProfile {
    games.sort_by_key(|a| std::cmp::Reverse(a.started_at_ms));
    let champions = aggregate_champions(&games);
    let roles = aggregate_roles(&games);
    PlayerProfile {
        origin,
        games,
        champions,
        roles,
    }
}

/// Per-role performance breakdown across the 5 Summoner's Rift positions, over
/// [`summarised_games`]. Ordered by games played descending, then canonical lane order.
pub fn aggregate_roles(games: &[PlayerGame]) -> Vec<RoleSummary> {
    const LANES: [Position; 5] = [
        Position::Top,
        Position::Jungle,
        Position::Mid,
        Position::Bot,
        Position::Support,
    ];
    let summarised = summarised_games(games);
    let mut out: Vec<RoleSummary> = LANES
        .iter()
        .filter_map(|&pos| {
            let lane_games: Vec<&PlayerGame> = summarised
                .iter()
                .copied()
                .filter(|g| g.position == Some(pos))
                .collect();
            if lane_games.is_empty() {
                return None;
            }
            let wins = lane_games.iter().filter(|g| g.win).count() as u32;
            let (k, d, a) = lane_games.iter().fold((0u32, 0u32, 0u32), |acc, g| {
                (acc.0 + g.kills, acc.1 + g.deaths, acc.2 + g.assists)
            });
            let minutes: f32 = lane_games
                .iter()
                .map(|g| g.duration_secs as f32 / 60.0)
                .sum::<f32>()
                .max(1.0);
            let cs: u32 = lane_games.iter().map(|g| g.cs).sum();
            let dmg: u32 = lane_games.iter().map(|g| g.damage_to_champions).sum();
            Some(RoleSummary {
                position: pos,
                games: lane_games.len() as u32,
                wins,
                kda: (k + a) as f32 / d.max(1) as f32,
                cs_per_min: cs as f32 / minutes,
                dpm: dmg as f32 / minutes,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.games
            .cmp(&a.games)
            .then_with(|| a.position.desktop_index().cmp(&b.position.desktop_index()))
    });
    out
}

/// Groups newest-first games into UTC calendar-day sessions with W-L record (excluding remakes)
/// and average performance score.
pub fn group_sessions(games: &[&PlayerGame]) -> Vec<SessionGroup> {
    let mut groups: Vec<SessionGroup> = Vec::new();
    for (idx, g) in games.iter().enumerate() {
        let day_key = if g.started_at_ms > 0 {
            g.started_at_ms.div_euclid(86_400_000)
        } else {
            0
        };
        if let Some(last) = groups.last_mut().filter(|s| s.day_key == day_key) {
            last.game_indices.push(idx);
        } else {
            groups.push(SessionGroup {
                day_key,
                wins: 0,
                losses: 0,
                avg_score: None,
                game_indices: vec![idx],
            });
        }
    }
    for group in &mut groups {
        let mut score_sum = 0u32;
        let mut score_count = 0u32;
        for &idx in &group.game_indices {
            let g = games[idx];
            if !g.is_remake() {
                if g.win {
                    group.wins += 1;
                } else {
                    group.losses += 1;
                }
            }
            if let Some(score) = g.performance_score {
                score_sum += u32::from(score);
                score_count += 1;
            }
        }
        group.avg_score = (score_count > 0).then(|| score_sum as f32 / score_count as f32);
    }
    groups
}

/// One summary per champion, most played first (ties: most recent first), over
/// [`summarised_games`]: Summoner's Rift games when there are any, never remakes.
/// `games` should be newest first: ties between rune pages and items go to the most recent.
pub fn aggregate_champions(games: &[PlayerGame]) -> Vec<PlayerChampionStats> {
    let mut order: Vec<i64> = Vec::new();
    let mut by_champion: HashMap<i64, Vec<&PlayerGame>> = HashMap::new();
    for game in summarised_games(games) {
        by_champion
            .entry(game.champion_id)
            .or_insert_with(|| {
                order.push(game.champion_id);
                Vec::new()
            })
            .push(game);
    }
    let mut out: Vec<PlayerChampionStats> = order
        .iter()
        .map(|id| summarize(*id, &by_champion[id]))
        .collect();
    // Stable: equal game counts keep their most-recent-first order.
    out.sort_by_key(|a| std::cmp::Reverse(a.games));
    out
}

/// Summary of one champion's games: per-game KDA averages, per-minute rates over the total
/// time played, the most played rune page, spells, and the most frequent build items.
pub fn summarize(champion_id: i64, games: &[&PlayerGame]) -> PlayerChampionStats {
    let n = games.len() as f32;
    let minutes: f32 = games.iter().map(|g| g.duration_secs as f32 / 60.0).sum();
    let per_game = |f: fn(&PlayerGame) -> u32| {
        if n > 0.0 {
            games.iter().map(|g| f(g) as f32).sum::<f32>() / n
        } else {
            0.0
        }
    };
    let per_min = |f: fn(&PlayerGame) -> u32| {
        if minutes > 0.0 {
            games.iter().map(|g| f(g) as f32).sum::<f32>() / minutes
        } else {
            0.0
        }
    };
    PlayerChampionStats {
        champion_id,
        games: games.len() as u32,
        wins: games.iter().filter(|g| g.win).count() as u32,
        kills: per_game(|g| g.kills),
        deaths: per_game(|g| g.deaths),
        assists: per_game(|g| g.assists),
        dpm: per_min(|g| g.damage_to_champions),
        cs_per_min: per_min(|g| g.cs),
        gold_per_min: per_min(|g| g.gold),
        vision_per_min: per_min(|g| g.vision_score),
        runes: most_used_runes(games),
        spells: most_used_spells(games),
        items: most_used_items(games),
    }
}

/// `(kills + assists) / max(deaths, 1)`, the usual KDA ratio.
pub fn kda_ratio(stats: &PlayerChampionStats) -> f32 {
    (stats.kills + stats.assists) / stats.deaths.max(1.0)
}

fn most_used_runes(games: &[&PlayerGame]) -> Option<RuneSetup> {
    let mut tally: Vec<(&RuneSetup, u32)> = Vec::new();
    for runes in games.iter().filter_map(|g| g.runes.as_ref()) {
        match tally.iter_mut().find(|(r, _)| *r == runes) {
            Some((_, n)) => *n += 1,
            None => tally.push((runes, 1)),
        }
    }
    // `max_by_key` keeps the last maximum; reverse so the first (most recent) wins ties.
    tally
        .into_iter()
        .rev()
        .max_by_key(|(_, n)| *n)
        .map(|(r, _)| r.clone())
}

fn most_used_spells(games: &[&PlayerGame]) -> Option<SpellPair> {
    let mut tally: Vec<(SpellPair, u32)> = Vec::new();
    for g in games {
        if g.spells.first <= 0 || g.spells.second <= 0 {
            continue;
        }
        let pair = if g.spells.first <= g.spells.second {
            g.spells
        } else {
            SpellPair {
                first: g.spells.second,
                second: g.spells.first,
            }
        };
        match tally.iter_mut().find(|(p, _)| *p == pair) {
            Some((_, n)) => *n += 1,
            None => tally.push((pair, 1)),
        }
    }
    tally
        .into_iter()
        .rev()
        .max_by_key(|(_, n)| *n)
        .map(|(p, _)| p)
}

fn most_used_items(games: &[&PlayerGame]) -> Vec<i64> {
    let mut tally: Vec<(i64, u32)> = Vec::new();
    for id in games
        .iter()
        .flat_map(|g| g.items.iter().copied())
        .filter(|id| *id > 0 && !NON_BUILD_ITEMS.contains(id))
    {
        match tally.iter_mut().find(|(i, _)| *i == id) {
            Some((_, n)) => *n += 1,
            None => tally.push((id, 1)),
        }
    }
    tally.sort_by_key(|a| std::cmp::Reverse(a.1));
    tally
        .into_iter()
        .take(TOP_ITEMS)
        .map(|(id, _)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_shared::SpellPair;

    fn game(id: i64, champion_id: i64, started: i64, win: bool) -> PlayerGame {
        PlayerGame {
            game_id: id,
            champion_id,
            win,
            duration_secs: 1800,
            started_at_ms: started,
            kills: 6,
            deaths: 2,
            assists: 8,
            damage_to_champions: 30_000,
            cs: 240,
            gold: 12_000,
            vision_score: 30,
            items: vec![3118, 4645, 3020, 3340, 2003],
            spells: SpellPair {
                first: 4,
                second: 14,
            },
            ..PlayerGame::default()
        }
    }

    fn page(keystone: i64) -> RuneSetup {
        RuneSetup {
            primary_style_id: 8100,
            sub_style_id: 8200,
            perk_ids: vec![keystone, 8139, 8140, 8106, 8226, 8210],
            shard_ids: vec![5008, 5008, 5001],
        }
    }

    #[test]
    fn averages_per_game_and_rates_per_minute() {
        let mut short = game(2, 103, 200, false);
        short.duration_secs = 1200;
        short.kills = 0;
        short.deaths = 4;
        short.assists = 2;
        short.damage_to_champions = 10_000;
        short.cs = 120;
        short.gold = 6_000;
        short.vision_score = 10;
        let games = [game(1, 103, 100, true), short];
        let refs: Vec<&PlayerGame> = games.iter().collect();
        let s = summarize(103, &refs);

        assert_eq!((s.games, s.wins), (2, 1));
        assert_eq!((s.kills, s.deaths, s.assists), (3.0, 3.0, 5.0));
        // 50 minutes in total.
        assert!((s.dpm - 800.0).abs() < 1e-3);
        assert!((s.cs_per_min - 7.2).abs() < 1e-3);
        assert!((s.gold_per_min - 360.0).abs() < 1e-3);
        assert!((s.vision_per_min - 0.8).abs() < 1e-3);
        assert!((kda_ratio(&s) - 8.0 / 3.0).abs() < 1e-3);
        assert_eq!(
            s.items,
            vec![3118, 4645, 3020],
            "trinkets and potions are not build items"
        );
    }

    #[test]
    fn picks_the_most_used_runes_and_items() {
        let mut games: Vec<PlayerGame> = (0..4).map(|i| game(i, 103, 1000 - i, true)).collect();
        games[0].runes = Some(page(8128));
        games[1].runes = Some(page(8112));
        games[2].runes = Some(page(8112));
        games[3].runes = Some(page(8128));
        games[1].items = vec![3089, 3157];
        games[2].items = vec![3089, 3157, 3135];
        let refs: Vec<&PlayerGame> = games.iter().collect();
        let s = summarize(103, &refs);
        assert_eq!(
            s.runes.unwrap().perk_ids[0],
            8128,
            "a tie goes to the most recent page"
        );
        assert_eq!(s.items, vec![3118, 4645, 3020, 3089, 3157, 3135]);
    }

    #[test]
    fn profile_orders_champions_and_skips_remakes() {
        let mut remake = game(9, 99, 5000, false);
        remake.duration_secs = 200;
        let games = vec![
            game(1, 103, 100, true),
            game(2, 222, 400, false),
            game(3, 103, 300, true),
            game(4, 412, 500, true),
            remake,
        ];
        let profile = build_profile(games, StatsOrigin::Local);
        assert_eq!(profile.origin, StatsOrigin::Local);
        assert_eq!(profile.games.len(), 5, "remakes stay in the list");
        assert_eq!(profile.games[0].game_id, 9, "newest first");
        let order: Vec<i64> = profile.champions.iter().map(|c| c.champion_id).collect();
        assert_eq!(order, vec![103, 412, 222], "most played, then most recent");
        assert!(profile.champions.iter().all(|c| c.champion_id != 99));
    }

    #[test]
    fn empty_input_gives_an_empty_profile() {
        let profile = build_profile(Vec::new(), StatsOrigin::Local);
        assert!(
            profile.games.is_empty() && profile.champions.is_empty() && profile.roles.is_empty()
        );
        let s = summarize(1, &[]);
        assert_eq!((s.games, s.kills, s.dpm), (0, 0.0, 0.0));
        assert_eq!(s.spells, None);
    }

    #[test]
    fn aggregates_roles_and_groups_sessions() {
        let day1 = 20_412 * 86_400_000;
        let day2 = 20_413 * 86_400_000;
        let mut g1 = game(1, 103, day2 + 7_200_000, true);
        g1.position = Some(Position::Mid);
        g1.queue = Some(hypatia_shared::StatsQueue::RankedSolo);
        g1.performance_score = Some(86);

        let mut g2 = game(2, 103, day2 + 3_600_000, false);
        g2.position = Some(Position::Mid);
        g2.queue = Some(hypatia_shared::StatsQueue::RankedSolo);
        g2.performance_score = Some(62);

        let mut g3 = game(3, 222, day1 + 3_600_000, true);
        g3.position = Some(Position::Bot);
        g3.queue = Some(hypatia_shared::StatsQueue::RankedSolo);
        g3.performance_score = Some(78);

        let mut remake = game(4, 99, day2 + 1_000_000, false);
        remake.position = Some(Position::Mid);
        remake.queue = Some(hypatia_shared::StatsQueue::RankedSolo);
        remake.duration_secs = 200;

        let profile = build_profile(vec![g1, g2, g3, remake], StatsOrigin::Local);
        assert_eq!(profile.roles.len(), 2);
        assert_eq!(profile.roles[0].position, Position::Mid);
        assert_eq!((profile.roles[0].games, profile.roles[0].wins), (2, 1));
        assert_eq!(profile.roles[1].position, Position::Bot);
        assert_eq!((profile.roles[1].games, profile.roles[1].wins), (1, 1));

        let refs: Vec<&PlayerGame> = profile.games.iter().collect();
        let sessions = group_sessions(&refs);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].day_key, 20_413);
        assert_eq!(
            (sessions[0].wins, sessions[0].losses),
            (1, 1),
            "remake excluded from W/L"
        );
        assert!((sessions[0].avg_score.unwrap() - 74.0).abs() < 1e-3);
        assert_eq!(sessions[0].game_indices, vec![0, 1, 2]);
        assert_eq!(sessions[1].day_key, 20_412);
        assert_eq!((sessions[1].wins, sessions[1].losses), (1, 0));
    }
}
