#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# Renamed from FRANK_SIDECAR_TARGET: there is no separate "sidecar" concept
# anymore (see the frank-gui -> iced migration plan) -- frank and frank-gui
# are just two binaries built for the same target and bundled together.
target="${FRANK_GUI_TARGET:-}"
if [[ -z "$target" ]]; then
  target="$(rustc -vV | awk '$1 == "host:" { print $2 }')"
fi
[[ -n "$target" ]] || { echo 'unable to determine Rust host target' >&2; exit 2; }

if [[ "$target" == 'universal-apple-darwin' ]]; then
  [[ "$(uname -s)" == 'Darwin' ]] || { echo 'universal macOS builds require macOS' >&2; exit 2; }
  cargo build --locked --release -p frank-cli -p frank-gui --target x86_64-apple-darwin
  cargo build --locked --release -p frank-cli -p frank-gui --target aarch64-apple-darwin
  mkdir -p "$root/target/release"
  for binary in frank frank-gui; do
    x86="$root/target/x86_64-apple-darwin/release/$binary"
    arm="$root/target/aarch64-apple-darwin/release/$binary"
    [[ -f "$x86" && -f "$arm" ]] || { echo "missing architecture-specific $binary" >&2; exit 1; }
    lipo -create "$x86" "$arm" -output "$root/target/release/$binary"
    chmod 0755 "$root/target/release/$binary"
  done
  printf 'built universal frank + frank-gui\n'
else
  cargo build --locked --release -p frank-cli -p frank-gui --target "$target"
  # cargo-packager's `binaries` list in crates/frank-gui/Cargo.toml points at
  # the fixed, target-less `target/release/` path (matching how every other
  # cargo invocation in this repo already works) rather than a per-target
  # path, so a plain `--target` build has to be copied there too.
  if [[ "$target" != "$(rustc -vV | awk '$1 == "host:" { print $2 }')" ]]; then
    mkdir -p "$root/target/release"
    for binary in frank frank-gui; do
      src="$root/target/$target/release/$binary"
      [[ -f "$src" ]] && install -m 0755 "$src" "$root/target/release/$binary"
    done
  fi
fi

if [[ "${FRANK_SKIP_GUI:-0}" == "1" ]]; then
  printf 'release build prerequisites complete (GUI packaging skipped)\n'
  exit 0
fi

command -v cargo-packager >/dev/null 2>&1 || {
  echo 'cargo-packager is required (cargo install cargo-packager --locked)' >&2
  exit 2
}

mkdir -p "$root/dist"

bundles="${FRANK_BUNDLES:-}"
if [[ -z "$bundles" ]]; then
  case "$(uname -s)" in
    Darwin) bundles='dmg' ;;
    Linux) bundles='deb,rpm' ;;
    MINGW*|MSYS*|CYGWIN*) bundles='msi' ;;
    *) echo 'set FRANK_BUNDLES for this platform' >&2; exit 2 ;;
  esac
fi

IFS=',' read -r -a bundle_kinds <<< "$bundles"
packager_formats=()
want_rpm=0
for kind in "${bundle_kinds[@]}"; do
  case "$kind" in
    dmg|deb) packager_formats+=("$kind") ;;
    msi) packager_formats+=(wix) ;;
    rpm)
      # cargo-packager's PackageFormat has no Rpm variant (verified against
      # its own source during M-6) -- cargo-generate-rpm builds it
      # separately below instead.
      want_rpm=1
      ;;
    *) echo "unsupported bundle kind: $kind" >&2; exit 2 ;;
  esac
done

if ((${#packager_formats[@]} > 0)); then
  formats_arg=$(IFS=,; echo "${packager_formats[*]}")
  cargo packager --release --formats "$formats_arg" -p frank-gui
fi

if ((want_rpm)); then
  command -v cargo-generate-rpm >/dev/null 2>&1 || {
    echo 'cargo-generate-rpm is required for rpm (cargo install cargo-generate-rpm --locked)' >&2
    exit 2
  }
  cargo generate-rpm -p crates/frank-gui
  shopt -s nullglob
  rpms=("$root/target/generate-rpm/"*.rpm)
  shopt -u nullglob
  ((${#rpms[@]} > 0)) || { echo 'cargo-generate-rpm produced no .rpm file' >&2; exit 1; }
  cp -f "${rpms[@]}" "$root/dist/"
fi

printf 'release build complete: %s\n' "$bundles"
