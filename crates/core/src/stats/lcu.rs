//! Local Champion Stats read from the League client: Riot's recommended rune pages and the
//! player's own match history. `GET` only, through [`ReadOnlyRiotClient`].
//!
//! The response shapes here follow the LCU `lol-perks` and `lol-match-history` plugins; the
//! sample files in `crates/core/tests/data/lcu_*.json` mirror them. Check them against the
//! Observatory's endpoint catalog after a client update.

use hypatia_shared::{
    ChampionBuild, PlayerGame, Position, Rates, RuneSetup, SpellPair, StatsOrigin, StatsQuery,
    StatsQueue,
};
use serde_json::Value;

use super::Fail;
use crate::riot_http::ReadOnlyRiotClient;

/// Summoner's Rift, the only map Riot recommends rune pages for.
pub const SUMMONERS_RIFT_MAP_ID: i64 = 11;
/// Howling Abyss: every ARAM variant, whatever its queue id.
pub const HOWLING_ABYSS_MAP_ID: i64 = 12;

/// The player's last 20 games, newest first. Only the player's own participant is read.
pub const MATCH_HISTORY_PATH: &str =
    "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=20";

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
/// compared locally to pick the player's participant; nothing about other players is kept.
pub fn parse_match_history(json: &Value, self_puuid: Option<&str>) -> Vec<PlayerGame> {
    let mut games: Vec<PlayerGame> = games_array(json)
        .iter()
        .filter_map(|g| parse_game(g, self_puuid))
        .collect();
    games.sort_by_key(|a| std::cmp::Reverse(a.started_at_ms));
    games
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
    let me = if participants.len() == 1 {
        &participants[0]
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
            .find(|p| p.get("participantId").and_then(Value::as_i64) == Some(participant_id))?
    };
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

    Some(PlayerGame {
        game_id: int(game, &["gameId"]),
        champion_id,
        position: if on_rift { position_of(me) } else { None },
        queue,
        win: stats.get("win").and_then(Value::as_bool).unwrap_or(false),
        duration_secs: count(duration_secs),
        started_at_ms: int(game, &["gameCreation"]),
        kills: count(int(stats, &["kills"])),
        deaths: count(int(stats, &["deaths"])),
        assists: count(int(stats, &["assists"])),
        damage_to_champions: count(int(stats, &["totalDamageDealtToChampions"])),
        cs: count(int(stats, &["totalMinionsKilled"]) + int(stats, &["neutralMinionsKilled"])),
        gold: count(int(stats, &["goldEarned"])),
        vision_score: count(int(stats, &["visionScore"])),
        items,
        runes,
        spells: SpellPair {
            first: int(me, &["spell1Id"]),
            second: int(me, &["spell2Id"]),
        },
    })
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
    let puuid = if needs_identity(&body) {
        match client.get(port, Some(token), CURRENT_SUMMONER_PATH).await {
            Ok((status, _, me)) if status.is_success() => {
                me.get("puuid").and_then(Value::as_str).map(str::to_string)
            }
            _ => None,
        }
    } else {
        None
    };
    Ok(parse_match_history(&body, puuid.as_deref()))
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
}
