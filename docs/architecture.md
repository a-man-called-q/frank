# Frank Architecture

Deep technical architecture, design decisions, and implementation details for Frank contributors.

## Design Philosophy

Frank prioritizes **honest measurement** over performance claims. The original Caveman claimed ~1-1.5k tokens/turn based on file size estimates. Frank measures actual token usage from session JSONL and reports:
- **3176 B ≈ 860 tokens** (once per session)  
- **~45 tokens/turn** (reinforcement)

The ledger (`frank-ledger`) is non-negotiable—every other milestone can be dropped, but token accounting is why this project exists.

## Crate Dependency Graph

```
frank-cli ──> pack, state, ledger, compress, target, mcp, app
frank-state ──> frank-pack, frank-safeio
frank-ledger ──> frank-state, frank-safeio
frank-target ──> frank-pack, frank-safeio
frank-mcp ──> frank-compress
frank-app ──> pack, state, ledger, target, safeio
frank-gui-core ──> frank-app
frank-gui ──> frank-app, frank-gui-core, frank-safeio
frank-pack, frank-compress, frank-safeio ──> (no internal deps)
```

### Why This Structure?

**No `frank-core` grab-bag**: Types live where they're conceptually owned:
- `Level`/`LevelId` → `frank-pack` (levels are a pack concept)
- JSONC parser → `frank-target` (single consumer)
- Marker fence editor → `frank-target` (single consumer)

Each module has one consumer. Premature splitting adds complexity without isolation benefits.

## Detailed Crate Responsibilities

### `frank-safeio` - Security Kernel

Provides symlink-safe, size-capped, atomic file operations for flags and logs.

**Key constraint**: Never use `tempfile` or `atomicwrites` crates—they don't support `O_NOFOLLOW`. Instead:
1. Open parent directory with `openat`
2. Hold directory fd
3. All operations through `rustix::fs::openat` with `O_NOFOLLOW`

**Why**: Prevents TOCTOU attacks where an attacker swaps a file for a symlink between check and use.

Ported from `src/hooks/caveman-config.js:132-346`.

### `frank-pack` - Persona System

Handles pack manifests, fragment composition, prompt compilation, and level resolution.

**Pack budget enforcement**: `[pack.budget]` in `pack.toml` causes **compile-time failure**. This is enforced during `build-packs` xtask, not at runtime. Example:

```toml
[pack.budget]
max_tokens = 1000
max_bytes = 4096
```

If compiled prompt exceeds limits, build fails. This prevents token bloat at the source.

**Level system**: Hierarchical activation (e.g., `base` → `verbose` → `debug`). Each level inherits and extends parent fragments.

Ported from `skills/caveman/SKILL.md` content structure.

### `frank-state` - Mode State Machine

Tracks agent mode state with three attribution bases:

1. **Log basis**: Reads explicit mode entries from state log
2. **Flag mtime basis**: Infers mode from flag file modification times
3. **Whole-session basis**: Attributes entire session to one mode

**Config precedence**: workspace > user > defaults. Never merge arrays—later config completely replaces earlier.

Ported from `src/hooks/caveman-mode-tracker.js`.

### `frank-ledger` - Token Accounting

Scans session JSONL files and attributes tokens to Frank levels.

**Critical rule**: Never sum measured + estimated tokens into one unlabeled number. The ledger distinguishes:
- `measured`: From JSONL `input_tokens`, `cache_creation_input_tokens`
- `estimated`: From file size heuristics when JSONL unavailable

**Verdict threshold**: Refuses lifetime verdict below ~20 sessions / 200 turns. Below this, noise dominates signal.

**Attribution algorithm**:
1. Parse JSONL chronologically
2. Load state log (mode changes)
3. For each turn, attribute tokens to active mode at turn timestamp
4. Unattributed tokens are excluded (never guessed)

Ported from `src/hooks/caveman-stats.js` but with actual JSONL token reading.

### `frank-compress` - Deterministic Compression

Compresses Markdown/text while preserving structure. Must be deterministic: same input → same output.

**Validation**: Five immutable fixtures in `crates/frank-compress/tests/fixtures/` from original Caveman. Tests compare against these oracles. **Never edit fixtures to pass tests**.

**File classification**: Detects text vs. binary, classifies Markdown structure (headers, code blocks, lists) to guide compression strategy.

Ported from:
- `src/mcp-servers/caveman-shrink/compress.js`
- `skills/caveman-compress/scripts/{detect,validate}.py`

### `frank-target` - Integration Layer

Handles AI assistant integrations (Claude Code, Codex, Cline).

