# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Native Windows desktop application targeting League of Legends on PC (portable Rust + Slint allows local development and `--demo` runs on macOS/Linux), no browser and no webview. Rust everywhere: the existing backend crates (lockfile discovery, Riot HTTP client, WAMP, Live Client Data on port 2999, redaction) feed a **Slint** UI. An embedded Axum server exists only to serve the opt-in Mobile Remote page on the LAN. The design language is Hypatia's own, not per-OS native conventions (Platform is recorded as `web` for that reason).

## Users

Any League of Legends player on PC, regardless of technical knowledge. They open Hypatia at two moments:
- **Champion select**: to understand their team at a glance (roles, picks, bans, timer, visible info about allies).
- **During the game**: on a second monitor or glanced at between fights, to follow the match state (score, objectives, key events, players).

A minority of curious or technical players want to see what the Riot client exposes locally; they are served by the **Observatory**, an opt-in advanced view.

## Product Purpose

Give every player a simple, immediately readable companion for champion select and the live game, built only from what the Riot clients already expose on their own machine. Success: a player who has never seen the app understands what it shows within seconds, and keeps it open during their games.

## Positioning

Unlike Porofessor, Blitz, OP.GG or U.GG, Hypatia is:
- **100 % local by default, no account**: nothing leaves the machine unless the player turns on Online stats or agrees to the update check, neither of which sends any identity; no sign-up, no ads, no telemetry.
- **Safe for the account**: read-only, no overlay injected into the game, no automation of any kind.
- **Light and discreet**: fast start, low CPU and memory, does not cost FPS.
- **Transparent**: shows exactly what the Riot client exposes, with the Observatory for those who want to look under the hood.

## Operating Context

- Launched alongside the Riot Client / League Client; Riot services start, stop, and change port and password on every launch, and Hypatia follows them without user action.
- Kept open during champion select and on a second monitor during the game, often in a dark room.
- Must behave gracefully when League is not running, when the player is idle, or between phases.

## Capabilities and Constraints

- Match phases tracked: Lobby, Matchmaking queue, Champion select, Live game (port 2999), End of game. Player-facing focus is Champion select and Live game.
- **Read-only by default, with an explicit opt-in Mobile Remote exception for accessibility**: the desktop observatory (`ReadOnlyRiotClient`) performs strictly `GET` requests and passive WebSocket subscriptions. When the user explicitly activates **Mobile Remote** (via the desktop UI toggle or `--remote`), a separate `LcuActionClient` and local LAN server paired via a 128-bit QR token allow one-handed mobile control of lobby role selection, queue start/stop, ready-check accept/decline, champion hover/lock, summoner spells, skins, rune pages, and role/pick-order/champion swaps. Every LCU write belongs to a closed enum allowlist (`RemoteCommand`) and requires an explicit user tap — zero background automation.
- **Local data first**: every piece of information comes from local Riot APIs by default. **Champion Stats** (builds, runes, items, skill order, tier list, matchups, team comp, My Stats) are built from the client's own game data, Riot's recommended rune pages and the player's local match history.
- **Online stats, opt-in only** (revisited 2026-11-18): global win rates, builds, tier list and matchups may come from one online provider, behind the `online_stats` setting, **off by default**. Switched off it performs no I/O. When on, requests carry only champion, role, queue, rank bracket, patch and the language names should come back in, never a summoner name, PUUID, token or anything from the lockfile. Every value is labelled with its origin (Local, Riot client, Online, Demo), and Online values link to the provider page they came from (MetaBot.GG, see `docs/research/online-stats-source.md`).
- **Update Check, asked once** (added 2026-11-22): on first launch the player answers whether Hypatia may ask GitHub's public releases API for a newer version (`Settings::update_check`, `Ask` until answered, changeable in Performance). Unanswered or off, it performs no I/O. On, it sends one `GET` at launch and every 12 h (10 min after a check that could not reach GitHub) with a generic User-Agent and nothing else. A newer release shows an "Update to vX.Y.Z" button; installing happens only on the player's click, and only for a zip carrying a minisign signature by the release key for that exact tag. The running `hypatia.exe` is swapped in place and restarted; settings are untouched.
- **Respect Riot anonymization**: in Ranked Solo/Duo champion select, ally identities are hidden by Riot; Hypatia must never try to reveal them.
- Secrets (lockfile passwords, tokens, JWTs, PUUIDs) masked by default; only revealable in the Observatory.
- **Observatory** (closed by default, opened from the left rail or `--telemetry`): live WAMP event stream, `GET` endpoint catalog and runner, JSON inspector and diff, Riot processes and ports.
- Interface language: English & French (live `EN | FR` switcher in Desktop & Mobile Remote, persisted locally with OS auto-detection and `--lang <en|fr>` CLI override).

## Evidence on Hand

- Demo fixtures in `crates/core/src/fixtures.rs` simulate all five match phases, WAMP streams and callable endpoints; usable for screenshots and design work without League running. Demo data must stay clearly labelled as demo. Demo Mode is a developer tool: the release build shows it only when launched with `--demo`; debug builds always offer it.
- Data model of what can be shown: `crates/shared/src/summary.rs` (summoner, lobby, matchmaking, champion select, live game players and events).
- No testimonials, user counts or press exist; do not fabricate any.

## Product Principles

1. **Understood in seconds**: the default view answers "what is happening in my game right now?" with no jargon.
2. **The moment decides the screen**: the app shows what matters for the current phase, without the player navigating.
3. **Never risky**: read-only, local, no overlay; trust is the product.
4. **Discreet companion**: small footprint, glanceable from a second monitor.
5. **Depth is opt-in**: technical detail exists but never intrudes on the basic view.

## Accessibility & Inclusion

WCAG AA contrast in both **Dark (Night)** and **Light (Day)** themes, full keyboard navigation on desktop, `prefers-reduced-motion` equivalent honored, text legible at a glance from a second monitor, and one-handed mobile ergonomics (44px+ touch targets, bottom thumb-zone navigation, audio/vibration/notification alerts for Match Found and draft turns).
