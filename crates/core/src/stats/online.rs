//! The Online provider: MetaBot.GG's public, read-only MCP endpoint
//! (`docs/research/online-stats-source.md` records the choice, its terms and the fallback).
//!
//! - **Off means nothing**: every entry point checks `Flags::online_stats()` first and returns
//!   [`Fail::OnlineDisabled`] before touching the network, the disk cache or even the HTTP client.
//! - **No identity**: requests carry only `game`, a champion name, a role filter, a mode (ARAM /
//!   Arena) and the language of item and rune names. No summoner name, PUUID, token, lockfile
//!   value or cookie. The HTTP client is our own (system TLS, no cookie store, generic UA).
//! - **Per-patch disk cache** under `<config dir>/stats-cache/metabot/<patch>/`, fresh for
//!   [`FRESH_FOR`]; a newer patch replaces older ones. A failed refresh serves the stale copy.
//! - **Attribution**: MetaBot.GG's terms require a visible link to the page the data came from;
//!   each result carries it in `attribution`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hypatia_shared::{
    Attribution, BuiltinChampionCatalog, ChampionBuild, ChampionLookup, ItemBlock, ItemStage, Lang,
    Matchup, MatchupList, Position, RankBracket, Rates, RuneSetup, StatsOrigin, StatsQuery,
    StatsQueue, StatsResult, StatsSource, Tier, TierEntry, TierList,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::game_data::GameDataIndex;
use super::{load_lang, Fail};
use crate::settings::Flags;

pub const ENDPOINT: &str = "https://metabot.gg/api/mcp";
pub const PROVIDER: &str = "MetaBot.GG";
/// How long a cached answer is served without asking again.
pub const FRESH_FOR: Duration = Duration::from_secs(6 * 3600);
const TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const USER_AGENT: &str = concat!("Hypatia/", env!("CARGO_PKG_VERSION"));
/// Only links to the provider's own pages are kept as attribution.
const LINK_PREFIX: &str = "https://metabot.gg/";

/// Languages MetaBot.GG translates item and rune names into (besides `en`).
const LANGS: &[&str] = &[
    "fr_FR", "es_ES", "de_DE", "ko_KR", "pl_PL", "pt_BR", "tr_TR", "zh_CN", "vi_VN", "ja_JP",
    "ru_RU", "th_TH", "it_IT", "cs_CZ", "ar_AE", "zh_TW", "id_ID",
];

/// Roles of the ranked tier list and the filter MetaBot.GG expects for each.
const ROLE_FILTERS: [(Position, &str); 5] = [
    (Position::Top, "top"),
    (Position::Jungle, "jungle"),
    (Position::Mid, "mid"),
    (Position::Bot, "adc"),
    (Position::Support, "support"),
];

/// What turns provider names into ids: the client's game data and its locale.
#[derive(Clone, Default)]
pub struct ResolveContext {
    pub index: Arc<GameDataIndex>,
    /// Client locale (`fr_FR`); item and rune names are requested in it so they match `index`.
    pub locale: Option<String>,
}

/// One MCP tool call: the tool and its arguments, which are also the cache key.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub tool: &'static str,
    pub args: Value,
}

impl Request {
    fn cache_key(&self) -> String {
        let mut key = self.tool.to_string();
        if let Some(args) = self.args.as_object() {
            let mut keys: Vec<&String> = args.keys().collect();
            keys.sort();
            for k in keys {
                if k == "game" {
                    continue;
                }
                key.push('-');
                key.push_str(args[k].as_str().unwrap_or(""));
            }
        }
        sanitize(&key)
    }
}

pub struct MetaBotSource {
    flags: Arc<Flags>,
    lang: Arc<AtomicU8>,
    endpoint: String,
    http: OnceLock<reqwest::Client>,
    cache: PatchCache,
    champions: Arc<dyn ChampionLookup + Send + Sync>,
    context: RwLock<ResolveContext>,
}

impl MetaBotSource {
    /// `cache_dir` is the provider's own folder (`<config dir>/stats-cache/metabot`); it is not
    /// created until a first answer is stored.
    pub fn new(
        flags: Arc<Flags>,
        lang: Arc<AtomicU8>,
        cache_dir: PathBuf,
        champions: Arc<dyn ChampionLookup + Send + Sync>,
    ) -> Self {
        Self {
            flags,
            lang,
            endpoint: ENDPOINT.to_string(),
            http: OnceLock::new(),
            cache: PatchCache { dir: cache_dir },
            champions,
            context: RwLock::new(ResolveContext::default()),
        }
    }