**Install planning pattern**:
```rust
impl NativeTarget {
    fn plan(&self) -> Result<InstallPlan> {
        // Returns Actions, doesn't execute
    }
}
```

Executor applies plan separately. This makes `--dry-run` exact by construction—no threading boolean flags through call sites.

**Fail-closed principle**: Missing/mismatched checksum manifest → refuse install. The original's fail-open was a critical defect.

**Marker fences**: Wraps injected config with comments like:
```jsonc
// BEGIN FRANK MANAGED
{ "hook": "session-start" }
// END FRANK MANAGED
```

Allows safe removal and updates without corrupting user config.

Ported from `bin/install.js`, `bin/lib/{settings,openclaw}.js`.

### `frank-mcp` - MCP Server

stdio-based Model Context Protocol server. Runs on two std threads:
- Thread 1: stdin → message parsing
- Thread 2: stdout ← response writing

Exposes compression tools to AI assistants.

Ported from `src/mcp-servers/caveman-shrink/index.js`.

### `frank-cli` - Binary

Main `frank` binary. Handles argv dispatch and formatting.

**Performance optimization**: Hook dispatch happens **before clap construction**. Peek `argv[1]` in `main()`:
- `session-start` | `user-prompt-submit` | `statusline` → direct dispatch
- Other commands → build clap parser

**Why**: clap's builder allocation dominates startup cost. Hook invocations are latency-sensitive (AI waits on them).

**Exit code contract**: All hook paths exit 0, even on panic. Uses `catch_unwind` + `panic = "unwind"` in release profile. Rust's default panic→101 would break AI integrations expecting exit 0.

Ported from `bin/install.js` CLI surface.

### `frank-app` - Shared Service

Application service layer used by CLI, hooks, and GUI. Provides unified interface to pack/state/ledger operations.

New addition (not in original Caveman).

### `frank-gui-core` - GUI State & View Layer

Backend-agnostic core of the desktop control panel: `Model`, `Message`, the pure `reduce()` state machine, and the iced `view()` layer (shell + 4 pages: overview, personas, integrations, settings).

**Pure reducer, no side effects**: `update()` never builds an `iced::Task` directly. `reduce(&mut Model, Message) -> Effect` is a plain state machine — easy to unit-test and reach 100% mutation coverage on, since every branch lives here. A separate `interpret(Effect, &Platform, &Backend) -> Task<Message>` is a flat match with no logic, generic over a `trait Backend` (the 7 `FrankService` entry points) and a `trait Platform` (tray/autostart/file-picker), which is what keeps `frank-app` and `tray-icon`/`auto-launch`/`rfd` out of the coverage-gated core.

**Testing**: unit tests over `reduce()`, plus `iced_test::simulator` headless UI tests that render the real `view()` output (tiny-skia, no GPU/display) and assert on `.click()`/`.find()`/`.into_messages()`.

New addition (not in original Caveman — replaces the Tauri 2 + React `apps/frank-gui` frontend).

### `frank-gui` - Desktop Binary

The `frank-gui` binary: an `iced::daemon` shell around `frank-gui-core`. Owns everything that needs a real OS event loop and can't be unit-tested — tray icon/menu (`tray-icon` + `muda`), single-instance locking (via a `frank-safeio`-owned `flock`/`LockFileEx` lock, not a third-party lock crate), window lifecycle (`--hidden` launch, close-to-tray, reopen from tray), autostart (`auto-launch`, with macOS `.app`-bundle path resolution), and CLI-path resolution (`frank` ships as a sibling binary in every platform bundle).

Deliberately excluded from the Rust coverage report (`--exclude frank-gui` in `scripts/verify-strict.sh`) — it's a thin platform shell that needs a real tray/event loop to exercise; `scripts/native-smoke.sh` is its acceptance test instead. Packaged via `cargo-packager` (dmg/msi/deb) and `cargo-generate-rpm` (rpm — `cargo-packager` has no `PackageFormat::Rpm` variant).

New addition (not in original Caveman — replaces the Tauri shell `apps/frank-gui/src-tauri`).

### `xtask` - Build Tasks

Cargo xtask for:
- `build-packs`: Compiles pack fragments into final prompts
- `checksums`: Generates/verifies integrity manifests
- `lint-targets`: Validates target TOML files
- `dist`: Prepares release artifacts

**Verification**: `build-packs` followed by `git diff --exit-code packs/` ensures nobody hand-edited generated files.

Ported from `.github/workflows/sync-skill.yml`.

## Critical Contracts

### 1. Pack Budget is Enforced at Compile Time

Not documented in comments—enforced by build. `pack.toml` exceeds budget → build fails.

### 2. Hooks Always Exit 0

Even on panic/error. Required for AI integration stability.

