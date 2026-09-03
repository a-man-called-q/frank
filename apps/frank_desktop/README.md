# Frank Desktop

The Flutter client prototype for Frank's agent operations office.

## Local setup

Use the repository's Proto pin (Flutter 3.47.1), then run from this directory:

```sh
proto install
proto run flutter -- pub get
proto run flutter -- run -d macos
```

The macOS host enables Flutter GPU permanently through
`macos/Runner/Info.plist` (`FLTEnableFlutterGPU`). The floor depends on the
exact pre-1.0 pin `flutter_scene: 0.23.0`; run the official asset setup when
starting from a fresh checkout:

```sh
proto run dart -- run flutter_scene:init --no-skills
```

Keep future `.glb`, `.fscene`, and `.fmat` sources under `assets/`. The hook in
`hook/build.dart` compiles them into `flutter_scene_generated/`; that generated
directory is ignored and must not be committed. Re-run the setup after a
package upgrade, inspect the hook/pubspec diff, then run `pub get`, analyze,
tests, build, and the macOS scene smoke.

Moon exposes the same workflow from the repository root:

```sh
moon run frank-desktop:analyze
moon run frank-desktop:test
moon run frank-desktop:build
moon run frank-desktop:scene-smoke
moon run frank-desktop:run --interactive
```

The `run` target is persistent; use `--interactive` so Flutter's hot-reload
keys and stdin are forwarded to the terminal.

The first milestone is intentionally local-only. It uses deterministic fixture
data and does not connect to `frankd`. The shell is desktop-only with a
minimum window width of 880px and a persistent, fixed-width off-canvas sidebar
that can be hidden completely so the main surface uses the full width. The
sidebar provides Office/Projects navigation, attention/pinned/draft/active/
completed mission shelves, scope and inline search, Office sections, a
floating Account Executive conversation, and a static stylized low-poly 3D
office foundation. The room currently has only procedural slab, walls, a
central platform, and Frank accent strips; agents, desks, selection, status,
physics, and live gateway mapping are later milestones. Shell, Projects, Chat,
and floor state are isolated in feature BLoCs/widgets; layout and inbox
preferences stay local to the client.

`flutter test` runs without an Impeller context, so its floor assertions cover
the deterministic loading/error placeholder and semantics. The
`scene-smoke` target launches the actual macOS app, waits for `SceneView`, and
checks that GPU rendering survives sidebar resize and collapse.

Forui and FlowUI are pinned exactly because both libraries are still evolving
before 1.0. The app keeps its transport behind `FrankGateway` so the fixture can
later be replaced by the authenticated Frank protocol client without changing
the shell widgets.
