# Hypatia

Local, read-only League of Legends companion in Rust + Slint. Crates: `shared` (models, Match Projection), `core` (discovery, Riot connectors, Mobile Remote server), `desktop` (Slint app).

## Pointers

- [CONTEXT.md](CONTEXT.md): domain terms and their invariants. Read before changing projection, the champion catalog, positions, the live board, the change signal, settings, or the Mobile Remote; name things in code and commits with its terms.
- [DESIGN.md](DESIGN.md): the visual system. Read before editing `crates/desktop/ui/*.slint` or `crates/core/src/remote_ui.html`.
- [PRODUCT.md](PRODUCT.md): users, scope, positioning. Read when a change adds a feature, a data source, or a network call.
- Skill `remote-command`: adding or changing a Mobile Remote action.

## Guardrails

Trust is the product. Every change keeps these true:

- Riot access is `GET` plus passive WebSocket subscriptions through `ReadOnlyRiotClient`. The only writes are `RemoteCommand` variants, run by `LcuActionClient` after an explicit tap on the paired phone.
- Data comes from local Riot APIs on `127.0.0.1`; fonts and assets ship in the binary. Traffic stays on loopback and the paired LAN.
- Ranked Solo/Duo teammates stay exactly as anonymized as Riot leaves them.
- Secrets pass through `redaction` and stay masked until the user reveals them in the Observatory.
- Demo identities live in `crates/core/src/fixtures.rs` and `DemoRemoteState`; production extractors fall back to neutral empty values.

## Every string ships in English and French

Strings have three homes, each with its own mechanism:

- **Slint UI**: write `@tr("English")`. Then, from `crates/desktop`, regenerate the template with `slint-tr-extractor ui/app.slint ui/parts.slint ui/theme.slint -o lang/hypatia.pot`, and add the French `msgstr` to `lang/fr/LC_MESSAGES/hypatia.po` by hand (no `msgmerge` here). Each entry carries a `msgctxt` equal to its Slint component name; copy it with the `msgid`.
- **Rust** (errors, labels, alerts): `lang.pick("English", "Français")`.
- **Mobile page**: add the key under both `en` and `fr` in the `I18N` object of `remote_ui.html`, read with `t(key)`.

## Naming

Player-facing copy says **Observatory** (FR *Observatoire*). Code and older text call the same view Telemetry / Telemetry Lab (`--telemetry`, `Flags::telemetry()`).

## Done

- `cargo test --workspace` passes. CI runs it on Windows and Ubuntu, then `cargo build --release -p hypatia`.
- UI changes are seen running: `cargo run -p hypatia -- --demo`, with `--phase <lobby|queue|ready|champ-select|live|end>` to pin a phase and `--remote` for the phone view.