### 3. Hook Dispatch Before Clap

Latency-sensitive paths bypass parser construction.

### 4. Measured ≠ Estimated Tokens

Ledger never mixes these. Always labeled separately.

### 5. Unattributed Tokens Excluded

Never guessed. Attribution uses three-basis model.

### 6. Flag IO Uses O_NOFOLLOW

No `tempfile`/`atomicwrites`. Always `rustix::openat` with symlink protection.

### 7. Install Returns Plan

Targets return `InstallPlan`, don't write directly. Executor applies.

### 8. Install Fails Closed

Bad checksum → refuse. No silent fallback.

### 9. Fixtures Are Immutable

`crates/frank-compress/tests/fixtures/` never edited for test compliance.

### 10. Ledger is Non-Negotiable

M3 cannot be cut. It's the project's raison d'être.

## Build System Details

### Cargo Workspace

Standard Rust workspace. `cargo` is source of truth for builds.

### Moon Orchestration

Moon 2.4.5 (via proto) orchestrates task graph but doesn't replace cargo. Used for:
- Cross-crate task dependencies
- Release packaging
- Verification gates

**Verification gates**:
```bash
moon run :verify        # Fast: tests + clippy + fmt
moon run :verify-strict # + coverage + audit + cargo-deny
```

### Desktop GUI (iced)

Desktop app (optional), native Rust throughout — no Node/pnpm toolchain required anywhere in this repo:
- Framework: [iced](https://iced.rs) 0.14, `iced::daemon` (tray-first, zero windows by default)
- Tray: `tray-icon` + `muda`
- Autostart: `auto-launch`
- File picker: `rfd`

`frank-cli` never links any of these — `frank` and `frank-gui` are separate
binaries sharing the `frank-app` facade, so hook invocations never pay for
GPU/windowing startup cost. `scripts/verify-strict.sh` enforces this
mechanically with a `cargo tree -p frank-cli` check.

Development:
```bash
cargo run --locked -p frank-gui
```

Release:
```bash
moon run release:bundle  # → .dmg / .msi / .deb / .rpm
```

MVP packages unsigned. Signing tracked.

Verify host-native packages after building them:

```bash
FRANK_GUI_BINARY=target/release/frank-gui moon run release:native-smoke
```

Validates: hidden launch, single-instance hand-off, clean quit.

**Accessibility**: iced 0.14 has no accessibility tree yet. `frank`, the CLI,
is the screen-reader-native way to perform every operation the GUI exposes,
and every installer places both binaries side by side — see `SECURITY.md`.

## Historical Context

### Why Rebuild in Rust?

Original Caveman (Node.js) had three issues:

1. **Unmeasured claims**: Token cost estimated from file size, never validated
2. **Stats didn't read tokens**: `caveman-stats.js` parsed JSONL but ignored `input_tokens`
3. **Fail-open installer**: Missing checksums → proceed anyway

Frank fixes these while maintaining feature parity.

### What Changed?

**Architecture**: Added `frank-app` service layer, stricter separation. The
desktop control panel itself moved from Tauri 2 + React onto native Rust +
iced 0.14 (`frank-gui-core` + `frank-gui`), removing the entire Node/pnpm/
webview toolchain from the repo.

**Security**: Symlink protection, fail-closed installs, immutable fixtures.

**Measurement**: Actual token counting from JSONL, clear measured vs. estimated distinction.

**Performance**: Rust binary, early hook dispatch, compile-time budget enforcement.

### What Stayed?

**Persona system**: Levels, packs, fragment composition.

**Integration model**: Hook-based injection, marker fences.

**Compression**: Deterministic algorithm, same fixtures.

## Milestone Sequence

- **M0**: Skeleton + `frank-safeio` security kernel
- **M1**: State machine (`frank-state`)
- **M2**: Claude Code installer (`frank-target`) → **v0.1 release**
- **M3**: Ledger (`frank-ledger`) → honest accounting
- **M4**: Compressor (`frank-compress`)
- **M5**: Declarative targets + Codex/Cline support
- **M6**: Distribution (install script, packages)
- **M7**: Antigravity + third-party pack ecosystem

Each milestone has demo criteria in `buat-plan-untuk-bangun-soft-marshmallow.md`.

## References

- **Contributing**: See [`CONTRIBUTING.md`](CONTRIBUTING.md) for development workflow and guidelines
- **Original Caveman**: https://github.com/JuliusBrussee/caveman
- **Design plan**: `buat-plan-untuk-bangun-soft-marshmallow.md`
- **Pack authoring**: `docs/pack-authoring.md`
- **Roadmap**: `roadmap.md`
