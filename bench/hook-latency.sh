#!/usr/bin/env bash
# Manual latency measurement for Frank's session-start hook. An optional
# external legacy hook can be supplied with FRANK_LEGACY_NODE_HOOK if a
# separately checked-out Caveman reference is available.
#
# hyperfine isn't installed on this machine, so this does the same thing by
# hand: N iterations, wall-clock via `date +%s%N`, mean + min + max.
# Superseded by a real `hyperfine` run in CI at M6 — see AGENTS.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
N="${1:-200}"

FRANK_BIN="$ROOT/target/release/frank"
LEGACY_NODE_HOOK="${FRANK_LEGACY_NODE_HOOK:-}"

FRANK_DIR="$(mktemp -d)"
NODE_DIR="$(mktemp -d)"
trap 'rm -rf "$FRANK_DIR" "$NODE_DIR"' EXIT

if [[ -n "$LEGACY_NODE_HOOK" && ! -f "$LEGACY_NODE_HOOK" ]]; then
  echo "FRANK_LEGACY_NODE_HOOK does not point to a file: $LEGACY_NODE_HOOK" >&2
  exit 2
fi

run_n() {
  local label="$1"; shift
  local -a times=()
  for _ in $(seq 1 "$N"); do
    local start end
    start=$(date +%s%N)
    "$@" > /dev/null
    end=$(date +%s%N)
    times+=( $(( (end - start) / 1000 )) ) # microseconds
  done
  local sum=0 min=${times[0]} max=${times[0]}
  for t in "${times[@]}"; do
    sum=$((sum + t))
    if (( t < min )); then min=$t; fi
    if (( t > max )); then max=$t; fi
  done
  local mean=$((sum / N))
  printf '%-28s n=%-4d mean=%6d us   min=%6d us   max=%6d us\n' "$label" "$N" "$mean" "$min" "$max"
}

echo "warming up..."
CLAUDE_CONFIG_DIR="$FRANK_DIR" "$FRANK_BIN" on full > /dev/null
for _ in $(seq 1 5); do
  CLAUDE_CONFIG_DIR="$FRANK_DIR" "$FRANK_BIN" hook session-start > /dev/null
  if [[ -n "$LEGACY_NODE_HOOK" ]]; then
    CLAUDE_CONFIG_DIR="$NODE_DIR" CAVEMAN_DEFAULT_MODE=full node "$LEGACY_NODE_HOOK" > /dev/null
  fi
done

echo "running $N iterations each..."
run_n "frank hook session-start" env CLAUDE_CONFIG_DIR="$FRANK_DIR" "$FRANK_BIN" hook session-start
if [[ -n "$LEGACY_NODE_HOOK" ]]; then
  run_n "external legacy hook" env CLAUDE_CONFIG_DIR="$NODE_DIR" CAVEMAN_DEFAULT_MODE=full node "$LEGACY_NODE_HOOK"
else
  echo "HOLD(M6): set FRANK_LEGACY_NODE_HOOK for a cross-implementation comparison; measuring Frank only."
fi
