# Frank security gate

The release job runs `cargo audit --deny warnings`. The command contains only
exact advisory identifiers, never a package-wide or wildcard ignore.

The Rust 1.85 pin currently constrains the Tauri 2 dependency graph to
`plist 1.8.0`/`quick-xml 0.38.4` and `time 0.3.45`. Their patched upstream
releases require Rust 1.88, while the Tauri Linux backend also brings archived
GTK3 and Unicode helper crates. The IDs are therefore listed explicitly in
`scripts/verify-strict.sh` with this review record rather than silently
discarded:

- `RUSTSEC-2026-0009`, `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`: transitive
  Tauri parser advisories; no Frank network/parser input reaches these paths.
  Re-evaluate before changing the GUI or accepting remote pack/config data.
- `RUSTSEC-2024-0429`: transitive GTK3 unsoundness; Frank does not call the
  affected iterator, but the Linux webview remains a platform dependency.
- `RUSTSEC-2024-0370`, `RUSTSEC-2024-0411` through `RUSTSEC-2024-0420`, and
  `RUSTSEC-2025-0075`, `RUSTSEC-2025-0080`, `RUSTSEC-2025-0081`,
  `RUSTSEC-2025-0098`, `RUSTSEC-2025-0100`: unmaintained transitive GUI/macro
  crates with no Rust 1.85-compatible replacement in the selected Tauri
  release.

This is a temporary dependency-review exception, not a claim that those
upstream advisories are fixed. Upgrade the toolchain/Tauri graph, remove each
ID, and rerun the audit before enabling production signing or auto-update.

