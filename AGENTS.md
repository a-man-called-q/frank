# Frank

Rust implementation inspired by [Caveman](https://github.com/JuliusBrussee/caveman). Frank is the
*engine*; "caveman" is the default persona pack shipped with it. Third parties add
packs (personas) and targets (agent integrations) without forking.

The historical Node.js original was the reference implementation and fixture source
for this rebuild. The working-tree copy is removed after its required compressor
fixtures are vendored under `crates/frank-compress/tests/fixtures/`; the history still
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
frank-cli ──> pack, state, ledger, compress, target, mcp
frank-state ──> frank-pack, frank-safeio
frank-ledger ──> frank-state, frank-safeio
frank-target ──> frank-pack, frank-safeio
frank-mcp ──> frank-compress
frank-pack, frank-compress, frank-safeio ──> (leaves)
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
| `frank-cli` | binary `frank` — argv dispatch, formatting | `bin/install.js` CLI surface |
| `xtask` | `build-packs`, `checksums`, `lint-targets`, `dist` | `.github/workflows/sync-skill.yml` |

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
  Markdown inputs live under `crates/frank-compress/tests/fixtures/`; do not edit
  them to make a differential test pass.
- **Never cut the ledger (M3) to save schedule.** Every other milestone is
  droppable in a pinch; the ledger is the reason this project exists instead of
  being a faster version of the same unverified claim.

## Milestones

M0 skeleton+safety kernel → M1 state machine → M2 Claude Code installer (**v0.1**) →
M3 ledger → M4 deterministic compressor → M5 declarative targets + Codex →
M6 distribution → M7 Antigravity + third-party packs. See the plan doc for demo
criteria per milestone.

## Verification

- `cargo test --workspace`
- `cargo run -p xtask -- build-packs` then `git diff --exit-code packs/` — compiled
  prompts must match source; a diff here means someone edited generated output by hand.
- `cargo run -p xtask -- lint-targets` — every `targets/*.toml` must parse, use only
  known probe kinds, and expand paths safely.
