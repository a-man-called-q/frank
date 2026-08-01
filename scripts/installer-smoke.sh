#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
installer="$root/apps/frank-gui/src-tauri/packaging/install-frank-cli.command"

[[ -x "$installer" ]] || { echo "installer is not executable: $installer" >&2; exit 1; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/frank-installer-smoke.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

bundle="$tmp/bundle with spaces"
mkdir -p "$bundle"
printf '%s\n' '#!/bin/sh' 'printf frank-smoke' > "$bundle/frank"
chmod 0755 "$bundle/frank"
cp "$installer" "$bundle/install-frank-cli.command"
chmod 0755 "$bundle/install-frank-cli.command"

fake_home="$tmp/home with spaces"
mkdir -p "$fake_home"
HOME="$fake_home" "$bundle/install-frank-cli.command" >/dev/null
installed="$fake_home/.local/bin/frank"
[[ -x "$installed" ]] || { echo 'fresh install did not create frank' >&2; exit 1; }
[[ "$("$installed")" == 'frank-smoke' ]] || { echo 'installed CLI content mismatch' >&2; exit 1; }

# Reinstall/upgrade replaces a regular existing binary atomically.
printf '%s\n' '#!/bin/sh' 'printf frank-upgraded' > "$bundle/frank"
chmod 0755 "$bundle/frank"
HOME="$fake_home" "$bundle/install-frank-cli.command" >/dev/null
[[ "$("$installed")" == 'frank-upgraded' ]] || { echo 'upgrade did not replace the existing CLI' >&2; exit 1; }

# A symlinked destination must fail closed and must not touch its target.
outside="$tmp/outside"
printf '%s\n' 'untouched' > "$outside"
rm -f "$installed"
ln -s "$outside" "$installed"
if HOME="$fake_home" "$bundle/install-frank-cli.command" >/dev/null 2>&1; then
  echo 'installer followed a symlinked destination' >&2
  exit 1
fi
[[ "$(cat "$outside")" == 'untouched' ]] || { echo 'symlink target was modified' >&2; exit 1; }

# A symlinked parent is equally unsafe, even when the final destination name
# does not exist yet.
rm -f "$installed"
rm -rf "$fake_home/.local"
ln -s "$tmp/redirect" "$fake_home/.local"
if HOME="$fake_home" "$bundle/install-frank-cli.command" >/dev/null 2>&1; then
  echo 'installer traversed a symlinked parent' >&2
  exit 1
fi

printf '%s\n' 'installer smoke: clean install, upgrade, spaces, and symlink refusal passed'