    /// Points the provider at another endpoint (tests).
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn default_cache_dir() -> PathBuf {
        crate::settings::default_config_dir()
            .join("stats-cache")
            .join("metabot")
    }

    /// Updates the names → ids context once the client's game data is known.
    pub fn set_context(&self, context: ResolveContext) {
        *self.context.write().unwrap() = context;
    }

    fn context(&self) -> ResolveContext {
        self.context.read().unwrap().clone()
    }

    fn check_enabled(&self) -> Result<(), Fail> {
        if self.flags.online_stats() {
            Ok(())
        } else {
            Err(Fail::OnlineDisabled)
        }
    }

    // ---- Requests -----------------------------------------------------------------------

    fn champion_name(champion_id: i64) -> Result<String, Fail> {
        let name = BuiltinChampionCatalog.resolve_champion_name_localized(champion_id, Lang::En);
        if champion_id <= 0 || name.starts_with("Champion #") {
            return Err(Fail::NoData);
        }
        Ok(name)
    }

    /// The build page (item and rune names in the client's language) and the champion overview
    /// (tier, win and pick rate, in English so labels parse).
    pub fn build_requests(&self, query: &StatsQuery) -> Result<[Request; 2], Fail> {
        if query.queue != StatsQueue::RankedSolo {
            return Err(Fail::NoData);
        }
        let name = Self::champion_name(query.champion_id)?;
        let lang = metabot_lang(self.context().locale.as_deref());
        Ok([
            Request {
                tool: "get_entity_build",
                args: json!({ "game": "league", "name": name, "lang": lang }),
            },
            Request {
                tool: "lookup_game_entity",
                args: json!({ "game": "league", "name": name, "lang": "en" }),
            },
        ])
    }

    pub fn matchups_request(&self, query: &StatsQuery) -> Result<Request, Fail> {
        if query.queue != StatsQueue::RankedSolo {
            return Err(Fail::NoData);
        }
        let name = Self::champion_name(query.champion_id)?;
        Ok(Request {
            tool: "get_counters",
            args: json!({ "game": "league", "name": name, "lang": "en" }),
        })
    }

    /// One request per role for ranked, one for ARAM or Arena. Flex and normals: no data.
    pub fn tier_requests(queue: StatsQueue) -> Result<Vec<(Position, Request)>, Fail> {
        let meta = |args: Value| Request {
            tool: "get_game_meta",
            args,
        };
        match queue {
            StatsQueue::RankedSolo => Ok(ROLE_FILTERS
                .iter()
                .map(|(p, f)| {
                    (
                        *p,
                        meta(json!({ "game": "league", "filter": f, "lang": "en" })),
                    )
                })
                .collect()),
            StatsQueue::Aram => Ok(vec![(
                Position::Fill,
                meta(json!({ "game": "league", "mode": "aram", "lang": "en" })),
            )]),
            StatsQueue::Arena => Ok(vec![(
                Position::Fill,
                meta(json!({ "game": "league", "mode": "arena", "lang": "en" })),
            )]),
            StatsQueue::RankedFlex | StatsQueue::Normal => Err(Fail::NoData),
        }
    }

    // ---- Async fetches (the service runs these off the UI thread) -----------------------

    pub async fn fetch_build(&self, query: &StatsQuery) -> Result<ChampionBuild, Fail> {
        self.check_enabled()?;
        let [build, overview] = self.build_requests(query)?;
        let build = self.call(&build).await?;
        let overview = self.call(&overview).await.ok();
        assemble_build(query, &build, overview.as_ref(), &self.context().index)
    }

    pub async fn fetch_matchups(&self, query: &StatsQuery) -> Result<MatchupList, Fail> {
        self.check_enabled()?;
        let request = self.matchups_request(query)?;
        let content = self.call(&request).await?;
        assemble_matchups(query, &content, &*self.champions)
    }

    pub async fn fetch_tier_list(&self, queue: StatsQueue) -> Result<TierList, Fail> {
        self.check_enabled()?;
        let mut parts = Vec::new();
        for (position, request) in Self::tier_requests(queue)? {
            parts.push((position, self.call(&request).await?));
        }
        assemble_tier_list(queue, &parts, &*self.champions)
    }

