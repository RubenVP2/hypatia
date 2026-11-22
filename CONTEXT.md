# Hypatia Domain Model

Hypatia is a local League of Legends companion, opt-in mobile draft controller, and telemetry observatory built atop Riot's local League Client (`LCU`), Live Client Data, Riot Client, and Replay APIs.

## Core Terms

### Match Projection
The single normalized snapshot of the current player state and match lifecycle (`Lobby`, `Matchmaking`, `ReadyCheck`, `ChampSelect`, `LiveGame`, `LiveBoard`, `EndOfGame`) produced once per poll/query from raw Riot payloads and localized for the active user language.
- **Invariant**: Both Desktop and Mobile Remote views derive from this single projection rather than independently parsing raw Riot JSON.
- **Invariant**: Ranked Solo/Duo teammate anonymization and phase countdown scaling (converting Riot millisecond timers into whole seconds) happen once inside projection and never leak raw unmasked teammates or unscaled millisecond values.
- **Invariant**: Missing fields resolve to neutral empty/unknown values; demo identities and champions belong only to explicit demo fixtures and state managers, never inside production extractors.

### Champion Catalog
The bidirectional lookup uniting Riot numeric champion IDs, localized display names, internal game aliases (such as `rawChampionName` suffixes), and Summoner's Rift role assignments behind one read-only catalog interface.
- **Invariant**: Names come from Game Text by ID in the Hypatia language shown; live client metadata supplies custom champion icons, and its names only for a champion newer than Game Text. Built-in entries merge in so locale differences never break ID resolution or Jungle/role filtering.
- **Invariant**: Placeholder values (`Selecting...`, `Sélection...`, `None`, `Unknown`) always resolve to champion ID `0`.

### Game Text
The names of champions, items, runes, rune trees, summoner spells and skins by Riot ID, one table per Hypatia language (`crates/shared/data/game_text/<lang>.tsv`), shipped in the binary and regenerated each patch with `cargo run -p hypatia-core --example update_game_text` (CommunityDragon, at development time only).
- **Invariant**: The League client's language never reaches the screen: every Riot name shown is looked up by ID in Game Text for the Hypatia language, whatever locale the client runs in (players pick Korean for the voices or the font). The client's own name only fills an ID Game Text does not know yet.
- **Invariant**: Every language table names the same IDs; a new Hypatia language is a new table plus its `Lang` arm.

### Position
Canonical Summoner's Rift role (`Top`, `Jungle`, `Mid`, `Bot`, `Support`, `Fill`) normalizing inconsistent Riot strings (`MIDDLE`/`MID`, `BOTTOM`/`BOT`, `UTILITY`/`SUPPORT`) across lobby role preferences, draft assignments, live client scoreboard slots, and lane filters.
- **Invariant**: All lane indices, Riot LCU codes, and localized display labels originate from this single canonical type.

### Live Board
The tactical Summoner's Rift summary derived during `LiveGame`, tracking lane-ordered champion death/respawn states, three turret tiers per lane for Order and Chaos, epic objectives (`Baron`, `Elder`, `Drake`, `Herald`, `Grubs`), and recent combat/objective events.

### Sighting
An enemy champion's last known place on the Live Board: the latest event that credits them (killer or assister) at a fixed spot (a tower, an inhibitor, the dragon or Baron pit), or a death under a tower. Shown with the game time it happened.
- **Invariant**: Live Client Data carries no coordinates, so a place comes only from such events; a champion kill alone places no one. Hypatia never reads game memory or the screen to find positions.
- **Invariant**: Sightings and pit timers exist only on Summoner's Rift (`CLASSIC`); the epic monster schedule lives in `rift_schedule` and follows the current patch.

