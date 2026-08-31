#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dist="$root/dist"
manifest="$dist/frank-update-v1.json"
signature="$manifest.minisig"

[[ -n "${FRANK_UPDATE_PRIVATE_KEY_B64:-}" ]] || {
  echo 'FRANK_UPDATE_PRIVATE_KEY_B64 is required; refusing to publish an unsigned update feed' >&2
  exit 2
}
[[ -n "${FRANK_UPDATE_PUBLIC_KEY_B64:-}" ]] || {
  echo 'FRANK_UPDATE_PUBLIC_KEY_B64 is required; refusing to publish a feed with an unverifiable binary key' >&2
  exit 2
}

version="${FRANK_VERSION:-$(sed -n 's/^version = "\([0-9][^"]*\)"/\1/p' "$root/Cargo.toml" | head -1)}"
release_url="${FRANK_RELEASE_BASE_URL:-https://github.com/a-man-called-q/frank/releases/download/v$version}"
export FRANK_MANIFEST_DIST="$dist" FRANK_MANIFEST_VERSION="$version" FRANK_MANIFEST_URL="$release_url" FRANK_MANIFEST_PATH="$manifest"

python3 - <<'PY'
import hashlib, json, os, pathlib, time

dist = pathlib.Path(os.environ["FRANK_MANIFEST_DIST"])
version = os.environ["FRANK_MANIFEST_VERSION"]
base = os.environ["FRANK_MANIFEST_URL"]
artifacts = []
for path in sorted(dist.iterdir()):
    if path.name in {"SHA256SUMS", "frank-update-v1.json", "frank-update-v1.json.minisig"}:
        continue
    if not path.is_file() or path.name.endswith(".sig"):
        continue
    name = path.name.lower()
    if name.endswith(".dmg"):
        kind = "dmg"
    elif name.endswith(".msi"):
        kind = "msi"
    elif name.endswith(".deb"):
        kind = "deb"
    elif name.endswith(".rpm"):
        kind = "rpm"
    elif name.endswith(".tar.gz"):
        kind = "tar.gz"
    elif name.endswith(".zip"):
        kind = "zip"
    else:
        continue
    target = name.removeprefix("frank-")
    for suffix in (".tar.gz", ".zip", ".dmg", ".msi", ".deb", ".rpm"):
        if target.endswith(suffix):
            target = target[:-len(suffix)]
            break
    data = path.read_bytes()
    artifacts.append({
        "target": target,
        "package_kind": kind,
        "url": f"{base}/{path.name}",
        "size": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    })
manifest = {
    "schema_version": 1,
    "frank_version": version,
    "release_timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "protocol_min": 1,
    "protocol_max": 1,
    "minimum_rollback_version": version,
    "artifacts": artifacts,
    "release_notes_url": f"https://github.com/a-man-called-q/frank/releases/tag/v{version}",
    "key_id": os.environ.get("FRANK_UPDATE_KEY_ID", "frank-release-v1"),
}
if not artifacts:
    raise SystemExit("no release artifacts found for update manifest")
pathlib.Path(os.environ["FRANK_MANIFEST_PATH"]).write_text(json.dumps(manifest, separators=(",", ":")) + "\n")
PY

key_file="$(mktemp)"
trap 'rm -f "$key_file"' EXIT HUP INT TERM
if base64 --decode </dev/null >/dev/null 2>&1; then
  printf '%s' "$FRANK_UPDATE_PRIVATE_KEY_B64" | base64 --decode > "$key_file"
else
  # BSD base64 (macOS) spells the same operation -D.
  printf '%s' "$FRANK_UPDATE_PRIVATE_KEY_B64" | base64 -D > "$key_file"
fi
cargo run --locked --release -p frank-release-cli -- sign-manifest \
  --manifest "$manifest" --private-key "$key_file" --output "$signature"
# Fail before publication if the offline private key and the public key baked
# into release binaries do not describe the same Ed25519 identity.
cargo run --locked --release -p frank-release-cli -- verify-manifest \
  --manifest "$manifest" --signature "$signature" \
  --public-key "$FRANK_UPDATE_PUBLIC_KEY_B64"
chmod 0644 "$manifest" "$signature"
printf 'signed update manifest: %s\n' "$manifest"