    /// Serves a fresh cached answer, else asks MetaBot.GG and stores the answer; on failure a
    /// stale cached answer beats nothing.
    async fn call(&self, request: &Request) -> Result<CachedContent, Fail> {
        self.check_enabled()?;
        let key = request.cache_key();
        let cached = self.cache.read(&key);
        if let Some(entry) = cached.as_ref().filter(|e| e.is_fresh(now_secs())) {
            return Ok(entry.clone());
        }
        match self.post(request).await {
            Ok(content) => {
                let entry = CachedContent {
                    fetched_at: now_secs(),
                    patch: content
                        .get("patch")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_string(),
                    content,
                };
                if let Err(e) = self.cache.write(&key, &entry) {
                    tracing::debug!("Online stats cache write failed: {e}");
                }
                Ok(entry)
            }
            Err(Fail::NoData) => Err(Fail::NoData),
            Err(fail) => cached.ok_or(fail),
        }
    }

    async fn post(&self, request: &Request) -> Result<Value, Fail> {
        let http = self.http.get_or_init(|| {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .connect_timeout(CONNECT_TIMEOUT)
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_default()
        });
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": request.tool, "arguments": request.args },
        });
        let response = http
            .post(&self.endpoint)
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .json(&body)
            .send()
            .await
            .map_err(|_| Fail::Network)?;
        let status = response.status().as_u16();
        if status == 429 {
            return Err(Fail::RateLimited);
        }
        if !response.status().is_success() {
            return Err(Fail::OnlineHttp(status));
        }
        let text = response.text().await.map_err(|_| Fail::Network)?;
        parse_mcp_body(&text)
    }

    // ---- Cache-only reads (the synchronous trait) ----------------------------------------

    fn cached(&self, request: &Request) -> Result<CachedContent, Fail> {
        self.cache.read(&request.cache_key()).ok_or(Fail::NoData)
    }

    fn localize<T>(&self, result: Result<T, Fail>) -> StatsResult<T> {
        result.map_err(|f| f.localize(load_lang(&self.lang)))
    }
}

/// The synchronous contract reads the disk cache only (no network): the async `fetch_*`
/// methods fill it. Switched off, it returns `OnlineDisabled` before reading anything.
impl StatsSource for MetaBotSource {
    fn build(&self, query: &StatsQuery) -> StatsResult<ChampionBuild> {
        self.localize((|| {
            self.check_enabled()?;
            let [build, overview] = self.build_requests(query)?;
            let build = self.cached(&build)?;
            let overview = self.cached(&overview).ok();
            assemble_build(query, &build, overview.as_ref(), &self.context().index)
        })())
    }

    fn tier_list(&self, queue: StatsQueue, _bracket: RankBracket) -> StatsResult<TierList> {
        self.localize((|| {
            self.check_enabled()?;
            let parts = Self::tier_requests(queue)?
                .into_iter()
                .map(|(p, r)| Ok((p, self.cached(&r)?)))
                .collect::<Result<Vec<_>, Fail>>()?;
            assemble_tier_list(queue, &parts, &*self.champions)
        })())
    }

    fn matchups(&self, query: &StatsQuery) -> StatsResult<Vec<Matchup>> {
        self.localize((|| {
            self.check_enabled()?;
            let content = self.cached(&self.matchups_request(query)?)?;
            Ok(assemble_matchups(query, &content, &*self.champions)?.entries)
        })())
    }
}

// ---- Parsing (pure) -------------------------------------------------------------------

/// Extracts `result.structuredContent` from an MCP answer sent as JSON or as one SSE event.
pub fn parse_mcp_body(text: &str) -> Result<Value, Fail> {
    let trimmed = text.trim_start();
    let json_text = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        let mut data = Vec::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                data.push(rest.trim_start());
            } else if line.trim().is_empty() && !data.is_empty() {
                break;
            }
        }
        data.join("\n")
    };
    let message: Value = serde_json::from_str(&json_text).map_err(|_| Fail::BadFormat)?;
    if message.get("error").is_some() {
        return Err(Fail::BadFormat);
    }
    let result = message.get("result").ok_or(Fail::BadFormat)?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(Fail::NoData);
    }
    result
        .get("structuredContent")
        .filter(|c| c.is_object())
        .cloned()
        .ok_or(Fail::BadFormat)
}

/// `"52.1%"` → `0.521`.
pub fn parse_percent(raw: &str) -> Option<f32> {
    let value: f32 = raw
        .trim()
        .trim_end_matches('%')
        .trim()
        .replace(',', ".")
        .parse()
        .ok()?;
    Some((value / 100.0).clamp(0.0, 1.0))
}

/// The language MetaBot.GG should name items and runes in, from the client locale.
pub fn metabot_lang(locale: Option<&str>) -> &'static str {
    let Some(locale) = locale else { return "en" };
    let locale = locale.replace('-', "_");
    if let Some(exact) = LANGS.iter().find(|l| l.eq_ignore_ascii_case(&locale)) {
        return exact;
    }
    let language = locale.split('_').next().unwrap_or("").to_ascii_lowercase();
    LANGS
        .iter()
        .find(|l| l.split('_').next() == Some(language.as_str()))
        .copied()
        .unwrap_or("en")
}

