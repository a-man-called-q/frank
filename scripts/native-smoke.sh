#!/usr/bin/env bash
set -euo pipefail

# Native smoke is intentionally separate from Playwright: it exercises the
# actual Tauri process boundary (tray-first hidden launch and the
# single-instance hand-off). A package/VM job supplies the built executable via
# FRANK_GUI_BINARY; silently skipping would turn this into a decorative test.
binary="${FRANK_GUI_BINARY:-}"
[[ -n "$binary" ]] || { echo 'FRANK_GUI_BINARY must point at the built Frank GUI executable' >&2; exit 2; }
[[ -x "$binary" ]] || { echo "GUI executable is missing or not executable: $binary" >&2; exit 1; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/frank-native-smoke.XXXXXX")"
trap 'if [[ -n "${second_pid:-}" ]] && kill -0 "$second_pid" 2>/dev/null; then kill "$second_pid" 2>/dev/null || true; fi; if [[ -n "${first_pid:-}" ]] && kill -0 "$first_pid" 2>/dev/null; then kill "$first_pid" 2>/dev/null || true; fi; rm -rf "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/home" "$tmp/config" "$tmp/data"

env_args=(
  "HOME=$tmp/home"
  "XDG_CONFIG_HOME=$tmp/config"
  "XDG_DATA_HOME=$tmp/data"
  "CLAUDE_CONFIG_DIR=$tmp/home/.claude"
)
mkdir -p "$tmp/home/.claude"

launcher=()
if [[ "$(uname -s)" == Linux* ]] && command -v xvfb-run >/dev/null 2>&1; then
  launcher=(xvfb-run -a)
fi

# Bash with `set -u` treats an empty `${array[@]}` as an unset expansion on
# some macOS system versions. Build one complete argv instead of expanding an
# empty optional array inline.
launch_args=(env "${env_args[@]}")
if ((${#launcher[@]})); then
  launch_args+=("${launcher[@]}")
fi
launch_args+=("$binary" --hidden)

"${launch_args[@]}" >"$tmp/first.log" 2>&1 &
first_pid=$!
for _ in {1..30}; do
  if kill -0 "$first_pid" 2>/dev/null; then break; fi
  sleep 0.2
done
kill -0 "$first_pid" 2>/dev/null || {
  cat "$tmp/first.log" >&2 || true
  echo 'hidden GUI launch exited before becoming ready' >&2
  exit 1
}

# The second invocation must hand off to the first process and exit; two
# independent GUI writers would violate the plan/apply concurrency contract.
"${launch_args[@]}" >"$tmp/second.log" 2>&1 &
second_pid=$!
for _ in {1..50}; do
  if ! kill -0 "$second_pid" 2>/dev/null; then break; fi
  sleep 0.2
done
if kill -0 "$second_pid" 2>/dev/null; then
  cat "$tmp/second.log" >&2 || true
  echo 'second GUI launch did not hand off to the existing instance' >&2
  exit 1
fi

kill "$first_pid" 2>/dev/null || true
wait "$first_pid" 2>/dev/null || true
printf 'native smoke: hidden launch, single-instance hand-off, and clean quit passed\n'
