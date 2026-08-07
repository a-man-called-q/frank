#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

required=(cargo-nextest cargo-llvm-cov cargo-deny cargo-audit)
missing=()
for tool in "${required[@]}"; do
  command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if ((${#missing[@]})); then
  printf 'strict verification requires missing tools: %s\n' "${missing[*]}" >&2
  printf 'Install them through the pinned developer toolchain before running :verify-strict.\n' >&2
  exit 2
fi

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo nextest run --workspace --locked --profile ci
cargo test --workspace --doc --locked
# frank-gui (crates/frank-gui) is covered by its platform smoke job --
# native-smoke.sh -- not by this Rust unit-test coverage run. It is a thin
# platform shell (tray/window lifecycle/single-instance) that needs a real
# event loop and OS tray to exercise; its actual logic lives in
# frank-gui-core, which stays in this report. Also exclude standalone test
# source files so tests cannot inflate production coverage. The xtask checker
# applies the per-crate floors and uncovered-count ceilings from
# .config/coverage.toml; keeping one report avoids dependency coverage being
# counted repeatedly by separate package invocations.
mkdir -p target/llvm-cov
cargo llvm-cov nextest \
  --workspace --exclude frank-gui --all-features --locked --profile ci \
  --jobs 1 \
  --no-cfg-coverage \
  --json \
  --ignore-filename-regex '(^|/)(tests/|[^/]*_tests\.rs$)' \
  --output-path target/llvm-cov/frank-summary.json
cargo run --locked -p xtask -- coverage-check \
  --report target/llvm-cov/frank-summary.json \
  --policy .config/coverage.toml

# cargo-deny 0.18 (still pinned post-1.88 bump; 0.20.2 is available and may
# fix this, but hasn't been verified -- follow-up, not required by the MSRV
# bump itself) cannot read the CVSS-4 records currently in the RustSec
# database, so the advisory job below remains the source of truth while deny
# still gates every ban, license, and source policy.
cargo deny -L error check -c .config/deny.toml bans licenses sources

# These are explicit, reviewable exceptions. They are not a blanket
# allow-list: every identifier is tied to a concrete upstream constraint and
# must be revisited when the GUI's dependency graph changes. See SECURITY.md
# for the full review record.
#
# 9 are for the gtk-rs 0.18 GTK3 bindings (atk, atk-sys, gdk, gdk-sys, glib,
# gtk, gtk-sys, gtk3-macros, and gtk3-macros's own proc-macro-error
# dependency). These are NOT a Tauri leftover -- Tauri was fully removed at
# M-5 of the frank-gui -> iced migration, and gtk-rs is still here because
# tray-icon/muda's own Linux tray backend (libappindicator) links it
# directly. They will not disappear until that Linux tray implementation
# changes.
#
# 2 are for the iced 0.14 dependency graph added by that same migration's
# M-3: `paste` (unmaintained, via metal -> wgpu-hal -> wgpu, iced's GPU
# renderer) and `ttf-parser` (unmaintained, via cosmic-text/winit's glyph
# rendering).
#
# All 11 are "unmaintained"/"unsound" advisories against crates with no
# compatible replacement in the currently pinned dependency graph, not
# advisories Frank's own code path is exposed to.
cargo audit --deny warnings \
  --ignore RUSTSEC-2024-0370 \
  --ignore RUSTSEC-2024-0412 \
  --ignore RUSTSEC-2024-0413 \
  --ignore RUSTSEC-2024-0415 \
  --ignore RUSTSEC-2024-0416 \
  --ignore RUSTSEC-2024-0418 \
  --ignore RUSTSEC-2024-0419 \
  --ignore RUSTSEC-2024-0420 \
  --ignore RUSTSEC-2024-0429 \
  --ignore RUSTSEC-2024-0436 \
  --ignore RUSTSEC-2026-0192
cargo run --locked -p xtask -- build-packs
git diff --exit-code -- packs/
cargo run --locked -p xtask -- lint-targets
cargo run --locked -p xtask -- version-check
cargo run --locked -p xtask -- architecture-check

# frank-cli is on the hot path of every hook invocation (see CLAUDE.md's
# startup-cost contract); xtask's architecture-check only tracks frank-*
# crates and cannot see this, so it's enforced here directly. The GUI is a
# separate binary precisely so wgpu/iced never link into the binary that runs
# every turn.
if cargo tree -e normal -p frank-cli --prefix none --locked | grep -qE '^(iced|wgpu|winit|tray-icon) '; then
  echo 'frank-cli must not link GUI dependencies' >&2
  exit 1
fi

# cargo-fuzz enables AddressSanitizer and -Zbuild-std by default. Those flags
# require nightly Rust, while this mandatory gate is intentionally pinned to
# the stable toolchain in rust-toolchain.toml. The nightly scrutiny task owns
# the fuzz suite and runs it with an explicit nightly toolchain.

printf 'strict verification: passed\n'
