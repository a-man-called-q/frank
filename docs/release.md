# M6 distribution checklist

The local M6 slice is deliberately small but real:

```sh
cargo build --release -p frank-cli
cargo run -p xtask -- dist
cargo run -p xtask -- checksums
(cd dist && shasum -a 256 --check SHA256SUMS)
```

`xtask dist --target <triple>` packages an already-built target binary. The
Unix and PowerShell installers fail closed when `SHA256SUMS` is missing,
malformed, duplicated, or mismatched. The local Unix installer has been tested
with a `file://` release directory.

## End-to-end GitHub Release

`moon run release:bump` is the release entrypoint. It requires a clean working
tree, updates the workspace version and internal path-dependency constraints,
refreshes `Cargo.lock`, commits the release files, creates `v<version>`, and
pushes the branch and tag to `origin`. The tag push triggers
`.github/workflows/release.yml`; that workflow builds the platform artifacts
and publishes the GitHub Release with `SHA256SUMS`.

```sh
# Publish the version already in Cargo.toml (for example, 0.2.0).
moon run release:bump

# Or calculate the next version first.
moon run release:bump -- patch
moon run release:bump -- minor
moon run release:bump -- 0.2.0

# Inspect the full plan without changing files or git refs.
moon run release:bump -- --dry-run
```

Use `--no-push` to create the commit and tag locally without triggering
GitHub Actions. A release can only appear on GitHub after the tag is pushed.

## Held before release-ready

<!-- HOLD(M6): these require runners or credentials not available in the
current workspace. Keep them visible rather than calling the local artifact a
published release. -->

- Execute the full macOS, Linux-musl, and Windows-MSVC target matrix.
- Run `dist/install.ps1` on Windows PowerShell 5.1 and 7 for x64 and arm64.
- Test HTTPS redirects, tagged release URLs, malformed checksum manifests,
  duplicate entries, and corrupted archives.
- Publish a GitHub Release from a clean runner and verify asset names.
- Add minisign key management, signature generation, and installer signature
  verification. SHA-256 alone is not the final M6 trust model.

The draft CI workflow in `.github/workflows/release.yml` captures the intended
matrix and keeps the signing step as an explicit hold until a key and a secure
secret-handling policy exist.
