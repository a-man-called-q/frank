import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/bloc/shell_bloc.dart';
import 'package:frank_desktop/features/shell/bloc/shell_preferences.dart';
import 'package:frank_desktop/features/shell/sidebar_layout.dart';

import '../support/fake_gateway.dart';

void main() {
  late FakeGateway retryGateway;

  test('Office defaults to the project inbox', () {
    const state = ShellState();
    expect(state.activeView, WorkspaceView.office);
    expect(state.settingsSection, isNull);
    expect(SettingsSection.values.map((section) => section.label), [
      'Models & OpenRouter',
      'Organization',
      'Team',
      'Ledger',
      'Taskboard',
      'Journal',
    ]);
  });

  blocTest<ShellBloc, ShellState>(
    'Settings sections are typed and returning from Office resets Organization',
    build: () => ShellBloc(gateway: FakeGateway()),
    act: (bloc) {
      bloc.add(const ShellSettingsSectionSelected(SettingsSection.journal));
      bloc.add(const ShellViewSelected(WorkspaceView.office));
      bloc.add(const ShellViewSelected(WorkspaceView.settings));
    },
    expect: () => [
      predicate<ShellState>(
        (state) => state.settingsSection == SettingsSection.journal,
      ),
      predicate<ShellState>(
        (state) => state.activeView == WorkspaceView.office,
      ),
      predicate<ShellState>(
        (state) => state.settingsSection == SettingsSection.organization,
      ),
    ],
  );

  test('shell destinations always expose a workspace view', () {
    const office = ShellState(destination: OfficeDestination());
    const settings = ShellState(destination: SettingsDestination());

    expect(office.activeView, WorkspaceView.office);
    expect(settings.activeView, WorkspaceView.settings);
    expect(office.activeView, isNotNull);
    expect(settings.activeView, isNotNull);
  });

  blocTest<ShellBloc, ShellState>(
    'loads the workspace and reports ready',
    build: () => ShellBloc(gateway: FakeGateway()),
    act: (bloc) => bloc.add(const ShellStarted()),
    wait: const Duration(milliseconds: 220),
    expect: () => [
      predicate<ShellState>((state) => state.isLoading),
      predicate<ShellState>(
        (state) => state.isReady && state.workspace?.name == 'Frank Agency',
      ),
    ],
  );

  blocTest<ShellBloc, ShellState>(
    'reports load failures and can retry',
    build: () =>
        ShellBloc(gateway: FakeGateway(loadError: StateError('offline'))),
    act: (bloc) => bloc.add(const ShellStarted()),
    wait: const Duration(milliseconds: 220),
    expect: () => [
      predicate<ShellState>((state) => state.isLoading),
      predicate<ShellState>(
        (state) => state.hasError && state.error!.contains('offline'),
      ),
    ],
  );

  blocTest<ShellBloc, ShellState>(
    'retries a failed load after the gateway recovers',
    build: () {
      final gateway = FakeGateway(loadError: StateError('offline'));
      retryGateway = gateway;
      return ShellBloc(gateway: gateway);
    },
    act: (bloc) async {
      bloc.add(const ShellStarted());
      await Future<void>.delayed(const Duration(milliseconds: 220));
      retryGateway.loadError = null;
      bloc.add(const ShellRetryRequested());
    },
    wait: const Duration(milliseconds: 420),
    expect: () => [
      predicate<ShellState>((state) => state.isLoading),
      predicate<ShellState>((state) => state.hasError),
      predicate<ShellState>((state) => state.isLoading),
      predicate<ShellState>((state) => state.isReady),
    ],
  );

  test('fixture remains compatible with shell state loading', () async {
    final workspace = await FixtureFrankGateway().loadWorkspace();
    expect(workspace.projects, isNotEmpty);
  });

  test('restores visibility and drops an invalid scope', () async {
    final preferences = MemoryShellPreferences(
      initial: const ShellPreferenceSnapshot(
        sidebarVisible: false,
        projectScope: 'missing-project',
        pinnedMissionIds: ['mission-2', 'mission-1'],
      ),
    );
    final bloc = ShellBloc(gateway: FakeGateway(), preferences: preferences);
    bloc.add(const ShellStarted());
    await Future<void>.delayed(const Duration(milliseconds: 240));

    expect(bloc.state.isReady, isTrue);
    expect(bloc.state.sidebarVisible, isFalse);
    expect(bloc.state.projectScope, isNull);
    expect(bloc.state.pinnedMissionIds, ['mission-2', 'mission-1']);
    expect(bloc.state.preferencesStatus, ShellPreferencesStatus.ready);
    await bloc.close();
  });

  test(
    'preference failure falls back without blocking the workspace',
    () async {
      final preferences = MemoryShellPreferences()
        ..loadError = StateError('storage unavailable');
      final bloc = ShellBloc(gateway: FakeGateway(), preferences: preferences);
      bloc.add(const ShellStarted());
      await Future<void>.delayed(const Duration(milliseconds: 240));

      expect(bloc.state.isReady, isTrue);
      expect(bloc.state.preferencesStatus, ShellPreferencesStatus.fallback);
      expect(bloc.state.sidebarVisible, isTrue);
      await bloc.close();
    },
  );

  test('toggle, scope, and pin order persist best effort', () async {
    final preferences = MemoryShellPreferences();
    final bloc = ShellBloc(gateway: FakeGateway(), preferences: preferences);
    bloc.add(const ShellStarted());
    await Future<void>.delayed(const Duration(milliseconds: 240));

    bloc.add(const ShellSidebarToggled());
    bloc.add(const ShellProjectScopeChanged('meridian-finance'));
    bloc.add(const ShellPinnedMissionOrderChanged(['m2', 'm1']));
    await Future<void>.delayed(const Duration(milliseconds: 30));

    expect(bloc.state.sidebarVisible, isFalse);
    expect(bloc.state.projectScope, 'meridian-finance');
    expect(bloc.state.pinnedMissionIds, ['m2', 'm1']);
    expect(preferences.snapshot.sidebarVisible, isFalse);
    expect(preferences.snapshot.projectScope, 'meridian-finance');
    expect(preferences.snapshot.pinnedMissionIds, ['m2', 'm1']);
    await bloc.close();
  });

  test(
    'resize commits bounds, snaps partial widths, and preserves collapse width',
    () async {
      final preferences = MemoryShellPreferences();
      final bloc = ShellBloc(gateway: FakeGateway(), preferences: preferences);
      bloc.add(const ShellStarted());
      await Future<void>.delayed(const Duration(milliseconds: 240));

      bloc.add(const ShellSidebarResizeEnded(401));
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(bloc.state.sidebarVisible, isTrue);
      expect(bloc.state.sidebarWidth, 401);
      expect(preferences.snapshot.sidebarWidth, 401);

      bloc.add(const ShellSidebarResizeEnded(999));
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(bloc.state.sidebarWidth, SidebarLayout.maxWidth);

      bloc.add(const ShellSidebarResizeEnded(230));
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(bloc.state.sidebarVisible, isTrue);
      expect(bloc.state.sidebarWidth, SidebarLayout.minWidth);
      expect(preferences.snapshot.sidebarWidth, SidebarLayout.minWidth);

      bloc.add(const ShellSidebarResizeEnded(190));
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(bloc.state.sidebarVisible, isFalse);
      expect(bloc.state.sidebarWidth, SidebarLayout.minWidth);
      expect(preferences.snapshot.sidebarWidth, SidebarLayout.minWidth);

      bloc.add(const ShellSidebarToggled());
      await Future<void>.delayed(const Duration(milliseconds: 30));
      expect(bloc.state.sidebarVisible, isTrue);
      expect(bloc.state.sidebarWidth, SidebarLayout.minWidth);
      expect(preferences.snapshot.sidebarVisible, isTrue);
      await bloc.close();
    },
  );

  test(
    'restored sidebar width is normalized before entering the shell',
    () async {
      final preferences = MemoryShellPreferences(
        initial: const ShellPreferenceSnapshot(sidebarWidth: 999),
      );
      final bloc = ShellBloc(gateway: FakeGateway(), preferences: preferences);
      bloc.add(const ShellStarted());
      await Future<void>.delayed(const Duration(milliseconds: 240));

      expect(bloc.state.sidebarWidth, SidebarLayout.maxWidth);
      await bloc.close();
    },
  );

  test(
    'write failures never turn layout events into a workspace error',
    () async {
      final preferences = MemoryShellPreferences()
        ..writeError = StateError('disk');
      final bloc = ShellBloc(gateway: FakeGateway(), preferences: preferences);
      bloc.add(const ShellStarted());
      await Future<void>.delayed(const Duration(milliseconds: 240));
      bloc.add(const ShellSidebarToggled());
      await Future<void>.delayed(const Duration(milliseconds: 30));

      expect(bloc.state.isReady, isTrue);
      expect(bloc.state.sidebarVisible, isFalse);
      expect(bloc.state.error, isNull);
      await bloc.close();
    },
  );
}
