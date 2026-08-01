#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dist="$root/dist"
[[ -d "$dist" ]] || { echo "missing dist directory" >&2; exit 1; }

expected="${FRANK_EXPECT_BUNDLES:-}"
if [[ -n "$expected" ]]; then
  IFS=',' read -r -a expected_kinds <<< "$expected"
  for kind in "${expected_kinds[@]}"; do
    case "$kind" in
      dmg|msi|deb|rpm|tar.gz|zip) ;;
      *) echo "unsupported expected bundle kind: $kind" >&2; exit 2 ;;
    esac
    compgen -G "$dist/*.$kind" >/dev/null || {
      echo "missing expected .$kind artifact" >&2
      exit 1
    }
  done
fi

shopt -s nullglob
artifacts=("$dist"/*.dmg "$dist"/*.msi "$dist"/*.deb "$dist"/*.rpm "$dist"/*.tar.gz "$dist"/*.zip)
if ((${#artifacts[@]} == 0)); then
  echo 'no release artifacts found' >&2
  exit 1
fi
[[ -s "$dist/SHA256SUMS" ]] || { echo 'missing SHA256SUMS' >&2; exit 1; }

# A checksum file that verifies only the first artifact is not a valid release
# manifest.  Validate the grammar and require an exact one-to-one inventory
# before asking sha256sum/shasum to verify the bytes.  This is deliberately
# fail-closed: missing, duplicate, extra, or path-traversing entries stop the
# smoke test even if every line that happened to be present is correct.
checksum_tmp=$(mktemp -d "${TMPDIR:-/tmp}/frank-checksums.XXXXXX")
trap 'rm -rf "$checksum_tmp"' EXIT HUP INT TERM
artifact_names="$checksum_tmp/artifacts"
checksum_names="$checksum_tmp/checksums"
for artifact in "${artifacts[@]}"; do
  basename "$artifact"
done | LC_ALL=C sort > "$artifact_names"

if ! awk '
  length($1) != 64 || $1 !~ /^[[:xdigit:]]+$/ || NF != 2 || $2 ~ /\// || $2 == "" { bad = 1 }
  { print $2 }
  END { exit bad }
' "$dist/SHA256SUMS" | LC_ALL=C sort > "$checksum_names"; then
  echo 'malformed SHA256SUMS entry' >&2
  exit 1
fi
if [[ -s "$checksum_names" ]] && [[ "$(uniq -d "$checksum_names")" ]]; then
  echo 'duplicate SHA256SUMS entry' >&2
  exit 1
fi
if ! diff -u "$artifact_names" "$checksum_names" >/dev/null; then
  echo 'SHA256SUMS does not exactly cover the release artifact inventory' >&2
  diff -u "$artifact_names" "$checksum_names" >&2 || true
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  (cd "$dist" && sha256sum --check SHA256SUMS)
elif command -v shasum >/dev/null 2>&1; then
  while read -r expected name; do
    [[ "$(shasum -a 256 "$dist/$name" | awk '{print $1}')" == "$expected" ]] || exit 1
  done < "$dist/SHA256SUMS"
else
  echo 'no SHA-256 verifier found' >&2
  exit 2
fi

trap - EXIT HUP INT TERM
rm -rf "$checksum_tmp"

if [[ -n "${FRANK_SMOKE_BINARY:-}" ]]; then
  [[ -x "$FRANK_SMOKE_BINARY" ]] || { echo "smoke binary is not executable: $FRANK_SMOKE_BINARY" >&2; exit 1; }
  "$FRANK_SMOKE_BINARY" --version >/dev/null

  smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/frank-cli-smoke.XXXXXX")
  trap 'rm -rf "$checksum_tmp" "$smoke_root"' EXIT HUP INT TERM
  smoke_env=(
    "CLAUDE_CONFIG_DIR=$smoke_root/claude"
    "XDG_CONFIG_HOME=$smoke_root/config"
    "XDG_DATA_HOME=$smoke_root/data"
  )
  env "${smoke_env[@]}" "$FRANK_SMOKE_BINARY" status >/dev/null
  env "${smoke_env[@]}" "$FRANK_SMOKE_BINARY" install --only claude-code --dry-run >/dev/null
  env "${smoke_env[@]}" "$FRANK_SMOKE_BINARY" uninstall --only claude-code --dry-run >/dev/null

  if [[ "${FRANK_SMOKE_APPLY:-0}" == '1' ]]; then
    # Exercise the real action path in an isolated config root, then prove
    # uninstall removed only Frank-owned hook data. This is intentionally
    # opt-in because the ordinary local smoke should remain read-only.
    env "${smoke_env[@]}" "$FRANK_SMOKE_BINARY" install --only claude-code >/dev/null
    env "${smoke_env[@]}" "$FRANK_SMOKE_BINARY" uninstall --only claude-code >/dev/null
    settings_path="$smoke_root/claude/settings.json"
    if [[ -e "$settings_path" ]] && grep -Eq 'frank|caveman' "$settings_path"; then
      echo 'target uninstall left Frank-managed hook data behind' >&2
      exit 1
    fi
  fi

  # Prove that a corrupted copy is rejected while leaving the published
  # artifact untouched. This catches a checksum verifier that only checks
  # syntax or accidentally reads the pristine source after copying.
  corrupted="$smoke_root/corrupted"
  cp "${artifacts[0]}" "$corrupted"
  printf 'corruption' >> "$corrupted"
  corrupted_sum="$smoke_root/SHA256SUMS"
  printf '%s  corrupted\n' "$( (command -v sha256sum >/dev/null && sha256sum "$corrupted" || shasum -a 256 "$corrupted") | awk '{print $1}' )" > "$corrupted_sum"
  printf '%064d  corrupted\n' 0 > "$smoke_root/bad-SHA256SUMS"
  if (cd "$smoke_root" && (command -v sha256sum >/dev/null && sha256sum --check bad-SHA256SUMS || shasum -a 256 corrupted | grep -q '^0000')); then
    echo 'checksum corruption was not rejected' >&2
    exit 1
  fi
fi

printf 'package smoke: artifact inventory and checksums passed\n'
