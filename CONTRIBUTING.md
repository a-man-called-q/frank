# Contributing to Frank

Thank you for your interest in contributing to Frank! This guide will help you get started.

## Getting Started

### Prerequisites

- **Rust**: 1.85 or newer
- **Proto** (optional): For pinned toolchain versions
  - Node 24
  - pnpm 10
  - Moon 2.4.5

### Clone and Build

```bash
git clone https://github.com/a-man-called-q/frank.git
cd frank
cargo build --release -p frank-cli
./target/release/frank --help
```

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test -p frank-ledger

# With verbose output
cargo test --workspace -- --nocapture
```

### Development Workflow

For GUI development:

```bash
pnpm install --frozen-lockfile
moon run frank-gui:dev
```

## Before Committing

Every commit must pass these checks:

### 1. Tests Pass

```bash
cargo test --workspace
```

### 2. Code Formatting

```bash
cargo fmt
```

### 3. No Clippy Warnings

```bash
cargo clippy -- -D warnings
```

### 4. Generated Files Match Source

```bash
cargo run -p xtask -- build-packs
git diff --exit-code packs/
```

This ensures nobody hand-edited generated pack files.

### 5. Target Manifests Valid

```bash
cargo run -p xtask -- lint-targets
```

### Fast Verification Gate

```bash
moon run :verify
```

### Strict Verification Gate (CI)

```bash
moon run :verify-strict
```

Includes coverage, audit, browser tests, and clean generated output checks.

## Project Structure

```
frank/
├── crates/           # Rust crates
│   ├── frank-cli/    # Main binary
│   ├── frank-pack/   # Persona system
│   ├── frank-state/  # State machine
│   ├── frank-ledger/ # Token accounting
│   ├── frank-compress/ # Compression
│   ├── frank-target/ # AI integrations
│   ├── frank-mcp/    # MCP server
│   ├── frank-safeio/ # Security kernel
│   └── frank-app/    # Shared service
├── apps/
│   └── frank-gui/    # Tauri desktop app
├── packs/            # Persona packs
│   └── caveman/      # Default pack
├── targets/          # AI assistant integrations
├── xtask/            # Build tasks
└── docs/             # Documentation
```

## How to Contribute

### Reporting Bugs

Open an issue with:
- Clear description of the problem
- Steps to reproduce
- Expected vs. actual behavior
- Your environment (OS, Rust version, Frank version)
- Relevant logs or error messages

### Suggesting Features

Open an issue with:
- Clear description of the feature
- Use case and motivation
- Proposed implementation (if you have ideas)
- Potential impact on existing functionality

### Pull Requests

1. **Fork the repository**
2. **Create a feature branch** from `main`
   ```bash
   git checkout -b feature/my-feature
   ```
3. **Make your changes**
4. **Run all verification checks** (see "Before Committing")
5. **Write tests** for new functionality
6. **Update documentation** if changing public APIs
7. **Commit with clear messages**
   ```bash
   git commit -m "feat(ledger): add per-session cost breakdown"
   ```
8. **Push to your fork**
   ```bash
   git push origin feature/my-feature
   ```
9. **Open a pull request** against `main`

### Commit Message Format

We follow conventional commits:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types**:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, no code change
- `refactor`: Code restructuring, no behavior change
- `perf`: Performance improvement
- `test`: Adding/updating tests
- `chore`: Maintenance, dependencies

**Scopes**: `cli`, `pack`, `state`, `ledger`, `compress`, `target`, `mcp`, `safeio`, `app`, `gui`

**Examples**:
```
feat(ledger): add per-session cost breakdown
fix(target): handle missing checksum manifest correctly
docs(pack): update authoring guide with new syntax
refactor(compress): simplify markdown parser
```

## Development Guidelines

### Code Style

- Follow Rust standard style (`cargo fmt`)
- Use meaningful variable names
- Add comments for complex logic
- Keep functions focused and small
- Prefer explicit over clever

### Testing Philosophy

- **Unit tests**: Test individual functions/modules
- **Integration tests**: Test cross-crate interactions
- **Fixture tests**: Compare against immutable oracles
- **No mocking**: Test against real files/processes where practical

### Adding New Features

#### New Crate

1. Create under `crates/`
2. Update `Cargo.toml` workspace members
3. Update dependency graph in `docs/architecture.md`
4. Add README explaining purpose
5. Add tests

#### New Target (AI Integration)

1. Create TOML in `targets/`
2. Follow schema from existing targets
3. Add detection logic if needed
4. Test install/verify/remove
5. Add to supported list in README

#### New Persona Pack

Follow the guide in `docs/pack-authoring.md`.

Key points:
- Define `pack.toml` with metadata and budget
- Create fragment files
- Compile with `cargo run -p xtask -- build-packs`
- Test budget enforcement

#### Security-Sensitive Changes

If touching file I/O, path handling, or anything security-related:

1. Review `frank-safeio` contracts
2. Ensure symlink protection (`O_NOFOLLOW`)
3. Validate path traversal prevention
4. Add security-focused tests
5. Document security assumptions

## Architecture Overview

For deep technical details, see [`docs/architecture.md`](docs/architecture.md).

Key principles:

1. **Honest measurement**: Token accounting is non-negotiable
2. **Fail closed**: Security and integrity checks refuse on error
3. **Compile-time enforcement**: Budget limits enforced at build time
4. **Symlink safety**: All file I/O uses `O_NOFOLLOW`
5. **Plan before execute**: Install returns plan, executor applies

## Testing

### Running Specific Tests

```bash
# Crate tests
cargo test -p frank-ledger

# Single test
cargo test -p frank-ledger test_attribution_algorithm

# Integration tests
cargo test --test '*'

# Doc tests
cargo test --doc
```

### Test Fixtures

**Critical**: Fixtures in `crates/frank-compress/tests/fixtures/` are **immutable**. They represent the original Caveman's ground truth. Never edit them to make tests pass—fix the code instead.

### Smoke Tests

Test packaged binaries:

```bash
FRANK_GUI_BINARY=/path/to/Frank moon run frank-release:native-smoke
```

## Documentation

### Code Documentation

- Add doc comments to public APIs
- Include examples in doc comments
- Run `cargo doc --open` to preview

### User Documentation

- Update README.md for user-facing changes
- Update pack authoring guide for pack system changes
- Update docs/architecture.md for internal architecture changes

## Getting Help

- **Questions**: Open a discussion on GitHub
- **Bugs**: Open an issue
- **Chat**: Join our community (link TBD)
- **Architecture questions**: Read `docs/architecture.md` first

## Code of Conduct

Be respectful and constructive. We're all here to make Frank better.

- Be kind and patient
- Respect different viewpoints
- Accept constructive criticism
- Focus on what's best for the project
- Show empathy toward other contributors

## License

By contributing, you agree that your contributions will be licensed under the MIT License. See [`LICENSE.md`](LICENSE.md) for details.

## Recognition

Contributors will be recognized in:
- Git commit history
- Release notes for significant contributions
- Future CONTRIBUTORS.md file

Thank you for contributing to Frank! 🚀
