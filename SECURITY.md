# Frank security gate

The release job runs `cargo audit --deny warnings`. The command contains only
exact advisory identifiers, never a package-wide or wildcard ignore.

The Rust 1.88 floor (bumped from 1.85) let `plist` move 1.8.0 -> 1.10.0,
pulling `quick-xml` 0.38.4 -> 0.41.0 and `time` 0.3.45 -> 0.3.55 along with
it. That resolved and removed the three `RUSTSEC-2026-*` IDs that used to be
listed here. `cargo audit` is clean of them as of this Rust bump; re-run it
after any future dependency change to confirm they stay resolved.

What remains is the Tauri Linux tray backend's GTK3 dependency graph, which
does **not** move with the Rust version -- these are archived/unmaintained
`gtk-rs` 0.18 bindings and the `unic-*` Unicode helpers `gtk3-macros` pulls
in. The IDs are listed explicitly in `scripts/verify-strict.sh` with this
review record rather than silently discarded:

- `RUSTSEC-2024-0429`: transitive GTK3 unsoundness; Frank does not call the
  affected iterator, but the Linux webview remains a platform dependency.
- `RUSTSEC-2024-0411` through `RUSTSEC-2024-0420`, and `RUSTSEC-2025-0075`,
  `RUSTSEC-2025-0080`, `RUSTSEC-2025-0081`, `RUSTSEC-2025-0098`,
  `RUSTSEC-2025-0100`: unmaintained transitive GUI/macro crates with no
  replacement available in the gtk-rs 0.18 line the Tauri 2 Linux backend
  pins to.
- `RUSTSEC-2024-0370`: unmaintained `proc-macro-error`, pulled in by
  `gtk3-macros`.

This is a temporary dependency-review exception, not a claim that those
upstream advisories are fixed. These 17 IDs are expected to disappear
entirely once the desktop GUI moves off Tauri (see the frank-gui -> native
Rust/iced migration plan) rather than through a further toolchain bump --
they are tied to the GTK3 graph, not the Rust version. Until then, upgrade
the toolchain/Tauri graph, remove each ID, and rerun the audit before
enabling production signing or auto-update.

## iced 0.14 graph (added by the migration itself, M-3)

Two more IDs, unrelated to the Tauri/GTK3 set above -- these come from
`frank-gui-core`'s new `iced` dependency, so they will **outlive** the Tauri
removal rather than disappear with it:

- `RUSTSEC-2024-0436`: `paste` is unmaintained. Pulled in via
  `metal -> wgpu-hal -> wgpu -> iced_wgpu`, iced's GPU renderer backend.
- `RUSTSEC-2026-0192`: `ttf-parser` is unmaintained. Pulled in via
  `cosmic-text`/`winit`'s glyph rendering (both the tiny-skia and wgpu
  render paths).

Both are "unmaintained" warnings, not active vulnerabilities, and both have
no replacement available in iced 0.14's pinned dependency graph. Re-evaluate
on the next iced upgrade rather than assuming they resolve on their own.

