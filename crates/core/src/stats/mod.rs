//! Champion Stats data layer. Views use one type: [`ChampionStats`], reachable as
//! `AppState::stats`.
//!
//! # API for the desktop and the Mobile Remote
//!
//! Every getter is **synchronous and non-blocking**, safe to call from a UI loop on every
//! repaint. It returns a [`StatsState`]:
//! - `Ready(value)`: draw it, labelled with its `origin` ([`StatsOrigin`]) and, when present,
//!   its `attribution` as a visible clickable link (required by the online provider's terms).
//! - `Loading`: a fetch is running; the [`ChangeSignal`] is bumped when it lands, so wait on
//!   the signal and call the getter again.
//! - `Unavailable(why)`: `OnlineDisabled` (offer to turn on Online stats), `ClientOffline`
//!   (League is not running), `NoData`, or `Failed(message)` already in the active language.
//!   Failures are retried on a later call after a short cool-down.
//!
//! | Getter | Source | Origin |
//! |---|---|---|
//! | [`ChampionStats::game_data`] | client `lol-game-data` (items, runes, styles, spells) | — |
//! | [`ChampionStats::recommended_build`] | client recommended rune pages + spells | `RiotClient` |
//! | [`ChampionStats::build`] | online provider (opt-in) | `Online` |
//! | [`ChampionStats::tier_list`] | online provider (opt-in) | `Online` |
//! | [`ChampionStats::matchups`] | online provider (opt-in) | `Online` |
//! | [`ChampionStats::player_profile`] | client match history (last 20 games) | `Local` |
//!
//! In demo mode (`--demo`) every getter answers at once from [`demo::DemoStats`] with origin
//! `Demo`, Online stats switch or not. Icons for ids in these values come from the asset cache:
//! `AppState::resolve_cached_asset("item" | "perk" | "style" | "spell" | "champion", id)`.
//!
//! Online data describes what the provider serves: champion-wide builds (`query.position` is
//! `None`), all ranks (`bracket` is `All`), Ranked Solo/Duo (plus ARAM and Arena tier lists).
//! Call [`ChampionStats::refresh_player_profile`] after a game ends to re-read the history.

pub mod demo;
pub mod game_data;
pub mod lcu;
pub mod online;
pub mod profile;

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub use hypatia_shared::StatsState;
use hypatia_shared::{
    ChampionBuild, GameData, HelperStats, Lang, MatchupList, PlayerProfile, Position, RankBracket,
    StatsOrigin, StatsQuery, StatsQueue, StatsSource, StatsUnavailable, TierList,
};

use self::demo::DemoStats;
use self::game_data::LoadedGameData;
use self::online::{MetaBotSource, ResolveContext};
use crate::assets::{resolve_live_lcu_creds, GameAssetCache};
use crate::discovery::DiscoveryManager;
use crate::riot_http::ReadOnlyRiotClient;
use crate::settings::Flags;
use crate::signal::ChangeSignal;

/// Client data for one session: re-read only when the client restarts (new port).
const SESSION_TTL: Duration = Duration::from_secs(24 * 3600);
const RECOMMENDED_TTL: Duration = Duration::from_secs(3600);
const PROFILE_TTL: Duration = Duration::from_secs(300);
/// Online answers are re-checked against the disk cache (which holds them for 6 h).
const ONLINE_TTL: Duration = Duration::from_secs(1800);
const LOCAL_RETRY: Duration = Duration::from_secs(15);
const ONLINE_RETRY: Duration = Duration::from_secs(60);

/// Why a fetch failed, localized only when a view reads it (so a language switch applies).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    OnlineDisabled,
    ClientOffline,
    NoData,
    ClientNoAnswer,
    ClientHttp(u16),
    ClientBadFormat,
    Network,
    RateLimited,
    OnlineHttp(u16),
    BadFormat,
}

