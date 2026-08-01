# Pack authoring

Frank packs are data-only persona directories. A pack contains `pack.toml` and
the Markdown files referenced by it; it cannot run shell commands or hooks.
The compiler validates the manifest, inheritance graph, activation regexes,
and `[pack.budget]` before a pack is installed.

## Minimal pack

```text
mypack/
├── pack.toml
└── levels/
    └── full.md
```

```toml
schema = 1

[pack]
id = "mypack"
version = "1.0.0"
default_level = "full"

[pack.budget]
max_activation_bytes = 1200
max_reinforce_bytes = 220

[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
reinforce = "PACK ACTIVE. Keep answers concise."
```

Build and preview a pack without installing it:

```sh
frank pack build ./mypack
frank pack show caveman
```

Install a local pack. `add` compiles it, prints the default activation prompt,
computes a stable SHA-256 digest over relative paths and file bytes, and writes
the copy plus metadata to `$XDG_DATA_HOME/frank/` (or
`~/.local/share/frank/`). Use `--yes` for non-interactive use.

```sh
frank pack add ./mypack --yes
frank pack list
frank pack use mypack
frank levels
frank pack remove mypack
```

`packs.lock` records the selected pack, source path, installed relative path,
version, and digest. Every hook recompiles and verifies the selected copy. If
the copy is missing or changed, the hook emits nothing and exits zero; normal
commands report the error instead of silently switching personas.

## Held work

<!-- HOLD(M7): remote sources need a deliberately reviewed downloader, proxy/
certificate policy, and end-to-end network tests. They are not accepted as a
local path by accident. -->

`github:owner/repo@v1.2.0` and HTTPS sources are intentionally reported as
`HOLD(M7)` for now. The local lifecycle is complete and testable; remote
registry/download support should be added only with checksum/signature policy
and a CI test matrix.

<!-- HOLD(M7): runtime activation has been implemented for local packs. -->

Third-party packs may use only manifest data and Markdown. Keep activation
patterns narrow, include a budget, and add a benchmark entry only when the
measurement method, sample count, spread, and model are known.
