<p align="center">
  <img src="assets/branding/frank-hero-banner-with-text.png" alt="Frank" width="100%" />
</p>

# Frank

A lightweight AI agent persona engine inspired by [Caveman](https://github.com/JuliusBrussee/caveman). Frank helps you manage and optimize your AI coding assistant interactions with honest token tracking and smart prompt compression.

## What is Frank?

Frank is designed to make working with AI coding assistants more efficient and transparent:

- **Smart Prompt Management**: Inject context and instructions into your AI sessions without bloating your token usage
- **Honest Token Tracking**: Know exactly how many tokens you're using and saving, with clear accounting
- **Multiple Personas**: Switch between different AI assistant personalities and behaviors for different tasks
- **Offline Compression**: Compress documents before sending them to your AI, saving tokens and costs

Built with Rust for speed and reliability.

## Installation

### Quick Install (macOS/Linux)

```sh
curl -fsSL https://raw.githubusercontent.com/YOUR_USERNAME/frank/main/dist/install.sh | bash
```

### From Source

Requirements: Rust 1.85 or newer. The repository also pins Node 24, pnpm 10,
and Moon 2.4.5 through proto for GUI and release tasks.

```sh
git clone https://github.com/YOUR_USERNAME/frank.git
cd frank
cargo build --release -p frank-cli
./target/release/frank --help
```

### Desktop control panel

The Tauri desktop app is an optional control plane; hooks and the CLI never
need the GUI process to be running. During development:

```sh
pnpm install --frozen-lockfile
moon run frank-gui:dev
```

The tray-first app exposes status/levels, persona packs, integrations,
diagnostics, and settings. Release builds can produce `.dmg`, `.msi`, `.deb`,
and `.rpm` packages with `moon run frank-release:bundle`. MVP packages are
unsigned and are labelled as development builds; signing and notarization are
tracked in [`TODO.md`](TODO.md).

## Getting Started

After installation, set up Frank with your AI coding assistant:

```sh
# Check available personas/levels
frank levels

# Turn on a specific level
frank on [LEVEL_NAME]

# Check current status
frank status

# Install into your AI assistant (Claude Code, Codex, etc.)
frank install

# View token usage statistics
frank stats
```

## Key Features

### Persona Management

Switch between different AI assistant behaviors for different tasks:

```sh
# List available personas
frank pack list

# Add a new persona pack
frank pack add ./my-custom-pack

# Switch to a different persona
frank pack use my-custom-pack
```

### Document Compression

Save tokens by compressing documents before sending to AI:

```sh
# Compress files
frank compress document.md notes.txt

# Preview compression without modifying files
frank compress document.md --dry-run

# Restore original files
frank compress document.md --restore
```

### Token Statistics

Get honest metrics about your AI usage:

```sh
# View session statistics
frank stats --session path/to/session.jsonl

# See all sessions
frank stats --all

# Detailed breakdown
frank stats --explain
```

## Supported AI Assistants

Frank works with popular AI coding assistants:

- **Claude Code**: Full integration with lifecycle hooks
- **Codex**: Static configuration support
- **Cline**: Static configuration support

Check compatibility with your setup:

```sh
frank targets --detected
```

Preview installation without making changes:

```sh
frank install --dry-run
```

## For Developers

### Contributing

Want to contribute? Check out:

- [`AGENTS.md`](AGENTS.md) - Technical architecture and design decisions
- [`TODO.md`](TODO.md) - Upcoming features and testing checklist
- [`docs/pack-authoring.md`](docs/pack-authoring.md) - Create custom persona packs

### Building from Source

```sh
# Build the project
cargo build --release -p frank-cli

# Run tests
cargo test --workspace

# Compile persona packs
cargo run -p xtask -- build-packs

# Validate target manifests
cargo run -p xtask -- lint-targets
```

The fast workspace gate is:

```sh
moon run :verify
```

The mandatory strict gate (coverage, audit, browser tests, and clean generated
output checks) is:

```sh
moon run :verify-strict
```

Native Tauri smoke (hidden launch, single-instance hand-off, and quit) runs
against a packaged executable supplied by the platform job:

```sh
FRANK_GUI_BINARY=/path/to/Frank moon run frank-release:native-smoke
```

`cargo` remains the source of truth for Rust builds; Moon only orchestrates
the task graph. Release tasks intentionally run without affected filtering or
cache reuse.

### Project Structure

Frank is built with modular Rust crates:

- `frank-cli` - Main command-line interface
- `frank-pack` - Persona pack management
- `frank-state` - State machine and mode switching
- `frank-ledger` - Token usage tracking and accounting
- `frank-compress` - Document compression engine
- `frank-target` - AI assistant integration layer
- `frank-app` - shared application service used by CLI, hooks, and GUI
- `apps/frank-gui` - Tauri v2 + React desktop control plane

Full architecture details in [`AGENTS.md`](AGENTS.md).


## History

Frank started as a simple Rust port of [Caveman](https://github.com/JuliusBrussee/caveman). The goal was straightforward: take the proven Node.js implementation and rebuild it in Rust for better performance and reliability.

But midway through development, new ideas emerged. What began as a faithful port evolved into something more - a reimagined engine with its own architecture, philosophy, and goals. The codebase diverged as we explored better ways to handle token accounting, introduced stricter security guarantees, and built a more flexible persona system.

At that point, calling Frank a "port" or "rebuild" no longer felt honest. It's inspired by Caveman's vision, but it's become its own thing - a new implementation that learned from the original while charting its own path.

The original Caveman remains the reference for what this kind of tool should do. Frank is simply another take on how to do it.

## License

See [LICENSE](LICENSE) file for details.

## Acknowledgments

Special thanks to [Julius Brussee](https://github.com/JuliusBrussee) and the [Caveman](https://github.com/JuliusBrussee/caveman) community for pioneering AI assistant persona management and proving the concept works. Frank wouldn't exist without that foundation.