impl Fail {
    pub fn localize(self, lang: Lang) -> StatsUnavailable {
        let message = match self {
            Fail::OnlineDisabled => return StatsUnavailable::OnlineDisabled,
            Fail::ClientOffline => return StatsUnavailable::ClientOffline,
            Fail::NoData => return StatsUnavailable::NoData,
            Fail::ClientNoAnswer => lang
                .pick(
                    "The League client did not answer.",
                    "Le client League n'a pas répondu.",
                )
                .to_string(),
            Fail::ClientHttp(status) => format!(
                "{} (HTTP {status})",
                lang.pick(
                    "The League client refused the request",
                    "Le client League a refusé la requête"
                )
            ),
            Fail::ClientBadFormat => lang
                .pick(
                    "The League client answered in an unexpected format.",
                    "Le client League a répondu dans un format inattendu.",
                )
                .to_string(),
            Fail::Network => lang
                .pick(
                    "Could not reach MetaBot.GG. Check your connection.",
                    "Impossible de joindre MetaBot.GG. Vérifiez votre connexion.",
                )
                .to_string(),
            Fail::RateLimited => lang
                .pick(
                    "MetaBot.GG asks to slow down; trying again shortly.",
                    "MetaBot.GG demande de ralentir ; nouvel essai sous peu.",
                )
                .to_string(),
            Fail::OnlineHttp(status) => format!(
                "{} (HTTP {status})",
                lang.pick(
                    "MetaBot.GG answered with an error",
                    "MetaBot.GG a répondu par une erreur"
                )
            ),
            Fail::BadFormat => lang
                .pick(
                    "MetaBot.GG answered in an unexpected format.",
                    "MetaBot.GG a répondu dans un format inattendu.",
                )
                .to_string(),
        };
        StatsUnavailable::Failed(message)
    }
}

pub(crate) fn load_lang(lang: &AtomicU8) -> Lang {
    if lang.load(Ordering::Relaxed) == Lang::Fr as u8 {
        Lang::Fr
    } else {
        Lang::En
    }
}

/// One memoized value per key: the last result, when it landed, and whether a fetch runs.
struct Memo<K, V> {
    slots: Mutex<HashMap<K, Slot<V>>>,
}

struct Slot<V> {
    result: Option<Result<V, Fail>>,
    at: Instant,
    in_flight: bool,
}

impl<K: Eq + Hash + Clone, V: Clone> Memo<K, V> {
    fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }

    /// The state to show, and whether the caller must start a fetch (now marked in flight).
    /// A stale success keeps showing while it refreshes.
    fn read(
        &self,
        key: &K,
        ok_ttl: Duration,
        err_ttl: Duration,
        lang: Lang,
    ) -> (StatsState<V>, bool) {
        let mut slots = self.slots.lock().unwrap();
        let Some(slot) = slots.get_mut(key) else {
            slots.insert(
                key.clone(),
                Slot {
                    result: None,
                    at: Instant::now(),
                    in_flight: true,
                },
            );
            return (StatsState::Loading, true);
        };
        let state = match &slot.result {
            None => StatsState::Loading,
            Some(Ok(v)) => StatsState::Ready(v.clone()),
            Some(Err(f)) => StatsState::Unavailable(f.localize(lang)),
        };
        if slot.in_flight {
            return (state, false);
        }
        let ttl = if matches!(slot.result, Some(Ok(_))) {
            ok_ttl
        } else {
            err_ttl
        };
        if slot.at.elapsed() < ttl {
            return (state, false);
        }
        slot.in_flight = true;
        match state {
            StatsState::Ready(_) => (state, true),
            _ => (StatsState::Loading, true),
        }
    }

    fn finish(&self, key: K, result: Result<V, Fail>) {
        self.slots.lock().unwrap().insert(
            key,
            Slot {
                result: Some(result),
                at: Instant::now(),
                in_flight: false,
            },
        );
    }

    fn ok(&self, key: &K) -> Option<V> {
        match self.slots.lock().unwrap().get(key)?.result.as_ref()? {
            Ok(v) => Some(v.clone()),
            Err(_) => None,
        }
    }

    fn clear(&self) {
        self.slots.lock().unwrap().clear();
    }
}

/// The Champion Stats service. Cheap to clone; see the module docs for the API.
#[derive(Clone)]
pub struct ChampionStats {
    inner: Arc<Inner>,
}

