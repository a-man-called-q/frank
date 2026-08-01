#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

command -v cargo-fuzz >/dev/null 2>&1 || { echo 'cargo-fuzz is required for nightly scrutiny' >&2; exit 2; }
command -v cargo-mutants >/dev/null 2>&1 || { echo 'cargo-mutants is required for nightly scrutiny' >&2; exit 2; }

cargo test --workspace --all-features --locked
cargo fuzz run pack_manifest -- -max_total_time=900
cargo fuzz run intent_parser -- -max_total_time=900
cargo fuzz run session_jsonl -- -max_total_time=900
cargo fuzz run ledger_attribution -- -max_total_time=900
cargo fuzz run compressor -- -max_total_time=900
cargo fuzz run jsonc_settings -- -max_total_time=900
cargo fuzz run marker_fences -- -max_total_time=900
cargo fuzz run tauri_payloads -- -max_total_time=900
./scripts/mutation-gate.sh

if command -v cargo-miri >/dev/null 2>&1; then
  cargo miri test -p frank-pack -p frank-state -p frank-ledger
fi
if command -v cargo-asan >/dev/null 2>&1; then
  cargo asan test --workspace
fi

printf 'nightly scrutiny: passed\n'
