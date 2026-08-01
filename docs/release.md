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
