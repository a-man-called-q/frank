#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

target="${FRANK_SIDECAR_TARGET:-}"
if [[ -z "$target" ]]; then
  target="$(rustc -vV | awk '$1 == "host:" { print $2 }')"
fi
[[ -n "$target" ]] || { echo 'unable to determine Rust host target' >&2; exit 2; }

if [[ "$target" == 'universal-apple-darwin' ]]; then
  [[ "$(uname -s)" == 'Darwin' ]] || { echo 'universal macOS sidecars require macOS' >&2; exit 2; }
  cargo build --locked --release -p frank-cli --target x86_64-apple-darwin
  cargo build --locked --release -p frank-cli --target aarch64-apple-darwin
  x86="$root/target/x86_64-apple-darwin/release/frank"
  arm="$root/target/aarch64-apple-darwin/release/frank"
  destination="$root/apps/frank-gui/src-tauri/binaries/frank-sidecar-${target}"
  [[ -f "$x86" && -f "$arm" ]] || { echo 'missing architecture-specific CLI binary' >&2; exit 1; }
  lipo -create "$x86" "$arm" -output "$destination"
  chmod 0755 "$destination"
  mkdir -p "$root/apps/frank-gui/src-tauri/packaging"
  install -m 0755 "$destination" "$root/apps/frank-gui/src-tauri/packaging/frank"
  install -m 0755 "$destination" "$root/apps/frank-gui/src-tauri/packaging/frank.exe"
  printf 'prepared universal Frank sidecar %s\n' "$destination"
  exit 0
fi

case "$target" in
  *windows*)
    binary_name='frank.exe'
    sidecar_name="frank-sidecar-${target}.exe"
    ;;
  *)
    binary_name='frank'
    sidecar_name="frank-sidecar-${target}"
    ;;
esac

cargo build --locked --release -p frank-cli --target "$target"
source="$root/target/$target/release/$binary_name"
destination="$root/apps/frank-gui/src-tauri/binaries/$sidecar_name"
[[ -f "$source" ]] || { echo "missing CLI binary: $source" >&2; exit 1; }

mkdir -p "$(dirname "$destination")"
install -m 0755 "$source" "$destination"
mkdir -p "$root/apps/frank-gui/src-tauri/packaging"
install -m 0755 "$destination" "$root/apps/frank-gui/src-tauri/packaging/frank"
# Keep both resource names present in every build so the single Tauri config
# validates on all hosts. Windows executes the `.exe` copy; Unix uses the
# extension-less resource and the shell installer. WiX installs the Windows
# copy into INSTALLDIR (and adds that directory to PATH).
install -m 0755 "$source" "$root/apps/frank-gui/src-tauri/packaging/frank.exe"
printf 'prepared Frank sidecar %s for %s\n' "$sidecar_name" "$target"
