//! Local Champion Stats read from the League client: Riot's recommended rune pages and the
//! player's own match history. `GET` only, through [`ReadOnlyRiotClient`].
//!
//! The response shapes here follow the LCU `lol-perks` and `lol-match-history` plugins; the
//! sample files in `crates/core/tests/data/lcu_*.json` mirror them. Check them against the
//! Observatory's endpoint catalog after a client update.

use hypatia_shared::{
    ChampionBuild, MatchBadge, PlayerGame, Position, Rates, RuneSetup, SpellPair, StatsOrigin,
    StatsQuery, StatsQueue, REMAKE_SECS,
};
use serde_json::Value;

use super::Fail;
use crate::riot_http::ReadOnlyRiotClient;

/// Summoner's Rift, the only map Riot recommends rune pages for.
pub const SUMMONERS_RIFT_MAP_ID: i64 = 11;
/// Howling Abyss: every ARAM variant, whatever its queue id.
pub const HOWLING_ABYSS_MAP_ID: i64 = 12;

/// The player's last 50 games, newest first. Only the player's own stats and anonymized
/// opponent/team aggregates (champion IDs and numeric shares) are kept.
pub const MATCH_HISTORY_PATH: &str =
    "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=50";

/// Maximum number of recent games enriched via `/lol-match-history/v1/games/{gameId}` when the
/// summary list only returned a single participant per game.
const MAX_DETAIL_ENRICH: usize = 20;

/// Read locally, and only when a game lists several participants, to find the player's own.
const CURRENT_SUMMONER_PATH: &str = "/lol-summoner/v1/current-summoner";

/// `/lol-perks/v1/recommended-pages/champion/{id}/position/{POSITION}/map/11`; Fill and no
/// position ask for `NONE`.
pub fn recommended_pages_path(champion_id: i64, position: Option<Position>) -> String {
    let position = match position {
        Some(Position::Fill) | None => "NONE",
        Some(p) => p.lcu_code(),
    };
    format!(
        "/lol-perks/v1/recommended-pages/champion/{champion_id}/position/{position}/map/{SUMMONERS_RIFT_MAP_ID}"
    )
}

