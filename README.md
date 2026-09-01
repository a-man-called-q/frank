<p align="center">
  <img src="assets/branding/frank-hero-banner-with-text.png" alt="Frank" width="100%" />
</p>

# Frank 🧟‍♂️⚡

> *"IT'S ALIVE!... and it's super friendly, nerdy, and ready to save your tokens!"*

Meet **Frank** — your friendly, nerdily precise Frankenstein monster of an AI persona engine and prompt compressor. Stitched together from high-performance Rust crates, Frank stands guard between you and your AI coding assistants (like Claude Code, Codex, and Cline) to optimize context windows, enforce honest token tracking, and switch persona packs with lightning speed!

Frank 1.0 adds an agent operations office: `frankd` runs the supervisor,
workers, SQLite, worktrees, terminals, and Git workflow on a PC/server while the
Flutter desktop client will connect over authenticated HTTPS + WebSocket. The
client and daemon are always separate processes, even on localhost.

---

## 🟢 What is Frank?

Frank might look like a monster assembled from individual Rust modules, but deep inside he's just a warm, passionate nerd who loves token efficiency and clean code:

- 🧠 **Honest & Nerdy Token Ledger**: Frank refuses to guess. He measures exact input and output token usage directly from real session logs so you get 100% verified net token accounting.
- ⚡ **Stitched with Rust**: Built from the ground up for microsecond execution, atomic flag safety (`O_NOFOLLOW`), and zero runtime bloat.
- 🎭 **Persona Pack Switching**: Need hyper-concise replies? Frank can swap into "Caveman" mode (or custom packs) seamlessly without breaking your context.
- 🗜️ **Deterministic Compression**: Shrink massive documents and code snippets before feeding them to AI models without losing structural intent.
- 💚 **Super Friendly & Transparent**: Frank never hides numbers or makes unverified performance claims. Honesty is in his DNA!

---

## ⚡ Quick Start & Installation

### Quick Install (macOS / Linux)

```sh
curl -fsSL https://raw.githubusercontent.com/a-man-called-q/frank/main/dist/install.sh | bash
```

### Build From Source

Requirements for the backend: Rust 1.89 or newer. The desktop prototype uses the
Flutter 3.47.1 toolchain pinned through Proto and lives under `apps/frank_desktop`.

```sh
git clone https://github.com/a-man-called-q/frank.git
cd frank
cargo build --release -p frank-cli
./target/release/frank --help
```

### 🖥️ Desktop Agent Office

