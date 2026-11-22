# Changelog

All notable changes to Hypatia are documented here, generated from Conventional Commits
by [release-plz](https://release-plz.dev). Versions follow [Semantic Versioning](https://semver.org).

## [Unreleased]

## [0.6.1](https://github.com/RubenVP2/hypatia/compare/v0.6.0...v0.6.1) - 2026-11-22

### Fixed
- *(update)* Retry a failed update check after 10 minutes ([#26](https://github.com/RubenVP2/hypatia/pull/26))

## [0.6.0](https://github.com/RubenVP2/hypatia/compare/v0.5.0...v0.6.0) - 2026-11-22

### Added
- *(remote)* Hextech Mobile Remote, with Results on the phone
- *(remote)* Redesign Mobile Remote V2 (Ready Check, 5v5 mirror, unified Loadout, Live Duel & Pit Timers) ([#21](https://github.com/RubenVP2/hypatia/pull/21))
- *(ui)* Add 3 League of Legends visual variants + Summoner's Rift map & splash backdrops
- *(ui)* Single Hextech direction, client art pipeline and repaint fixes
- *(ui)* Hextech champion select with splash banners and draft moments
- *(ui)* Hextech ready check medallion with countdown ring
- *(ui)* Hextech live game HUD, player banners and live moments
- *(ui)* Hextech Results with crest, stat tiles and both teams
- *(ui)* Hextech mini widget, fitted to each phase
- *(board)* Show where each enemy was last seen, with epic monster timers
- *(perf)* Add performance profiles, granular controls, and live footprint monitor
- *(stats)* Champion Stats, Draft Helper, Game Text and centered dialogs ([#10](https://github.com/RubenVP2/hypatia/pull/10))
- *(stats,draft,live,eog)* DPM.LOL Feature Parity & Local-First Analytics Roadmap (#11–#19) ([#20](https://github.com/RubenVP2/hypatia/pull/20))
- *(update)* Check for new releases and install them in one click ([#25](https://github.com/RubenVP2/hypatia/pull/25))

### Fixed
- *(board)* Say "Zed vous a éliminé" when the player is killed in French
- *(board)* Name tower and minion killers in the feed
- *(ui,assets)* Render SplashBackdrop inside header Plate and ingest LCU skins.json
- *(desktop)* Remove mini widget and fix Windows mixed-DPI resize loop
- *(remote)* Lock hovered champion via PATCH+POST and sync champ-select countdown timer ([#22](https://github.com/RubenVP2/hypatia/pull/22))

### Changed
- Deepen Match Projection, DemoRemoteState, Mobile Remote execution, Settings, and desktop views

## [0.5.0](https://github.com/RubenVP2/hypatia/compare/v0.4.0...v0.5.0) - 2026-11-13

### Added
- Observatory redesign, live phase clocks and event-driven core
- *(ui)* Use Slint bundled translations for EN/FR

### Fixed
- Clear demo data on stop and make buttons fire on first click

### Changed
- Unify Match Projection, ChampionLookup, and Position across the workspace
- *(ui)* Slint best-practice pass and layout fixes

## [0.4.0](https://github.com/RubenVP2/hypatia/compare/v0.3.0...v0.4.0) - 2026-11-13

### Added
- Live EN/FR language switcher across desktop, core, and Mobile Remote

## [0.3.0](https://github.com/RubenVP2/hypatia/releases/tag/v0.3.0) - 2026-11-13

### Added
- Initial English release of the Hypatia companion
- `/api/summary` endpoint and broadcast Recap view
- Slint desktop companion, Observatory, Mobile Remote and Pit-Wall Chronometer
- Local game icons and text UI across desktop and mobile
