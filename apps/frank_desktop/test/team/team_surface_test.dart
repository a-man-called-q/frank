import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_team.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/team_models.dart';
import 'package:frank_desktop/features/team/team_surface.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('roster presents all fixture agents and operational summaries', (
    tester,
  ) async {
    _setSize(tester, const Size(1600, 1000));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    final loadedWorkspace = workspace!;

    await tester.pumpWidget(_app(TeamSurface(workspace: loadedWorkspace)));
    await tester.pump();

    expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
    expect(find.byKey(const ValueKey('team-agent-grid')), findsOneWidget);
    expect(
      find.byKey(const ValueKey('team-agent-card-ae-maya')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-agent-card-analyst-budi')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-agent-card-programmer-nia')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-agent-card-accountant-dimas')),
      findsOneWidget,
    );
    expect(find.text('Generalist'), findsOneWidget);
    expect(find.text('Researcher'), findsOneWidget);
    expect(find.text('Builder'), findsOneWidget);
    expect(find.text('Reviewer'), findsOneWidget);
    expect(find.text('Map warehouse intake'), findsOneWidget);
    expect(find.text('Review approval controls'), findsOneWidget);

    final grid = tester.widget<GridView>(
      find.byKey(const ValueKey('team-agent-grid')),
    );
    final delegate =
        grid.gridDelegate as SliverGridDelegateWithFixedCrossAxisCount;
    expect(delegate.crossAxisCount, 4);
    expect(tester.takeException(), isNull);
  });

  testWidgets('profile opens inline, switches tabs, and closes on Escape', (
    tester,
  ) async {
    _setSize(tester, const Size(1600, 1000));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    final loadedWorkspace = workspace!;

    await tester.pumpWidget(_app(TeamSurface(workspace: loadedWorkspace)));
    await tester.pump();
    await tester.tap(
      find.byKey(const ValueKey('team-agent-card-programmer-nia')),
    );
    await tester.pump();

    expect(
      find.bySemanticsLabel('Character profile for Nia Alvarez'),
      findsOneWidget,
    );
    expect(find.text('Design replenishment dashboard'), findsWidgets);
    expect(
      find.byKey(const ValueKey('team-profile-tab-overview')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-profile-panel-overview')),
      findsOneWidget,
    );

    await tester.tap(find.byKey(const ValueKey('team-profile-tab-identity')));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('team-profile-panel-identity')),
      findsOneWidget,
    );
    expect(find.text('Curious'), findsOneWidget);
    expect(find.text('Practical'), findsOneWidget);
    expect(find.text('Methodical'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('team-profile-tab-setup')));
    await tester.pump();
    expect(
      find.byKey(const ValueKey('team-profile-panel-setup')),
      findsOneWidget,
    );
    expect(find.text('Codex'), findsOneWidget);
    expect(find.text('Default'), findsOneWidget);
    expect(find.text('caveman'), findsNWidgets(2));
    expect(find.text('full'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
    expect(find.byKey(const ValueKey('team-character-profile')), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('roster uses two columns at tablet width and one when compact', (
    tester,
  ) async {
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    final loadedWorkspace = workspace!;
    _setSize(tester, const Size(900, 800));
    await tester.pumpWidget(_app(TeamSurface(workspace: loadedWorkspace)));
    await tester.pump();
    var grid = tester.widget<GridView>(
      find.byKey(const ValueKey('team-agent-grid')),
    );
    var delegate =
        grid.gridDelegate as SliverGridDelegateWithFixedCrossAxisCount;
    expect(delegate.crossAxisCount, 2);

    tester.view.physicalSize = const Size(680, 800);
    await tester.pump();
    grid = tester.widget<GridView>(
      find.byKey(const ValueKey('team-agent-grid')),
    );
    delegate = grid.gridDelegate as SliverGridDelegateWithFixedCrossAxisCount;
    expect(delegate.crossAxisCount, 1);
  });

  testWidgets(
    'back button returns to roster and reduced motion remains stable',
    (tester) async {
      _setSize(tester, const Size(900, 800));
      final workspace = await tester.runAsync(
        () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
      );
      final loadedWorkspace = workspace!;

      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: _app(TeamSurface(workspace: loadedWorkspace)),
        ),
      );
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('team-agent-card-ae-maya')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('team-profile-back')));
      await tester.pump();

      expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
      expect(find.byKey(const ValueKey('team-agent-grid')), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  test(
    'fixture profile data maps existing employee IDs without gateway changes',
    () async {
      final workspace = await FixtureFrankGateway(
        latency: Duration.zero,
      ).loadWorkspace();
      final profiles = fixtureTeamProfiles(workspace);

      expect(profiles, hasLength(4));
      expect(profiles.map((profile) => profile.employeeId), [
        'ae-maya',
        'analyst-budi',
        'programmer-nia',
        'accountant-dimas',
      ]);
      expect(profiles[1].status, TeamAgentStatus.working);
      expect(profiles[2].capabilities.map((capability) => capability.label), [
        'Terminal',
        'Database inspect',
        'Drive',
      ]);
    },
  );
}

Widget _app(Widget child) {
  return MaterialApp(theme: buildFrankTheme(Brightness.dark), home: child);
}

void _setSize(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}
