//! Reproducible micro-benchmark of Hypatia's hot paths.
//!
//! Run with `cargo run --release -p hypatia-core --example bench`.
//! Prints one `name=value unit` line per metric so two runs can be diffed.

use std::collections::HashMap;
use std::time::Instant;

use hypatia_core::catalog::CatalogStore;
use hypatia_core::discovery::DiscoveryManager;
use hypatia_core::redaction::SecretVault;
use hypatia_core::AppState;
use hypatia_shared::{EndpointInfo, EndpointOrigin, EventKind, Lang, SourceId};
use serde_json::{json, Value};

const CHAMPS: [&str; 5] = ["Ahri", "Garen", "Lee Sin", "Jinx", "Thresh"];
const POSITIONS: [&str; 5] = ["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY"];

fn rss_mb() -> f64 {
    let pid = sysinfo::Pid::from_u32(std::process::id());
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid)
        .map(|p| p.memory() as f64 / 1_048_576.0)
        .unwrap_or(0.0)
}

/// Roughly the shape and size (~45 KB) of a real `/liveclientdata/allgamedata` payload.
fn allgamedata(tick: u64) -> Value {
    let players: Vec<Value> = (0..10)
        .map(|i| {
            let items: Vec<Value> = (0..6)
                .map(|s| {
                    json!({
                        "canUse": false, "consumable": false, "count": 1,
                        "displayName": format!("Item of the Long Descriptive Name {s}"),
                        "itemID": 3000 + s, "price": 3100, "rawDescription": "GeneratedTip_Item_3031_Description",
                        "rawDisplayName": "Item_3031_Name", "slot": s
                    })
                })
                .collect();
            json!({
                "championName": CHAMPS[i % 5],
                "isBot": false, "isDead": tick % 7 == i as u64, "items": items,
                "level": 1 + (tick / 30).min(17), "position": POSITIONS[i % 5],
                "rawChampionName": "game_character_displayname_Ahri",
                "rawSkinName": "game_character_skin_displayname_Ahri_14",
                "respawnTimer": (tick % 40) as f64 * 0.5,
                "riotId": format!("Player{i}#EUW"), "riotIdGameName": format!("Player{i}"), "riotIdTagLine": "EUW",
                "runes": {
                    "keystone": {"displayName": "Electrocute", "id": 8112, "rawDescription": "perk_tooltip_Electrocute", "rawDisplayName": "perk_displayname_Electrocute"},
                    "primaryRuneTree": {"displayName": "Domination", "id": 8100, "rawDescription": "perkstyle_tooltip_7200", "rawDisplayName": "perkstyle_displayname_7200"},
                    "secondaryRuneTree": {"displayName": "Sorcery", "id": 8200, "rawDescription": "perkstyle_tooltip_7202", "rawDisplayName": "perkstyle_displayname_7202"}
                },
                "scores": {"assists": tick % 11, "creepScore": tick * 3 % 400, "deaths": tick % 5, "kills": tick % 9, "wardScore": 12.5},
                "skinID": 14, "skinName": "Elderwood Ahri",
                "summonerName": format!("Player{i}#EUW"),
                "summonerSpells": {
                    "summonerSpellOne": {"displayName": "Flash", "rawDescription": "GeneratedTip_SummonerSpell_SummonerFlash_Description", "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerFlash_DisplayName"},
                    "summonerSpellTwo": {"displayName": "Ignite", "rawDescription": "GeneratedTip_SummonerSpell_SummonerDot_Description", "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerDot_DisplayName"}
                },
                "team": if i < 5 { "ORDER" } else { "CHAOS" }
            })
        })
        .collect();
    let events: Vec<Value> = (0..(tick.min(120)))
        .map(|e| {
            json!({
                "EventID": e, "EventName": if e % 9 == 0 { "TurretKilled" } else { "ChampionKill" },
                "EventTime": e as f64 * 14.2, "KillerName": format!("Player{}", e % 10),
                "VictimName": format!("Player{}", (e + 5) % 10), "TurretKilled": "Turret_T2_L_03_A",
                "Assisters": ["Player1", "Player2"]
            })
        })
        .collect();
    let abilities: HashMap<&str, Value> = ["Q", "W", "E", "R", "Passive"]
        .into_iter()
        .map(|k| (k, json!({"abilityLevel": 3, "displayName": "Orb of Deception", "id": "AhriQ", "rawDescription": "GeneratedTip_Spell_AhriQ_Description", "rawDisplayName": "GeneratedTip_Spell_AhriQ_DisplayName"})))
        .collect();
    let stats: HashMap<String, f64> = (0..48).map(|i| (format!("championStat{i}"), i as f64 * 1.37 + tick as f64)).collect();
    json!({
        "activePlayer": {
            "abilities": abilities, "championStats": stats, "currentGold": 1234.5 + tick as f64, "level": 9,
            "riotId": "Player0#EUW", "riotIdGameName": "Player0", "riotIdTagLine": "EUW", "summonerName": "Player0#EUW",
            "fullRunes": {"generalRunes": (0..6).map(|i| json!({"displayName": "Rune", "id": 8100 + i, "rawDescription": "perk_tooltip", "rawDisplayName": "perk_displayname"})).collect::<Vec<_>>()}
        },
        "allPlayers": players,
        "events": {"Events": events},
        "gameData": {"gameMode": "CLASSIC", "gameTime": tick as f64, "mapName": "Map11", "mapNumber": 11, "mapTerrain": "Default"}
    })
}

