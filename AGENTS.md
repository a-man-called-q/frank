# Frank

Rust implementation inspired by [Caveman](https://github.com/JuliusBrussee/caveman). Frank is the
*engine*; "caveman" is the default persona pack shipped with it. Third parties add
packs (personas) and targets (agent integrations) without forking.

The historical Node.js original was the reference implementation and fixture source
for this rebuild. The working-tree copy is removed after its required compressor
fixtures are vendored under `backend/crates/frank-compress/tests/fixtures/`; the history still
contains the original when a provenance check is needed.

Full design rationale lives in the approved plan this project was built from. If you
need the "why" behind a decision below and it isn't here, look for
`buat-plan-untuk-bangun-soft-marshmallow.md`.

## Why this exists

Caveman's own historical docs admit the skill costs
~1–1.5k input tokens per turn and can be net-negative. That number was never
measured — it was inferred from file size. The measured filtered injection is
**3176 B ≈ 860 tokens once per session**, plus ~45 tok/turn of reinforcement.
Worse: `caveman-stats.js` never reads `input_tokens` or
`cache_creation_input_tokens` from the session JSONL it already parses, so the
project could never verify its own central claim.

**Frank's job is to be honest about this**, not just fast. See `frank-ledger`.

## Architecture

```
backend/crates/frank-cli ──> app, client, protocol, pack, state, ledger, compress, target, mcp
backend/crates/frank-state ──> frank-pack, frank-safeio
backend/crates/frank-ledger ──> frank-state, frank-safeio
backend/crates/frank-target ──> frank-pack, frank-safeio
backend/crates/frank-mcp ──> frank-compress
backend/crates/frank-app ──> frank-pack, frank-state, frank-safeio, frank-service, frank-target, frank-ledger
backend/crates/frank-protocol ──> (leaves)
backend/crates/frank-store ──> frank-protocol, frank-safeio
backend/crates/frank-agent ──> frank-protocol
backend/crates/frank-orchestrator ──> frank-store, frank-agent, frank-ledger, frank-protocol
backend/crates/frank-server ──> frank-orchestrator, frank-store, frank-agent, frank-service, frank-protocol, frank-safeio
backend/crates/frank-client ──> frank-protocol
backend/crates/frank-agent-mcp ──> frank-client, frank-protocol
backend/crates/frank-update ──> (leaves)
backend/crates/frank-updater ──> frank-update
backend/crates/frank-release-cli ──> frank-update
backend/crates/frank-pack, frank-compress, frank-safeio, frank-service ──> (leaves)
apps/frank_desktop ──> Flutter + Forui + FlowUI + flutter_scene ──> FrankGateway
```

| Crate | Responsibility | Ported from (historical Caveman source) |
|---|---|---|
| `frank-safeio` | Symlink-safe, size-capped, atomic flag/log IO. Security kernel. | `src/hooks/caveman-config.js:132-346` |
| `frank-pack` | Pack manifest, fragment composition, prompt compiler, level resolution | `skills/caveman/SKILL.md` (content) |
| `frank-state` | Mode state machine + config precedence | `src/hooks/caveman-mode-tracker.js` |
| `frank-ledger` | Session JSONL scan, attribution, net-token accounting, pricing | `src/hooks/caveman-stats.js` |
| `frank-compress` | Deterministic compressor, validator, file classifier | `src/mcp-servers/caveman-shrink/compress.js`, `skills/caveman-compress/scripts/{detect,validate}.py` |
| `frank-target` | Target schema, detection, install planning, JSONC/settings merge, marker fences | `bin/install.js`, `bin/lib/{settings,openclaw}.js` |
| `frank-mcp` | stdio proxy, two std threads | `src/mcp-servers/caveman-shrink/index.js` |
| `frank-protocol` | Versioned wire DTOs, typed IDs, command/event envelopes, API errors, capabilities, terminal frames | *n/a — v1 remote contract* |
| `frank-store` | SQLite WAL source of truth, migrations, idempotency, event log/outbox, audit JSONL exporter | *n/a — v1 persistence* |
| `frank-agent` | Codex/Claude structured runtime adapters, lifecycle, usage telemetry, shell boundary | *n/a — v1 providers* |
| `frank-orchestrator` | Mission/task DAG, scheduler, mailbox, approvals, budgets, leases, delivery invariants | *n/a — v1 orchestrator* |
| `frank-server` | `frankd` authenticated HTTPS/WebSocket API, pairing, fan-out, artifacts, terminals | *n/a — v1 daemon* |
| `frank-client` | Reconnecting HTTPS/WebSocket client, certificate pinning, snapshot/event streams | *n/a — v1 remote client* |
| `frank-agent-mcp` | Local authenticated task-scoped MCP bridge for provider sessions | *n/a — v1 provider bridge* |
| `frank-cli` | binary `frank` — hook fast path, local engine, remote pairing/admin | `bin/install.js` CLI surface |
| `frank-app` | Server-side facade for legacy pack/state/target/ledger operations and v1 paths | *n/a — v1 server facade* |
| `frank-service` | Per-user `frankd` service descriptor rendering, install preview and detection | *n/a — v1 service boundary* |
| `apps/frank_desktop` | Flutter desktop client: permanent navigation, AE chat/composer, Projects drawer, Organization/Team/Ledger surfaces, and a stylized 3D `flutter_scene` floor with an orbit/pan/zoom camera | *n/a — v1 client migration* |
| `frank-update` | Signed update manifest, target selection, staging, compatibility and rollback validation | *n/a — v1 updater contract* |
| `frank-updater` | Small helper binary for verified bundle swap, restart and rollback boundary | *n/a — v1 updater helper* |
| `frank-release-cli` | Release manifest signing/verification and artifact inventory tooling | *n/a — release tooling* |
| `xtask` | `build-packs`, `checksums`, `lint-targets`, `dist` | `.github/workflows/sync-skill.yml` |

**Frank 1.0 is a clean break.** `frankd` is headless and owns SQLite, provider
processes, PTYs, worktrees, Git writes, and the global event sequence. The Flutter
desktop app is always a remote client (including localhost) and never reaches
`frank-app`, the database, or a project filesystem directly. The first Flutter
milestone uses local fixtures behind `FrankGateway`; the next milestone swaps in
the reconnecting `frank-client` transport. The CLI remains available for
screen-reader-first operation and automation.

Service descriptors are rendered by `frank-app::service` and applied by the
CLI; updater code is isolated in `frank-update`/`frank-updater` so daemon and
GUI do not invent separate signing or rollback paths.

### Flutter scene contract

The desktop floor is a stylized low-poly 3D foundation implemented with the
exact pre-1.0 dependency `flutter_scene: 0.23.0` and direct `vector_math`.
`FLTEnableFlutterGPU=true` is permanent in the macOS host. The floor awaits
`Scene.initializeStaticResources()` before constructing its retained scene and
orthographic camera. `SceneView` is wrapped in `IgnorePointer` and the camera is
driven from an `OfficeSceneInteractionSurface` layered above it, so pan, orbit
and zoom never depend on the scene widget receiving pointer events. GPU failure
must show a retryable nonfatal fallback while chat remains usable.

The official `dart run flutter_scene:init --no-skills` setup owns
`apps/frank_desktop/hook/build.dart`, the `flutter_scene_generated/` asset entry,
and its generated-output `.gitignore`. Track future `.glb`/`.fscene`/`.fmat`
sources under `assets/`, never compiled output. Headless Flutter tests assert
the deterministic placeholder and semantics; `moon run frank-desktop:scene-smoke`
is the macOS render gate. The current room has no agents, desks, selection,
status mapping, physics, or live gateway integration -- the camera is
interactive, the scene it looks at is not.

**Deliberately not split further:** no `frank-core` grab bag — `Level`/`LevelId` live
in `frank-pack` because levels are a pack concept. The JSONC parser, marker-fence
editor, and hook-ownership model stay as modules inside `frank-target` — each has one
consumer, splitting buys nothing.

## Contracts that must not drift

- **Pack budget is enforced at compile time, not documented.** `[pack.budget]` in
  `pack.toml` is a hard build failure, not a guideline. See `packs/caveman/pack.toml`.
- **Every hook path must exit 0.** `catch_unwind` around hook bodies; `panic = "unwind"`
  in the release profile (see root `Cargo.toml`). Rust's default panic→101 would be a
  regression vs. the Node original, which always exited 0 even on internal errors.
- **`hook` dispatch happens before clap is constructed.** Peek `argv[1]` in `main()`
  and hand off directly for `session-start` / `user-prompt-submit` / `statusline`.
  Clap's builder allocation is the dominant startup cost in a binary this small.
- **Never sum a measured token count and an estimated one into one unlabeled number.**
  The ledger (`frank-ledger`) always distinguishes measured from estimated, and
  refuses a lifetime verdict below ~20 sessions / 200 turns rather than print noise.
  See the plan's "Net-token ledger" section for the full quantity table.
- **Unattributed tokens are excluded, never guessed.** Ported principle from
  `caveman-stats.js`'s `attributeByMode` — keep the three-basis model
  (`log` / `flag-mtime` / `whole-session`).
- **Flag IO never uses `tempfile`/`atomicwrites`.** Those crates don't do
  `O_NOFOLLOW`. Use `rustix` `openat` against a held directory fd. See
  `frank-safeio`.
- **Native install targets return a plan, they don't perform writes.**
  `NativeTarget::plan()` returns an `InstallPlan` of `Action`s; the executor applies
  it. This is what makes `--dry-run` exact by construction, instead of threaded by
  hand through every call site like the original.
- **The install/verify path fails closed.** A missing or mismatched checksum manifest
  refuses the install. The original's fail-open behavior
  (the historical installer’s checksum branch) is the most serious defect found in the original
  — do not reintroduce it.
- **Historical fixtures are immutable.** The compressor oracle and five original
  Markdown inputs live under `backend/crates/frank-compress/tests/fixtures/`; do not edit
  them to make a differential test pass.
- **Never cut the ledger (M3) to save schedule.** Every other milestone is
  droppable in a pinch; the ledger is the reason this project exists instead of
  being a faster version of the same unverified claim.

## Frank 1.0 checkpoints

1. Refresh Graphify and lock dependency boundaries.
2. Protocol/store/server skeleton: WAL, migrations, pairing, TLS, events, `frankd`.
3. Reconnecting client and remote-only GUI backend.
4. Projects, persistent agents, missions, DAG tasks, broker, approvals, budgets, memory.
5. Structured Codex/Claude adapters, scoped MCP, crash recovery and fake-provider tests.
6. Worktrees, checks, supervisor acceptance, squash merge, push, draft PR delivery.
7. Flutter floor/board/wizards/settings, terminal lease, 3D scene/asset lab, notifications, tray.
8. Per-user service installers, packages, docs, Graphify boundary query, full E2E gates.

No v1 package is released between checkpoints; 0.2.x data/config remains untouched.

## Verification

Run the gate through Moon, not bare `cargo`. Moon resolves the pinned toolchain
from `.prototools` and `.moon/toolchains.yml`; a stray newer `rustc` on `PATH`
will happily build code that the pinned 1.89 clippy rejects, which is how a
lint sat undetected until 2c546c0.

- `moon run frank-rust:verify` — fmt, clippy `-D warnings`, tests, doctests,
  packs, targets, architecture, remote crates.
- `moon run frank-scrutiny:strict` — the mandatory PR gate: the above plus
  coverage against `.config/coverage.toml`, `cargo deny` and `cargo audit`.
  Needs `cargo-nextest`, `cargo-llvm-cov`, `cargo-deny` and `cargo-audit`
  installed, and the `llvm-tools-preview` component that `rust-toolchain.toml`
  declares.
- `moon run frank-desktop:analyze` and `moon run frank-desktop:test`.
- `cargo run -p xtask -- build-packs` then `git diff --exit-code packs/` — compiled
  prompts must match source; a diff here means someone edited generated output by hand.
- `cargo run -p xtask -- lint-targets` — every `targets/*.toml` must parse, use only
  known probe kinds, and expand paths safely.

### Known debt

Recorded so it is not rediscovered from scratch.

- **Coverage.** `.config/coverage.toml` carries four crates that breach their own
  floors, and nine v1 crates registered at a first measured baseline that is in
  places very low (frank-orchestrator 20%, frank-client 12%, frank-agent-mcp
  11%). The sharpest single gap is `frank-cli/src/server_cmd.rs`: 1036 lines of
  v1 remote/pairing/service commands at 0.0% coverage. These floors are raised
  by writing tests, never by editing the numbers.
- **Ledger and Team bypass the gateway.** `LedgerSurface` and `TeamSurface`
  accept injected data but fall back to `fixtureLedgerDashboard()` /
  `fixtureTeamProfiles()` inside the widget, and neither has a bloc, unlike
  chat / shell / projects / organization. When the gateway swaps to the real
  `frank-client` transport these two surfaces will keep rendering fixture data
  with no error. **Must be resolved before checkpoint 3.**
- **The orchestrator's update flow reaches the network.** `update_flow.rs` owns
  its own reqwest client. Moving the fetch/download half into `frank-update`
  would keep that out of the orchestration crate but changes the dependency
  graph pinned by `xtask architecture-check`.
- **`frank-orchestrator` keeps its tests inline.** It is the only large crate
  without a `tests/` directory. Moving the existing 16 tests out would drop the
  crate's measured coverage, since inline test code is measured and `tests/` is
  not; create the directory when new tests are added, not by relocating these.
