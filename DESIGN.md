---
name: Hypatia — Observatory Chronometer
description: A calm, read-only observation instrument for League of Legends: desktop companion, mini widget, Observatory (advanced view), and LAN mobile remote.
colors:
  shell: "#070b12"
  ground: "#0b111b"
  backdrop: "#05080d"
  panel: "#111926"
  panel-raised: "#182334"
  rule: "#223047"
  rule-strong: "#53462c"
  text: "#eef0f5"
  muted: "#9aa7bd"
  plate: "#edeff4"
  on-plate: "#0f1116"
  signal: "#f5b400"
  signal-text: "#ffd24d"
  live-green: "#45c47f"
  blue: "#3b9eff"
  red: "#f0605a"
  gold: "#6e5730"
  gold-bright: "#c8aa6e"
  accent: "#0ac8b9"
  lane-top: "#e3b341"
  lane-jungle: "#45c47f"
  lane-mid: "#b07cf0"
  lane-bot: "#ef8bb5"
typography:
  specimen:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "88px"
    fontWeight: 800
    lineHeight: 0.92
    letterSpacing: "-0.02em"
  display:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "58px"
    fontWeight: 800
    lineHeight: 0.95
    letterSpacing: "-0.02em"
  score:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "56px"
    fontWeight: 800
    lineHeight: 1.0
  score-mobile:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "46px"
    fontWeight: 800
    lineHeight: 1.0
  timer:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "32px"
    fontWeight: 800
    lineHeight: 1.0
  headline:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "26px"
    fontWeight: 800
    lineHeight: 1.15
    letterSpacing: "0.01em"
  title:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "22px"
    fontWeight: 800
    lineHeight: 1.2
    letterSpacing: "0.02em"
  name:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "18px"
    fontWeight: 800
    lineHeight: 1.2
  body:
    fontFamily: "Overpass, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.4
  small:
    fontFamily: "Overpass, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.35
  label:
    fontFamily: "Overpass Mono, monospace"
    fontSize: "11px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "0.06em"
rounded:
  sm: "4px"
  md: "6px"
  round: "9999px"
spacing:
  xxs: "2px"
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
motion:
  fast: "110ms ease-out"
  base: "180ms ease-out"
  slow: "240ms ease-out"
  pulse: "650ms ease-out x2"
components:
  button-primary:
    backgroundColor: "{colors.plate}"
    textColor: "{colors.on-plate}"
    rounded: "{rounded.sm}"
    height: "30px"
    padding: "0 14px"
  button-secondary:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    height: "30px"
    padding: "0 12px"
  card-panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: "16px"
  nav-rail-item:
    backgroundColor: "{colors.shell}"
    textColor: "{colors.muted}"
    rounded: "{rounded.sm}"
    height: "36px"
    padding: "0 10px"
  status-chip:
    backgroundColor: "{colors.panel-raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    height: "24px"
    padding: "0 8px"
---

# Design System: Hypatia — Observatory Chronometer

## Overview

**Creative North Star: "Observatory Chronometer"**

Hypatia is named after the Alexandrian astronomer: an instrument that observes and never touches. It frames every stage of a League of Legends match — idle standby, lobby, queue, ready check, Champion Select, the live game, and the post-match summary — as a quiet observation instrument with one running clock. Unhurried carbon panels keep a calm, low-glare backdrop on a second monitor; amber appears only when the player must act, and it gets louder as time runs out.

The product name always travels with its descriptor: **Hypatia · LoL companion** (FR: *compagnon LoL*). The advanced view is the **Observatory** (FR: *Observatoire*); player-facing copy avoids motorsport and "telemetry" vocabulary.

Both the desktop native shell (`MainWindow` and `MiniWidget` in Slint) and the mobile LAN remote (`remote_ui.html`) speak one visual grammar built around three structural anchors: a persistent workspace rail separating mode navigation and Riot process health from phase telemetry, a single specimen-scale condensed timing clock or score, and a clean 1px grid with zero artificial drop shadows or decorative gradients.

