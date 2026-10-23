# Hypatia — Riot Local Observatory

> Observatoire local haute fidélité pour les services, API et flux d'événements Riot Games sur votre machine.

**Hypatia** est un tableau de bord local 100% **Rust + WebAssembly** conçu pour inspecter, comprendre et surveiller en temps réel l'ensemble des surfaces d'exposition locales créées par les applications Riot Games (Riot Client, League Client Ux / LCU, Live Client Data et Replay API sur le port 2999).

Distribué sous la forme d'un **binaire unique et autonome** pour Windows, Hypatia intègre l'ensemble de son interface web (Leptos WebAssembly) directement dans l'exécutable grâce à `rust-embed`.

---

## 🛡️ Garanties absolues et sécurité

Hypatia est un instrument d'observation et d'audit, **pas un outil d'automatisation ni un client alternatif**. Il repose sur des garanties strictes :

1. **Lecture seule absolue (Read-Only By Design)**
   - Le serveur n'émet **strictement aucune** requête d'écriture (`POST`, `PUT`, `PATCH`, `DELETE`) vers les API locales de Riot.
   - Les requêtes sont restreintes aux verbes `GET` et aux abonnements WebSocket WAMP passifs via un client HTTP bridé par typage (`ReadOnlyRiotClient`).
2. **Masquage automatique des secrets et coffre éphémère**
   - Tous les jetons d'authentification (`accessToken`, tokens JWT, mots de passe de lockfile, clés de session, en-têtes Authorization) sont automatiquement détectés et remplacés par des références anonymisées `{"$secret": { id, len, hint }}`.
   - Les secrets réels sont conservés dans un coffre chiffré en mémoire vive (LRU). La révélation d'une valeur est **explicite et unitaire**, journalisée et assortie d'un compte à rebours de **30 secondes** avant ré-occultation automatique.
3. **Protection contre le DNS Rebinding et CSRF**
   - Le serveur écoute exclusivement sur l'interface locale `127.0.0.1`.
   - Chaque requête HTTP et WebSocket est filtrée par un middleware de sécurité qui valide rigoureusement les en-têtes `Host` et `Origin`. Tout appel provenant d'un navigateur pointant vers un domaine externe est rejeté (HTTP 403 Forbidden).
4. **Confidentialité totale et zéro télémétrie**
   - Aucune donnée ne quitte votre machine locale. Aucune requête externe n'est effectuée à l'exécution (les polices et styles sont embarqués en local).
   - Les connexions réseau distantes vers les serveurs de Riot sont affichées telles quelles (`IP:Port`) sans aucune résolution DNS.

---

## ⚡ Démarrage rapide

### Prérequis

- **Windows 10 / 11** (64 bits)
- **Rust** (toolchain stable `x86_64-pc-windows-msvc`)
- **WSL** (avec `trunk` et la cible `wasm32-unknown-unknown` installés pour la compilation du frontend WebAssembly)

### Compilation unifiée en une commande

Depuis la racine du projet sous PowerShell :

```powershell
.\build.ps1
```

Ce script automatise l'ensemble du cycle de fabrication :
1. Compile le frontend WebAssembly Leptos en release sous WSL avec Trunk (`crates/web/dist`).
2. Compile le serveur natif Windows en release (`target/release/hypatia-server.exe`) en embarquant les assets dans le binaire.
3. Vérifie l'intégrité et confirme la conformité au budget de taille (**< 25 Mo**, environ 8 Mo effectifs).

### Lancement

```powershell
.\target\release\hypatia-server.exe
```

Le serveur démarre immédiatement et ouvre automatiquement votre navigateur par défaut sur `http://127.0.0.1:7878` (ou `8334` si le port par défaut est réservé par le système).

---

## 🕹️ Mode Démonstration (`--demo`)

Vous pouvez tester et explorer l'intégralité des fonctionnalités d'Hypatia sans lancer League of Legends ni le Riot Client grâce au mode démonstration intégré :

```powershell
.\target\release\hypatia-server.exe --demo
```

