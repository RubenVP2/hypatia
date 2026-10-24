# Hypatia

> **Observatoire local haute fidélité et inspecteur d'événements temps réel pour les API Riot Games.**

[![CI](https://github.com/RubenVP2/hypatia/actions/workflows/ci.yml/badge.svg)](https://github.com/RubenVP2/hypatia/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/RubenVP2/hypatia?color=blue)](https://github.com/RubenVP2/hypatia/releases/latest)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![WebAssembly](https://img.shields.io/badge/frontend-Leptos%20WASM-purple.svg)](https://leptos.dev)

[English](README.md) | Français

---

## Vue d'ensemble

**Hypatia** est un observatoire local autonome écrit en **Rust** et **WebAssembly (Leptos 0.7)**. Il découvre, inspecte et transmet en temps réel les événements émis par les services locaux de Riot Games sur votre machine :

- **Riot Client** & **League Client Ux (LCU)** (découverte par lockfile, WebSocket WAMP passif, catalogue OpenAPI/help)
- **Live Client Data & Replay API** (polling temps réel de la partie en cours sur le port `2999`)
- **Empreinte Système** (processus Riot actifs, ports locaux en écoute, sockets distantes établies)

Distribué sous la forme d'un **binaire unique et autonome (< 10 Mo)** avec l'interface web entièrement embarquée.

---

## 🛡️ Garanties de Sécurité & Confidentialité

| Garantie | Mécanisme |
|---|---|
| **Lecture seule absolue** | Proxy HTTP restreint aux requêtes GET (`ReadOnlyRiotClient`) et abonnements WAMP passifs. Aucune écriture (`POST`/`PUT`/`PATCH`/`DELETE`). |
| **Coffre éphémère de secrets** | Les jetons sensibles (`accessToken`, JWT, mots de passe lockfile) sont masqués en `{"$secret": { id, len, hint }}`. Les valeurs réelles sont stockées en mémoire vive (LRU) avec ré-occultation automatique après 30s. |
| **Protection DNS Rebinding & CSRF** | Écoute exclusivement sur `127.0.0.1`. Validation stricte des en-têtes `Host` et `Origin` sur chaque requête HTTP/WebSocket (rejet immédiat en HTTP 403 des domaines tiers). |
| **Zéro télémétrie** | Fonctionnement 100% hors-ligne. Les connexions distantes sont affichées en `IP:Port` sans résolution DNS inverse. Aucun appel vers des CDN ou serveurs tiers. |

---

## ⚡ Démarrage Rapide

### 1. Télécharger le binaire précompilé

Récupérez l'archive correspondant à votre système depuis les [Dernières Releases](https://github.com/RubenVP2/hypatia/releases/latest) :

- **Windows x86_64** : `hypatia-windows-x86_64.zip`
- **Linux x86_64** : `hypatia-linux-x86_64.tar.gz`
- **macOS Apple Silicon** : `hypatia-macos-arm64.tar.gz`
- **macOS Intel** : `hypatia-macos-x86_64.tar.gz`

### 2. Lancement

```bash
# Lance Hypatia (ouvre automatiquement http://127.0.0.1:7878 dans votre navigateur)
./hypatia-server

# Sous Windows
.\hypatia-server.exe
```

---

## 🕹️ Mode Démonstration

Explorez l'ensemble des fonctionnalités sans lancer League of Legends ni le Riot Client :

```bash
./hypatia-server --demo
```

Simule l'intégralité du cycle de partie :
$$\text{Lobby} \longrightarrow \text{Matchmaking} \longrightarrow \text{ReadyCheck} \longrightarrow \text{ChampSelect} \longrightarrow \text{InProgress} \longrightarrow \text{EndOfGame}$$

Ajustez la cadence d'émission jusqu'à 300 événements/seconde :
```bash
./hypatia-server --demo --demo-rate 100
```

---

## ⌨️ Raccourcis Clavier

| Raccourci | Action |
|---|---|
| `1` | **En direct** (vue esport, chronologie proportionnelle, flux d'événements virtualisé) |
| `2` | **Endpoints** (catalogue interactif des API Riot, visualiseur de schémas, testeur d'appels GET) |
| `3` | **Système** (processus Riot en cours, ports en écoute, connexions distantes chiffrées) |
| `4` | **À propos** (spécification sécurité, fonctionnement des lockfiles, guide Replay) |
| `Espace` | **Pause / Reprise** du flux d'événements (avec compteur d'événements manqués) |
| `Ctrl + K` / `Cmd + K` | **Palette de commandes** (recherche floue d'endpoints, navigation rapide) |
| `/` | Focus instantané sur le filtre de recherche du rail gauche |
| `Échap` | Désélectionner / fermer la palette de commandes |

---

## ⚙️ Options en ligne de commande (CLI)

```text
Usage: hypatia-server [OPTIONS]

Options:
  -p, --port <PORT>        Port d'écoute HTTP / WebSocket [défaut: 7878]
      --no-open            Désactive l'ouverture automatique du navigateur au démarrage
      --dev                Autorise l'origine du serveur Trunk dev (port 8080)
      --demo               Active le mode démonstration avec flux synthétique
      --demo-rate <RATE>   Nombre d'événements synthétiques par seconde [défaut: 10.0]
  -h, --help               Affiche l'aide
```

---

## 🏗️ Architecture

```
crates/
├── shared/   # Modèles partagés, protocole WebSocket (ServerMsg/ClientMsg), types de sécurité
├── server/   # Backend natif Axum + Tokio, découverte de processus, coffre de secrets, rust-embed
└── web/      # Frontend SPA en Leptos 0.7 WASM (Thème sombre esport, rendu virtualisé, JsonDiff)
```

### Compiler depuis les sources

```bash
# Prérequis : Rust 1.85+, wasm32-unknown-unknown, Trunk
git clone https://github.com/RubenVP2/hypatia.git
cd hypatia

# 1. Compilation du frontend WebAssembly
cd crates/web && trunk build --release && cd ../..

# 2. Compilation du binaire serveur natif
cargo build --release -p hypatia-server

# Ou sous Windows via le script PowerShell :
.\build.ps1
```

---

## 📄 Licence & Mentions

Hypatia est un projet de recherche et d'observation open-source indépendant. Il n'est **ni** affilié à, ni approuvé ou sponsorisé par Riot Games, Inc. League of Legends et Riot Games sont des marques ou marques déposées de Riot Games, Inc.
