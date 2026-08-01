#!/usr/bin/env bash
# Frank release installer.
#
# The installer deliberately fails closed: an absent, malformed, or mismatched
# SHA256SUMS entry is an error. Set FRANK_RELEASE_BASE_URL to a release asset
# directory when testing locally, and FRANK_INSTALL_DIR to choose the install
# location. No shell code is downloaded or executed from the release.

set -euo pipefail

die() {
  printf 'frank installer: %s\n' "$*" >&2
  exit 1
}

download() {
  if command -v curl >/dev/null 2>&1; then
    curl --fail --silent --show-error --location --retry 3 --output "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget --quiet --tries=3 --output-document="$2" "$1"
  else
    die "curl or wget is required"
  fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    die "sha256sum or shasum is required"
  fi
}

case "${FRANK_TARGET_TRIPLE:-}" in
  "")
    frank_os=$(uname -s)
    frank_arch=$(uname -m)
    case "$frank_os:$frank_arch" in
      Darwin:arm64|Darwin:aarch64) target_triple="aarch64-apple-darwin" ;;
      Darwin:x86_64|Darwin:amd64) target_triple="x86_64-apple-darwin" ;;
      Linux:x86_64|Linux:amd64) target_triple="x86_64-unknown-linux-musl" ;;
      Linux:aarch64|Linux:arm64) target_triple="aarch64-unknown-linux-musl" ;;
      *) die "unsupported platform: ${frank_os}/${frank_arch}" ;;
    esac
    ;;
  *) target_triple="$FRANK_TARGET_TRIPLE" ;;
esac

archive_name="frank-${target_triple}.tar.gz"
release_base="${FRANK_RELEASE_BASE_URL:-}"
if [ -z "$release_base" ]; then
  frank_version="${FRANK_VERSION:-latest}"
  if [ "$frank_version" = "latest" ]; then
    release_base="https://github.com/JuliusBrussee/frank/releases/latest/download"
  else
    release_base="https://github.com/JuliusBrussee/frank/releases/download/${frank_version}"
  fi
fi
release_base=${release_base%/}

command -v tar >/dev/null 2>&1 || die "tar is required"
command -v mktemp >/dev/null 2>&1 || die "mktemp is required"

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/frank-install.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT

archive_path="$tmp_dir/$archive_name"
sums_path="$tmp_dir/SHA256SUMS"
download "$release_base/$archive_name" "$archive_path" || die "could not download $archive_name"
download "$release_base/SHA256SUMS" "$sums_path" || die "could not download SHA256SUMS"

expected_hashes=$(awk -v name="$archive_name" '$2 == name { print $1 }' "$sums_path")
expected_count=$(printf '%s\n' "$expected_hashes" | awk 'NF { n += 1 } END { print n + 0 }')
[ "$expected_count" -eq 1 ] || die "SHA256SUMS has no unique entry for $archive_name"
expected_hash=$(printf '%s\n' "$expected_hashes" | tr '[:upper:]' '[:lower:]')
printf '%s\n' "$expected_hash" | grep -Eq '^[0-9a-f]{64}$' || die "invalid SHA256SUMS entry for $archive_name"

actual_hash=$(sha256 "$archive_path" | tr '[:upper:]' '[:lower:]')
[ "$actual_hash" = "$expected_hash" ] || die "checksum mismatch for $archive_name"

entries=$(tar -tzf "$archive_path") || die "archive is not a readable tar.gz"
entry_count=$(printf '%s\n' "$entries" | sed '/^$/d' | awk 'END { print NR + 0 }')
[ "$entry_count" -eq 1 ] || die "archive must contain exactly one file"
printf '%s\n' "$entries" | grep -Eq '^(\./)?frank$' || die "archive contains an unexpected path"

extract_dir="$tmp_dir/extract"
mkdir -m 700 "$extract_dir"
tar -xzf "$archive_path" -C "$extract_dir" || die "could not extract archive"
binary_path="$extract_dir/frank"
[ -f "$binary_path" ] || die "archive did not contain frank"
[ ! -L "$binary_path" ] || die "archive contained a symlink instead of frank"

install_dir="${FRANK_INSTALL_DIR:-${HOME:?HOME must be set}/.local/bin}"
mkdir -p "$install_dir"
destination="$install_dir/frank"
[ ! -L "$destination" ] || die "refusing to replace symlink $destination"

if command -v install >/dev/null 2>&1; then
  install -m 0755 "$binary_path" "$destination"
else
  cp "$binary_path" "$destination"
  chmod 0755 "$destination"
fi

printf 'Installed Frank %s to %s\n' "$target_triple" "$destination"
case ":${PATH:-}:" in
  *":$install_dir":*) ;;
  *) printf 'Add %s to PATH to invoke `frank` directly.\n' "$install_dir" ;;
esac
