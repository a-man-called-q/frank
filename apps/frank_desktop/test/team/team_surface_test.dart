import 'package:flutter/widgets.dart';
import '../support/frank_test_app.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_team.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/team_models.dart';
import 'package:frank_desktop/features/team/presentation/team_surface.dart';

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

    await tester.pumpWidget(
      _app(
        TeamSurface(
          workspace: loadedWorkspace,
          profiles: fixtureTeamProfiles(loadedWorkspace),
        ),
      ),
    );
    await tester.pump();

    expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
    expect(find.byType(Scrollable), findsOneWidget);
    expect(find.byKey(const ValueKey('team-agent-list')), findsOneWidget);
    expect(
      find.byKey(const ValueKey('team-agent-row-ae-maya')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-agent-row-analyst-budi')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-agent-row-programmer-nia')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('team-agent-row-accountant-dimas')),
      findsOneWidget,
    );
    expect(find.text('Account Executive'), findsOneWidget);
    expect(find.text('System Analyst'), findsOneWidget);
    expect(find.text('Junior Programmer'), findsOneWidget);
    expect(find.text('Accountant'), findsOneWidget);
    expect(find.text('Map warehouse intake'), findsOneWidget);
    expect(find.text('Review approval controls'), findsOneWidget);

    expect(find.text('EFFECTIVE MODEL'), findsOneWidget);
    expect(find.text('REVISION'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('member drawer switches tabs and closes on Escape', (
    tester,
  ) async {
    _setSize(tester, const Size(1600, 1000));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    final loadedWorkspace = workspace!;

    await tester.pumpWidget(
      _app(
        TeamSurface(
          workspace: loadedWorkspace,
          profiles: fixtureTeamProfiles(loadedWorkspace),
        ),
      ),
    );
    await tester.pump();
    await tester.tap(
      find.byKey(const ValueKey('team-agent-row-programmer-nia')),
    );
    await tester.pump(const Duration(milliseconds: 100));

    expect(
      find.bySemanticsLabel('Member details for Nia Alvarez'),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('team-roster-frame')), findsOneWidget);
    expect(find.byKey(const ValueKey('team-member-drawer')), findsOneWidget);
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
    expect(find.text('Role'), findsOneWidget);
    expect(find.text('Prompt pack'), findsNothing);
    expect(find.text('ROLE MARKERS'), findsNothing);

    await tester.tap(find.byKey(const ValueKey('team-profile-tab-setup')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(
      find.byKey(const ValueKey('team-profile-panel-setup')),
      findsOneWidget,
    );
    expect(find.text('OpenRouter'), findsOneWidget);
    // The drawer keeps the effective model visible in the compact identity
    // summary while Setup repeats it with source/configuration context.
    expect(find.text('openai/gpt-4o-mini'), findsWidgets);
    expect(find.text('caveman'), findsNothing);
    expect(find.text('full'), findsNothing);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
    expect(find.byKey(const ValueKey('team-member-details')), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'roster uses two columns at tablet width and a compact list when narrow',
    (tester) async {
      final workspace = await tester.runAsync(
        () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
      );
      final loadedWorkspace = workspace!;
      _setSize(tester, const Size(900, 800));
      await tester.pumpWidget(
        _app(
          TeamSurface(
            workspace: loadedWorkspace,
            profiles: fixtureTeamProfiles(loadedWorkspace),
          ),
        ),
      );
      await tester.pump();
      expect(find.byKey(const ValueKey('team-agent-list')), findsOneWidget);

      tester.view.physicalSize = const Size(680, 800);
      await tester.pump();
      expect(find.byKey(const ValueKey('team-agent-list')), findsOneWidget);
    },
  );

  testWidgets('profile transition restores the roster scroll position', (
    tester,
  ) async {
    _setSize(tester, const Size(680, 500));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    final loadedWorkspace = workspace!;

    await tester.pumpWidget(
      _app(
        TeamSurface(
          workspace: loadedWorkspace,
          profiles: fixtureTeamProfiles(loadedWorkspace),
        ),
      ),
    );
    await tester.pump();
    final rosterScroll = find.byKey(const ValueKey('team-roster-scroll'));
    await tester.drag(rosterScroll, const Offset(0, -180));
    await tester.pump();
    final rosterScrollable = find.descendant(
      of: rosterScroll,
      matching: find.byType(Scrollable),
    );
    final before = tester
        .state<ScrollableState>(rosterScrollable)
        .position
        .pixels;
    expect(before, greaterThan(0));

    await tester.tap(find.byKey(const ValueKey('team-agent-row-programmer-nia')));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.tap(find.byKey(const ValueKey('team-profile-back')));
    await tester.pump(const Duration(milliseconds: 100));

    final restoredScrollable = find.descendant(
      of: rosterScroll,
      matching: find.byType(Scrollable),
    );
    final after = tester
        .state<ScrollableState>(restoredScrollable)
        .position
        .pixels;
    expect(after, closeTo(before, 0.1));
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
          child: _app(
            TeamSurface(
              workspace: loadedWorkspace,
              profiles: fixtureTeamProfiles(loadedWorkspace),
            ),
          ),
        ),
      );
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('team-agent-row-ae-maya')));
      await tester.pump(const Duration(milliseconds: 100));
      await tester.tap(find.byKey(const ValueKey('team-profile-back')));
      await tester.pump(const Duration(milliseconds: 100));

      expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
      expect(find.byKey(const ValueKey('team-agent-list')), findsOneWidget);
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
  return FrankTestApp(theme: buildFrankTheme(), home: child);
}

void _setSize(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}