struct Inner {
    discovery: Arc<DiscoveryManager>,
    client: ReadOnlyRiotClient,
    assets: Arc<GameAssetCache>,
    flags: Arc<Flags>,
    signal: ChangeSignal,
    lang: Arc<AtomicU8>,
    runtime: Option<tokio::runtime::Handle>,
    online: MetaBotSource,
    /// Bumped when client game data lands, so online names are resolved again.
    game_data_generation: AtomicU64,
    /// Bumped whenever a fetch lands or a cache is cleared (see [`ChampionStats::revision`]).
    revision: AtomicU64,
    demo_game_data: [OnceLock<Arc<GameData>>; 2],
    game_data: Memo<u16, Arc<LoadedGameData>>,
    recommended: Memo<(u16, i64, Option<Position>), ChampionBuild>,
    profile: Memo<u16, PlayerProfile>,
    builds: Memo<(StatsQuery, u64), ChampionBuild>,
    tiers: Memo<StatsQueue, TierList>,
    matchups: Memo<StatsQuery, MatchupList>,
}

impl ChampionStats {
    /// Built by `AppState` from its shared parts; call from inside the Tokio runtime so
    /// fetches can be spawned from any thread later.
    pub fn new(
        discovery: Arc<DiscoveryManager>,
        client: ReadOnlyRiotClient,
        assets: Arc<GameAssetCache>,
        flags: Arc<Flags>,
        signal: ChangeSignal,
        lang: Arc<AtomicU8>,
    ) -> Self {
        let online = MetaBotSource::new(
            flags.clone(),
            lang.clone(),
            MetaBotSource::default_cache_dir(),
            assets.clone(),
        );
        Self {
            inner: Arc::new(Inner {
                discovery,
                client,
                assets,
                flags,
                signal,
                lang,
                runtime: tokio::runtime::Handle::try_current().ok(),
                online,
                game_data_generation: AtomicU64::new(0),
                revision: AtomicU64::new(0),
                demo_game_data: [OnceLock::new(), OnceLock::new()],
                game_data: Memo::new(),
                recommended: Memo::new(),
                profile: Memo::new(),
                builds: Memo::new(),
                tiers: Memo::new(),
                matchups: Memo::new(),
            }),
        }
    }

