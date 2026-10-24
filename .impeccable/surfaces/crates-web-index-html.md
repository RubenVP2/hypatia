---
version: 1
slug: "crates-web-index-html"
primary_target: "crates/web/index.html"
related_targets: ["crates/web/src"]
---

# Surface brief — Main Dashboard (crates/web)

Scope: The entire web application (a single tool surface), in Operate mode.
Audience: The PC owner, a tech-savvy Riot gamer, on a secondary screen in a dark room, while playing or during exploratory sessions.
Tasks: (1) See what is running and what is exposed, (2) Follow the real-time event stream, (3) Explore an endpoint and its data.
Constraints: Read-only, secrets masked by default, minimal CPU cost, zero Riot brand assets.

## Direction contract

THESIS: Local Riot surfaces presented as an observed broadcast. Client state changes arrive like broadcast title cards, traffic scrolls like a stream ticker. The direction rejects the generic gray DevTools panel with a single blue accent, and also rejects LoL fan-art: no golden Hextech filigree, no champion art, no Riot logo.

OWN-WORLD: Broadcast control room midnight blue background (#0B1220), banner surfaces (#111A2C, #172238), rules #24314B. Each source has its team color applied as a chamfered left border: Riot Client crimson #FF5A4F, League Client blue #3FA7FF, Live Game gold #F2B33D, System violet #A98BFF. A red "LIVE" indicator dot #FF3B47. Typography: Barlow Condensed for headings and numbers, Barlow for UI, JetBrains Mono for data, with tabular figures. 8px chamfered angle reserved for phase cards and source badges.

STORY: Upon opening, the user sees which sources are on air and on which port. They then see the client's current phase, followed by scrolling traffic. They click on an event or an endpoint and read its exact data, including what changed. They never doubt that the tool is strictly read-only.

FIRST VIEWPORT: 1440×900. A 48px top bar: name, four source chips (color border, status, port), events-per-second sparkline, secrets toggle, Ctrl+K search. On the left, a 280px rail with the endpoint tree by source and activity dots. In the center, the Live view: the phase card and phase timeline, with segment lengths strictly proportional to actual durations, above the virtualized event table. On the right, a 440px inspector with the JSON tree and diff. At the bottom, the 32px ticker. No primary action button: the primary action is selecting.

FORM: Esports broadcast overlay (IMPECCABLE'S PICK, rank 1 on my list). Signature: phase cards and the live ticker. Borrowed additions: duration represented by exact length (labanotation), fixed key-by-key alignment for diffing (botanical folio), accent reserved for a single role (consumer app), neutrals on a strict ramp (specimen board). seed:557514a5172f9f2c

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