**Key Characteristics:**
- Dark carbon timing wall (`Night`, default) paired with an unbleached drafting-paper variant (`Day`) meeting WCAG AA contrast.
- Condensed timing display figures (`Barlow Condensed`) contrasted against neutral UI labels (`Overpass`) and strict columnar telemetry streams (`Overpass Mono`).
- Semantic color discipline dividing Riot API health (`live-green`), immediate action (`signal`), team allegiance (`blue` / `red`), and map lane identity (`Top` / `Jungle` / `Mid` / `Bot`).
- Minimum `11px` functional typography floor across every label, badge, feed clock, and telemetry row.

## Colors

The Night printing pairs deep neutral carbon layers with warm pit-wall amber signals (`#f5b400`) and saturated Summoner's Rift team/lane inks; the Day printing adapts identically onto `#efece3` paper and `#faf8f2` plates (`#b87300` amber signal).

### Primary
- **Signal Amber** (`#f5b400` Night / `#b87300` Day): Urgency only — the player's own pick/ban turn, a ready check, the lock-in button during that turn, a countdown at 10 s or less, an incoming swap, and the current phase node in the ticker.
- **Amber Text** (`#ffd24d` Night / `#8a5600` Day): High-contrast amber text used on dark or soft-tinted plates where `#f5b400` alone would lose contrast at small body sizes.

### Secondary
- **Team Blue** (`#3b9eff` Night / `#1f62c9` Day): Strictly scoped to Allied scoreboard headers, objective indicators, standing blue towers, and allied draft plates/bans.
- **Team Red** (`#f0605a` Night / `#c9342e` Day): Strictly scoped to Enemy scoreboard headers, objective indicators, standing red towers, and red team bans or respawn timers.
- **Live Telemetry Green** (`#45c47f` Night / `#1e7f48` Day): Riot source connection status (`LCU`, `Riot Client`, `Live :2999`), paired WebSocket dots, and Ready Check `ACCEPT` CTA.

### Neutral
- **Carbon Shell** (`#070b12` Night / `#e4e1d5` Day): Left `216px` workspace rail and mobile sticky topbar/bottom dock.
- **Primary Ground** (`#0b111b` Night / `#ede9dd` Day): Phase canvas background and roundel core fill.
- **Viewport Backdrop** (`#05080d` Night / `#d9d5c8` Day): Recessed code/JSON inspector well and text input wells.
- **Instrument Panel** (`#111926` Night / `#f9f7ef` Day): Default card surface, telemetry stream container, and scoreboard rows.
- **Raised Surface** (`#182334` Night / `#e6e1d3` Day): Hover fills, source cards, and secondary chip backing.
- **Hairline Rule & Strong Rule** (`#223047` / `#53462c` Night; `#c9c1ad` / `#99855a` Day): Exact `1px` boundaries replacing shadows.
- **Primary Ink & Muted Ink** (`#eef0f5` / `#9aa7bd` Night; `#14171f` / `#525968` Day): Readable foreground ink and secondary copy.

### Hextech Materials
One direction, Hextech: Hypatia's instrument grammar dressed in League's own materials. There is no style switcher.
- **Gold & Bright Gold** (`#6e5730` / `#c8aa6e` Night; `#8a6d38` / `#7a5c24` Day): metallic frames — map border, allied portrait rims, the player's own portrait and map label, card top bevel.
- **Hextech Cyan** (`#0ac8b9` Night / `#087c73` Day): calm accent tint behind headers when no artwork is cached.
- Gold is a material, never a signal: it does not replace amber for "act now".

