#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# The Rust side now ships as a headless backend. The Flutter desktop client is
# built separately from apps/frank_desktop, so this script only prepares the
# backend binaries consumed by `xtask dist`.
target="${FRANK_TARGET:-}"
if [[ -z "$target" ]]; then
  target="$(rustc -vV | awk '$1 == "host:" { print $2 }')"
fi
[[ -n "$target" ]] || { echo 'unable to determine Rust host target' >&2; exit 2; }

packages=(-p frank-cli -p frank-server -p frank-updater)

if [[ "$target" == 'universal-apple-darwin' ]]; then
  [[ "$(uname -s)" == 'Darwin' ]] || {
    echo 'universal macOS builds require macOS' >&2
    exit 2
  }
  cargo build --locked --release "${packages[@]}" --target x86_64-apple-darwin
  cargo build --locked --release "${packages[@]}" --target aarch64-apple-darwin
  mkdir -p "$root/target/release"
  for binary in frank frankd frank-updater; do
    x86="$root/target/x86_64-apple-darwin/release/$binary"
    arm="$root/target/aarch64-apple-darwin/release/$binary"
    [[ -f "$x86" && -f "$arm" ]] || {
      echo "missing architecture-specific $binary" >&2
      exit 1
    }
    lipo -create "$x86" "$arm" -output "$root/target/release/$binary"
    chmod 0755 "$root/target/release/$binary"
  done
  printf 'built universal backend binaries for macOS\n'
else
  cargo build --locked --release "${packages[@]}" --target "$target"
  mkdir -p "$root/target/release"
  suffix=""
  [[ "$target" == *windows* ]] && suffix='.exe'
  for binary in frank frankd frank-updater; do
    src="$root/target/$target/release/$binary$suffix"
    [[ -f "$src" ]] || { echo "missing target binary: $src" >&2; exit 1; }
    install -m 0755 "$src" "$root/target/release/$binary$suffix"
  done
fi

if [[ "${FRANK_BUILD_FLUTTER:-0}" == '1' ]]; then
  command -v flutter >/dev/null 2>&1 || {
    echo 'FRANK_BUILD_FLUTTER=1 requires Flutter on PATH' >&2
    exit 2
  }
  (
    cd apps/frank_desktop
    flutter pub get
    flutter build macos --release
  )
fi

printf 'backend release build complete for %s\n' "$target"