    /// Changes whenever a getter could answer differently than before (a fetch landed, a cache
    /// was cleared); the Match Projection keys its champ-select helper on it.
    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Acquire)
    }

    /// Whether Online stats are switched on (`Settings::online_stats`).
    pub fn online_enabled(&self) -> bool {
        self.inner.flags.online_stats()
    }

    /// Items, runes, rune styles and summoner spells of the running client (names in its
    /// language), for drawing names and icons. Read once per client session.
    pub fn game_data(&self) -> StatsState<Arc<GameData>> {
        let lang = self.lang();
        if self.is_demo() {
            let slot = &self.inner.demo_game_data[lang as usize];
            return StatsState::Ready(
                slot.get_or_init(|| Arc::new(DemoStats.game_data(lang)))
                    .clone(),
            );
        }
        let Some((port, token)) = self.creds() else {
            return StatsState::Unavailable(StatsUnavailable::ClientOffline);
        };
        let (state, start) = self
            .inner
            .game_data
            .read(&port, SESSION_TTL, LOCAL_RETRY, lang);
        if start {
            let inner = self.inner.clone();
            self.spawn(async move {
                Inner::load_game_data(&inner, port, &token).await;
                inner.landed();
            });
        }
        map_state(state, |g| g.data.clone())
    }

    /// Riot's recommended rune pages and summoner spells for a champion and position, read from
    /// the client (no rates). Origin `RiotClient`.
    pub fn recommended_build(&self, query: &StatsQuery) -> StatsState<ChampionBuild> {
        if self.is_demo() {
            return DemoStats.recommended_build(query).into();
        }
        let Some((port, token)) = self.creds() else {
            return StatsState::Unavailable(StatsUnavailable::ClientOffline);
        };
        let key = (port, query.champion_id, query.position);
        let (state, start) =
            self.inner
                .recommended
                .read(&key, RECOMMENDED_TTL, LOCAL_RETRY, self.lang());
        if start {
            let inner = self.inner.clone();
            let query = query.clone();
            self.spawn(async move {
                let result = lcu::load_recommended(&inner.client, port, &token, &query).await;
                inner.recommended.finish(key, result);
                inner.landed();
            });
        }
        state
    }

    /// Global build for a champion (runes, items, rates) from the online provider.
    pub fn build(&self, query: &StatsQuery) -> StatsState<ChampionBuild> {
        if self.is_demo() {
            return DemoStats.build(query).into();
        }
        if !self.online_enabled() {
            return StatsState::Unavailable(StatsUnavailable::OnlineDisabled);
        }
        let key = (
            query.clone(),
            self.inner.game_data_generation.load(Ordering::Relaxed),
        );
        let (state, start) = self
            .inner
            .builds
            .read(&key, ONLINE_TTL, ONLINE_RETRY, self.lang());
        if start {
            let inner = self.inner.clone();
            let creds = self.creds();
            self.spawn(async move {
                if let Some((port, token)) = creds {
                    Inner::ensure_game_data(&inner, port, &token).await;
                }
                let result = inner.online.fetch_build(&key.0).await;
                inner.builds.finish(key, result);
                inner.landed();
            });
        }
        state
    }

    /// Global tier list for a queue from the online provider. The provider aggregates all
    /// ranks, so the result says `bracket: All` whatever `bracket` asked.
    pub fn tier_list(&self, queue: StatsQueue, bracket: RankBracket) -> StatsState<TierList> {
        if self.is_demo() {
            return DemoStats.tier_list(queue, bracket).into();
        }
        if !self.online_enabled() {
            return StatsState::Unavailable(StatsUnavailable::OnlineDisabled);
        }
        let (state, start) = self
            .inner
            .tiers
            .read(&queue, ONLINE_TTL, ONLINE_RETRY, self.lang());
        if start {
            let inner = self.inner.clone();
            self.spawn(async move {
                let result = inner.online.fetch_tier_list(queue).await;
                inner.tiers.finish(queue, result);
                inner.landed();
            });
        }
        state
    }

    /// A champion's matchups from the online provider, best for the champion first.
    pub fn matchups(&self, query: &StatsQuery) -> StatsState<MatchupList> {
        if self.is_demo() {
            return DemoStats.matchup_list(query).into();
        }
        if !self.online_enabled() {
            return StatsState::Unavailable(StatsUnavailable::OnlineDisabled);
        }
        let (state, start) = self
            .inner
            .matchups
            .read(query, ONLINE_TTL, ONLINE_RETRY, self.lang());
        if start {
            let inner = self.inner.clone();
            let query = query.clone();
            self.spawn(async move {
                let result = inner.online.fetch_matchups(&query).await;
                inner.matchups.finish(query, result);
                inner.landed();
            });
        }
        state
    }

    /// My Stats: the player's last 20 games from the client and the per-champion summary.
    /// Origin `Local`.
    pub fn player_profile(&self) -> StatsState<PlayerProfile> {
        if self.is_demo() {
            return StatsState::Ready(DemoStats.player_profile());
        }
        let Some((port, token)) = self.creds() else {
            return StatsState::Unavailable(StatsUnavailable::ClientOffline);
        };
        let (state, start) = self
            .inner
            .profile
            .read(&port, PROFILE_TTL, LOCAL_RETRY, self.lang());
        if start {
            let inner = self.inner.clone();
            self.spawn(async move {
                let result = lcu::load_match_history(&inner.client, port, &token)
                    .await
                    .map(|games| profile::build_profile(games, StatsOrigin::Local));
                inner.profile.finish(port, result);
                inner.landed();
            });
        }
        state
    }

    /// Forgets the cached match history so the next [`Self::player_profile`] re-reads it.
    pub fn refresh_player_profile(&self) {
        self.inner.profile.clear();
        self.inner.landed();
    }

    fn lang(&self) -> Lang {
        load_lang(&self.inner.lang)
    }

    fn is_demo(&self) -> bool {
        self.inner.discovery.is_demo_override()
    }

    fn creds(&self) -> Option<(u16, String)> {
        resolve_live_lcu_creds(&self.inner.discovery)
    }

    fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        let handle = self
            .inner
            .runtime
            .clone()
            .or_else(|| tokio::runtime::Handle::try_current().ok());
        match handle {
            Some(h) => {
                h.spawn(fut);
            }
            None => tracing::warn!("Champion Stats fetch dropped: no Tokio runtime"),
        }
    }
}

