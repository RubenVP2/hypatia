# Hypatia

> **Local League of Legends companion — Pit-Wall Chronometer desktop app, second-monitor mini widget, toggleable Telemetry Observatory, and opt-in Mobile Remote accessibility controller.**

[![CI](https://github.com/RubenVP2/hypatia/actions/workflows/ci.yml/badge.svg)](https://github.com/RubenVP2/hypatia/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/release-v0.4.0-blue.svg)](https://github.com/RubenVP2/hypatia/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Slint](https://img.shields.io/badge/UI-Slint%201.18-purple.svg)](https://slint.dev)

---

## Overview

**Hypatia** is a self-contained native desktop companion written in **Rust** and **Slint**. It automatically discovers and follows your local Riot Games and League of Legends client services:

- **Main Window (`MainWindow`)**: Phase-aware Pit-Wall Chronometer view for Lobby, Queue, Ready Check, Champion Select (team line roster, lane diagram, enemy picks & bans), Live Game (team score, tower status map, objective ramp, event feed), and End of Game. Supports **Dark (Night)** and **Light (Day)** themes, as well as live **EN | FR** language switching (auto-detected from the OS on first launch, persisted locally, and synchronized with Mobile Remote).
- **Toggleable Telemetry Observatory (`Telemetry` button / `--telemetry`)**: Can be turned **ON or OFF** at any time from the top bar and displayed **Side-by-Side (alongside the companion view)** or **Full Screen**:
  1. **Live Telemetry Stream**: Real-time WAMP (`OnJsonApiEvent`) and LiveClient (`:2999`) event stream with source/kind filters, pause/resume, **JSON Payload Inspector**, **Previous vs Current State Diff**, and **Secret Reveal**.
  2. **API Endpoints & GET Tester**: Interactive OpenAPI / `/help` catalog for LCU, Riot Client, and Live Game (`:2999`) with an integrated read-only `GET` runner.
  3. **System Footprint**: Active Riot processes (PID, CPU, RAM, executable path), listening local ports (`127.0.0.1`), and established network connections.
- **Mini Widget (`MiniWidget`)**: Compact, always-on-top companion for a second monitor while playing.
- **Mobile Remote Accessibility Companion (`Mobile remote` button / `--remote`)**: Opt-in LAN server paired via a 128-bit QR code token. Scan the QR code on your PC screen with your phone to control **Lobby role selection, Queue start/cancel, Match Found accept/decline, Champion ban & pick, Summoner Spells, Skins, Rune pages (1-tap recommended presets, saved pages, or 1-hand custom builder), and Role / Pick Order / Champion swaps** one-handed from your phone, with multi-layer **Notifications, Audio Chimes, Lock-Screen Media Controls, and Vibration**.

---

## 🛡️ Security & Privacy Guarantees

| Guarantee | Mechanism |
|---|---|
| **Read-Only Desktop Core** | The desktop observatory uses `ReadOnlyRiotClient` (strictly `GET` requests and passive WAMP WebSocket subscriptions). |
| **Allowlisted Mobile Remote (Opt-in)** | Disabled by default. When enabled, `LcuActionClient` only accepts variants of the closed `RemoteCommand` enum — no arbitrary paths or payloads, and zero background automation. Every action requires an explicit user tap. |
| **128-bit QR Pairing & Host Guard** | Pairing uses a random 128-bit token (`/?token=...`) exchangeable for an `HttpOnly; SameSite=Strict` session cookie, with one-click token rotation/revocation and `Host`/`Origin` validation. |
| **Ephemeral Secret Vault** | Auth tokens (`accessToken`, JWTs, lockfile passwords) are automatically masked as `{"$secret": { id, len, hint }}` in an in-memory LRU vault. |
| **Zero Telemetry** | 100% offline and local. Zero external CDN calls, zero analytics. |

---

## ⚡ Quick Start

```bash
# Run Hypatia with a live League of Legends client
cargo run -p hypatia

# Run with the Telemetry Observatory open alongside the companion
cargo run -p hypatia -- --telemetry

# Run in Demo Mode with Telemetry and Mobile Remote QR code open on startup
cargo run -p hypatia -- --demo --telemetry --remote
```

---

## 📱 Mobile Remote (Accessibility Companion)

1. Click **Mobile remote** in the top bar of the Hypatia desktop window (or pass `--remote` on startup).
2. Scan the QR code displayed on your PC with your phone camera (your phone and PC must be on the same local Wi-Fi network).
3. Tap **Arm Alerts** at the top of the mobile screen once to arm:
   - **Web Notifications & Service Worker**
   - **Synthesized Web Audio Chimes & Background Audio Keep-Alive** (with lock-screen `MediaSession` status)
   - **Haptic Vibration Patterns** (`navigator.vibrate`)
   - **Screen Wake Lock** (`navigator.wakeLock`)
4. Use the one-handed bottom thumb-zone dock to start queues, accept matches, ban/pick champions, switch Summoner Spells (including `D ↔ F` swap) and Skins, equip 1-tap Recommended or custom Runes, and request or accept Role / Pick Order / Champion swaps.

---

## ⚙️ CLI Reference

```text
Usage: hypatia [OPTIONS]

Options:
      --demo                   Start in demo mode with synthetic event streams
      --demo-rate <DEMO_RATE>  Demo events generated per second [default: 2.0]
      --lang <LANG>            Interface language override (en or fr)
      --telemetry              Start with the League of Legends Telemetry Observatory open
      --remote                 Start with Mobile Remote server enabled and QR code visible
      --no-remote              Disable automatic startup of the Mobile Remote server
      --remote-port <PORT>     Preferred LAN port for Mobile Remote server [default: 7879]
      --light                  Start in Light (Day) theme instead of Dark (Night)
      --widget                 Open the always-on-top Mini Widget on startup
      --phase <PHASE>          Pin a specific phase in Demo mode (lobby, queue, ready, champ-select, live, end)
  -h, --help                   Print help
  -V, --version                Print version
```

---

## 🏗️ Architecture

```text
crates/
├── shared/    # Common data models, LiveBoard, SummarySnapshot, RemoteCommand & RemoteStateSnapshot
├── core/      # Lockfile/process discovery, WAMP & LiveClient connectors, LcuActionClient, LAN Remote server
└── desktop/   # Native Slint 1.18 desktop application (MainWindow, MiniWidget, QR pairing drawer)
```

---

## 📄 License & Disclaimer

Hypatia is an independent open-source companion tool. It is **not** affiliated with, endorsed by, or sponsored by Riot Games, Inc. League of Legends and Riot Games are trademarks or registered trademarks of Riot Games, Inc.
