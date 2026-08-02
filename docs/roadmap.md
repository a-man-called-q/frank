# TODO

## Testing Checklist (M6 Release-Ready)

### Cross-Platform Testing

- [ ] Run `dist/install.ps1` on Windows PowerShell 5.1
- [ ] Run `dist/install.ps1` on Windows PowerShell 7
- [ ] Test Windows x64 artifact installation
- [ ] Test Windows arm64 artifact installation
- [ ] Build and test macOS x86_64 artifact
- [ ] Build and test macOS arm64 artifact
- [ ] Build and test Linux musl x86_64 artifact
- [ ] Build and test Linux musl arm64 artifact

### Network & Distribution Testing

- [ ] Test release downloads over HTTPS
- [ ] Test HTTPS redirects
- [ ] Test tagged version downloads
- [ ] Test missing assets handling
- [ ] Test malformed manifests handling
- [ ] Test duplicate manifest entries handling
- [ ] Test corrupted archive handling

### Installation Edge Cases

- [ ] Test installation into an existing directory
- [ ] Test installation into a path containing spaces
- [ ] Test installation when `frank` binary already exists
- [ ] Test installation to a destination symlink

### Real-World Integration Testing

- [ ] Run real Claude Code install/use/uninstall flow on macOS
- [ ] Run real Claude Code install/use/uninstall flow on Linux
- [ ] Verify Codex behavior against real installation
- [ ] Verify Cline behavior against real installation
- [ ] Confirm static-target limitation is visible to users
- [ ] Update `verified = false` flags for Codex and Cline when confirmed

### Hook & Performance Testing

- [ ] Exercise hook failure cases
- [ ] Measure hook startup with `hyperfine`
- [ ] Confirm every hook path exits zero on malformed config
- [ ] Confirm every hook path exits zero on malformed input

### Stats & Accounting Testing

- [ ] Feed real Claude session JSONL files to `frank stats`
- [ ] Manually verify `input_tokens` handling
- [ ] Manually verify `cache_creation_input_tokens` handling
- [ ] Check measured token bucket accuracy
- [ ] Check estimated token bucket accuracy
- [ ] Check unattributed token bucket accuracy
- [ ] Check sidechain token bucket accuracy

### Release Workflow Testing

- [ ] Run release workflow on a clean machine/runner
- [ ] Verify published assets match expectations
- [ ] Verify checksums match published artifacts
- [ ] Verify installer URLs work correctly

### Desktop Distribution Security (unsigned MVP)

The MVP desktop artifacts are intentionally unsigned and must be labelled
“unsigned / development build” wherever they are published. Do not describe an
unsigned artifact as trusted or production-ready.

- [ ] macOS Developer ID application signing and hardened-runtime entitlements
- [ ] macOS notarization, stapling, and Gatekeeper clean-machine verification
- [ ] Windows Authenticode signing (GUI, `frank.exe`, and MSI) with timestamping
- [ ] Linux package signing and repository metadata verification for `.deb`/`.rpm`
- [ ] minisign signatures for every archive, installer, and `SHA256SUMS`
- [ ] Public-key pinning and signature verification in the installer/smoke test
- [ ] Document secret storage, least-privilege CI access, and key rotation/revocation
- [ ] Enforce signed tags and protected release branches before publishing
- [ ] Keep auto-update disabled until signed metadata, rollback, and downgrade
      protections are reviewed

## Implementation Gaps (Held Features)

### M6 Gaps

- [ ] Implement minisign signing
- [ ] Implement signature verification
- [ ] Complete GitHub Release publishing automation
- [ ] Test full cross-compilation workflow on CI runner
- [ ] Normalize `cargo fmt --all -- --check` across workspace

### M7 Gaps (Antigravity & Third-Party Packs)

- [ ] Install Antigravity in disposable profile for verification
- [ ] Implement GitHub/HTTPS pack sources
- [ ] Add pack downloader with network tests
- [ ] Add pack signature verification
- [ ] Create third-party pack registry
- [ ] Document pack distribution workflow
- [ ] Test pack installation from remote sources