The first client milestone is a Flutter desktop shell using [Forui](https://forui.dev/)
for navigation, [FlowUI](https://github.com/StacDev/flow_ui) for the chat/composer,
and [Flame](https://pub.dev/packages/flame) for the reserved floor surface. It
currently runs against local fixtures so the information architecture can settle
before we wire the live reconnecting transport.

```sh
cd apps/frank_desktop
proto install
proto run flutter -- pub get
proto run flutter -- run -d macos
```

From the repository root, Moon provides the equivalent task targets:

```sh
moon run frank-desktop:run --interactive
```

Use `moon run frank-desktop:analyze`, `moon run frank-desktop:test`, or
`moon run frank-desktop:build` for the non-persistent checks.

The shell has a desktop-only, fixed-width off-canvas sidebar with Office/Projects
navigation and project missions, a floating Account Executive conversation, and
an intentionally empty floor. The minimum supported window width is 880px; the
sidebar can be hidden to give the main surface the full width. The CLI (`frank`)
remains available for screen-reader-first operation and automation.

### 🌐 Remote Frank 1.0

Start the headless daemon on the machine that owns your projects:

```sh
cargo run --release -p frank-server --bin frankd -- --bind 127.0.0.1:37465
```

Issue a one-time pairing ticket on that machine, then enter the printed address,
secret, and certificate fingerprint in the desktop client once live transport is
enabled:

```sh
frank server pair --role owner --address https://127.0.0.1:37465
```

Use `--insecure-local` only for deterministic local development without a TLS
certificate; non-loopback plaintext is always rejected.  After pairing, the
GUI can switch between saved servers, resume event streams after disconnects,
and manage projects, persistent agents, missions, approvals, terminals, and
draft-PR delivery without transporting provider API keys.

---

## 🧟‍♂️ Using Frank

Once installed, say hi to Frank and set up your environment:

```sh
# Check available persona levels & modes
frank levels

# Turn on a specific mode (e.g. Caveman ultra-compressed mode)
frank on full

# Check Frank's current status
frank status

# Attach Frank into your AI assistant (Claude Code, Codex, etc.)
frank install

# View nerdy token usage & honest savings statistics!
frank stats
```

---

## 🛠️ Key Capabilities

### 🎭 Persona Management

Swap between different AI assistant personalities depending on what you're working on:

```sh
# List installed persona packs
frank pack list

# Add a custom persona pack
frank pack add ./my-custom-pack

# Switch active pack
frank pack use my-custom-pack
```

### 🗜️ Document Compression

Feed large files into your AI without blowing through your token budget:

```sh
# Compress markdown and code files
frank compress document.md notes.txt

# Preview compression without altering original files
frank compress document.md --dry-run

# Restore files back to original state
frank compress document.md --restore
```

### 📊 Honest Token Accounting

Frank tracks token costs with scientific precision:

```sh
# View stats for a specific session JSONL file
frank stats --session path/to/session.jsonl

# View overall lifetime statistics
frank stats --all

# Get a detailed breakdown of prompt vs response savings
frank stats --explain
```

---

## 🤝 Supported AI Assistants

Frank loves collaborating with all your favorite coding tools:

- 🤖 **Claude Code**: Native lifecycle hooks (`SessionStart`, `UserPromptSubmit`, `Statusline`)
- 💻 **Codex**: Configuration and system prompt integration
- 🛠️ **Cline**: Static rules and system instructions

Check which AI tools Frank detected on your system:

```sh
frank targets --detected
```

Preview installation safely without writing changes:

```sh
frank install --dry-run
```

---

## 🧪 For Developers & Contributors

Want to explore Frank's inner workings or stitch together your own persona packs?

- 📜 [`CONTRIBUTING.md`](CONTRIBUTING.md) — Workflow, guidelines, and conventional commits
- 🏗️ [`docs/architecture.md`](docs/architecture.md) — Technical crate boundaries and security design
- 🗺️ [`docs/roadmap.md`](docs/roadmap.md) — Roadmap and release checklist
- 🎨 [`docs/pack-authoring.md`](docs/pack-authoring.md) — Build custom persona packs

### Building & Verification

```sh
# Build CLI binary
cargo build --release -p frank-cli

# Run workspace unit & integration tests
cargo test --workspace

# Rebuild compiled persona pack prompts
cargo run -p xtask -- build-packs

# Validate target integration schemas
cargo run -p xtask -- lint-targets
```

Fast verification gate:
```sh
moon run :verify
```

Mandatory strict CI gate:
```sh
moon run :verify-strict
```

---

## 📜 Origin & History

Frank began as a Rust port of [Caveman](https://github.com/JuliusBrussee/caveman). The initial goal was simple: take the Node.js reference implementation and rebuild it in Rust for extra speed and safety.

As development progressed, Frank evolved into much more! We redesigned the token ledger to measure exact session token usage (verifying real savings instead of relying on estimated numbers), built deterministic document compression engines, established modular Rust crate boundaries, and introduced multi-persona pack support.

While Frank is proud of his Caveman roots, today he stands tall as his own independent, friendly, and nerdy persona monster! 🧟‍♂️💚

---

## 📄 License

Licensed under the [MIT License](LICENSE.md) — free, open source, and friendly.

---

## 💚 Acknowledgments

Special thanks to [Julius Brussee](https://github.com/JuliusBrussee) and the [Caveman](https://github.com/JuliusBrussee/caveman) community for pioneering AI assistant persona management and proving the concept works. Frank wouldn't exist without that foundation!