/// Maps Riot's recommended pages to a [`ChampionBuild`] labelled [`StatsOrigin::RiotClient`].
/// Riot gives no rates, so every `Rates` is empty. `None` when no page is usable.
pub fn parse_recommended_pages(json: &Value, query: &StatsQuery) -> Option<ChampionBuild> {
    let mut build = ChampionBuild {
        query: query.clone(),
        origin: StatsOrigin::RiotClient,
        ..ChampionBuild::default()
    };
    for page in json.as_array().into_iter().flatten() {
        let primary = int(page, &["primaryPerkStyleId", "primaryStyleId"]);
        let sub = int(page, &["secondaryPerkStyleId", "subStyleId"]);
        let perks: Vec<i64> = page
            .get("perks")
            .or_else(|| page.get("selectedPerkIds"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_i64().or_else(|| p.get("id").and_then(Value::as_i64)))
            .collect();
        if let Some(runes) = rune_setup(primary, sub, perks) {
            if !build.runes.iter().any(|(r, _)| *r == runes) {
                build.runes.push((runes, Rates::default()));
            }
        }
        let spells: Vec<i64> = page
            .get("summonerSpellIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect();
        if let [first, second, ..] = spells[..] {
            let pair = SpellPair { first, second };
            if !build.spells.iter().any(|(s, _)| *s == pair) {
                build.spells.push((pair, Rates::default()));
            }
        }
    }
    (!build.runes.is_empty()).then_some(build)
}

/// Builds a rune page from a style pair and its perk ids, shards (`5000..6000`) set apart.
pub(crate) fn rune_setup(primary: i64, sub: i64, ids: Vec<i64>) -> Option<RuneSetup> {
    let (shard_ids, perk_ids): (Vec<i64>, Vec<i64>) = ids
        .into_iter()
        .filter(|id| *id > 0)
        .partition(|id| (5000..6000).contains(id));
    (primary > 0 && !perk_ids.is_empty()).then_some(RuneSetup {
        primary_style_id: primary,
        sub_style_id: sub.max(0),
        perk_ids,
        shard_ids,
    })
}

/// Whether some game lists several participants, so the player's own must be found by PUUID.
pub fn needs_identity(json: &Value) -> bool {
    games_array(json).iter().any(|g| {
        g.get("participants")
            .and_then(Value::as_array)
            .is_some_and(|p| p.len() > 1)
    })
}

/// Maps the match history list to the player's games, newest first. `self_puuid` is only
/// compared locally to pick the player's participant; no other player's PUUID or name is kept.
pub fn parse_match_history(json: &Value, self_puuid: Option<&str>) -> Vec<PlayerGame> {
    let mut games: Vec<PlayerGame> = games_array(json)
        .iter()
        .filter_map(|g| parse_game(g, self_puuid))
        .collect();
    games.sort_by_key(|a| std::cmp::Reverse(a.started_at_ms));
    games
}

/// Deterministic local `0..=100` performance score from per-game stats and optional team shares.
#[allow(clippy::too_many_arguments)]
pub fn compute_performance_score(
    kills: u32,
    deaths: u32,
    assists: u32,
    damage: u32,
    cs: u32,
    gold: u32,
    vision: u32,
    duration_secs: u32,
    win: bool,
    position: Option<Position>,
    kill_participation: Option<f32>,
    damage_share: Option<f32>,
) -> u8 {
    let minutes = (duration_secs as f32 / 60.0).max(1.0);
    let kda = (kills + assists) as f32 / deaths.max(1) as f32;
    let kda_pts = (kda.min(6.0) / 6.0) * 32.0;

    let kp_pts = kill_participation
        .map(|kp| (kp.clamp(0.0, 0.85) / 0.85) * 20.0)
        .unwrap_or(10.0);

    let dpm = damage as f32 / minutes;
    let dmg_pts = damage_share
        .map(|ds| (ds.clamp(0.0, 0.35) / 0.35) * 18.0)
        .unwrap_or_else(|| (dpm.min(900.0) / 900.0) * 18.0);

    let cs_min = cs as f32 / minutes;
    let vs_min = vision as f32 / minutes;
    let farm_vision_pts = if position == Some(Position::Support) {
        (vs_min.min(2.2) / 2.2) * 18.0
    } else {
        (cs_min.min(9.0) / 9.0) * 12.0 + (vs_min.min(1.4) / 1.4) * 6.0
    };

    let gpm = gold as f32 / minutes;
    let econ_pts = (gpm.min(500.0) / 500.0) * 6.0 + if win { 6.0 } else { 0.0 };

    (kda_pts + kp_pts + dmg_pts + farm_vision_pts + econ_pts)
        .clamp(0.0, 100.0)
        .round() as u8
}

fn games_array(json: &Value) -> &[Value] {
    let list = match json.get("games") {
        Some(games) => games.get("games").unwrap_or(games),
        None => json,
    };
    list.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn parse_game(game: &Value, self_puuid: Option<&str>) -> Option<PlayerGame> {
    let participants = game.get("participants")?.as_array()?;
    let me_idx = if participants.len() == 1 {
        0
    } else {
        let puuid = self_puuid.filter(|p| !p.is_empty())?;
        let participant_id = game
            .get("participantIdentities")?
            .as_array()?
            .iter()
            .find(|i| i.pointer("/player/puuid").and_then(Value::as_str) == Some(puuid))?
            .get("participantId")?
            .as_i64()?;
        participants
            .iter()
            .position(|p| p.get("participantId").and_then(Value::as_i64) == Some(participant_id))?
    };
    let me = &participants[me_idx];
    let stats = me.get("stats")?;
    let champion_id = int(me, &["championId"]);
    if champion_id <= 0 {
        return None;
    }

    // Seconds in the match history; a few client builds report milliseconds.
    let raw_duration = int(game, &["gameDuration"]).max(0);
    let duration_secs = if raw_duration > 20_000 {
        raw_duration / 1000
    } else {
        raw_duration
    };
    let duration_u32 = count(duration_secs);

    let items = (0..6)
        .map(|slot| int(stats, &[&format!("item{slot}")]))
        .filter(|id| *id > 0)
        .collect();
    let perks = (0..6)
        .map(|slot| int(stats, &[&format!("perk{slot}")]))
        .chain((0..3).map(|slot| int(stats, &[&format!("statPerk{slot}")])));
    let runes = rune_setup(
        int(stats, &["perkPrimaryStyle"]),
        int(stats, &["perkSubStyle"]),
        perks.collect(),
    );

    // Event queues come and go; the map says whether lanes mean anything.
    let map_id = int(game, &["mapId"]);
    let queue = queue_from_id(int(game, &["queueId"])).or(match map_id {
        HOWLING_ABYSS_MAP_ID => Some(StatsQueue::Aram),
        _ => None,
    });
    let on_rift =
        map_id == SUMMONERS_RIFT_MAP_ID || queue.is_some_and(StatsQueue::on_summoners_rift);
    let position = if on_rift { position_of(me) } else { None };
    let win = stats.get("win").and_then(Value::as_bool).unwrap_or(false);
    let kills = count(int(stats, &["kills"]));
    let deaths = count(int(stats, &["deaths"]));
    let assists = count(int(stats, &["assists"]));
    let damage_to_champions = count(int(stats, &["totalDamageDealtToChampions"]));
    let cs = count(int(stats, &["totalMinionsKilled"]) + int(stats, &["neutralMinionsKilled"]));
    let gold = count(int(stats, &["goldEarned"]));
    let vision_score = count(int(stats, &["visionScore"]));

    let mut out = PlayerGame {
        game_id: int(game, &["gameId"]),
        champion_id,
        position,
        queue,
        win,
        duration_secs: duration_u32,
        started_at_ms: int(game, &["gameCreation"]),
        kills,
        deaths,
        assists,
        damage_to_champions,
        cs,
        gold,
        vision_score,
        items,
        runes,
        spells: SpellPair {
            first: int(me, &["spell1Id"]),
            second: int(me, &["spell2Id"]),
        },
        opponent_champion_id: None,
        kill_participation: None,
        damage_share: None,
        gold_share: None,
        performance_score: None,
        placement: None,
        badge: None,
    };

    if duration_u32 >= REMAKE_SECS {
        if participants.len() > 1 {
            apply_team_metrics(&mut out, participants, me_idx, on_rift);
        } else {
            out.performance_score = Some(compute_performance_score(
                kills,
                deaths,
                assists,
                damage_to_champions,
                cs,
                gold,
                vision_score,
                duration_u32,
                win,
                position,
                None,
                None,
            ));
            let multikill = int(stats, &["largestMultiKill"]);
            if multikill >= 3 {
                out.badge = Some(MatchBadge::Multikill(multikill.min(5) as u8));
            }
        }
    }

    Some(out)
}

/// Computes opponent champion, KP%, DMG%, Gold%, performance score, placement, and MVP/ACE badge
/// from a full participant list. Only champion IDs and numeric totals are read.
fn apply_team_metrics(out: &mut PlayerGame, participants: &[Value], me_idx: usize, on_rift: bool) {
    let me = &participants[me_idx];
    let my_team = int(me, &["teamId"]);

    if on_rift {
        if let Some(my_pos) = out.position.filter(|p| *p != Position::Fill) {
            out.opponent_champion_id = participants
                .iter()
                .filter(|p| {
                    let team = int(p, &["teamId"]);
                    team != 0 && my_team != 0 && team != my_team
                })
                .find(|p| position_of(p) == Some(my_pos))
                .map(|p| int(p, &["championId"]))
                .filter(|id| *id > 0);
        }
    }

    // Per-team totals (kills, damage, gold).
    let team_totals = |team_id: i64| -> (u32, u32, u32) {
        participants
            .iter()
            .filter(|p| int(p, &["teamId"]) == team_id)
            .fold((0u32, 0u32, 0u32), |acc, p| {
                let Some(s) = p.get("stats") else {
                    return acc;
                };
                (
                    acc.0.saturating_add(count(int(s, &["kills"]))),
                    acc.1
                        .saturating_add(count(int(s, &["totalDamageDealtToChampions"]))),
                    acc.2.saturating_add(count(int(s, &["goldEarned"]))),
                )
            })
    };

    let (my_team_kills, my_team_dmg, my_team_gold) = if my_team != 0 {
        team_totals(my_team)
    } else {
        (0, 0, 0)
    };

    out.kill_participation = (my_team_kills > 0)
        .then(|| ((out.kills + out.assists) as f32 / my_team_kills as f32).clamp(0.0, 1.0));
    out.damage_share = (my_team_dmg > 0)
        .then(|| (out.damage_to_champions as f32 / my_team_dmg as f32).clamp(0.0, 1.0));
    out.gold_share =
        (my_team_gold > 0).then(|| (out.gold as f32 / my_team_gold as f32).clamp(0.0, 1.0));

    // Score every participant to compute placement (1..=10) and MVP / ACE.
    let mut scored: Vec<(usize, i64, bool, u8)> = Vec::with_capacity(participants.len());
    for (idx, p) in participants.iter().enumerate() {
        let Some(s) = p.get("stats") else {
            continue;
        };
        let team = int(p, &["teamId"]);
        let (tk, td, _) = if team != 0 {
            team_totals(team)
        } else {
            (0, 0, 0)
        };
        let k = count(int(s, &["kills"]));
        let d = count(int(s, &["deaths"]));
        let a = count(int(s, &["assists"]));
        let dmg = count(int(s, &["totalDamageDealtToChampions"]));
        let cs = count(int(s, &["totalMinionsKilled"]) + int(s, &["neutralMinionsKilled"]));
        let gold = count(int(s, &["goldEarned"]));
        let vis = count(int(s, &["visionScore"]));
        let win = s.get("win").and_then(Value::as_bool).unwrap_or(false);
        let pos = if on_rift { position_of(p) } else { None };
        let kp = (tk > 0).then(|| ((k + a) as f32 / tk as f32).clamp(0.0, 1.0));
        let ds = (td > 0).then(|| (dmg as f32 / td as f32).clamp(0.0, 1.0));
        let score = compute_performance_score(
            k,
            d,
            a,
            dmg,
            cs,
            gold,
            vis,
            out.duration_secs,
            win,
            pos,
            kp,
            ds,
        );
        scored.push((idx, team, win, score));
    }

    if let Some(&(_, _, _, my_score)) = scored.iter().find(|(idx, _, _, _)| *idx == me_idx) {
        out.performance_score = Some(my_score);
        // Only assign placement and MVP/ACE when we have a full match (at least 4 participants).
        if scored.len() >= 4 {
            let better = scored.iter().filter(|(_, _, _, s)| *s > my_score).count();
            out.placement = Some((better + 1).min(10) as u8);
            let best_on_team = scored
                .iter()
                .filter(|(_, team, _, _)| *team == my_team)
                .map(|(_, _, _, s)| *s)
                .max()
                .unwrap_or(0);
            if my_score >= best_on_team && best_on_team > 0 {
                out.badge = Some(if out.win {
                    MatchBadge::Mvp
                } else {
                    MatchBadge::Ace
                });
            }
        }
    }

    if out.badge.is_none() {
        if let Some(s) = me.get("stats") {
            let multikill = int(s, &["largestMultiKill"]);
            if multikill >= 3 {
                out.badge = Some(MatchBadge::Multikill(multikill.min(5) as u8));
            }
        }
    }
}

/// Enriches a single-participant `PlayerGame` using the full `/lol-match-history/v1/games/{id}`
/// payload when available.
pub fn enrich_game_from_detail(game: &mut PlayerGame, detail: &Value, self_puuid: Option<&str>) {
    if game.is_remake() {
        return;
    }
    let Some(participants) = detail.get("participants").and_then(Value::as_array) else {
        return;
    };
    if participants.len() <= 1 {
        return;
    }
    let me_idx = self_puuid
        .filter(|p| !p.is_empty())
        .and_then(|puuid| {
            let pid = detail
                .get("participantIdentities")?
                .as_array()?
                .iter()
                .find(|i| i.pointer("/player/puuid").and_then(Value::as_str) == Some(puuid))?
                .get("participantId")?
                .as_i64()?;
            participants
                .iter()
                .position(|p| p.get("participantId").and_then(Value::as_i64) == Some(pid))
        })
        .or_else(|| {
            // Fallback match by champion + KDA when participantIdentities lacks PUUID.
            participants.iter().position(|p| {
                int(p, &["championId"]) == game.champion_id
                    && p.get("stats").is_some_and(|s| {
                        count(int(s, &["kills"])) == game.kills
                            && count(int(s, &["deaths"])) == game.deaths
                            && count(int(s, &["assists"])) == game.assists
                    })
            })
        });
    let Some(me_idx) = me_idx else {
        return;
    };
    let map_id = int(detail, &["mapId"]);
    let on_rift = map_id == SUMMONERS_RIFT_MAP_ID || game.on_summoners_rift();
    apply_team_metrics(game, participants, me_idx, on_rift);
}

/// See [`StatsQueue::from_queue_id`].
pub fn queue_from_id(queue_id: i64) -> Option<StatsQueue> {
    StatsQueue::from_queue_id(queue_id)
}

fn position_of(participant: &Value) -> Option<Position> {
    for key in ["teamPosition", "individualPosition"] {
        if let Some(p) = participant
            .get(key)
            .and_then(Value::as_str)
            .and_then(Position::parse)
        {
            return Some(p);
        }
    }
    let lane = participant
        .pointer("/timeline/lane")
        .and_then(Value::as_str)?;
    let role = participant
        .pointer("/timeline/role")
        .and_then(Value::as_str)
        .unwrap_or("");
    match Position::parse(lane)? {
        Position::Bot if role.eq_ignore_ascii_case("DUO_SUPPORT") => Some(Position::Support),
        p => Some(p),
    }
}

fn int(value: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|k| {
            let v = value.get(*k)?;
            v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))
        })
        .unwrap_or(0)
}