fn champ_select(tick: u64) -> Value {
    let team = |base: i64| -> Vec<Value> {
        (0..5)
            .map(|i| {
                json!({
                    "assignedPosition": POSITIONS[i as usize],
                    "cellId": base + i, "championId": 100 + i, "championPickIntent": 0,
                    "puuid": format!("puuid-{base}-{i}"), "selectedSkinId": 103000, "spell1Id": 4, "spell2Id": 14,
                    "summonerId": 1000 + base + i, "team": 1, "wardSkinId": -1,
                    "gameName": format!("Mate{i}"), "tagLine": "EUW"
                })
            })
            .collect()
    };
    let actions: Vec<Value> = (0..8)
        .map(|turn| {
            Value::Array(
                (0..3)
                    .map(|a| json!({"actorCellId": (turn + a) % 10, "championId": 0, "completed": turn < 4, "id": turn * 3 + a, "isAllyAction": true, "isInProgress": turn == 4, "type": if turn < 2 { "ban" } else { "pick" }}))
                    .collect(),
            )
        })
        .collect();
    json!({
        "actions": actions, "bans": {"myTeamBans": [1, 2, 3, 4, 5], "theirTeamBans": [6, 7, 8, 9, 10], "numBans": 10},
        "localPlayerCellId": 2, "myTeam": team(0), "theirTeam": team(5),
        "timer": {"adjustedTimeLeftInPhase": 27_000 - (tick as i64 % 27) * 1000, "phase": "BAN_PICK", "totalTimeInPhase": 30_000},
        "queueType": "RANKED_FLEX_SR", "trades": [], "swaps": [], "positionSwaps": [],
        "accessToken": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
    })
}

fn report(name: &str, value: f64, unit: &str) {
    println!("{name}={value:.3} {unit}");
}

/// Feeds `TICKS` seconds of in-game traffic (1 Hz live poll, champ-select session and bursty
/// LCU chatter) and reports per-event cost and retained memory.
fn bench_ingest(app: &AppState, label: &str, gen_cost: std::time::Duration) {
    let telemetry = app.flags.telemetry();
    let rss_before = rss_mb();
    let lcu_uris: Vec<String> = (0..40).map(|i| format!("/lol-bench/v1/resource-{i}")).collect();
    let t = Instant::now();
    for tick in 0..TICKS {
        {
            let mut s = app.store.write().unwrap();
            s.insert_event(
                SourceId::LiveGame,
                EventKind::Poll,
                "/liveclientdata/allgamedata".to_string(),
                allgamedata(tick),
                &app.vault,
            );
            s.insert_event(
                SourceId::LeagueClient,
                EventKind::Update,
                "/lol-champ-select/v1/session".to_string(),
                champ_select(tick),
                &app.vault,
            );
            for k in 0..3 {
                let uri = &lcu_uris[((tick * 3 + k) % 40) as usize];
                s.insert_event(
                    SourceId::LeagueClient,
                    EventKind::Update,
                    uri.clone(),
                    json!({"tick": tick, "k": k, "state": "Updated", "token": "abcdef0123456789"}),
                    &app.vault,
                );
            }
        }
        // The endpoint catalog only records traffic while the Telemetry Lab is open.
        if telemetry {
            let mut cat = app.catalog.write().unwrap();
            cat.record_observed(SourceId::LiveGame, "GET", "/liveclientdata/allgamedata", 45_000);
            cat.record_observed(SourceId::LeagueClient, "GET", "/lol-champ-select/v1/session", 4_000);
        }
    }
    let ingest = t.elapsed().saturating_sub(gen_cost);
    report(
        &format!("ingest_per_event[{label}]"),
        ingest.as_secs_f64() * 1e6 / (TICKS * 5) as f64,
        "us",
    );
    report(
        &format!("ingest_retained_memory[{label}]"),
        rss_mb() - rss_before,
        "MB",
    );
}

