# Frank roadmap

Frank is an agent operations office: the headless Rust backend owns state and
side effects, while the Flutter desktop client is a remote client. Legacy 0.2.x
data remains in place and is not migrated automatically.

## Current milestone: Flutter office shell

- [x] Move Cargo crates and `xtask` under `backend/`.
- [x] Remove iced/wgpu/Bevy GUI dependencies from the active workspace.
- [x] Add a Flutter desktop package at `apps/frank_desktop`.
- [x] Add a desktop-only, persistent, fixed-width Forui off-canvas sidebar with a minimum 880px window width.
- [x] Add a central Account Executive conversation using FlowUI.
- [x] Add an operational work inbox with mission shelves, scope, search, and pinning.
- [x] Reserve an empty Flame floor surface.
- [x] Add semantic Lucide icon registry and native macOS titlebar integration.
- [x] Persist client-only sidebar visibility, scope, and pinned order.
- [x] Add local gateway fixtures and widget tests.
- [x] Run `proto run flutter` checks with Flutter 3.47.1.
- [x] Build the macOS debug bundle from the Flutter desktop scaffold.

## Next milestones

- [ ] Replace `FixtureFrankGateway` with the reconnecting `frank-client`
      transport and protocol DTO mapping.
- [ ] Add manual agent creation and positioning (junior programmer, system
      analyst, accounting, and other agency roles).
- [ ] Add project brief, mission, task DAG, approvals, budgets, and delivery
      flows to the Account Executive conversation.
- [ ] Add Team, Activity, Ledger, and Settings surfaces behind the existing
      sidebar destinations.
- [ ] Populate the Flame floor with agents, desks, status, and project work.
- [ ] Add tray, single-instance, reconnect, and desktop update flows with
      VoiceOver/NVDA acceptance tests.
- [ ] Publish Flutter desktop packages alongside portable backend archives.

## Backend release gates

```sh
cargo test --workspace --locked --no-fail-fast
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo run --locked -p xtask -- build-packs
git diff --exit-code packs/
cargo run --locked -p xtask -- lint-targets
cargo run --locked -p xtask -- architecture-check
```

No public desktop release is cut while the Flutter widget test, reconnect,
accessibility, or backend security gates are unchecked.
