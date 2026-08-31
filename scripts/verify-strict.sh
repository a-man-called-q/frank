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
# Keep one workspace report so dependency coverage is not counted repeatedly
# by separate package invocations. The xtask checker applies the per-crate
# floors and uncovered-count ceilings from .config/coverage.toml.
mkdir -p target/llvm-cov
cargo llvm-cov nextest \
  --workspace --all-features --locked --profile ci \
  --jobs 1 \
  --no-cfg-coverage \
  --json \
  --ignore-filename-regex '(^|/)(tests/|[^/]*_tests\.rs$)' \
  --output-path target/llvm-cov/frank-summary.json
cargo run --locked -p xtask -- coverage-check \
  --report target/llvm-cov/frank-summary.json \
  --policy .config/coverage.toml

# cargo-deny 0.18 (still pinned post-1.89 bump; 0.20.2 is available and may
# fix this, but hasn't been verified -- follow-up, not required by the MSRV
# bump itself) cannot read the CVSS-4 records currently in the RustSec
# database, so the advisory job below remains the source of truth while deny
# still gates every ban, license, and source policy.
cargo deny -L error check -c .config/deny.toml bans licenses sources

# These are explicit, reviewable exceptions for advisories with no compatible
# replacement in the currently pinned backend dependency graph. They are not
# a blanket allow-list; revisit each identifier when dependencies change. See
# SECURITY.md for the full review record.
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

# frank-cli is on the hot path of every hook invocation (see AGENTS.md's
# startup-cost contract); keep desktop rendering dependencies out of it.
if cargo tree -e normal -p frank-cli --prefix none --locked | grep -qE '^(iced|wgpu|winit|tray-icon) '; then
  echo 'frank-cli must not link GUI dependencies' >&2
  exit 1
fi

# cargo-fuzz enables AddressSanitizer and -Zbuild-std by default. Those flags
# require nightly Rust, while this mandatory gate is intentionally pinned to
# the stable toolchain in rust-toolchain.toml. The nightly scrutiny task owns
# the fuzz suite and runs it with an explicit nightly toolchain.

printf 'strict verification: passed\n'
