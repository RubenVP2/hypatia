//! My Stats: pure aggregation of the player's own games into a [`PlayerProfile`].

use std::collections::HashMap;

use hypatia_shared::{PlayerChampionStats, PlayerGame, PlayerProfile, RuneSetup, StatsOrigin};

/// Games shorter than this are remakes: listed, but left out of every average.
pub const REMAKE_SECS: u32 = 300;

/// Items that say nothing about a build: trinkets, potions, wards, elixirs.
const NON_BUILD_ITEMS: &[i64] = &[
    2003, 2010, 2031, 2033, 2055, 2138, 2139, 2140, 3330, 3340, 3363, 3364, 3513,
];

/// How many of a champion's most frequent items are kept.
const TOP_ITEMS: usize = 6;

/// Sorts `games` newest first and summarizes them per champion.
pub fn build_profile(mut games: Vec<PlayerGame>, origin: StatsOrigin) -> PlayerProfile {
    games.sort_by_key(|a| std::cmp::Reverse(a.started_at_ms));
    let champions = aggregate_champions(&games);
    PlayerProfile {
        origin,
        games,
        champions,
    }
}

/// One summary per champion, most played first (ties: most recent first). Remakes are skipped.
/// `games` should be newest first: ties between rune pages and items go to the most recent.
pub fn aggregate_champions(games: &[PlayerGame]) -> Vec<PlayerChampionStats> {
    let mut order: Vec<i64> = Vec::new();
    let mut by_champion: HashMap<i64, Vec<&PlayerGame>> = HashMap::new();
    for game in games.iter().filter(|g| counts(g)) {
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
/// time played, the most played rune page and the most frequent build items.
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
        items: most_used_items(games),
    }
}

/// `(kills + assists) / max(deaths, 1)`, the usual KDA ratio.
pub fn kda_ratio(stats: &PlayerChampionStats) -> f32 {
    (stats.kills + stats.assists) / stats.deaths.max(1.0)
}

fn counts(game: &PlayerGame) -> bool {
    game.duration_secs >= REMAKE_SECS
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
        assert!(profile.games.is_empty() && profile.champions.is_empty());
        let s = summarize(1, &[]);
        assert_eq!((s.games, s.kills, s.dpm), (0, 0.0, 0.0));
    }
}