fn stat<'a>(value: &'a Value, label: &str) -> Option<&'a str> {
    value
        .get("stats")?
        .as_array()?
        .iter()
        .find(|s| {
            s.get("label")
                .and_then(Value::as_str)
                .is_some_and(|l| l.eq_ignore_ascii_case(label))
        })?
        .get("value")?
        .as_str()
}

fn entries(content: &Value) -> &[Value] {
    content
        .get("entries")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn patch_of(content: &Value) -> Option<String> {
    content
        .get("patch")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn attribution(content: &Value) -> Option<Attribution> {
    let url = content.pointer("/cta/url").and_then(Value::as_str)?;
    url.starts_with(LINK_PREFIX).then(|| Attribution {
        provider: PROVIDER.to_string(),
        url: url.to_string(),
    })
}

/// The champion an entry names, from its page link (`/champion/<Slug>/…`) or its name.
fn entry_champion(entry: &Value, champions: &dyn ChampionLookup) -> i64 {
    let from_url = entry
        .get("url")
        .and_then(Value::as_str)
        .and_then(|u| u.split("/champion/").nth(1))
        .and_then(|rest| rest.split('/').next())
        .map(|slug| champions.resolve_champion_id(slug))
        .unwrap_or(0);
    if from_url > 0 {
        return from_url;
    }
    entry
        .get("name")
        .and_then(Value::as_str)
        .map(|n| champions.resolve_champion_id(n))
        .unwrap_or(0)
}

fn check_patch(query: &StatsQuery, patch: Option<&str>) -> Result<(), Fail> {
    match (&query.patch, patch) {
        (Some(wanted), Some(served)) if wanted != served => Err(Fail::NoData),
        _ => Ok(()),
    }
}

/// What the answer actually describes: champion-wide, all ranks, the served patch.
fn served_query(query: &StatsQuery, patch: Option<String>) -> StatsQuery {
    StatsQuery {
        champion_id: query.champion_id,
        position: None,
        queue: query.queue,
        bracket: RankBracket::All,
        patch,
    }
}

/// Turns the build page (+ optional overview) into a [`ChampionBuild`]. Sections are told apart
/// by what their names resolve to (titles are translated), names are matched against the
/// client's game data; unknown names are dropped.
pub fn assemble_build(
    query: &StatsQuery,
    build: &CachedContent,
    overview: Option<&CachedContent>,
    index: &GameDataIndex,
) -> Result<ChampionBuild, Fail> {
    let patch = patch_of(&build.content);
    check_patch(query, patch.as_deref())?;

    let mut runes: Option<RuneSetup> = None;
    let mut core = Vec::new();
    let mut boots = Vec::new();
    for section in build
        .content
        .get("sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let names: Vec<&str> = section
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let perks: Vec<i64> = names.iter().filter_map(|n| index.perk_id(n)).collect();
        let items: Vec<i64> = names.iter().filter_map(|n| index.item_id(n)).collect();
        if perks.len() > items.len() {
            if runes.is_none() {
                runes = runes_from_perks(perks, index);
            }
        } else {
            for id in items {
                let list = if index.is_boots(id) {
                    &mut boots
                } else {
                    &mut core
                };
                if !list.contains(&id) {
                    list.push(id);
                }
            }
        }
    }

    let rates = overview
        .map(|o| Rates {
            win_rate: stat(&o.content, "Win rate")
                .and_then(parse_percent)
                .unwrap_or(0.0),
            pick_rate: stat(&o.content, "Pick rate")
                .and_then(parse_percent)
                .unwrap_or(0.0),
            ban_rate: stat(&o.content, "Ban rate")
                .and_then(parse_percent)
                .unwrap_or(0.0),
            games: 0,
        })
        .unwrap_or_default();

    let mut items = Vec::new();
    if !boots.is_empty() {
        items.push(ItemBlock {
            stage: Some(ItemStage::Boots),
            item_ids: boots,
            rates: Rates::default(),
        });
    }
    if !core.is_empty() {
        items.push(ItemBlock {
            stage: Some(ItemStage::Core),
            item_ids: core,
            rates: Rates::default(),
        });
    }
    if runes.is_none() && items.is_empty() && rates == Rates::default() {
        return Err(Fail::NoData);
    }
    Ok(ChampionBuild {
        query: served_query(query, patch),
        origin: StatsOrigin::Online,
        rates,
        runes: runes.into_iter().map(|r| (r, Rates::default())).collect(),
        spells: Vec::new(),
        items,
        skills: Vec::new(),
        role_distribution: Vec::new(),
        tier_rank: None,
        attribution: attribution(&build.content),
    })
}

/// Keystone first; the primary style is the keystone's, the secondary the first other style.
fn runes_from_perks(perks: Vec<i64>, index: &GameDataIndex) -> Option<RuneSetup> {
    let primary = index.style_of(*perks.first()?)?;
    let sub = perks
        .iter()
        .filter_map(|p| index.style_of(*p))
        .find(|s| *s != primary)
        .unwrap_or(0);
    Some(RuneSetup {
        primary_style_id: primary,
        sub_style_id: sub,
        perk_ids: perks,
        shard_ids: Vec::new(),
    })
}

/// Toughest matchups as served, sorted best for the champion first.
pub fn assemble_matchups(
    query: &StatsQuery,
    content: &CachedContent,
    champions: &dyn ChampionLookup,
) -> Result<MatchupList, Fail> {
    let patch = patch_of(&content.content);
    check_patch(query, patch.as_deref())?;
    let mut list: Vec<Matchup> = entries(&content.content)
        .iter()
        .filter_map(|e| {
            let opponent_id = entry_champion(e, champions);
            let win_rate = stat(e, "Win rate").and_then(parse_percent)?;
            (opponent_id > 0 && opponent_id != query.champion_id).then(|| Matchup {
                champion_id: query.champion_id,
                opponent_id,
                position: None,
                rates: Rates {
                    win_rate,
                    ..Rates::default()
                },
                gold_diff_15: None,
                origin: StatsOrigin::Online,
            })
        })
        .collect();
    if list.is_empty() {
        return Err(Fail::NoData);
    }
    list.sort_by(|a, b| b.rates.win_rate.total_cmp(&a.rates.win_rate));
    Ok(MatchupList {
        query: served_query(query, patch),
        origin: StatsOrigin::Online,
        entries: list,
        attribution: attribution(&content.content),
    })
}

/// Merges one meta answer per role into a [`TierList`] (best tier, then win rate, first).
pub fn assemble_tier_list(
    queue: StatsQueue,
    parts: &[(Position, CachedContent)],
    champions: &dyn ChampionLookup,
) -> Result<TierList, Fail> {
    let mut list = TierList {
        queue,
        bracket: RankBracket::All,
        origin: StatsOrigin::Online,
        ..TierList::default()
    };
    for (position, part) in parts {
        if list.patch.is_empty() {
            list.patch = patch_of(&part.content).unwrap_or_default();
        }
        if list.attribution.is_none() {
            list.attribution = attribution(&part.content);
        }
        for e in entries(&part.content) {
            let champion_id = entry_champion(e, champions);
            let tier = e.get("tier").and_then(Value::as_str).and_then(parse_tier);
            let (Some(tier), true) = (tier, champion_id > 0) else {
                continue;
            };
            list.entries.push(TierEntry {
                champion_id,
                position: *position,
                tier,
                rates: Rates {
                    win_rate: stat(e, "Win rate").and_then(parse_percent).unwrap_or(0.0),
                    pick_rate: stat(e, "Pick rate").and_then(parse_percent).unwrap_or(0.0),
                    ban_rate: stat(e, "Ban rate").and_then(parse_percent).unwrap_or(0.0),
                    games: 0,
                },
            });
        }
    }
    if list.entries.is_empty() {
        return Err(Fail::NoData);
    }
    list.entries.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(b.rates.win_rate.total_cmp(&a.rates.win_rate))
    });
    Ok(list)
}