const TICKS: u64 = 3000;

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    // Payload generation cost is identical across builds; measure it once to subtract.
    let t = Instant::now();
    for tick in 0..TICKS {
        std::hint::black_box((allgamedata(tick), champ_select(tick)));
    }
    let gen_cost = t.elapsed();

    // --- 1. Event ingestion ------------------------------------------------------------------
    let app = AppState::start_with_lang(true, 0.5, Lang::En);
    std::thread::sleep(std::time::Duration::from_millis(300));
    bench_ingest(&app, "lab closed", gen_cost);
    app.set_telemetry(true);
    bench_ingest(&app, "lab open", gen_cost);

    // --- 2. UI refresh: match projection and telemetry snapshot --------------------------------
    const UI_TICKS: u32 = 2000;
    let t = Instant::now();
    for _ in 0..UI_TICKS {
        std::hint::black_box(app.get_summary());
    }
    report("get_summary[unchanged]", t.elapsed().as_secs_f64() * 1e6 / UI_TICKS as f64, "us");

    // Alternating the language invalidates the projection, forcing a full recomputation.
    let t = Instant::now();
    for i in 0..UI_TICKS {
        app.set_lang(if i % 2 == 0 { Lang::Fr } else { Lang::En });
        std::hint::black_box(app.get_summary());
    }
    report("get_summary[changed]", t.elapsed().as_secs_f64() * 1e6 / UI_TICKS as f64, "us");

    let t = Instant::now();
    for _ in 0..UI_TICKS {
        std::hint::black_box(app.get_snapshot());
    }
    report("get_snapshot", t.elapsed().as_secs_f64() * 1e6 / UI_TICKS as f64, "us");

    // --- 3. Endpoint catalog with a realistic 3000-entry swagger import ------------------------
    let mut catalog = CatalogStore::new();
    catalog.add_declared(
        (0..3000)
            .map(|i| EndpointInfo {
                source: SourceId::LeagueClient,
                method: "GET".to_string(),
                path_template: format!("/lol-namespace-{}/v1/resource-{i}", i % 60),
                namespace: format!("lol-namespace-{}", i % 60),
                description: Some("Declared endpoint".to_string()),
                origin: EndpointOrigin::Declared,
                last_seen: None,
                event_count: 0,
                bytes_total: 0,
            })
            .collect(),
    );
    const OBS: u32 = 20_000;
    let t = Instant::now();
    for i in 0..OBS {
        let n = (i * 7) % 3000;
        let uri = format!("/lol-namespace-{}/v1/resource-{n}", n % 60);
        catalog.record_observed(SourceId::LeagueClient, "GET", &uri, 512);
    }
    report("catalog_record_observed", t.elapsed().as_secs_f64() * 1e6 / OBS as f64, "us");

    // --- 4. Discovery scan (real OS process table, machine dependent) -------------------------
    let vault = SecretVault::new();
    for (label, detailed) in [("light", false), ("detailed", true)] {
        let mut sys = sysinfo::System::new();
        let mut since = HashMap::new();
        DiscoveryManager::scan_cycle(&mut sys, &vault, &mut since, detailed);
        const SCANS: u32 = 15;
        let t = Instant::now();
        for _ in 0..SCANS {
            std::hint::black_box(DiscoveryManager::scan_cycle(&mut sys, &vault, &mut since, detailed));
        }
        report(
            &format!("discovery_scan[{label}]"),
            t.elapsed().as_secs_f64() * 1e3 / SCANS as f64,
            "ms",
        );
    }

    rt.shutdown_background();
}