impl Inner {
    /// A fetch landed: views may draw something new.
    fn landed(&self) {
        self.revision.fetch_add(1, Ordering::Release);
        self.signal.bump();
    }

    async fn load_game_data(
        inner: &Arc<Inner>,
        port: u16,
        token: &str,
    ) -> Option<Arc<LoadedGameData>> {
        let result = game_data::load(&inner.client, port, token, &inner.assets)
            .await
            .map(Arc::new);
        let loaded = result.as_ref().ok().cloned();
        if let Some(g) = &loaded {
            inner.online.set_context(ResolveContext {
                index: g.index.clone(),
                locale: g.locale.clone(),
            });
            inner.game_data_generation.fetch_add(1, Ordering::Relaxed);
        }
        inner.game_data.finish(port, result);
        loaded
    }

    /// Game data for name resolution before an online fetch: the loaded copy, else a load.
    async fn ensure_game_data(inner: &Arc<Inner>, port: u16, token: &str) {
        if inner.game_data.ok(&port).is_none() {
            Inner::load_game_data(inner, port, token).await;
        }
    }
}

/// The champ-select helper reads Champion Stats through this seam of the Match Projection.
impl HelperStats for ChampionStats {
    fn online_enabled(&self) -> bool {
        ChampionStats::online_enabled(self)
    }

    fn recommended_build(&self, query: &StatsQuery) -> StatsState<ChampionBuild> {
        ChampionStats::recommended_build(self, query)
    }

    fn online_build(&self, query: &StatsQuery) -> StatsState<ChampionBuild> {
        self.build(query)
    }

    fn matchups(&self, query: &StatsQuery) -> StatsState<MatchupList> {
        ChampionStats::matchups(self, query)
    }

    fn player_profile(&self) -> StatsState<PlayerProfile> {
        ChampionStats::player_profile(self)
    }

    fn game_data(&self) -> StatsState<Arc<GameData>> {
        ChampionStats::game_data(self)
    }
}

