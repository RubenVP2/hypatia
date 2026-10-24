---
name: Hypatia Design System
description: Visual esports broadcast control room styling for Riot Games local observatory
colors:
  bg: "#0B1220"
  surface: "#111A2C"
  raised: "#172238"
  line: "#24314B"
  text: "#E8ECF4"
  text-muted: "#8C98B0"
  text-faint: "#5F7091"
  src-riot: "#FF5A4F"
  src-lcu: "#3FA7FF"
  src-live: "#F2B33D"
  src-system: "#A98BFF"
  on-air: "#FF3B47"
  focus: "#FFD166"
  kind-create: "#3DD68C"
  kind-update: "#B9C3D6"
  kind-delete: "#FF7A6B"
typography:
  display:
    fontFamily: "'Barlow Condensed', system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 700
    lineHeight: "1.1"
    letterSpacing: "0.02em"
  ui:
    fontFamily: "'Barlow', system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 500
    lineHeight: "1.4"
    letterSpacing: "normal"
  data:
    fontFamily: "'JetBrains Mono', ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: "1.5"
    letterSpacing: "normal"
rounded:
  sm: "2px"
  md: "4px"
  chamfer: "8px"
spacing:
  gap-1: "4px"
  gap-2: "8px"
  gap-3: "12px"
  gap-4: "16px"
  gap-6: "24px"
components:
  top-bar:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    height: "48px"
  source-chip:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
  phase-card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.chamfer}"
    padding: "12px 16px"
  ticker:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-muted}"
    height: "32px"
---

# Hypatia Design System

## Overview

Hypatia is a local dashboard and observatory for Riot Games APIs (Riot Client, League Client, Live Client Data :2999, and system connections). The application runs in **Operate** mode: designed for tech-savvy players on a secondary screen in a dark room, it presents machine and game state in the style of an **esports broadcast overlay**.

The system explicitly rejects:
- The generic gray developer tool panel with a single blue accent.
- Hextech / League of Legends fan-art ornamentation (no golden watermarks, no champion art, no proprietary Riot Games logos).

The visual signature relies on:
1. **Chamfered phase cards** (`PhaseCard`) announcing major transitions (Lobby, Matchmaking, Champ Select, In Progress).
2. The **proportional timeline** (`PhaseTimeline`) reflecting the actual time spent in each phase.
3. The **scrolling on-air ticker** (`Ticker`) embodying continuous data flow under the `LIVE ●` label.
4. Strict assignment of **team colors** to each local source (Riot Client, League Client, Live Game, System).

---

## Colors

### Broadcast Control Room Neutral Ramp (Strict)
The atmosphere reflects an esports broadcast control room:

| Token | Hex Code | Primary Role |
|---|---|---|
| `--n-0` | `#070B14` | Deep black canvas backdrop |
| `--n-1` / `--bg` | `#0B1220` | Main application background canvas |
| `--n-2` / `--surface` | `#111A2C` | Panels, headers, cards, and inspector |
| `--n-3` / `--raised` | `#172238` | Raised containers, input fields, chips |
| `--n-4` / `--line` | `#24314B` | Dividers, borders, and structural rules |
| `--n-5` | `#3A4A6B` | Active borders and secondary hovers |
| `--n-6` / `--text-faint` | `#5F7091` | Subdued metadata, path separators |
| `--n-7` / `--text-muted` | `#8C98B0` | Secondary labels, timestamps, units |
| `--n-8` | `#B9C3D6` | JSON keys, standard UI text |
| `--n-9` / `--text` | `#E8ECF4` | Headings, active data, high-contrast text |

### Team Colors (Source Identity)
Each local source has a unique brand hue:

- **Riot Client**: `--src-riot: #FF5A4F` (Crimson)
- **League Client (LCU)**: `--src-lcu: #3FA7FF` (Esports Blue)
- **Live Game (:2999)**: `--src-live: #F2B33D` (Competitive Gold)
- **System (Processes & Ports)**: `--src-system: #A98BFF` (Surveillance Violet)