### Client Artwork
Imagery comes only from the player's own League client, fetched read-only over the LCU and never bundled or redistributed (`hypatia_core::art`, asset category `art`):
- **Painted Summoner's Rift** (`map-south` / `map-north`, beam on the player's base) behind the `LineMap` markers; the vector rift remains the fallback.
- **Rift valley backdrop** (`gameflow-background`), pre-blurred, behind the pre-draft phase headers.
- **Ready-check medallion**: the client's round valley disc takes the centre of the ready check (`ReadyMedallion`), clipped to its circle, inside a countdown ring that loses one notch per second. The ring is amber, then red under 5 s; Hextech cyan with a check once accepted; red with a cross if declined. The medallion stays a dark object in Day as well.
- **Victory and defeat crests** take the centre of Results (`ResultCrest`) over the player's own skin splash, above the outcome in `58px` display type: gold for a victory; cold grey for a defeat, with a cold veil over the splash, since red stays the enemy's color. Below come the player's figures as tiles (KDA ratio, CS per minute, gold, damage, vision) and both teams' rows with their skin banners, read from the client's post-game block.
- **Champion splash art** behind phase headers once a champion is known. In Champion Select it runs full-bleed behind the header (the player's own skin, veiled on the left, fading into the page), and the headline plate turns translucent (`glass`) instead of carrying its own copy.
- **Draft banners** (asset category `banner`): a 480x120 strip cut from each ally's chosen-skin splash fills the right of their draft row and melts into it from the left. A hovered champion shows at 40%, a locked pick at full strength. In game the same strip, from each player's `skinID`, fills the right of all ten scoreboard rows and goes out (10%) while that player is dead.
- **In game**, the player's own skin splash runs full-bleed behind the live HUD, as in Champion Select; a dark red veil falls over it while the player is dead.
Each piece is cropped, downscaled to about twice its largest on-screen size, and blurred if needed **once in Rust** before it reaches the disk cache; the UI never scales a full-size texture or blurs at render time. The *Client artwork* switch (with *Game icons*) turns all of it off.

### Named Rules
**The Signal Restraint Rule.** Amber means "act now". It never marks a selected toggle, a filter, an active nav item, the player's own row, a victory, or a demo badge. Selection is quiet: `panel-raised` fill with a 1px `text` border; a panel's single main action is an ivory `plate` fill. The phase plate is a quiet `panel` by default and turns amber only for the player's turn or a ready check.

**The Urgency Ramp Rule.** A countdown is `text` above 10 s, `signal-text` from 10 s, and `danger` at 5 s or less, with a 3px bar beneath it that steps down once per second. Counting-up clocks (queue time, game time) never escalate.

**The Lane-Team Separation Rule.** Lane colors (`#e3b341`, `#45c47f`, `#b07cf0`, `#ef8bb5`) draw only metro tracks and role roundels, while Team Blue and Team Red own champions, objectives, towers, and bans.

## Typography

**Display Font:** Barlow Condensed (Bold `700`, ExtraBold `800`)
**Body Font:** Overpass (Regular `400`, SemiBold `600`, ExtraBold `800`)
**Label/Mono Font:** Overpass Mono (Variable `400..700`)

**Character:** Compressed telemetry numerals and urgent plate headlines cut vertical height cleanly against calm humanist prose and rigid JSON/URI inspection columns.

### Hierarchy
- **Specimen** (`800`, `88px` desktop / `64px` compact and mini widget / `56px` mobile, `-1px` tracking): The one running clock, or the final KDA, on each phase header.
- **Display Score** (`800`, `28px` in the desktop live HUD / `46px` mobile / `32px` mini widget): team kills. On desktop they sit in the `LiveHud` score band (blue plate, gold-ruled clock plate, red plate), like the in-game scoreboard; the respawn countdown that replaces the player's line while dead is `40px` `danger`.
- **Timer** (`800`, `32px`): Secondary clocks such as the mini widget game time.
- **Headline / Title** (`800`, `22px–26px`, uppercase condensed in panel headers): Attention plates, modal headers, and mobile section headings.
- **Body** (`600`, `14px`): Primary status copy, summoner/champion rows, and descriptions.
- **Label / Mono** (`700`, `11px`, tabular figures): Clock timestamps, HTTP endpoints, JSON payloads, badges, and source ports.

### Named Rules
**The 11px Floor Rule.** Every interactive label, timestamp, badge, and secondary cell stays at `11px` or larger (`11px` minimum) across desktop Slint and mobile HTML.

**The No-Eyebrow Rule.** Section cards and attention plates lead directly with their primary headline; supporting detail and metadata sit beside or below the heading, never stacked above as a miniature kicker.

## Layout

Desktop uses a split horizontal shell: a fixed `216px` left workspace rail (scrollable when the window is short) housing Hypatia branding with its descriptor, mode switches (`Companion`, `Observatory`, `Mobile Remote`, `Mini Widget`, `Performance`), optional demo keypad, live Riot source health (`LCU`, `Riot Client`, `Live Game :2999`), and EN/FR and Night/Day controls; beside a `52px` phase progression ticker and a scrollable (`Flickable`) content stage (`20px` gutter). Every gap and padding comes from the spacing scale (`Space.xxs` 2 to `Space.xl` 20).

The **mini widget** (360px, always on top) follows the phase: a quiet plate with the queue or ready-check clock between games; in Champion Select the turn plate, a 64px countdown and the five allied slots; in game the score with a running clock, the player's status, a ten-portrait strip with dead players faded and their respawn seconds, a 140px tower map beside the objectives, and recent events.

When the Observatory opens in split mode (`horizontal-stretch` 3:2 split, `min-width: 380px` telemetry pane) or window width drops, Companion phase views detect `compact: width < 680px` and stack secondary panels cleanly beneath primary draft/match rosters without compressing labels. Mobile Remote flows in a single `560px` max-width centered column with a sticky topbar and a fixed thumb-zone bottom dock. The dock carries the phase's tabs and its one main action (Accept with a separate Decline below, Lock/Ban during your turn). Every tap target is at least `44px` (demo chips excepted); re-rendering on a state push keeps the scroll position.

## Elevation & Depth

Surfaces are **flat architectural planes** separated strictly by step tonal value (`shell` -> `ground` -> `panel` -> `raised`) and crisp `1px` perimeter hairlines (`rule` / `rule-strong`). Zero drop shadows or soft blurs appear on cards, buttons, inputs, or badges. Only the pairing and Performance sheets dim the canvas, with the themed `scrim` token (Night `#090b0fb3`, Day `#14171f73`); both close on backdrop click or Esc.

### Named Rules
**The Full-Perimeter 1px Rule.** Highlighted rows and alerts distinguish themselves through background tint (`signal-soft`, `danger-soft`, `green-soft`), leading icons/roundels, and `1px` full-perimeter borders — never thick (>1px) left or right accent stripes.

## Shapes

All interactive controls, inputs, status chips, and tabs share a crisp `4px` (`radius-sm`) architectural radius; cards, team strips, and champion plates share a `6px` (`radius`) corner radius; lane roundels, objective weight chips, and turret markers remain pure circles (`50%`) to mirror transit-style telemetry hubs cleanly. Icons use uniform `2px` round-capped SVG / Slint `Path` strokes rather than Unicode glyphs.

## Components

### Buttons
- **Primary Action:** ivory `plate` fill with `on-plate` ink, `4px` radius, one per panel. On mobile the lock/ban button during your turn is the one amber button (`on-signal` ink, `48px`), and `ACCEPT MATCH` is `live-green` (`58px`).
- **Toggle (selected):** `panel-raised` fill, `1px` `text` border, label weight 800.
- **Secondary Action:** `panel` fill, `1px` `rule` border, fading (`110ms ease-out`) to `panel-raised` on hover, with a `2px` `focus` outer ring on keyboard focus.

### Navigation & Phase Ticker
- **Left Rail Item (`RailNavItem`):** `36px` height, `4px` radius, left `16×16` vector `Path` icon (`2px` stroke); active state is a `panel-raised` fill, `rule-strong` border and `text`-colored icon.
- **Phase Ticker (`PhaseLine`):** Five milestones (`LOBBY`, `QUEUE`, `CHAMPIONS`, `LIVE`, `RESULTS`; FR `SALON`, `FILE`, `CHAMPIONS`, `EN JEU`, `RÉSULTATS`), a `2px` rail filling to `signal` up to the current phase over `240ms`.
- **Mobile Tab Bar:** Equal-width grid in the bottom dock (`Pick`, `Skins`, `Loadout`, `Team` in draft; `Allies`, `Enemies` in game); the active tab is an ivory plate.

### Cards & Telemetry Rows
- **Roster & Player Rows:** Single-row `1px` `panel` containers (`50px–70px` tall) pairing a circular lane `Roundel` (`Top`/`Jgl`/`Mid`/`Bot`/`Sup`), team-colored `ChampPlate`, name + status subtitle, and right-aligned tabular telemetry figures (`Overpass Mono`).
- **Line Map (`LineMap`):** Three lane paths plus optional dashed jungle corridor on a `400×400` grid; standing vs fallen tower nodes and non-overlapping staggered ally labels (`Bot` positioned above track, `Support` below track).
- **Telemetry JSON Inspector:** Scrollable recessed `backdrop` pane hosting copy-selectable `read-only` monospace text (`Overpass Mono`, `11px`) with JSON / Structural Diff / Previous views and secret reveal toggle.

## Motion

Motion marks a change of state that matters to the player; nothing loops and nothing decorates.

**The Idle Budget Rule.** At rest the window draws nothing new: no animation may run while nothing changes. Transitions are triggered by an event, last under `400ms`, and only touch opacity, position, or scale (never layout). A loop is allowed only while the player must act (their turn, a ready check) and stops under reduced motion or eco mode. Every visual change is measured with `bench/measure.ps1` (idle, champ select, live game) before it ships.

- **Ready check arrival:** the medallion scales in from 86% (`240ms`) and an amber halo fades out twice (`650ms` x2), then holds still; the answer swaps the ring color and fades in the check or cross (`240ms`).
- **Turn start:** the phase plate fades to amber (`180ms`) and a 2px ring around it fades out twice (`650ms` x2), then holds still. On mobile the spotlight card's outline does the same.
- **Countdown:** the urgency ramp colors fade over `240ms`; the drain bar steps down once per second with no tween: under Slint 1.18 any `animate` on it kept the window repainting at ~36 fps for the whole draft, and a few-pixel step does not need one. Clocks are extrapolated locally between Riot events (desktop: a 250ms Slint timer; mobile: a 250ms interval), so they never freeze.
- **Rows and plates:** the acting row's tint fades in and out (`180ms`); a player who dies fades to 45% (`240ms`).
- **Draft moments** (`dur-reveal`, `360ms`), each played once when its element is created:
  - **Lock-in:** a gold glint crosses the ally's row and the banner rises to full strength.
  - **Ban:** the chip fades in and a red strike draws across the dimmed portrait.
  - **Enemy reveal:** the portrait scales in from 80% and a red ring fades out.

  The draft row models live for the whole session and are updated in place (`DraftModels` in `companion.rs`), so a new model never rebuilds the rows and replays them. Between moments the window stays at the idle frame rate.
- **Results arrival:** the crest settles from 115% with a fade (`360ms`); a victory adds one golden glow that fades out after it (`360ms`, `110ms` delay); the outcome title fades in just after (`240ms`). Then nothing moves.
- **Live moments**, each played once on the event, never on a drop (`LiveModels` keep the rows in place):
  - **Kill:** the scoring team's figure swells to 130% and its plate lights in the team color (`110ms` up, `240ms` back).
  - **Objective taken:** the chip fades in (`240ms`) and a ring flares from its pit on the map and fades (`Ping`, `360ms`).
  - **Tower destroyed:** a ring in the tower's team color flares from it and fades (`Ping`, `360ms`).
  - **Death:** the red veil fades over the splash (`240ms`) and the respawn countdown steps down once per second, untweened.
- **Phase ticker:** connector and node fill over `240ms`. Whole screens never crossfade.
- **Reduced motion:** `Theme.motion` is false when Windows "Show animations" is off, when *Reduce motion* is on in the Performance sheet, or with `--reduce-motion`; every duration then becomes 0. The mobile page honors `prefers-reduced-motion`.

## Do's and Don'ts

### Do:
- **Do** highlight the acting pick and incoming ready checks with `signal-soft` fills plus `1px` full-perimeter borders; highlight selected tabs and toggles quietly.
- **Do** use `Overpass Mono` with tabular numerals for every countdown, HTTP path, byte size, port, and KDA readout.
- **Do** bundle and serve font files offline inside desktop assets and local Axum `/fonts/*` routes.

### Don't:
- **Don't** prefix navigation tabs or section headings with ordinal numbers (`1 ·`, `2 ·`, `01`) or use Unicode symbols (`↔`, `🔔`, `✓`) in place of `2px` vector icons.
- **Don't** introduce drop shadows, glow gradients, external CDNs, or third-party telemetry calls.