fn count(value: i64) -> u32 {
    value.clamp(0, u32::MAX as i64) as u32
}

pub(crate) async fn load_recommended(
    client: &ReadOnlyRiotClient,
    port: u16,
    token: &str,
    query: &StatsQuery,
) -> Result<ChampionBuild, Fail> {
    if query.champion_id <= 0 {
        return Err(Fail::NoData);
    }
    let path = recommended_pages_path(query.champion_id, query.position);
    let (status, _, body) = client
        .get(port, Some(token), &path)
        .await
        .map_err(|_| Fail::ClientNoAnswer)?;
    if status.as_u16() == 404 {
        return Err(Fail::NoData);
    }
    if !status.is_success() {
        return Err(Fail::ClientHttp(status.as_u16()));
    }
    parse_recommended_pages(&body, query).ok_or(Fail::NoData)
}

pub(crate) async fn load_match_history(
    client: &ReadOnlyRiotClient,
    port: u16,
    token: &str,
) -> Result<Vec<PlayerGame>, Fail> {
    let (status, _, body) = client
        .get(port, Some(token), MATCH_HISTORY_PATH)
        .await
        .map_err(|_| Fail::ClientNoAnswer)?;
    if !status.is_success() {
        return Err(Fail::ClientHttp(status.as_u16()));
    }
    if !body.is_object() && !body.is_array() {
        return Err(Fail::ClientBadFormat);
    }
    let puuid = match client.get(port, Some(token), CURRENT_SUMMONER_PATH).await {
        Ok((status, _, me)) if status.is_success() => {
            me.get("puuid").and_then(Value::as_str).map(str::to_string)
        }
        _ => None,
    };
    let mut games = parse_match_history(&body, puuid.as_deref());

    // Enrich up to MAX_DETAIL_ENRICH recent non-remake games that lacked full participant lists.
    for game in games
        .iter_mut()
        .filter(|g| !g.is_remake() && g.kill_participation.is_none() && g.game_id > 0)
        .take(MAX_DETAIL_ENRICH)
    {
        let path = format!("/lol-match-history/v1/games/{}", game.game_id);
        if let Ok((st, _, detail)) = client.get(port, Some(token), &path).await {
            if st.is_success() {
                enrich_game_from_detail(game, &detail, puuid.as_deref());
            }
        }
    }

    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn builds_the_recommended_pages_path() {
        assert_eq!(
            recommended_pages_path(103, Some(Position::Mid)),
            "/lol-perks/v1/recommended-pages/champion/103/position/MIDDLE/map/11"
        );
        assert_eq!(
            recommended_pages_path(412, Some(Position::Support)),
            "/lol-perks/v1/recommended-pages/champion/412/position/UTILITY/map/11"
        );
        assert!(recommended_pages_path(1, None).contains("/position/NONE/"));
        assert!(recommended_pages_path(1, Some(Position::Fill)).contains("/position/NONE/"));
    }

    #[test]
    fn parses_recommended_pages() {
        let query = StatsQuery {
            champion_id: 103,
            position: Some(Position::Mid),
            ..StatsQuery::default()
        };
        let build = parse_recommended_pages(&sample("lcu_recommended_pages.json"), &query).unwrap();
        assert_eq!(build.origin, StatsOrigin::RiotClient);
        assert_eq!(build.query, query);
        assert_eq!(build.runes.len(), 2, "the empty page is skipped");
        let (first, rates) = &build.runes[0];
        assert_eq!(first.primary_style_id, 8100);
        assert_eq!(first.sub_style_id, 8200);
        assert_eq!(first.perk_ids, vec![8112, 8139, 8140, 8106, 8226, 8210]);
        assert_eq!(first.shard_ids, vec![5008, 5008, 5001]);
        assert_eq!(*rates, Rates::default());
        assert_eq!(
            build.runes[1].0.perk_ids[0], 8229,
            "bare id arrays are accepted"
        );
        assert_eq!(
            build
                .spells
                .iter()
                .map(|(s, _)| (s.first, s.second))
                .collect::<Vec<_>>(),
            vec![(4, 14), (4, 12)]
        );

        assert!(parse_recommended_pages(&serde_json::json!([]), &query).is_none());
        assert!(parse_recommended_pages(&Value::Null, &query).is_none());
    }

    #[test]
    fn parses_the_players_match_history() {
        let json = sample("lcu_match_history.json");
        assert!(needs_identity(&json));

        let games = parse_match_history(&json, Some("self-puuid"));
        assert_eq!(games.len(), 3);
        assert!(games
            .windows(2)
            .all(|w| w[0].started_at_ms >= w[1].started_at_ms));

        let ahri = &games[0];
        assert_eq!((ahri.champion_id, ahri.game_id), (103, 7000000003));
        assert_eq!(ahri.position, Some(Position::Mid));
        assert_eq!(ahri.queue, Some(StatsQueue::RankedSolo));
        assert!(ahri.win);
        assert_eq!((ahri.kills, ahri.deaths, ahri.assists), (9, 3, 7));
        assert_eq!(ahri.cs, 240);
        assert_eq!(ahri.duration_secs, 1800);
        assert_eq!(
            ahri.items,
            vec![3118, 4645, 3020, 3089, 2003],
            "empty slots and the trinket drop"
        );
        let runes = ahri.runes.as_ref().unwrap();
        assert_eq!(runes.perk_ids.len(), 6);
        assert_eq!(runes.shard_ids, vec![5008, 5008, 5001]);
        assert_eq!(
            ahri.spells,
            SpellPair {
                first: 4,
                second: 14
            }
        );

        let thresh = &games[1];
        assert_eq!(
            thresh.champion_id, 412,
            "the player's participant, not the first one"
        );
        assert_eq!(thresh.position, Some(Position::Support));
        assert_eq!(thresh.queue, Some(StatsQueue::RankedFlex));
        assert!(!thresh.win);
        assert!(thresh.runes.as_ref().unwrap().shard_ids.is_empty());

        let lux = &games[2];
        assert_eq!(lux.queue, Some(StatsQueue::Aram));
        assert_eq!(lux.position, None);
        assert_eq!(lux.duration_secs, 1200, "milliseconds are normalized");
        assert_eq!(lux.runes, None);

        let without_identity = parse_match_history(&json, None);
        assert_eq!(
            without_identity.len(),
            2,
            "a multi-participant game needs the local PUUID"
        );
    }

    #[test]
    fn maps_queue_ids() {
        assert_eq!(queue_from_id(420), Some(StatsQueue::RankedSolo));
        assert_eq!(queue_from_id(490), Some(StatsQueue::Normal));
        assert_eq!(queue_from_id(1700), Some(StatsQueue::Arena));
        assert_eq!(queue_from_id(0), None);
    }

    #[test]
    fn extracts_team_metrics_opponent_and_mvp_badge_from_full_game() {
        let detail = serde_json::json!({
            "gameId": 7000000099i64,
            "gameCreation": 1795450000000i64,
            "gameDuration": 1800,
            "mapId": 11,
            "queueId": 420,
            "participantIdentities": [
                {"participantId": 1, "player": {"puuid": "self-puuid"}},
                {"participantId": 2, "player": {"puuid": "ally-puuid"}},
                {"participantId": 6, "player": {"puuid": "enemy-mid"}},
                {"participantId": 7, "player": {"puuid": "enemy-jg"}}
            ],
            "participants": [
                {
                    "participantId": 1, "teamId": 100, "championId": 103, "spell1Id": 4, "spell2Id": 14,
                    "teamPosition": "MIDDLE",
                    "stats": {
                        "win": true, "kills": 12, "deaths": 2, "assists": 8,
                        "totalDamageDealtToChampions": 32000, "totalMinionsKilled": 230, "neutralMinionsKilled": 10,
                        "goldEarned": 15000, "visionScore": 30, "largestMultiKill": 3,
                        "item0": 3118, "item1": 4645
                    }
                },
                {
                    "participantId": 2, "teamId": 100, "championId": 64, "spell1Id": 4, "spell2Id": 11,
                    "teamPosition": "JUNGLE",
                    "stats": {
                        "win": true, "kills": 8, "deaths": 5, "assists": 6,
                        "totalDamageDealtToChampions": 18000, "totalMinionsKilled": 40, "neutralMinionsKilled": 140,
                        "goldEarned": 12000, "visionScore": 20
                    }
                },
                {
                    "participantId": 6, "teamId": 200, "championId": 238, "spell1Id": 4, "spell2Id": 14,
                    "teamPosition": "MIDDLE",
                    "stats": {
                        "win": false, "kills": 5, "deaths": 7, "assists": 3,
                        "totalDamageDealtToChampions": 19000, "totalMinionsKilled": 190, "neutralMinionsKilled": 5,
                        "goldEarned": 10500, "visionScore": 16
                    }
                },
                {
                    "participantId": 7, "teamId": 200, "championId": 234, "spell1Id": 4, "spell2Id": 11,
                    "teamPosition": "JUNGLE",
                    "stats": {
                        "win": false, "kills": 2, "deaths": 8, "assists": 4,
                        "totalDamageDealtToChampions": 11000, "totalMinionsKilled": 30, "neutralMinionsKilled": 110,
                        "goldEarned": 9000, "visionScore": 14
                    }
                }
            ]
        });

        let games = parse_match_history(
            &serde_json::json!({"games": {"games": [detail.clone()]}}),
            Some("self-puuid"),
        );
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!(
            g.opponent_champion_id,
            Some(238),
            "enemy mid laner detected"
        );
        assert!(
            (g.kill_participation.unwrap() - 1.0).abs() < 1e-3,
            "20/20 KP"
        );
        assert!(
            (g.damage_share.unwrap() - 0.64).abs() < 1e-3,
            "32000/50000 DMG"
        );
        assert_eq!(g.placement, Some(1));
        assert_eq!(g.badge, Some(MatchBadge::Mvp));
        assert!(g.performance_score.unwrap() >= 80);

        // Also verify enrich_game_from_detail works on a single-participant game.
        let mut bare = PlayerGame {
            game_id: 7000000099,
            champion_id: 103,
            position: Some(Position::Mid),
            queue: Some(StatsQueue::RankedSolo),
            win: true,
            duration_secs: 1800,
            kills: 12,
            deaths: 2,
            assists: 8,
            damage_to_champions: 32000,
            cs: 240,
            gold: 15000,
            vision_score: 30,
            ..PlayerGame::default()
        };
        enrich_game_from_detail(&mut bare, &detail, Some("self-puuid"));
        assert_eq!(bare.opponent_champion_id, Some(238));
        assert_eq!(bare.badge, Some(MatchBadge::Mvp));
        assert_eq!(bare.placement, Some(1));
    }
}
