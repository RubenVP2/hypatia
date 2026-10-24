# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Rust everywhere, by user choice. A native binary (Axum + tokio) reads lockfiles, connects to local Riot APIs (self-signed TLS, Basic Auth, WAMP WebSocket), and serves the frontend. Frontend built in Leptos (CSR) compiled to WebAssembly. A single binary to launch, running strictly locally (127.0.0.1).

## Users

A single user: the PC owner who plays Riot games (League of Legends, Valorant) and has technical expertise. They want to know what the Riot launcher exposes and transmits on their machine. Typical usage: on a secondary monitor alongside the game, often in the evening in a dark room. They also use the tool during dedicated exploratory sessions.

## Product Purpose

A local dashboard providing real-time visibility into everything Riot services expose on the machine:
- Riot processes and their listening ports;
- Local APIs (Riot Client, League Client/LCU, Live Client Data, and Replay on port 2999);
- Their endpoints and data;
- The events being transmitted.

Success is defined as the user being able to answer within seconds: "what is running, what is exposed, what just happened, and what data does it contain."

## Positioning

Neither an alternative client nor an automation tool. It is a read-only audit and observation instrument covering all local Riot surfaces in a single live view. Existing tools (LCU Explorer, Needlework) explore only one API at a time and do not display the live stream.

## Operating Context

- Three confirmed use cases: auditing and understanding, monitoring an event feed during a match, and exploring endpoints like a dev tool (GET requests and response inspection).
- Riot services start, stop, and change port and password on every launch. The dashboard must track these lifecycles seamlessly without intervention.
- Runs alongside the game: must remain lightweight in CPU and memory footprint.

## Capabilities and Constraints

- Read-only: strictly GET requests and WebSocket subscriptions. No POST, PUT, PATCH, or DELETE requests are sent to Riot APIs.
- Secrets (lockfile passwords, access tokens, entitlements JWTs, Authorization headers) masked by default, with explicit, temporary reveals.
- The server binds strictly to 127.0.0.1.
- LCU and Riot Client APIs are not officially documented. Their endpoint catalog is discovered at runtime (`/help`, OpenAPI schemas when available).
- Zero data is transmitted off the machine.

## Evidence on Hand

No real-world data has been captured yet. Development fixtures must be marked as synthetic.

## Product Principles

1. See everything: everything that is local and observable is shown; nothing is hidden "for simplicity".
2. Live-first: present state and immediate events take precedence over deep history.
3. Observe without touching: the tool never mutates the state of Riot clients.
4. Discreet alongside the game: minimal resource footprint, glanceable on a secondary monitor.

## Accessibility & Inclusion

No confirmed specific requirements. Baseline standards apply: WCAG AA, full keyboard navigation, support for `prefers-reduced-motion`.
