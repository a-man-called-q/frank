<p align="center">
  <img src="assets/branding/frank-hero-banner-with-text.png" alt="Frank" width="100%" />
</p>

# Frank

**A self-hosted control plane for durable AI agent work.**

Frank coordinates projects, agent roles, missions, task graphs, approvals,
budgets, terminals, artifacts, and Git delivery from one auditable system. The
headless Rust daemon runs beside the repositories it operates on; the Flutter
desktop app connects to it over authenticated HTTPS and WebSocket.

Frank started as a Rust rebuild of
[Caveman](https://github.com/JuliusBrussee/caveman), but the current 1.0 codebase
is much broader than a persona or prompt-compression tool. Those capabilities
still exist as local CLI features; the primary product is now the agent
operations workspace.

> **Project status:** Frank 1.0 is under active development and is currently
> intended for source builds and local evaluation. No stable 1.0 desktop
> package has been published from this repository yet. The macOS desktop app is
> the reference client; cross-platform release acceptance is still in progress.

## Why Frank exists

Agent work becomes difficult to trust when its state lives only inside model
conversations. Frank keeps the operational record outside the provider session:

- missions and tasks survive process and daemon restarts;
- task dependencies, claims, reviews, and hand-offs are durable;
- privileged actions pass through explicit policy and approval boundaries;
- measured provider usage stays separate from estimates;
- worktrees, commits, artifacts, terminals, and delivery operations have one
  owner and one audit trail.

The provider executes work. Frank remains the source of truth.

## What is implemented

### Agent operations

- **Projects and missions** with persistent lifecycle state and supervisor plan
  proposals.
- **Role-based teams** with reusable role templates, persistent agents, model
  selection, budgets, capabilities, and revision-safe edits.
- **Taskboards and DAGs** with dependency enforcement, role-aware claims,
  pull-mode work offers, rework, human-input waits, shared activity feeds, and
  durable reviews.
- **Organization workflows** with draft/publish revisions, board routing,
  connector profiles, draining, relocation, and resume semantics.
- **OpenRouter runtime** with structured tool calls, streaming usage telemetry,
  retry boundaries, and resumable session state.
- **Approvals and budgets** enforced by the daemon rather than left to model
  convention.
- **Worktrees and Git delivery** with deterministic branch planning, validation,
  commit/review operations, conflict reporting, and recoverable retries.
- **Artifacts and terminals** with bounded uploads, retention rules, PTY
  streaming, and explicit control leases.
- **Signed updates** with target selection, digest and size validation, staged
  replacement, health checks, and rollback support.

### Desktop office

The Flutter client uses the real authenticated `HttpFrankGateway` when server
configuration is present. It exposes:

- owner login, secure session restoration, password changes, and logout-all;
- Office and project conversations;
- attention, pinned, draft, active, and completed mission shelves;
- Team, Organization, Taskboard, Ledger, and Models & OpenRouter surfaces;
- live snapshot/event refresh through the versioned v2 API;
- a keyboard-aware desktop shell and a stylized `flutter_scene` office floor.

Fixtures remain available for deterministic tests and explicit demo injection.
They are not the production data path. The 3D floor is currently decorative:
agents, desks, selection, status visualization, and physics are not rendered in
the scene yet.

### Local context tools

The `frank` CLI still provides the original local engine:

- persona packs with compile-time byte/token budgets;
- Claude Code lifecycle hooks and declarative Codex/Cline integrations;
- deterministic offline prose compression with reversible backups;
- measured session token accounting with estimates clearly labelled;
- an MCP proxy that compresses selected description fields in transit.

## Architecture

```text
┌──────────────────────────────┐
│ Flutter desktop      frank   │
│ office client        CLI     │
└──────────────┬───────────────┘
               │ authenticated HTTPS / WSS (protocol v2)
┌──────────────▼────────────────────────────────────────────┐
│ frankd                                                    │
│                                                          │
│ auth · snapshots · event streams · policy · approvals    │
│ supervisor · scheduler · OpenRouter sessions · tools      │
│ worktrees · Git · PTYs · artifacts · updates             │
└──────────────┬───────────────────────┬───────────────────┘
               │                       │
       ┌───────▼────────┐      ┌──────▼────────────┐
       │ SQLite WAL     │      │ project worktrees │
       │ event + audit  │      │ and local tools   │
       └────────────────┘      └───────────────────┘
```

The desktop app never opens the database or project filesystem directly.
`frankd` owns persistent state and side effects, including on localhost.

| Component | Responsibility |
| --- | --- |
| `frankd` / `frank-server` | Authenticated API, event fan-out, diagnostics, artifacts, terminals, and daemon lifecycle |
| `frank-orchestrator` | Mission/task state machines, scheduling, approvals, budgets, tools, worktrees, and recovery |
| `frank-store` | SQLite WAL source of truth, migrations, idempotency, projections, audit export, and retention |
| `frank-agent` | Structured OpenRouter runtime, streaming, usage telemetry, and PTYs |
| `frank-protocol` | Versioned command, event, snapshot, capability, and terminal contracts |
| `frank-client` | Reconnecting Rust client with TLS pinning and snapshot/event recovery |
| `apps/frank_desktop` | Remote-only Flutter operator interface |
| `frank` / `frank-cli` | Local hooks, packs, compression, ledger, target installation, and server administration |

See [the architecture guide](docs/architecture.md) for crate boundaries and
security invariants.

## Quick start

### Full macOS demo

This is the shortest path to the daemon and desktop app together. It requires
macOS, Docker Desktop, `curl`, `openssl`, Python 3, and
[Proto](https://moonrepo.dev/proto).

```sh
git clone https://github.com/a-man-called-q/frank.git
cd frank
proto install
moon run demo:run-mac --interactive
```

On first run Frank builds the daemon container, creates a local development TLS
identity, prompts for the single owner account, writes an ignored desktop
configuration file, and launches the Flutter client.

Stop the client and demo server while preserving the Docker volume:

```sh
moon run demo:stop-mac
```

### Run the self-hosted daemon with Docker

```sh
git clone https://github.com/a-man-called-q/frank.git
cd frank

./scripts/dev-cert.sh
docker compose build
docker compose run --rm --no-deps frankd auth init \
  --db /var/lib/frank/frank.sqlite3
docker compose up -d

curl --cacert docker/tls/cert.pem https://127.0.0.1:37465/v2/health
```

The Compose service binds to `127.0.0.1:37465` by default and stores SQLite
state in the `frank-data` volume. LAN deployments must provide TLS whose SAN
matches the client-visible hostname. Read
[the self-hosting guide](docs/self-hosted-auth.md) before exposing the daemon
beyond loopback.

### Build the backend and CLI

The repository pins Rust 1.89.0, Moon 2.4.5, and Flutter 3.47.1 through Proto.

```sh
proto install
moon run frank-rust:build

./target/debug/frank --help
./target/debug/frankd --help
```

Useful local CLI commands:

```sh
# Inspect supported AI-tool targets without changing their configuration
frank targets --detected
frank install --dry-run

# Select a persona pack/level
frank pack list
frank on full
frank status

# Inspect measured token usage
frank stats --all --explain

# Preview deterministic compression
frank compress notes.md --check
```

## Security model

- One locally bootstrapped owner account; no external identity provider or
  signup service.
- Argon2id password hashes and revocable session digests; raw credentials are
  not returned by the API.
- TLS is mandatory away from explicit loopback development paths.
- Commands carry protocol versions, idempotency identities, and optional
  expected revisions.
- Tool access is derived from published Organization capabilities and enforced
  server-side.
- File operations are bounded, atomic, and symlink-safe; install and update
  validation fails closed.
- Measured and estimated token values are never silently combined.

See [SECURITY.md](SECURITY.md) for the supported security scope and reporting
process.

## Current limitations

- Frank 1.0 is not yet distributed as a stable desktop installer or backend
  package from this repository.
- The self-hosted identity model currently has one owner rather than SaaS or
  multi-tenant accounts.
- The macOS desktop client is the active native reference build; Windows and
  Linux release/accessibility acceptance is pending.
- The office floor is a static visual foundation, not a live agent simulation.
- The Journal surface and some release UX remain incomplete.
- Legacy 0.2.x state is intentionally not migrated into the 1.0 data root.

## Development

Run verification through Moon so the pinned toolchains and repository gates are
used consistently:

```sh
# Rust formatting, clippy, tests, doctests, packs, targets, and architecture
moon run frank-rust:verify

# Flutter static analysis and widget/golden tests
moon run frank-desktop:analyze
moon run frank-desktop:test

# Real macOS Flutter GPU render gate
moon run frank-desktop:scene-smoke

# Mandatory CI gate, including coverage and dependency/security checks
moon run frank-scrutiny:strict
```

Repository layout:

```text
backend/crates/       Rust protocol, storage, runtime, daemon, client, and CLI
backend/xtask/        Repository build and validation tasks
apps/frank_desktop/   Flutter desktop client
packs/                Built-in persona packs
targets/              Declarative AI-tool integrations
scripts/              Demo, packaging, update, and scrutiny helpers
docs/                 Architecture, operations, release, and product notes
```

Contributor references:

- [Contributing guide](CONTRIBUTING.md)
- [Architecture](docs/architecture.md)
- [Self-hosted authentication](docs/self-hosted-auth.md)
- [Team and taskboard contract](docs/team-taskboard.md)
- [Release and recovery](docs/release.md)
- [Pack authoring](docs/pack-authoring.md)

## Lineage

Frank's pack, hook, ledger, and compressor foundations were inspired by
[Caveman](https://github.com/JuliusBrussee/caveman). Frank rebuilt those ideas
in Rust with measured token accounting, fail-closed installation, and
symlink-safe IO, then added the independent remote agent-operations platform.

## License

Frank is open source under the [MIT License](LICENSE.md). You may use, copy,
modify, distribute, sublicense, and sell copies of Frank, including for
commercial purposes, provided the copyright and license notice are preserved.
See [NOTICE.md](NOTICE.md) for third-party material that may carry separate
terms.