fn map_state<T, U>(state: StatsState<T>, f: impl FnOnce(T) -> U) -> StatsState<U> {
    match state {
        StatsState::Loading => StatsState::Loading,
        StatsState::Ready(v) => StatsState::Ready(f(v)),
        StatsState::Unavailable(why) => StatsState::Unavailable(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::SecretVault;
    use crate::settings::Settings;

    fn service(demo: bool, settings: &Settings) -> (ChampionStats, Arc<AtomicU8>) {
        let signal = ChangeSignal::new();
        let discovery = if demo {
            DiscoveryManager::new_demo(SecretVault::new(), signal.clone())
        } else {
            DiscoveryManager::new_offline(SecretVault::new(), signal.clone())
        };
        let dir = std::env::temp_dir().join("hypatia-stats-service-test");
        let lang = Arc::new(AtomicU8::new(Lang::En as u8));
        let stats = ChampionStats::new(
            discovery,
            ReadOnlyRiotClient::new(),
            Arc::new(GameAssetCache::with_dir(dir)),
            Arc::new(Flags::new(settings)),
            signal,
            lang.clone(),
        );
        (stats, lang)
    }

    fn ahri() -> StatsQuery {
        StatsQuery {
            champion_id: 103,
            position: Some(Position::Mid),
            ..StatsQuery::default()
        }
    }

    #[tokio::test]
    async fn demo_answers_everything_at_once() {
        let (stats, lang) = service(true, &Settings::default());
        assert_eq!(
            stats.build(&ahri()).ready().unwrap().origin,
            StatsOrigin::Demo
        );
        assert_eq!(
            stats.recommended_build(&ahri()).ready().unwrap().origin,
            StatsOrigin::Demo
        );
        assert_eq!(
            stats.matchups(&ahri()).ready().unwrap().origin,
            StatsOrigin::Demo
        );
        assert_eq!(
            stats
                .tier_list(StatsQueue::RankedSolo, RankBracket::EmeraldPlus)
                .ready()
                .unwrap()
                .origin,
            StatsOrigin::Demo
        );
        assert_eq!(
            stats.player_profile().ready().unwrap().origin,
            StatsOrigin::Demo
        );

        let flash = |stats: &ChampionStats| {
            stats
                .game_data()
                .ready()
                .unwrap()
                .spells
                .iter()
                .find(|s| s.id == 4)
                .unwrap()
                .name
                .clone()
        };
        assert_eq!(flash(&stats), "Flash");
        lang.store(Lang::Fr as u8, Ordering::Relaxed);
        assert_eq!(flash(&stats), "Saut éclair");
    }

    #[tokio::test]
    async fn without_the_client_or_the_switch_nothing_is_fetched() {
        let (stats, _) = service(false, &Settings::default());
        assert!(!stats.online_enabled());
        let offline = StatsUnavailable::ClientOffline;
        assert_eq!(stats.game_data(), StatsState::Unavailable(offline.clone()));
        assert_eq!(
            stats.recommended_build(&ahri()),
            StatsState::Unavailable(offline.clone())
        );
        assert_eq!(stats.player_profile(), StatsState::Unavailable(offline));

        let disabled = StatsUnavailable::OnlineDisabled;
        assert_eq!(
            stats.build(&ahri()),
            StatsState::Unavailable(disabled.clone())
        );
        assert_eq!(
            stats.matchups(&ahri()),
            StatsState::Unavailable(disabled.clone())
        );
        assert_eq!(
            stats.tier_list(StatsQueue::RankedSolo, RankBracket::All),
            StatsState::Unavailable(disabled)
        );
        assert!(
            stats.inner.builds.slots.lock().unwrap().is_empty(),
            "no fetch was scheduled"
        );
        assert!(stats.inner.tiers.slots.lock().unwrap().is_empty());
    }

    #[test]
    fn memo_schedules_one_fetch_and_retries_after_cooldown() {
        let memo: Memo<u8, u32> = Memo::new();
        let ttl = Duration::from_secs(60);
        assert_eq!(
            memo.read(&1, ttl, Duration::ZERO, Lang::En),
            (StatsState::Loading, true)
        );
        assert_eq!(
            memo.read(&1, ttl, Duration::ZERO, Lang::En),
            (StatsState::Loading, false)
        );
        memo.finish(1, Err(Fail::Network));
        // Zero retry cool-down: the failure is retried right away, shown as loading.
        assert_eq!(
            memo.read(&1, ttl, Duration::ZERO, Lang::En),
            (StatsState::Loading, true)
        );
        memo.finish(1, Err(Fail::Network));
        let (state, start) = memo.read(&1, ttl, ttl, Lang::Fr);
        assert!(!start);
        assert_eq!(
            state,
            StatsState::Unavailable(StatsUnavailable::Failed(
                "Impossible de joindre MetaBot.GG. Vérifiez votre connexion.".into()
            ))
        );
        memo.finish(1, Ok(7));
        assert_eq!(
            memo.read(&1, ttl, ttl, Lang::En),
            (StatsState::Ready(7), false)
        );
        // Expired success: still shown while it refreshes.
        assert_eq!(
            memo.read(&1, Duration::ZERO, ttl, Lang::En),
            (StatsState::Ready(7), true)
        );
        assert_eq!(memo.ok(&1), Some(7));
    }

    #[test]
    fn failures_read_in_both_languages() {
        for fail in [
            Fail::ClientNoAnswer,
            Fail::ClientHttp(500),
            Fail::ClientBadFormat,
            Fail::Network,
            Fail::RateLimited,
            Fail::OnlineHttp(503),
            Fail::BadFormat,
        ] {
            let (StatsUnavailable::Failed(en), StatsUnavailable::Failed(fr)) =
                (fail.localize(Lang::En), fail.localize(Lang::Fr))
            else {
                panic!("{fail:?} must be a Failed message");
            };
            assert_ne!(en, fr, "{fail:?}");
        }
        assert_eq!(
            Fail::OnlineDisabled.localize(Lang::Fr),
            StatsUnavailable::OnlineDisabled
        );
    }
}
