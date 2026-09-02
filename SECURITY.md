# Frank security gate

The release job runs `cargo audit --deny warnings`. The command contains only
exact advisory identifiers, never a package-wide or wildcard ignore. The Rust
workspace is headless: it no longer links iced, wgpu, Bevy, tray-icon, or a
desktop web runtime. This keeps the daemon and hook fast path independent from
desktop rendering dependencies.

The Flutter client is a separate package under `apps/frank_desktop`. It never
opens the database or project filesystem directly. Production transport will go
through the authenticated `frank-client` boundary; the current prototype uses
in-memory fixtures only.

## Accessibility

The Flutter shell is responsible for keyboard navigation and platform
semantics (including VoiceOver on macOS and NVDA on Windows). Accessibility is a
release gate for every interactive surface: the permanent sidebar, Projects
tree, chat thread, composer, streaming state, and the `flutter_scene` floor
status/fallback. The current 3D objects are static and do not enter keyboard
traversal; future interactive agents must add scene semantics explicitly. The
CLI remains the screen-reader-first fallback for administration and automation.

## Data and transport

- `frankd` owns SQLite, provider processes, PTYs, worktrees, Git writes, and the
  global event sequence.
- Desktop clients receive state through authenticated HTTPS/WebSocket and must
  not bypass `frank-server` or `frank-store`.
- Pairing secrets, certificate pins, update signatures, and audit records stay
  in the backend trust boundary.
- Local fixtures must never be mistaken for production credentials or persisted
  user data.

Re-run the audit and the Flutter dependency review after every dependency
upgrade. Keep generated preview bundles, coverage scratch files, compiled
`flutter_scene_generated/` output, and legacy client backups outside version
control. Treat `FLTEnableFlutterGPU` and the scene build hook as part of the
trusted host/build boundary; a GPU failure must remain a nonfatal UI fallback.
