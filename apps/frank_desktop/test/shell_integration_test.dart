import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/core/models/team_models.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

import 'support/fake_gateway.dart';

void main() {
  testWidgets('injected gateway failure can recover through retry', (
    tester,
  ) async {
    _setDesktopSize(tester);
    final gateway = FakeGateway(loadError: StateError('offline'));
    await tester.pumpWidget(FrankApp(gateway: gateway, showLogin: false));
    await tester.pump(const Duration(milliseconds: 20));

    expect(
      find.textContaining('Could not load the local workspace.'),
      findsOneWidget,
    );
    expect(find.textContaining('offline'), findsOneWidget);

    gateway.loadError = null;
    await tester.tap(find.text('Retry'));
    await tester.pump(const Duration(milliseconds: 500));

    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(
      find.bySemanticsLabel(RegExp(r'Office floor (loading|unavailable)')),
      findsOneWidget,
    );
  });

  testWidgets('a rejected Team projection can be retried after reopening', (
    tester,
  ) async {
    _setDesktopSize(tester);
    final gateway = _TeamProfilesRetryGateway(
      teamProfilesError: StateError('offline'),
    );
    await tester.pumpWidget(FrankApp(gateway: gateway, showLogin: false));
    await tester.pump(const Duration(milliseconds: 500));

    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.byKey(const ValueKey('settings-section-team')));
    await tester.pump(const Duration(milliseconds: 500));

    gateway.teamProfilesError = null;
    await tester.tap(
      find.byKey(const ValueKey('settings-section-organization')),
    );
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.byKey(const ValueKey('settings-section-team')));
    await tester.pump(const Duration(seconds: 2));
    expect(
      find.byKey(const ValueKey('team-agent-row-ae-maya')),
      findsOneWidget,
    );
  });

  testWidgets('Office startup keeps Organization lazy until Settings opens', (
    tester,
  ) async {
    _setDesktopSize(tester);
    final gateway = FakeGateway();
    await tester.pumpWidget(FrankApp(gateway: gateway, showLogin: false));
    await tester.pump(const Duration(milliseconds: 500));

    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(
      find.byKey(const ValueKey('mission-shelf-scroll-view')),
      findsOneWidget,
    );
    expect(gateway.organizationLoadCalls, 0);

    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 500));

    expect(find.bySemanticsLabel('Organization flow editor'), findsOneWidget);
    expect(gateway.organizationLoadCalls, 1);
  });

  testWidgets('an empty injected workspace opens Office safely', (
    tester,
  ) async {
    _setDesktopSize(tester);
    const workspace = OfficeWorkspace(
      name: 'Empty workspace',
      projects: [],
      employees: [],
      accountExecutive: OfficeEmployee(
        id: 'ae',
        name: 'Maya Chen',
        role: 'Account Executive',
        status: 'Available',
        initials: 'MC',
        color: 0xFF9A68A5,
      ),
    );
    await tester.pumpWidget(
      FrankApp(gateway: FakeGateway(workspace: workspace), showLogin: false),
    );
    await tester.pump(const Duration(milliseconds: 20));

    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(find.text('No projects yet'), findsOneWidget);
    expect(
      find.text('Projects will appear here after you create one.'),
      findsOneWidget,
    );
    expect(find.bySemanticsLabel('Organization flow editor'), findsNothing);

    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.text('Organization'), findsWidgets);
    expect(find.text('Maya Chen'), findsOneWidget);
    expect(find.text('Email'), findsOneWidget);
    expect(find.bySemanticsLabel('Organization flow editor'), findsOneWidget);
  });

  testWidgets('Organization drawer covers the shell from the root top', (
    tester,
  ) async {
    _setDesktopSize(tester);
    final gateway = FakeGateway();
    await tester.pumpWidget(FrankApp(gateway: gateway, showLogin: false));
    await tester.pump(const Duration(milliseconds: 500));

    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 500));
    final sidebar = tester.getRect(find.byKey(const ValueKey('sidebar-slot')));
    final surfaceBefore = tester.getRect(
      find.byKey(const ValueKey('main-surface')),
    );
    await tester.tap(find.text('Maya Chen'));
    await tester.pump();

    final drawer = tester.getRect(
      find.byKey(const ValueKey('organization-inspector-rail')),
    );
    final contextBar = tester.getRect(
      find.byKey(const ValueKey('main-context-strip')),
    );
    expect(drawer.top, 0);
    expect(drawer.bottom, tester.view.physicalSize.height);
    expect(drawer.right, tester.view.physicalSize.width);
    expect(drawer.left, lessThan(drawer.right));
    expect(drawer.left, greaterThanOrEqualTo(sidebar.right));
    expect(
      tester.getRect(find.byKey(const ValueKey('main-surface'))),
      surfaceBefore,
    );
    expect(contextBar.top, 0);
    await tester.pump(const Duration(milliseconds: 80));
  });
}

class _TeamProfilesRetryGateway extends FakeGateway {
  _TeamProfilesRetryGateway({super.teamProfilesError});

  @override
  Future<List<TeamAgentProfile>> loadTeamProfiles() {
    final error = teamProfilesError;
    if (error != null) return Future<List<TeamAgentProfile>>.error(error);
    return super.loadTeamProfiles();
  }

  @override
  Future<List<TeamRoleSummary>> loadTeamRoles() async => const [];
}

void _setDesktopSize(WidgetTester tester) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(1600, 1000);
  addTearDown(tester.view.reset);
}
