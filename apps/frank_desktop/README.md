# Frank Desktop

The Flutter client prototype for Frank's agent operations office.

## Local setup

Use the repository's Proto pin (Flutter 3.47.1), then run from this directory:

```sh
proto install
proto run flutter -- pub get
proto run flutter -- run -d macos
```

Moon exposes the same workflow from the repository root:

```sh
moon run frank-desktop:analyze
moon run frank-desktop:test
moon run frank-desktop:build
moon run frank-desktop:run --interactive
```

The `run` target is persistent; use `--interactive` so Flutter's hot-reload
keys and stdin are forwarded to the terminal.

The first milestone is intentionally local-only. It uses deterministic fixture
data and does not connect to `frankd`. The shell is desktop-only with a
minimum window width of 880px and a persistent, fixed-width off-canvas sidebar
that can be hidden completely so the main surface uses the full width. The
sidebar provides Office/Projects navigation, attention/pinned/draft/active/
completed mission shelves, scope and inline search, Settings sections, a
floating Account Executive conversation, and an empty office floor ready for a
later floor implementation. Shell, Projects, and Chat state are isolated in
feature BLoCs; layout and inbox preferences stay local to the client.

Forui and FlowUI are pinned exactly because both libraries are still evolving
before 1.0. The app keeps its transport behind `FrankGateway` so the fixture can
later be replaced by the authenticated Frank protocol client without changing
the shell widgets.
