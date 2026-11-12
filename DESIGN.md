---
name: Hypatia — Pit-Wall Chronometer
description: Racing pit-wall telemetry aesthetic for League of Legends desktop companion, Telemetry Lab, and LAN mobile remote.
colors:
  shell: "#0b0d11"
  ground: "#0f1116"
  backdrop: "#090b0f"
  panel: "#161922"
  panel-raised: "#1f2430"
  rule: "#262b38"
  rule-strong: "#3c4356"
  text: "#edeff4"
  muted: "#9ca3b5"
  plate: "#edeff4"
  on-plate: "#0f1116"
  signal: "#f5b400"
  signal-text: "#ffd24d"
  live-green: "#45c47f"
  blue: "#4c8df6"
  red: "#f0605a"
  lane-top: "#e3b341"
  lane-jungle: "#45c47f"
  lane-mid: "#b07cf0"
  lane-bot: "#ef8bb5"
typography:
  specimen:
    fontFamily: "Barlow Condensed, Overpass, sans-serif"
    fontSize: "96px"
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
  xs: "4px"
  sm: "8px"
  md: "14px"
  lg: "20px"
components:
  button-primary:
    backgroundColor: "{colors.signal}"
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

# Design System: Hypatia — Pit-Wall Chronometer

## Overview

**Creative North Star: "Pit-Wall Chronometer"**

Hypatia frames every stage of a League of Legends match — idle standby, lobby lane setup, queue countdown, Champion Select, active :2999 game telemetry, and post-match summary — as a compact motorsport timing wall. Unhurried carbon panels establish a calm, low-glare backdrop on second monitors, reserving pure inverted ivory plates and crisp amber timing signals strictly for moments that demand immediate player attention.

Both the desktop native shell (`MainWindow` and `MiniWidget` in Slint) and the mobile LAN remote (`remote_ui.html`) speak one visual grammar built around three structural anchors: a persistent workspace rail separating mode navigation and Riot process health from phase telemetry, a single specimen-scale condensed timing clock or score, and a clean 1px grid with zero artificial drop shadows or decorative gradients.

**Key Characteristics:**
- Dark carbon timing wall (`Night`, default) paired with an unbleached drafting-paper variant (`Day`) meeting WCAG AA contrast.
- Condensed timing display figures (`Barlow Condensed`) contrasted against neutral UI labels (`Overpass`) and strict columnar telemetry streams (`Overpass Mono`).
- Semantic color discipline dividing Riot API health (`live-green`), immediate action (`signal`), team allegiance (`blue` / `red`), and map lane identity (`Top` / `Jungle` / `Mid` / `Bot`).
- Minimum `11px` functional typography floor across every label, badge, feed clock, and telemetry row.

## Colors

The Night printing pairs deep neutral carbon layers with warm pit-wall amber signals (`#f5b400`) and saturated Summoner's Rift team/lane inks; the Day printing adapts identically onto `#efece3` paper and `#faf8f2` plates (`#b87300` amber signal).

### Primary
- **Pit-Wall Amber** (`#f5b400` Night / `#b87300` Day): Reserved strictly for active navigation highlight borders/dots, active turn spotlights, demo mode indicators, primary CTAs, and live phase marker fill.
- **Amber Text** (`#ffd24d` Night / `#8a5600` Day): High-contrast amber text used on dark or soft-tinted plates where `#f5b400` alone would lose contrast at small body sizes.

### Secondary
- **Team Blue** (`#4c8df6` Night / `#2664d4` Day): Strictly scoped to Allied scoreboard headers, objective indicators, standing blue towers, and allied draft plates/bans.
- **Team Red** (`#f0605a` Night / `#c9342e` Day): Strictly scoped to Enemy scoreboard headers, objective indicators, standing red towers, and red team bans or respawn timers.
- **Live Telemetry Green** (`#45c47f` Night / `#1e7f48` Day): Riot source connection status (`LCU`, `Riot Client`, `Live :2999`), paired WebSocket dots, and Ready Check `ACCEPT` CTA.

