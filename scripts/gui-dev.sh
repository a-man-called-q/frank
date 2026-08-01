#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Tauri validates configured resources before starting its dev window. Build a
# local CLI sidecar first so the GUI and hooks use the same executable.
./scripts/prepare-sidecar.sh
exec pnpm --dir apps/frank-gui tauri dev