fn parse_tier(raw: &str) -> Option<Tier> {
    match raw.trim().chars().next()?.to_ascii_uppercase() {
        'S' => Some(Tier::S),
        'A' => Some(Tier::A),
        'B' => Some(Tier::B),
        'C' => Some(Tier::C),
        'D' => Some(Tier::D),
        _ => None,
    }
}

// ---- Per-patch disk cache --------------------------------------------------------------

/// One stored answer: when it was fetched, the patch it describes, its structured content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedContent {
    pub fetched_at: u64,
    pub patch: String,
    pub content: Value,
}

impl CachedContent {
    fn is_fresh(&self, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) < FRESH_FOR.as_secs()
    }
}

/// `<dir>/current-patch` names the newest patch seen; answers live in `<dir>/<patch>/<key>.json`.
/// Writing an answer for a newer patch drops every other patch folder.
pub(crate) struct PatchCache {
    dir: PathBuf,
}

impl PatchCache {
    fn current_patch(&self) -> Option<String> {
        let raw = std::fs::read_to_string(self.dir.join("current-patch")).ok()?;
        let patch = sanitize(raw.trim());
        (!patch.is_empty()).then_some(patch)
    }

    pub(crate) fn read(&self, key: &str) -> Option<CachedContent> {
        let patch = self.current_patch()?;
        let bytes = std::fs::read(self.dir.join(patch).join(format!("{key}.json"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub(crate) fn write(&self, key: &str, entry: &CachedContent) -> std::io::Result<()> {
        let patch = sanitize(&entry.patch);
        let patch_dir = self.dir.join(&patch);
        std::fs::create_dir_all(&patch_dir)?;
        let path = patch_dir.join(format!("{key}.json"));
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(entry)?)?;
        std::fs::rename(&tmp, &path)?;
        if self.current_patch().as_deref() != Some(patch.as_str()) {
            std::fs::write(self.dir.join("current-patch"), &patch)?;
            prune_except(&self.dir, &patch);
        }
        Ok(())
    }
}

fn prune_except(dir: &Path, keep: &str) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        if entry.path().is_dir() && entry.file_name().to_string_lossy() != keep {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Keeps file names to `[a-z0-9._-]`.
fn sanitize(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::stats::game_data::parse_game_data;
    use std::sync::atomic::AtomicU8;

    fn data_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name)
    }

    fn sample_content(name: &str) -> CachedContent {
        let content = parse_mcp_body(&std::fs::read_to_string(data_path(name)).unwrap()).unwrap();
        CachedContent {
            fetched_at: now_secs(),
            patch: patch_of(&content).unwrap(),
            content,
        }
    }

    fn index() -> GameDataIndex {
        let json = |n: &str| serde_json::from_slice(&std::fs::read(data_path(n)).unwrap()).unwrap();
        parse_game_data(
            &json("lcu_items.json"),
            &json("lcu_perks.json"),
            &json("lcu_perkstyles.json"),
            &json("lcu_summoner_spells.json"),
        )
        .1
    }

    fn ahri() -> StatsQuery {
        StatsQuery {
            champion_id: 103,
            position: Some(Position::Mid),
            ..StatsQuery::default()
        }
    }

    fn source(online: bool, cache_dir: PathBuf) -> MetaBotSource {
        let flags = Arc::new(Flags::new(&Settings {
            online_stats: online,
            ..Settings::default()
        }));
        MetaBotSource::new(
            flags,
            Arc::new(AtomicU8::new(0)),
            cache_dir,
            Arc::new(BuiltinChampionCatalog),
        )
    }

    #[test]
    fn reads_json_and_sse_answers() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"structuredContent":{"patch":"1"}}}"#;
        assert_eq!(parse_mcp_body(json).unwrap()["patch"], "1");
        let sse = format!("event: message\ndata: {json}\n\n");
        assert_eq!(parse_mcp_body(&sse).unwrap()["patch"], "1");
        assert_eq!(
            parse_mcp_body(r#"{"result":{"isError":true,"content":[]}}"#),
            Err(Fail::NoData)
        );
        assert_eq!(
            parse_mcp_body(r#"{"error":{"code":-32600}}"#),
            Err(Fail::BadFormat)
        );
        assert_eq!(parse_mcp_body("<html>"), Err(Fail::BadFormat));
    }

    #[test]
    fn parses_percent_and_languages() {
        let close = |raw: &str, want: f32| (parse_percent(raw).unwrap() - want).abs() < 1e-5;
        assert!(close("52.1%", 0.521));
        assert!(close("36,4 %", 0.364));
        assert_eq!(parse_percent("n/a"), None);
        assert_eq!(metabot_lang(Some("fr_FR")), "fr_FR");
        assert_eq!(metabot_lang(Some("fr-CA")), "fr_FR");
        assert_eq!(metabot_lang(Some("en_GB")), "en");
        assert_eq!(metabot_lang(Some("hu_HU")), "en");
        assert_eq!(metabot_lang(None), "en");
    }

    #[test]
    fn assembles_a_build_from_names() {
        let build = assemble_build(
            &ahri(),
            &sample_content("metabot_build.sse"),
            Some(&sample_content("metabot_lookup.sse")),
            &index(),
        )
        .unwrap();
        assert_eq!(build.origin, StatsOrigin::Online);
        assert_eq!(
            build.query.position, None,
            "the provider's build is champion-wide"
        );
        assert_eq!(build.query.bracket, RankBracket::All);
        assert_eq!(build.query.patch.as_deref(), Some("26.22"));
        assert!((build.rates.win_rate - 0.508).abs() < 1e-4);
        assert!((build.rates.pick_rate - 0.063).abs() < 1e-4);

        let (runes, _) = &build.runes[0];
        assert_eq!(runes.perk_ids, vec![8112, 8139, 8140, 8106, 8473, 8242]);
        assert_eq!((runes.primary_style_id, runes.sub_style_id), (8100, 8400));

        assert_eq!(build.items[0].stage, Some(ItemStage::Boots));
        assert_eq!(build.items[0].item_ids, vec![3175]);
        assert_eq!(build.items[1].stage, Some(ItemStage::Core));
        assert_eq!(
            build.items[1].item_ids,
            vec![4645, 3118, 3089, 3157],
            "unknown names drop"
        );
        assert_eq!(
            build.attribution,
            Some(Attribution {
                provider: "MetaBot.GG".into(),
                url: "https://metabot.gg/en/league/champion/Ahri/build".into()
            })
        );

        let other_patch = StatsQuery {
            patch: Some("25.1".into()),
            ..ahri()
        };
        assert_eq!(
            assemble_build(
                &other_patch,
                &sample_content("metabot_build.sse"),
                None,
                &index()
            ),
            Err(Fail::NoData)
        );
        // Without the client's game data nothing resolves and there are no rates: no data.
        assert_eq!(
            assemble_build(
                &ahri(),
                &sample_content("metabot_build.sse"),
                None,
                &GameDataIndex::default()
            ),
            Err(Fail::NoData)
        );
    }

    #[test]
    fn assembles_matchups_and_tier_lists() {
        let list = assemble_matchups(
            &ahri(),
            &sample_content("metabot_counters.sse"),
            &BuiltinChampionCatalog,
        )
        .unwrap();
        let opponents: Vec<i64> = list.entries.iter().map(|m| m.opponent_id).collect();
        // Smolder 40.9 %, Swain 39.0 %, Riven 37.1 %, Teemo 36.4 %: best for Ahri first.
        assert_eq!(opponents, vec![901, 50, 92, 17]);
        let teemo = list.entries.iter().find(|m| m.opponent_id == 17).unwrap();
        assert!((teemo.rates.win_rate - 0.364).abs() < 1e-4);
        assert!(list
            .entries
            .iter()
            .all(|m| m.champion_id == 103 && m.origin == StatsOrigin::Online));
        assert!(list.attribution.unwrap().url.ends_with("/Ahri/counters"));

        let parts = vec![(Position::Mid, sample_content("metabot_meta_mid.sse"))];
        let tiers =
            assemble_tier_list(StatsQueue::RankedSolo, &parts, &BuiltinChampionCatalog).unwrap();
        assert_eq!(tiers.patch, "26.22");
        assert_eq!(tiers.bracket, RankBracket::All);
        assert_eq!(tiers.entries.len(), 4);
        assert_eq!(tiers.entries[0].champion_id, 105, "Fizz leads");
        assert!(tiers
            .entries
            .iter()
            .all(|e| e.position == Position::Mid && e.tier == Tier::S));
        assert!(tiers.attribution.is_some());
    }

    #[test]
    fn requests_carry_no_identity() {
        let dir = tempfile::tempdir().unwrap();
        let src = source(true, dir.path().to_path_buf());
        src.set_context(ResolveContext {
            index: Arc::new(GameDataIndex::default()),
            locale: Some("fr_FR".into()),
        });
        let [build, overview] = src.build_requests(&ahri()).unwrap();
        let mut requests = vec![build, overview, src.matchups_request(&ahri()).unwrap()];
        requests.extend(
            MetaBotSource::tier_requests(StatsQueue::RankedSolo)
                .unwrap()
                .into_iter()
                .map(|(_, r)| r),
        );
        requests.extend(
            MetaBotSource::tier_requests(StatsQueue::Aram)
                .unwrap()
                .into_iter()
                .map(|(_, r)| r),
        );
        for r in &requests {
            for key in r.args.as_object().unwrap().keys() {
                assert!(
                    ["game", "name", "filter", "mode", "lang"].contains(&key.as_str()),
                    "{key}"
                );
            }
        }
        assert_eq!(requests[0].args["name"], "Ahri");
        assert_eq!(requests[0].args["lang"], "fr_FR");
        assert_eq!(requests[1].args["lang"], "en");
        assert_eq!(requests[0].cache_key(), "get_entity_build-fr_fr-ahri");
        assert_eq!(
            MetaBotSource::tier_requests(StatsQueue::Normal),
            Err(Fail::NoData)
        );
        assert!(src
            .build_requests(&StatsQuery {
                champion_id: 0,
                ..ahri()
            })
            .is_err());
    }

    #[test]
    fn cache_keeps_only_the_newest_patch() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PatchCache {
            dir: dir.path().join("metabot"),
        };
        assert!(cache.read("k").is_none());
        let entry = |patch: &str, at: u64| CachedContent {
            fetched_at: at,
            patch: patch.into(),
            content: json!({ "patch": patch }),
        };
        cache.write("k", &entry("26.21", 5)).unwrap();
        cache.write("other", &entry("26.21", 5)).unwrap();
        assert_eq!(cache.read("k").unwrap().fetched_at, 5);
        cache.write("k", &entry("26.22", 7)).unwrap();
        assert_eq!(cache.read("k").unwrap().patch, "26.22");
        assert!(cache.read("other").is_none(), "the old patch is gone");
        assert!(!dir.path().join("metabot").join("26.21").exists());

        let now = now_secs();
        assert!(entry("x", now - 60).is_fresh(now));
        assert!(!entry("x", now - FRESH_FOR.as_secs() - 1).is_fresh(now));
    }

