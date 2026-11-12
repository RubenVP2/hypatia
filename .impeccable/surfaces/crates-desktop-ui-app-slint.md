---
version: 1
slug: "crates-desktop-ui-app-slint"
primary_target: "crates/desktop/ui/app.slint"
related_targets: ["crates/desktop/ui/parts.slint","crates/desktop/ui/theme.slint","crates/core/src/remote_ui.html"]
---

# Hypatia Cross-Surface Companion (Desktop + Mini Widget + Telemetry + Mobile Remote)

Mode: Operate.

Scope: Complete global UI/UX redesign across the Slint Desktop MainWindow, always-on-top MiniWidget, Telemetry Observatory, and Axum-served Mobile Remote companion (`crates/desktop/ui/app.slint`, `crates/desktop/ui/parts.slint`, `crates/desktop/ui/theme.slint`, `crates/core/src/remote_ui.html`).

Audience and job: Any League of Legends player on PC (plus technical explorers via Telemetry Lab) who needs immediate clarity on match phase, active turn, lane roster, objective/tower map, and one-handed remote controls without horizontal crowding, vertical clipping, or modal disruption.

Content: 5-phase match lifecycle, real-time Champion Select roster + lane map + enemy picks & visual bans, 10-player In-Game scoreboard + towers + objectives + feed, End of Game recap, Local Riot sources heartbeat (`LCU`, `RC`, `:2999`), QR Mobile Remote pairing sheet, Advanced Telemetry stream + JSON/Diff inspector + GET catalog, and Mobile Remote one-hand draft/lobby/runes/live monitor.

Constraints: Local & read-only by default (`RemoteCommand` closed allowlist on explicit opt-in Mobile Remote), strict Riot Solo/Duo anonymization, English product copy across interfaces, Night & Day printings with WCAG AA contrast, keyboard navigation, reduced motion support, zero third-party requests from UI.

## Direction contract

THESIS: A carbon-framed Pit-Wall Chronometer built as an operating companion first: a persistent left rail separates Match Companion, Telemetry Lab, Mobile Remote, Mini Widget, and Demo controls alongside local Riot source health, while the main timing sector locks rosters, KDA splits, and phase clocks onto fixed tabular columns where signal amber isolates only what demands attention right now.

OWN-WORLD: Carbon-asphalt rail shell `#0b0d11` (`#e8e5dc` Day), timing ground `#13161c` (`#f3f1ea` Day), sector cards `#1a1e26` (`#ffffff` Day), crisp 1px structural rules `#2b303c` (`#d4d0c5` Day). Warm ivory reversed action plate `#f4f1e8` (`#151821` Day), signal acid-amber `#f5b400` (`#b87300` Day) strictly reserved for active pick turns, expiring timers, incoming swap alerts, and Ready Check urgency. Semantic lane lines (`Top #e3b341`, `Jungle #38b87c`, `Mid #a970ff`, `Bot/Support #ee7aa8`) and strict Blue `#3b82f6` / Red `#ef4444` team polarity. Typography pairs `Overpass` UI text, `Barlow Condensed` tabular display timers/scores, and `Overpass Mono` in Telemetry payloads and status tags.

STORY: Opening Hypatia immediately confirms live Riot client connectivity in the left rail while showing the active phase across the top sector ticker. In draft or queue, a high-contrast timing plate names the exact action while the active row lights cleanly without shifting lines; in game, scrollable team ledgers, non-overlapping lane markers, and a zero-clipping 360px Mini Widget mirror the match seamlessly alongside a compact one-handed Mobile Remote with complete rune/shard editing and live game tracking.

FIRST VIEWPORT: Fixed 196px left rail (Hypatia brand icon & title, Companion / Telemetry Lab / Mobile Remote / Mini Widget / Demo switches, local LCU/RC/:2999 source indicators, Day/Night printing switcher). Top 52px phase ticker (`Lobby` → `Queue` → `Champ Select` → `In Game` → `Results` + live queue badge). Timing header with reversed ivory action plate (`Support picking · Lock in 24s`, no eyebrow above headline) and `88px` condensed countdown timer on the right. Scrollable split body: 5 fixed-height tabular roster rows (roundel, champion badge/icon, champion + summoner, standing, state tag) beside the non-overlapping Sector Map, visual Ban/Enemy pick chips, and inline non-pushing Mobile QR sheet. Synchronized 360px Mini Widget and Mobile Remote with collapsible alert strip, always-visible draft strip, full 9-slot custom rune + shard builder, and live match view.

FORM: Pit-Wall Chronometer, candidate 3 of 7 on the ordered list, assigned by the roll. seed:41b6257743c360b6. Raises: clear active/idle state pill indicators (Zoo & Gardens Guide Map), full-canvas phase ownership with sidebar utility rail (Vertical Feed), dramatic scale hierarchy between condensed display numerals and dense tabular metadata (Emigre Bitmap Specimen), fixed-baseline five-slot roster alignment without vertical jump (Botanical Folio), and true monospace tabular alignment + colored diffs in Telemetry Lab (Ikeda Datamatics).

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Unresolved

- Local champion portrait bitmaps from LCU `lol-game-data` assets can enrich `ChampPlate` when cached locally; clean team-colored initials remain the instant fallback.
