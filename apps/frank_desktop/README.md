# Frank Desktop

The Flutter client prototype for Frank's agent operations office.

## Local setup

Use the repository's Proto pin (Flutter 3.47.1), then run from this directory:

```sh
proto install
proto run flutter -- pub get
proto run flutter -- run -d macos
```

The first milestone is intentionally local-only. It uses deterministic fixture
data and does not connect to `frankd`. The shell has a permanent main sidebar,
a central Account Executive conversation, a right-side Projects drawer, and an
empty Flame office surface ready for a later floor implementation.

Forui and FlowUI are pinned exactly because both libraries are still evolving
before 1.0. The app keeps its transport behind `FrankGateway` so the fixture can
later be replaced by the authenticated Frank protocol client without changing
the shell widgets.