### Champion Stats
The dpm.lol-style knowledge about champions and the player: Champion Build (runes, spells, items, skill order with rates), Tier List, Matchups, Team Comp profile, and My Stats (the player's own games and per-champion summary). Types live in `crates/shared/src/stats.rs`.
- **Invariant**: Local data (client game data, Riot recommended pages, local match history) is always available; Online data comes only through a `StatsSource` while `Settings::online_stats` is on, and switched off it performs no I/O.
- **Invariant**: Every value carries a `StatsOrigin` shown to the player; online requests never carry an identity (summoner name, PUUID, token).
- **Invariant**: My Stats totals and per-champion summaries count only Summoner's Rift games when the history has any (`summarised_games`), never remakes; ARAM (any Howling Abyss queue, ARAM: Mayhem included) and event modes are listed, not counted, and carry no lane.
- **Invariant**: Views read Champion Stats only through `ChampionStats` (`AppState::stats`): non-blocking getters returning a `StatsState`, the Change Signal bumped when a fetch lands.
- **Invariant**: The Mobile Remote's Stats tab reads the same getters through `GET /api/stats/{view}` (`remote_stats`), with the pairing token; it adds no `RemoteCommand`, and the phone cannot switch Online stats. The pushed snapshot's `stats_revision` tells it when a fetch landed.

### Draft Helper
The champ-select block of the Match Projection (`ChampSelectSummary::helper`, `crates/shared/src/draft_helper.rs`) for the player's hovered or locked champion: rune page, spells, core items, lane opponent with matchup win rate, both teams' Team Comp profile, and up to four comp tips. Shown on the desktop champ-select view, the mini widget, and the Mobile Remote's Guide tab.
- **Invariant**: Projection reads Champion Stats through the `HelperStats` seam, never fetching; each value keeps its `StatsOrigin` (Riot client first for runes and spells, then Online, then Local from the player's own history), and missing data leaves the field empty.
- **Invariant**: Team comps and the lane opponent use champion IDs only, so Ranked Solo/Duo allies stay as anonymized as the projection left them.
- **Invariant**: The Mobile Remote's apply button reuses the existing rune page command; the helper adds no Riot write.

### Attribution
The credit an online stats provider requires next to its data: its name and the page the data came from, carried by Online values and shown as a visible link. The provider is MetaBot.GG (`docs/research/online-stats-source.md`).

### Mobile Remote
The opt-in LAN companion serving an authenticated local web controller where a paired phone displays current match state and triggers a strictly closed allowlist of League Client actions (role preferences, queueing, ready check, champion hover/lock, spells, skins, rune pages, swaps/trades).
- **Invariant**: Arbitrary URIs, HTTP methods, and unvalidated payloads are rejected by the command allowlist; outside explicit opt-in Mobile Remote commands, Riot inspection remains strictly read-only.

### Telemetry Observatory
The inspection surface capturing live local endpoint traffic, discovered endpoint schemas, payload diffs across updates, and redacted credential indicators without interrupting match projection.
- **Invariant**: The event history, the complete WAMP stream, the endpoint catalog import and system footprint scans exist only while the observatory is open; closed, it retains and costs nothing.

### Change Signal
The single process-wide notification bumped whenever something a view could display has changed (a projected Riot payload, discovery, an icon landing in the cache, the language, a paired phone).
- **Invariant**: Desktop views and the Mobile Remote broadcaster wait on this signal instead of polling; an unchanged signal revision means nothing needs to be projected, serialized or painted.

### Update Check
Learning that a newer Hypatia release is published, and installing it on the player's click (`crates/core/src/update.rs`, desktop side `crates/desktop/src/updates.rs`).
- **Invariant**: Asked once on first launch (`Settings::update_check`: `Ask`, `On`, `Off`); anything but `On` performs no network I/O. Profiles keep the answer as the player set it.
- **Invariant**: The check is one `GET` to GitHub's `releases/latest` with a generic User-Agent; it carries nothing about the player, their account or their games. Drafts, pre-releases and versions not newer than the running build are never offered.
- **Invariant**: Nothing downloads before the player clicks Install. The archive installs only if it carries a minisign signature by `RELEASE_PUBLIC_KEY` whose trusted comment is `hypatia <tag> hypatia-windows-x86_64.zip`; `release.yml` signs it and refuses to publish without the key. The asset names are a contract with every installed copy.

### Settings
The persisted set of user switches, one per background activity (live game polling, game icons, Riot Client connection, Mobile Remote autostart, eco mode, Online stats, the Update Check, and the Telemetry-only scans).
- **Invariant**: A switched-off activity performs no I/O and no periodic work; switches apply immediately without restarting.
