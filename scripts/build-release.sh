#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

./scripts/prepare-sidecar.sh
if [[ "${FRANK_SKIP_GUI:-0}" != "1" ]]; then
  pnpm --dir apps/frank-gui build

  bundles="${FRANK_BUNDLES:-}"
  if [[ -z "$bundles" ]]; then
    case "$(uname -s)" in
      Darwin) bundles='dmg' ;;
      Linux) bundles='deb,rpm' ;;
      MINGW*|MSYS*|CYGWIN*) bundles='msi' ;;
      *) echo 'set FRANK_BUNDLES for this platform' >&2; exit 2 ;;
    esac
  fi

  tauri_args=(build --bundles "$bundles")
  if [[ "${FRANK_SIDECAR_TARGET:-}" == 'universal-apple-darwin' ]]; then
    tauri_args+=(--target universal-apple-darwin)
  fi
  pnpm --dir apps/frank-gui tauri "${tauri_args[@]}"

  mkdir -p "$root/dist"
  if [[ "${FRANK_SIDECAR_TARGET:-}" == 'universal-apple-darwin' ]]; then
    bundle_root="$root/apps/frank-gui/src-tauri/target/universal-apple-darwin/release/bundle"
  else
    bundle_root="$root/apps/frank-gui/src-tauri/target/release/bundle"
  fi
  IFS=',' read -r -a bundle_kinds <<< "$bundles"
  for kind in "${bundle_kinds[@]}"; do
    case "$kind" in
      dmg|msi|deb|rpm)
        shopt -s nullglob
        files=("$bundle_root/$kind"/*)
        shopt -u nullglob
        ((${#files[@]} > 0)) || { echo "Tauri produced no .$kind bundle" >&2; exit 1; }
        cp -f "${files[@]}" "$root/dist/"
        ;;
      *) echo "unsupported bundle kind: $kind" >&2; exit 2 ;;
    esac
  done
fi

printf 'release build prerequisites complete\n'
