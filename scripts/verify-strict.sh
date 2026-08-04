#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

required=(cargo-nextest cargo-llvm-cov cargo-deny cargo-audit cargo-mutants)
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
# The native Tauri crate is covered by its platform smoke jobs; its build script
# places a sidecar shell/executable in Cargo's target directory, which LLVM
# cannot instrument as Rust. Keep the workspace report focused on Rust crates
# and exclude standalone test source files so tests cannot inflate production
# coverage. The xtask checker applies the per-crate floors and uncovered-count
# ceilings from .config/coverage.toml; keeping one report avoids dependency coverage
# being counted repeatedly by separate package invocations.
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

# cargo-deny 0.18 is the Rust-1.85-compatible release. Its advisory parser
# cannot read the CVSS-4 records currently in the RustSec database, so the
# advisory job below remains the source of truth while deny still gates every
# ban, license, and source policy.
cargo deny -L error check -c .config/deny.toml bans licenses sources

# These are explicit, reviewable exceptions for the pinned Rust 1.85/Tauri 2
# graph. They are not a blanket allow-list: every identifier is tied to a
# concrete upstream constraint and must be revisited when either toolchain is
# upgraded. The three 2026 IDs are transitive Tauri advisories whose patched
# releases require Rust 1.88; the release checklist must keep these visible and
# block a release if the exception is not renewed during dependency review.
cargo audit --deny warnings \
  --ignore RUSTSEC-2024-0370 \
  --ignore RUSTSEC-2024-0411 \
  --ignore RUSTSEC-2024-0412 \
  --ignore RUSTSEC-2024-0413 \
  --ignore RUSTSEC-2024-0414 \
  --ignore RUSTSEC-2024-0415 \
  --ignore RUSTSEC-2024-0416 \
  --ignore RUSTSEC-2024-0417 \
  --ignore RUSTSEC-2024-0418 \
  --ignore RUSTSEC-2024-0419 \
  --ignore RUSTSEC-2024-0420 \
  --ignore RUSTSEC-2024-0429 \
  --ignore RUSTSEC-2025-0075 \
  --ignore RUSTSEC-2025-0080 \
  --ignore RUSTSEC-2025-0081 \
  --ignore RUSTSEC-2025-0098 \
  --ignore RUSTSEC-2025-0100 \
  --ignore RUSTSEC-2026-0009 \
  --ignore RUSTSEC-2026-0194 \
  --ignore RUSTSEC-2026-0195
cargo run --locked -p xtask -- build-packs
git diff --exit-code -- packs/
cargo run --locked -p xtask -- lint-targets
cargo run --locked -p xtask -- version-check
cargo run --locked -p xtask -- architecture-check

if [[ -f pnpm-lock.yaml ]]; then
  pnpm install --frozen-lockfile
fi
if [[ -f apps/frank-gui/package.json ]]; then
  pnpm --dir apps/frank-gui lint
  pnpm --dir apps/frank-gui typecheck
  pnpm --dir apps/frank-gui test --coverage
  pnpm --dir apps/frank-gui exec playwright install --with-deps chromium
  pnpm gui:e2e
  pnpm audit --prod --audit-level high
fi

# cargo-fuzz enables AddressSanitizer and -Zbuild-std by default. Those flags
# require nightly Rust, while this mandatory gate is intentionally pinned to
# the stable toolchain in rust-toolchain.toml. The nightly scrutiny task owns
# the fuzz suite and runs it with an explicit nightly toolchain.

./scripts/mutation-gate.sh

printf 'strict verification: passed\n'
