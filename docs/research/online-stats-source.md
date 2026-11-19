# Online stats source

Researched 2026-11-18 for the opt-in `online_stats` setting (Champion Stats). Hypatia uses one provider.

## Decision

**MetaBot.GG**, through its public MCP endpoint `https://metabot.gg/api/mcp` (JSON-RPC 2.0 `tools/call` over HTTPS; answers come as JSON or as a single SSE event carrying `result.structuredContent`). Implemented in `crates/core/src/stats/online.rs` (`MetaBotSource`).

Why this one: it is the only candidate whose terms explicitly allow reusing its data in another product.

- **Terms** ([Terms of Service](https://metabot.gg/en/legal/terms-of-service), section 10 "Data Usage & Attribution"; [AI/MCP page](https://metabot.gg/en/ai)):
  - It publishes aggregate statistics through a public MCP server.
  - Reuse in "generated, published, or displayed content" is allowed if MetaBot.GG gets a visible link to the page the data came from. Each tool result gives that URL.
  - Its figures must not be presented as our own, altered in a misleading way, or used to imply endorsement.
  - Access must stay at a reasonable rate: about 60 requests per minute per IP. Going over returns `429` with `Retry-After`.
  - No key or account is needed.
- **What Hypatia does to comply**:
  - Every Online value carries `attribution` (provider plus page URL, kept only when it starts with `https://metabot.gg/`). The UI must show it as a visible, clickable link next to the data.
  - Answers are cached on disk for 6 hours, and the service re-checks that cache at most every 30 minutes. After a failure it waits 60 seconds before retrying, `429` included.
  - Numbers are shown as served. Win and pick rates are converted from `"52.1%"` to `0.521`.

## What it gives, and what it does not

| Hypatia view | MetaBot tool | Coverage |
|---|---|---|
| Champion Build | `get_entity_build` + `lookup_game_entity` | Core items (with boots split out using client game data), one rune page (keystone + 5 runes, no shards), tier, win rate, pick rate. **No summoner spells, no skill order, no per-item rates, no per-role builds.** |
| Matchups | `get_counters` | The champion's ~8 toughest matchups, with their win rates. No games count and no gold diff. |
| Tier List | `get_game_meta` (`filter` = top/jungle/mid/adc/support; `mode` = aram/arena) | About the top 8 per role, with tier S–D, win rate and pick rate. |

Limits that the code states honestly:
- There is no rank bracket or patch selector, so results come back as `bracket: All` with the patch the provider serves.
- Builds are champion-wide (`query.position = None`).
- Ranked Flex and normal queues return `NoData`.
- Items and runes come back as names in the requested language. Hypatia asks in the League client's locale (`/riotclient/region-locale`, mapped to MetaBot's language list, `en` otherwise) and matches the names against the client's own `items.json` and `perks.json`. Names it cannot match are dropped. Champions are matched from the slug in each entry's page URL, so the language does not matter for them.
- Spells and the skill order come only from Riot's recommended pages in the local client (spells) and from the demo. Online data has no skill order.

## What a request carries

These arguments only: `game: "league"`, the champion's English name, a role `filter` or a `mode`, and `lang` (the language the names should come back in). Nothing about the player is sent: no summoner name, PUUID, token, lockfile value, region or cookie.

The client is a dedicated `reqwest` client: system TLS, no cookie store, `User-Agent: Hypatia/<version>`, 4 s connect timeout and 8 s total timeout. With `online_stats` off, it is never even built.

## Candidates rejected

- **OP.GG MCP** (`https://mcp-api.op.gg/mcp`, [opgginc/opgg-mcp](https://github.com/opgginc/opgg-mcp), MIT client): the richest data. It has per-role builds with play counts and win rates, spells, skill order, rank tiers and counters with `my_win_rate`, and a lane meta tier list. Its answers use a compact class-constructor text format rather than JSON. It was rejected on terms: [OP.GG's Terms of Service](https://op.gg/lol/policies/agreement) forbid using "automated scripts to collect information" and commercial use without written consent, and the MCP repository states no separate data-use terms. **This is the fallback**, but only once OP.GG gives written permission. `StatsSource` stays the same; only `online.rs` changes.
- **U.GG, Lolalytics, Mobalytics, League of Graphs, dpm.lol**: no documented public API. Their JSON endpoints are internal and their terms forbid scraping.
- **Riot API**: it needs a per-developer key and has no aggregate endpoints, so it would mean crawling matches ourselves.
- **Data Dragon, CommunityDragon, Meraki**: static data only, with no win rates or builds.

## To re-check

- MetaBot's tool schemas and `structuredContent` shapes: saved samples are in `crates/core/tests/data/metabot_*.sse` (patch 26.22, cut down to 4 entries).
- The LCU shapes for `lol-perks` recommended pages and `lol-match-history` were written from the known format, without a running client: `crates/core/tests/data/lcu_*.json`. Compare them against the Observatory's endpoint catalog with League running.