### Neutral
- **Carbon Shell** (`#0b0d11` Night / `#e5e1d6` Day): Left `196px` workspace rail and mobile sticky topbar/bottom dock.
- **Primary Ground** (`#0f1116` Night / `#efece3` Day): Phase canvas background and roundel core fill.
- **Viewport Backdrop** (`#090b0f` Night / `#dfdacf` Day): Recessed code/JSON inspector well and text input wells.
- **Instrument Panel** (`#161922` Night / `#faf8f2` Day): Default card surface, telemetry stream container, and scoreboard rows.
- **Raised Surface** (`#1f2430` Night / `#ebe7dc` Day): Hover fills, source cards, and secondary chip backing.
- **Hairline Rule & Strong Rule** (`#262b38` / `#3c4356` Night; `#d4cfc2` / `#a59f8f` Day): Exact `1px` boundaries replacing shadows.
- **Primary Ink & Muted Ink** (`#edeff4` / `#9ca3b5` Night; `#14171f` / `#555a66` Day): Readable foreground ink and secondary copy.

### Named Rules
**The Signal Restraint Rule.** Pit-Wall Amber never appears simultaneously as passive decoration on non-interactive cards; it belongs only to active navigation state, the active phase node, or the player's current turn/CTA.

**The Lane-Team Separation Rule.** Lane colors (`#e3b341`, `#45c47f`, `#b07cf0`, `#ef8bb5`) draw only metro tracks and role roundels, while Team Blue and Team Red own champions, objectives, towers, and bans.

## Typography

**Display Font:** Barlow Condensed (Bold `700`, ExtraBold `800`)
**Body Font:** Overpass (Regular `400`, SemiBold `600`, ExtraBold `800`)
**Label/Mono Font:** Overpass Mono (Variable `400..700`)

**Character:** Compressed telemetry numerals and urgent plate headlines cut vertical height cleanly against calm humanist prose and rigid JSON/URI inspection columns.

### Hierarchy
- **Specimen** (`800`, `88px–96px` desktop / `56px` mobile, `-1px` tracking): Single large countdown or final KDA figure on active phase headers.
- **Display Score** (`800`, `56px` desktop / `46px` mobile / `30px` mini-widget): Live team kill comparisons in `ScoreSpecimen`.
- **Headline / Title** (`800`, `22px–26px`, uppercase condensed in panel headers): Attention plates, modal headers, and mobile section headings.
- **Body** (`600`, `14px`): Primary status copy, summoner/champion rows, and descriptions.
- **Label / Mono** (`700`, `11px`, tabular figures): Clock timestamps, HTTP endpoints, JSON payloads, badges, and source ports.

### Named Rules
**The 11px Floor Rule.** Every interactive label, timestamp, badge, and secondary cell stays at `11px` or larger (`11px` minimum) across desktop Slint and mobile HTML.

**The No-Eyebrow Rule.** Section cards and attention plates lead directly with their primary headline; supporting detail and metadata sit beside or below the heading, never stacked above as a miniature kicker.

## Layout

Desktop uses a split horizontal shell: a fixed `196px` left workspace rail housing Hypatia branding, mode switches (`Companion`, `Telemetry Lab`, `Mobile Remote`, `Mini Widget`), optional 6-phase demo keypad, live Riot source health (`LCU`, `Riot Client`, `Live Game :2999`), and `Night`/`Day` controls; beside a `52px` phase progression ticker and a scrollable (`Flickable`) content stage (`20px` gutter).

When Telemetry Lab opens in split mode (`horizontal-stretch` 3:2 split, `min-width: 380px` telemetry pane) or window width drops, Companion phase views detect `compact: width < 680px` and stack secondary panels cleanly beneath primary draft/match rosters without compressing labels. Mobile Remote flows in a single `560px` max-width centered column with sticky topbar (`52px`) and sticky thumb-zone bottom action dock.

## Elevation & Depth

