---
name: remote-command
description: Add or change a Hypatia Mobile Remote action, a `RemoteCommand` the paired phone sends to the League Client. Use when the phone should trigger something new in the client, or an existing remote action changes its endpoint, parameters, or UI.
---

A remote command is the only way Hypatia writes to the League Client, so the allowlist stays closed: each variant has typed, validated parameters and maps to fixed LCU endpoints. Read the Mobile Remote entry in [CONTEXT.md](../../../CONTEXT.md) first.

## 1. Gate

The action qualifies when all three hold; otherwise stop and tell the user why:

- One tap triggers it. It never repeats, schedules, or reacts on its own.
- It maps to one or a few fixed LCU paths whose variable parts are typed ids.
- It is in scope per [PRODUCT.md](../../../PRODUCT.md): local data only, no automation, anonymization respected.

## 2. Variant

Add the variant to `RemoteCommand` in `crates/shared/src/remote.rs`, with a doc comment naming the LCU endpoint. The serde tag makes its wire name the snake_case variant name (`AcceptReadyCheck` is `{"cmd": "accept_ready_check"}`).

Adding it breaks two exhaustive `match`es in `crates/core/src/lcu_actions.rs`; `cargo check -p hypatia-core` lists them. Steps 3 and 4 fill them.

## 3. Resolve

In `resolve_lcu_command_localized`, return the `ResolvedLcuCall`s: method, path, body.

- Validate every parameter before building the path: ids positive, enums from a fixed set, lists bounded. Reject with `bail!` and a `lang.pick("English", "Français")` message.
- Build paths with `format!` from the validated typed values only.

## 4. Demo

In `DemoRemoteState::apply_command`, mirror the effect on the demo state, so the action works in `--demo` without League running.

## 5. State (only if the phone must show something new)

Add the field to `RemoteStateSnapshot` (`crates/shared/src/remote.rs`) with `#[serde(default)]`, and fill it in `extract_remote_state` (`crates/core/src/lcu_actions.rs`) from projected data, keeping the Match Projection invariants: neutral empty values when missing, teammates anonymized as Riot left them.

## 6. Phone UI

In `crates/core/src/remote_ui.html`:

- Render a button with `data-action="<name>"` and handle it in the click dispatcher (search `btn.dataset.action`) with `sendCmd({ cmd: "<snake_name>", ...params })`.
- Add every new label under both `en` and `fr` in `I18N`, read with `t(key)`.
- Follow [DESIGN.md](../../../DESIGN.md): tap targets at least 44px, one main action per dock, amber only for act-now moments.

## 7. Tests

In the `lcu_actions.rs` tests module:

- `test_resolve_allowlist_valid_commands`: assert the exact method, path, and body of the new command.
- `test_resolve_rejects_invalid_inputs`: one assertion per validation rule from step 3.

## Done

- Every validation rule from step 3 has a rejecting test, and `cargo test --workspace` passes.
- `cargo run -p hypatia -- --demo --remote`, then the action tapped on the phone page (or `curl` against `/api/action` with the pairing token) visibly changes the demo state, in both EN and FR.
