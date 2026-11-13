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
- **Invariant**: Live client metadata synced from the running Riot client takes precedence for localized names and custom champion icons while merging with built-in offline entries so locale differences never break ID resolution or Jungle/role filtering.
- **Invariant**: Placeholder values (`Selecting...`, `Sélection...`, `None`, `Unknown`) always resolve to champion ID `0`.

### Position
Canonical Summoner's Rift role (`Top`, `Jungle`, `Mid`, `Bot`, `Support`, `Fill`) normalizing inconsistent Riot strings (`MIDDLE`/`MID`, `BOTTOM`/`BOT`, `UTILITY`/`SUPPORT`) across lobby role preferences, draft assignments, live client scoreboard slots, and lane filters.
- **Invariant**: All lane indices, Riot LCU codes, and localized display labels originate from this single canonical type.

### Live Board
The tactical Summoner's Rift summary derived during `LiveGame`, tracking lane-ordered champion death/respawn states, three turret tiers per lane for Order and Chaos, epic objectives (`Baron`, `Elder`, `Drake`, `Herald`, `Grubs`), and recent combat/objective events.

### Mobile Remote
The opt-in LAN companion serving an authenticated local web controller where a paired phone displays current match state and triggers a strictly closed allowlist of League Client actions (role preferences, queueing, ready check, champion hover/lock, spells, skins, rune pages, swaps/trades).
- **Invariant**: Arbitrary URIs, HTTP methods, and unvalidated payloads are rejected by the command allowlist; outside explicit opt-in Mobile Remote commands, Riot inspection remains strictly read-only.

### Telemetry Observatory
The inspection surface capturing live local endpoint traffic, discovered endpoint schemas, payload diffs across updates, and redacted credential indicators without interrupting match projection.
- **Invariant**: The event history, the complete WAMP stream, the endpoint catalog import and system footprint scans exist only while the observatory is open; closed, it retains and costs nothing.

### Change Signal
The single process-wide notification bumped whenever something a view could display has changed (a projected Riot payload, discovery, an icon landing in the cache, the language, a paired phone).
- **Invariant**: Desktop views and the Mobile Remote broadcaster wait on this signal instead of polling; an unchanged signal revision means nothing needs to be projected, serialized or painted.

### Settings
The persisted set of user switches, one per background activity (live game polling, game icons, Riot Client connection, Mobile Remote autostart, eco mode, and the Telemetry-only scans).
- **Invariant**: A switched-off activity performs no I/O and no periodic work; switches apply immediately without restarting.