### Semantic & Unique Roles
- **On-Air Dot**: `--on-air: #FF3B47` (reserved exclusively for the ticker's LIVE indicator).
- **Keyboard Focus**: `--focus: #FFD166` (reserved for visible keyboard accessibility outlines).
- **Event Kinds**:
  - Create: `--kind-create: #3DD68C` (Mint green)
  - Update: `--kind-update: #B9C3D6` (Light neutral)
  - Delete: `--kind-delete: #FF7A6B` (Coral)
  - Active Poll: `--src-live: #F2B33D`
  - Direct Call: `--src-system: #A98BFF`

---

## Typography

The typographic system combines three distinct font families without redundancy:

1. **Headings and Impact Figures (`Barlow Condensed`)**:
   - Used for the `HYPATIA` logo, uppercase phase names in `PhaseCard` (28px bold), sparkline throughput rates, and status badges.
   - Weights: 600, 700.
2. **User Interface (`Barlow`)**:
   - Used for navigation, action buttons, tabs, table labels, and help text.
   - Weights: 400, 500, 600.
3. **Data and Code (`JetBrains Mono`)**:
   - Used for API URIs, JSON Pointers, numeric values, and inspector payloads.
   - Absolute rule: `font-variant-numeric: tabular-nums` on all numbers, timestamps, and stopwatches to prevent jitter during high-frequency refreshes.

---

## Layout

The reference viewport is **1440×900**, optimized for secondary 1080p and 1440p displays:

- **TopBar (48px)**: Brand, status indicators for all 4 sources, 60s sparkline, secret masking toggle, quick search `Ctrl+K`, and view tabs.
- **SourceRail (280px)**: Left-hand hierarchical tree for filtering by namespace or endpoint, featuring instantaneous activity indicators.
- **Fluid Center**:
  - *Live View*: Signature phase card, phase timeline, and virtualized event table (28px per row).
  - Alternative Views: *Endpoints* (exploratory catalog), *System* (socket and process monitoring), *About* (security guarantees).
- **Side Inspector (440px)**: Right-hand detail inspection drawer with `JsonTree` expandable tree, `JsonDiff` key-by-key comparator, and raw payload display.
- **Bottom Ticker (32px)**: Persistent marquee at the bottom of the screen displaying the last 30 received events.

---

## Elevation & Depth

Operate mode prioritizes clarity and analysis speed over decorative drop shadows:
- Planar separation via **crisp 1px rules** (`--line: #24314B`) rather than heavy drop shadows.
- Soft offset shadows with progressive blur reserved exclusively for the `CommandPalette` modal dialog (`box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6)`).
- Background blur (`backdrop-filter: blur(8px)`) reserved for the `Ctrl+K` modal overlay.

---

## Shapes

- **Signature Chamfer (8px chamfer)**: Implemented via `clip-path: polygon(...)`. Strictly reserved for phase cards (`.phase-card`) and source badges (`.chamfer-badge`).
- **Standard Rounded Corners**: `--radius: 4px` for general containers and controls; `--radius-sm: 2px` for keyboard keys (`<kbd>`).
- **Rejection of the 'side-tab' anti-pattern**: No thick lateral borders (no `border-left: 4px/6px`) on cards or alerts. Accentuation is achieved via chamfered edge borders, colored status dots, and soft tints using `color-mix`.

---

## Components

### 1. PhaseCard
- Chamfer cut on top-right corner.
- Displays the phase name in uppercase `Barlow Condensed` 28px.
- Integrates a live stopwatch of time spent in the current phase and subtly indicates the previous phase.

### 2. PhaseTimeline
- Segmented horizontal bar where each block has a width **strictly proportional to the actual duration** spent in that phase.
- Informative tooltip displaying start time and total elapsed duration in seconds/minutes.

### 3. EventTable (Virtualized Table)
- Constant row height of 28px for smooth virtualization.
- Fewer than 60 DOM elements rendered concurrently on screen, effortlessly handling up to 300 events per second.
- `Space` shortcut to pause/freeze the feed and accumulate new incoming events into a `+N new` badge counter.

### 4. JsonTree & JsonDiff
- `JsonTree`: Lazy expandable tree view with semantic syntax highlighting and one-click JSON Pointer copying on keys.
- `JsonDiff`: Key-by-key alignment comparing current payload to the previous version to highlight additions, deletions, and modifications.

### 5. SecretValue
- Masks any password, token, or signature string as `🔒 •••• (N bytes)`.
- Individual on-demand timed reveal with dynamic **30-second** countdown before automatic re-masking.

### 6. CommandPalette
- Global shortcut trigger via `Ctrl+K` or `Cmd+K`.
- Predictive fuzzy search across all views, commands, and discovered endpoints.

---

## Do's and Don'ts

### Do
- Maintain tabular figures (`tabular-nums`) for all data displays, timestamps, and timers.
- Use team colors (`--src-riot`, `--src-lcu`, etc.) exclusively to identify packet origin.
- Reserve the bright red `--on-air` (`#FF3B47`) strictly for the ticker's live transmission signal.
- Ensure complete keyboard navigation without requiring mouse interaction (`1-4`, `Space`, `Ctrl+K`, `/`, Arrow keys).
- Scrupulously respect the user's `prefers-reduced-motion` preference (replacing slide transitions with subtle cross-fades).

### Don'ts
- Never add thick lateral borders like 'side-tabs' (`border-left: 4px+`) on containers or list items.
- Never include champion images, icons, or proprietary Riot Games logos.
- Never use text gradients: visual hierarchy must be established through type scale and font weight.
- Never reveal secrets in plaintext without deliberate, timed user interaction.
- Never execute mutating requests (`POST`, `PUT`, `DELETE`) against local Riot Games APIs: the observatory remains strictly read-only.