Surfaces are **flat architectural planes** separated strictly by step tonal value (`shell` -> `ground` -> `panel` -> `raised`) and crisp `1px` perimeter hairlines (`rule` / `rule-strong`). Zero drop shadows or soft blurs appear on cards, buttons, inputs, or badges. Only the non-pushing Mobile Remote QR pairing overlay dims the background canvas with `#090b0faa` scrim so attention stays centered on the QR frame without reflowing underlying screens.

### Named Rules
**The Full-Perimeter 1px Rule.** Highlighted rows and alerts distinguish themselves through background tint (`signal-soft`, `danger-soft`, `green-soft`), leading icons/roundels, and `1px` full-perimeter borders — never thick (>1px) left or right accent stripes.

## Shapes

All interactive controls, inputs, status chips, and tabs share a crisp `4px` (`radius-sm`) architectural radius; cards, team strips, and champion plates share a `6px` (`radius`) corner radius; lane roundels, objective weight chips, and turret markers remain pure circles (`50%`) to mirror transit-style telemetry hubs cleanly. Icons use uniform `2px` round-capped SVG / Slint `Path` strokes rather than Unicode glyphs.

## Components

### Buttons
- **Primary Signal Action:** `1px` `signal` border and fill (`#f5b400` / `#b87300`) with dark contrast ink (`#0f1116`), `4px` radius (`30px` desktop, `48px` mobile, `58px` full-width `ACCEPT MATCH`).
- **Secondary Action:** `panel` fill, `1px` `rule-strong` border, transitions background cleanly (`110ms ease-out`) to `panel-raised` on hover, and renders an explicit `2px` `#f5b400` outer focus ring on keyboard focus.

### Navigation & Phase Ticker
- **Left Rail Item (`RailNavItem`):** `36px` height, `4px` radius, left `16×16` vector `Path` icon (`2px` stroke), active pill tinted with `signal-soft` and `1px` `signal` perimeter.
- **Phase Ticker (`PhaseLine`):** Five horizontal milestones (`LOBBY`, `QUEUE`, `CHAMP SELECT`, `IN GAME`, `RESULTS`), `2px` connecting rail transitioning to `signal` up to current phase.
- **Mobile 4-Tab Bar:** Equal-width grid inside `shell` border frame (`Pick`, `Skins`, `Loadout`, `Team`) with no ordinal `1 ·` / `2 ·` numbering.

### Cards & Telemetry Rows
- **Roster & Player Rows:** Single-row `1px` `panel` containers (`50px–70px` tall) pairing a circular lane `Roundel` (`Top`/`Jgl`/`Mid`/`Bot`/`Sup`), team-colored `ChampPlate`, name + status subtitle, and right-aligned tabular telemetry figures (`Overpass Mono`).
- **Line Map (`LineMap`):** Three lane paths plus optional dashed jungle corridor on a `400×400` grid; standing vs fallen tower nodes and non-overlapping staggered ally labels (`Bot` positioned above track, `Support` below track).
- **Telemetry JSON Inspector:** Scrollable recessed `backdrop` pane hosting copy-selectable `read-only` monospace text (`Overpass Mono`, `11px`) with JSON / Structural Diff / Previous views and secret reveal toggle.

## Do's and Don'ts

### Do:
- **Do** highlight active picks, incoming ready checks, and selected tabs cleanly using `signal-soft` fills plus `1px` full-perimeter borders.
- **Do** use `Overpass Mono` with tabular numerals for every countdown, HTTP path, byte size, port, and KDA readout.
- **Do** keep all functional labels, port indicators, and chip texts at `>= 11px` across both desktop and mobile targets.
- **Do** bundle and serve font files offline inside desktop assets and local Axum `/fonts/*` routes.

### Don't:
- **Don't** place small uppercase eyebrows above section headings or card titles.
- **Don't** use thick `> 1px` colored `border-left` / `border-right` stripes on alerts, cards, or list rows.
- **Don't** prefix navigation tabs or section headings with ordinal numbers (`1 ·`, `2 ·`, `01`) or use Unicode symbols (`↔`, `🔔`, `✓`) in place of `2px` vector icons.
- **Don't** introduce drop shadows, glow gradients, external CDNs, or third-party telemetry calls.
