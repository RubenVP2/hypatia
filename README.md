# Hypatia

> **High-fidelity local observatory and real-time event inspector for Riot Games client APIs.**

[![CI](https://github.com/RubenVP2/hypatia/actions/workflows/ci.yml/badge.svg)](https://github.com/RubenVP2/hypatia/actions/workflows/ci.yml)
[![Release](https://img.shields.io/badge/release-v0.1.0-blue.svg)](https://github.com/RubenVP2/hypatia/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![WebAssembly](https://img.shields.io/badge/frontend-Leptos%20WASM-purple.svg)](https://leptos.dev)

English | [Français](README.fr.md)

---

## Overview

**Hypatia** is a self-contained local developer observatory written in **Rust** and **WebAssembly (Leptos 0.7)**. It automatically discovers, inspects, and streams real-time events from local Riot Games services on your machine:

- **Riot Client** & **League Client Ux (LCU)** (lockfile discovery, passive WAMP WebSocket, OpenAPI/help catalog)
- **Live Client Data & Replay API** (active match polling on port `2999`)
- **System Footprint** (running Riot processes, listening ports, established remote sockets)

Distributed as a **single, dependency-free binary (< 10 MB)** with the web UI fully embedded.

---

## 🛡️ Security & Privacy Guarantees

| Guarantee | Mechanism |
|---|---|
| **Read-Only by Design** | Strict GET-only HTTP proxy (`ReadOnlyRiotClient`) and passive WAMP subscriptions. Zero write operations (`POST`/`PUT`/`PATCH`/`DELETE`). |
| **Ephemeral Secret Vault** | Auth tokens (`accessToken`, JWTs, lockfile passwords) are automatically masked as `{"$secret": { id, len, hint }}`. Plaintext values are stored in an in-memory LRU vault with explicit 30s timed reveals. |
| **DNS Rebinding & CSRF Protection** | Binds strictly to `127.0.0.1`. Middleware validates `Host` and `Origin` on all HTTP/WebSocket requests, dropping external browser payloads (HTTP 403). |
| **Zero Telemetry** | 100% offline and local. Remote socket endpoints are listed by IP:Port without DNS reverse lookups. No tracking, analytics, or external CDN calls. |

---

## ⚡ Quick Start

### 1. Download Prebuilt Binary

Grab the archive for your operating system from the [Latest Releases](https://github.com/RubenVP2/hypatia/releases/latest):

- **Windows x86_64**: `hypatia-windows-x86_64.zip`
- **Linux x86_64**: `hypatia-linux-x86_64.tar.gz`
- **macOS Apple Silicon**: `hypatia-macos-arm64.tar.gz`
- **macOS Intel**: `hypatia-macos-x86_64.tar.gz`

### 2. Run

```bash
# Launch Hypatia (automatically opens http://127.0.0.1:7878 in default browser)
./hypatia-server

# Windows
.\hypatia-server.exe
```

---

## 🕹️ Demo Mode

Test all features without League of Legends or Riot Client running:

```bash
./hypatia-server --demo
```

Simulates the complete match lifecycle:
$$\text{Lobby} \longrightarrow \text{Matchmaking} \longrightarrow \text{ReadyCheck} \longrightarrow \text{ChampSelect} \longrightarrow \text{InProgress} \longrightarrow \text{EndOfGame}$$

Tune event generation throughput up to 300 events/sec:
```bash
./hypatia-server --demo --demo-rate 100
```

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `1` | **Live Stream** (esport broadcast layout, proportional phase timeline, virtualized event feed) |
| `2` | **Endpoints** (interactive Riot API catalog, schema viewer, in-browser GET tester) |
| `3` | **System** (active Riot processes, listening ports, remote connections) |
| `4` | **About** (security spec, lockfile mechanics, replay guide) |
| `Space` | **Pause / Resume** real-time event ingestion (with missed counter badge) |
| `Ctrl + K` / `Cmd + K` | **Command Palette** (quick jump to any endpoint, filter, or view) |
| `/` | Focus search bar in the source rail |
| `Esc` | Clear selection / close command palette |

---

## ⚙️ CLI Reference

```text
Usage: hypatia-server [OPTIONS]

Options:
  -p, --port <PORT>        HTTP / WebSocket listen port [default: 7878]
      --no-open            Do not open default browser on startup
      --dev                Allow Trunk development server origin (port 8080)
      --demo               Start in demo mode with synthetic event streams
      --demo-rate <RATE>   Demo events generated per second [default: 10.0]
  -h, --help               Print help
```

---

## 🏗️ Architecture

```
crates/
├── shared/   # Common data models, WebSocket protocols (ServerMsg/ClientMsg), security types
├── server/   # Native Axum + Tokio backend, process discovery, secret vault, rust-embed
└── web/      # Single Page App in Leptos 0.7 WASM (Dark theme, virtual scroll, JsonDiff)
```

### Build from Source

```bash
# Prerequisites: Rust 1.85+, wasm32-unknown-unknown, Trunk
git clone https://github.com/RubenVP2/hypatia.git
cd hypatia

# 1. Build WebAssembly UI
cd crates/web && trunk build --release && cd ../..

# 2. Build Server Binary
cargo build --release -p hypatia-server

# Or on Windows via PowerShell script:
.\build.ps1
```

---

## 📄 License & Disclaimer

Hypatia is an independent open-source research and observation tool. It is **not** affiliated with, endorsed by, or sponsored by Riot Games, Inc. League of Legends and Riot Games are trademarks or registered trademarks of Riot Games, Inc.