Dans ce mode :
- Des sources synthétiques réalistes (`RiotClient`, `LeagueClient`, `LiveGame`, `System`) sont instanciées.
- Un générateur temps réel reproduit fidèlement le cycle de jeu complet :
  **Salon (`Lobby`) ➔ Recherche de partie (`Matchmaking`) ➔ Partie trouvée (`ReadyCheck`) ➔ Sélection des champions (`ChampSelect`) ➔ En jeu (`InProgress`) ➔ Fin de partie (`EndOfGame`)**.
- Les événements transitent avec des données réalistes (statuts d'amis, sessions de draft, kills, objectifs) et incluent des jetons d'authentification factices pour tester la révélation 30s des secrets.
- L'interface affiche un bandeau discret : `[DÉMO] Mode Démonstration (données synthétiques)`.
- La cadence d'émission peut être ajustée pour tester la fluidité et la virtualisation du DOM jusqu'à 300 événements/seconde :
  ```powershell
  .\target\release\hypatia-server.exe --demo --demo-rate 150
  ```

---

## ⌨️ Raccourcis clavier

Hypatia a été conçu pour être pilotable intégralement au clavier, avec des retours visuels à fort contraste (`--focus`) :

| Touche | Action |
|---|---|
| `1` | Vue **En direct** (carton de phase esport, chronologie proportionnelle, flux temps réel virtualisé) |
| `2` | Vue **Endpoints** (catalogue exhaustif des API Riot, explorateur interactif et test d'appels `GET`) |
| `3` | Vue **Système** (processus Riot actifs, ports en écoute et sockets distantes chiffrées) |
| `4` | Vue **À propos** (garanties, explications des lockfiles et guide Replay API) |
| `Espace` | **Pause / Reprise** du flux d'événements en direct (avec badge d'événements manqués) |
| `Ctrl + K` / `Cmd + K` | **Palette de commandes** globale (recherche floue d'endpoints, navigation rapide, actions) |
| `/` | Mettre le focus instantanément sur le champ de recherche du rail latéral gauche |
| `Flèches ← / →` | Redimensionner le panneau actif au clavier (rail gauche ou inspecteur) |
| `Échap` | Fermer la palette de commandes ou désélectionner l'élément actif |

---

## ⚙️ Options en ligne de commande (CLI)

```text
Usage: hypatia-server.exe [OPTIONS]

Options:
  -p, --port <PORT>        Port d'écoute HTTP / WebSocket [default: 7878]
      --no-open            Désactive l'ouverture automatique du navigateur au lancement
      --dev                Active le mode développement (autorise l'origine Trunk sur le port 8080)
      --demo               Active le mode démonstration avec flux d'événements synthétiques
      --demo-rate <RATE>   Cadence de génération du mode démo en événements/seconde [default: 10.0]
  -h, --help               Affiche l'aide
```

---

## 🏗️ Architecture du projet

Le projet est structuré sous forme de workspace Cargo multi-crates :

- **`crates/shared`** :
  Modèle de données partagé, types du protocole WebSocket (`ServerMsg`, `ClientMsg`), représentation des événements (`EventMeta`), états des sources (`SourceState`), snapshots système et définitions de sécurité.
- **`crates/server`** :
  Binaire serveur Windows en Rust (`Axum` + `Tokio`). Découverte dynamique des processus Riot (`sysinfo`), détection des lockfiles et ports locaux (`netstat2`), client HTTP lecture seule, connecteurs passifs WAMP, moteur d'occultation des secrets (`SecretVault`), batcher d'événements et intégration des assets statiques compilés (`rust-embed`).
- **`crates/web`** :
  Application frontend SPA en Rust compilée en WebAssembly (`Leptos 0.7 CSR`). Design system Dark Mode haute densité inspiré des habillages de diffusion esport (« Operate »), virtualisation de rendu pour supporter des centaines d'événements/s, explorateur d'arbre JSON paresseux, comparateur de diff visuel clé par clé et chronologie interactive des phases de jeu.

---

## 📄 Licence & Mentions

Hypatia est un projet de recherche et d'observation indépendant. Il n'est ni affilié à, ni approuvé par Riot Games.
Toutes les marques et propriétés intellectuelles associées à League of Legends et Riot Games appartiennent à Riot Games, Inc.