    #[tokio::test]
    async fn switched_off_performs_no_io() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/api/mcp", listener.local_addr().unwrap());
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("metabot");
        let src = source(false, cache_dir.clone()).with_endpoint(endpoint);

        assert_eq!(src.fetch_build(&ahri()).await, Err(Fail::OnlineDisabled));
        assert_eq!(src.fetch_matchups(&ahri()).await, Err(Fail::OnlineDisabled));
        assert_eq!(
            src.fetch_tier_list(StatsQueue::RankedSolo).await,
            Err(Fail::OnlineDisabled)
        );
        assert_eq!(
            StatsSource::build(&src, &ahri()),
            Err(hypatia_shared::StatsUnavailable::OnlineDisabled)
        );
        assert_eq!(
            StatsSource::tier_list(&src, StatsQueue::RankedSolo, RankBracket::EmeraldPlus),
            Err(hypatia_shared::StatsUnavailable::OnlineDisabled)
        );
        assert_eq!(
            StatsSource::matchups(&src, &ahri()),
            Err(hypatia_shared::StatsUnavailable::OnlineDisabled)
        );

        let accepted = tokio::time::timeout(Duration::from_millis(200), listener.accept()).await;
        assert!(
            accepted.is_err(),
            "no connection may be attempted while switched off"
        );
        assert!(!cache_dir.exists(), "no disk access either");
        assert!(
            src.http.get().is_none(),
            "the HTTP client is never even built"
        );
    }

    #[tokio::test]
    async fn switched_on_fetches_once_then_serves_the_cache() {
        use axum::{extract::State, routing::post, Json, Router};
        use std::sync::Mutex;
        /// Requests seen, then the canned build and lookup answers.
        type MockState = (Arc<Mutex<Vec<Value>>>, String, String);

        let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
        let build = std::fs::read_to_string(data_path("metabot_build.sse")).unwrap();
        let lookup = std::fs::read_to_string(data_path("metabot_lookup.sse")).unwrap();
        let app =
            Router::new()
                .route(
                    "/api/mcp",
                    post(
                        |State((seen, build, lookup)): State<MockState>,
                         Json(body): Json<Value>| async move {
                            let tool = body
                                .pointer("/params/name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            seen.lock().unwrap().push(body);
                            if tool == "get_entity_build" {
                                build
                            } else {
                                lookup
                            }
                        },
                    ),
                )
                .with_state((seen.clone(), build, lookup));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/api/mcp", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let dir = tempfile::tempdir().unwrap();
        let src = source(true, dir.path().join("metabot")).with_endpoint(endpoint);
        src.set_context(ResolveContext {
            index: Arc::new(index()),
            locale: Some("en_US".into()),
        });

        let first = src.fetch_build(&ahri()).await.unwrap();
        assert_eq!(first.runes[0].0.perk_ids[0], 8112);
        assert_eq!(seen.lock().unwrap().len(), 2);
        for body in seen.lock().unwrap().iter() {
            let text = body.to_string();
            assert!(
                !text.contains("puuid") && !text.contains("summoner"),
                "{text}"
            );
        }

        let again = src.fetch_build(&ahri()).await.unwrap();
        assert_eq!(again, first);
        assert_eq!(
            seen.lock().unwrap().len(),
            2,
            "fresh answers come from the cache"
        );
        assert_eq!(
            StatsSource::build(&src, &ahri()).unwrap(),
            first,
            "the sync read sees the cache"
        );
    }
}
