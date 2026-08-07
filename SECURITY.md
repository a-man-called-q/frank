# Frank security gate

The release job runs `cargo audit --deny warnings`. The command contains only
exact advisory identifiers, never a package-wide or wildcard ignore.

The Rust 1.88 floor (bumped from 1.85) let `plist` move 1.8.0 -> 1.10.0,
pulling `quick-xml` 0.38.4 -> 0.41.0 and `time` 0.3.45 -> 0.3.55 along with
it. That resolved and removed the three `RUSTSEC-2026-*` IDs that used to be
listed here. `cargo audit` is clean of them as of this Rust bump; re-run it
after any future dependency change to confirm they stay resolved.

## GTK3 tray backend (9 IDs)

`frank-gui` migrated off Tauri 2 + React onto native Rust + iced 0.14 (see
the frank-gui -> iced migration plan); Tauri was fully removed from the tree
at that migration's M-5. **These 9 IDs are not a Tauri leftover** -- they
persist because `tray-icon`/`muda`, the crates providing the Linux tray icon
and menu, link `gtk-rs` 0.18 (via `libappindicator`) directly for their own
Linux backend:

- `RUSTSEC-2024-0429`: `glib` unsoundness in `Iterator`/`DoubleEndedIterator`
  impls for `VariantStrIter`. Frank does not call the affected iterator, but
  `glib` remains a transitive platform dependency.
- `RUSTSEC-2024-0412`, `-0413`, `-0415`, `-0416`, `-0418`, `-0419`, `-0420`:
  unmaintained `gtk-rs` 0.18 bindings (`gdk`, `atk`, `gtk`, `atk-sys`,
  `gdk-sys`, `gtk3-macros`, `gtk-sys`) with no replacement available in the
  gtk-rs 0.18 line `tray-icon`'s Linux backend pins to.
- `RUSTSEC-2024-0370`: unmaintained `proc-macro-error`, pulled in by
  `gtk3-macros`.

This is a reviewed dependency exception, not a claim the upstream advisories
are fixed. Unlike the old Tauri-era framing, these do **not** have a known
removal path today -- they are tied to `tray-icon`'s Linux GTK backend, and
would only disappear if that crate (or Frank's use of it) changes. Re-check
this list whenever `tray-icon`/`muda` are upgraded, in case a newer release
drops the GTK3 dependency.

## iced 0.14 graph (2 IDs)

Unrelated to the GTK3 set above, from `frank-gui-core`'s `iced` dependency:

- `RUSTSEC-2024-0436`: `paste` is unmaintained. Pulled in via
  `metal -> wgpu-hal -> wgpu -> iced_wgpu`, iced's GPU renderer backend.
- `RUSTSEC-2026-0192`: `ttf-parser` is unmaintained. Pulled in via
  `cosmic-text`/`winit`'s glyph rendering (both the tiny-skia and wgpu
  render paths).

Both are "unmaintained" warnings, not active vulnerabilities, and both have
no replacement available in iced 0.14's pinned dependency graph. Re-evaluate
on the next iced upgrade rather than assuming they resolve on their own.

## Accessibility

iced 0.14 has no accessibility tree (no AT-SPI/UIA/NSAccessibility
integration). `frank`, the CLI, is the accessible, screen-reader-native way
to perform every operation `frank-gui` exposes, and every installer places
both binaries side by side. This is a documented product tradeoff, not an
oversight -- see the frank-gui -> iced migration plan's "Regresi" section.
