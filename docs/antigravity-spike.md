# Antigravity spike

Antigravity is not marked as a target yet. The old Caveman probe checked a
Gemini CLI directory and therefore does not establish that Antigravity is
installed. Frank intentionally does not ship a guessed target manifest.

## Verification checklist

Run these on a machine with the real Antigravity application installed and
record yes/no answers before adding `targets/antigravity.toml`:

1. Identify the actual per-platform config directory by installing into a
   temporary home and diffing the tree.
2. Check whether a stable CLI and `--version` exist.
3. Confirm whether `AGENTS.md` is read from the repository root, home, or both;
   record precedence against native rules.
4. Identify the native global/workspace rules format.
5. Confirm whether a SessionStart/UserPromptSubmit-like lifecycle hook exists.
6. Locate a parseable session transcript, or explicitly record that the ledger
   cannot measure Antigravity sessions.
7. Check whether the VS Code extension API can support a `.vsix` integration.
8. Run the equivalent of `npx skills add -a antigravity` in a temporary home
   and inspect exactly what changed.

## Fallback design

If the only reliable integration is `AGENTS.md`, use a declarative
`markdown-block` target with `soft = true`, `verified = false`, and
`runtime = false`. The installed level is then static: changing the Frank
level requires re-running the install command. `frank stats` must say that no
session log is available; it must not report zero as if that were a measured
result.

<!-- HOLD(M7): this spike cannot be completed from the current macOS workspace
without the real Antigravity product and a disposable user profile. Do not
change this document to "verified" or add a target manifest until the checklist
has been run and the result is attached to the release report. -->
