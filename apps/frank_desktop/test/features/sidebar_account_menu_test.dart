import 'package:flutter/widgets.dart';
import '../support/frank_test_app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:forui/forui.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/core/auth/auth_repository.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/main_sidebar.dart';
import 'package:frank_desktop/features/shell/office_shell.dart';

void main() {
  testWidgets('authenticated account menu exposes actions and Lucide icons', (
    tester,
  ) async {
    _setWindow(tester);
    final auth = await _authenticatedAuth();
    addTearDown(auth.dispose);

    await _pumpShell(
      tester,
      auth,
      onLogout: () async {},
      onLogoutAll: () async {},
      onChangePassword: (_, _) async {},
    );

    final trigger = find.byKey(const ValueKey('sidebar-user-button'));
    expect(trigger, findsOneWidget);
    final semantics = tester.getSemantics(trigger);
    expect(semantics.label, 'Account owner');
    expect(semantics.hint, 'Open account settings');

    await tester.tap(trigger);
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('sidebar-user-menu')), findsOneWidget);
    expect(find.byType(FPopoverMenu), findsOneWidget);
    expect(
      tester
          .getRect(
            find.byKey(
              const ValueKey('sidebar-account-action-change-password'),
            ),
          )
          .width,
      closeTo(tester.getRect(trigger).width, 0.01),
    );
    expect(find.text('Change password'), findsOneWidget);
    expect(find.text('Log out all devices'), findsOneWidget);
    expect(find.text('Log out'), findsOneWidget);
    expect(find.byIcon(FrankIcons.keyRound), findsOneWidget);
    expect(find.byIcon(FrankIcons.monitorSmartphone), findsOneWidget);
    expect(find.byIcon(FrankIcons.logOut), findsOneWidget);
    for (final key in [
      'sidebar-account-action-change-password',
      'sidebar-account-action-logout-all',
      'sidebar-account-action-logout',
    ]) {
      expect(tester.widget<FItem>(find.byKey(ValueKey(key))).enabled, isTrue);
    }
    expect(find.byIcon(FrankIcons.logOut), findsOneWidget);
  });

  testWidgets('account menu keeps unavailable actions disabled', (
    tester,
  ) async {
    _setWindow(tester);
    final auth = await _authenticatedAuth();
    addTearDown(auth.dispose);

    await _pumpShell(tester, auth);
    await tester.tap(find.byKey(const ValueKey('sidebar-user-button')));
    await tester.pumpAndSettle();

    for (final key in [
      'sidebar-account-action-change-password',
      'sidebar-account-action-logout-all',
      'sidebar-account-action-logout',
    ]) {
      expect(tester.widget<FItem>(find.byKey(ValueKey(key))).enabled, isFalse);
    }
  });

  testWidgets('account menu invokes logout and change-password callbacks', (
    tester,
  ) async {
    _setWindow(tester);
    final auth = await _authenticatedAuth();
    addTearDown(auth.dispose);
    var logoutCalls = 0;
    var logoutAllCalls = 0;
    String? currentPassword;
    String? newPassword;

    await _pumpShell(
      tester,
      auth,
      withToaster: false,
      onLogout: () async => logoutCalls++,
      onLogoutAll: () async => logoutAllCalls++,
      onChangePassword: (current, next) async {
        currentPassword = current;
        newPassword = next;
      },
    );

    await tester.tap(find.byKey(const ValueKey('sidebar-user-button')));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('sidebar-account-action-logout-all')),
    );
    await tester.pumpAndSettle();
    expect(logoutAllCalls, 1);

    await tester.tap(find.byKey(const ValueKey('sidebar-user-button')));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('sidebar-account-action-logout')),
    );
    await tester.pumpAndSettle();
    expect(logoutCalls, 1);
    expect(find.text('Log out'), findsNothing);

    await tester.tap(find.byKey(const ValueKey('sidebar-user-button')));
    await tester.pumpAndSettle();
    await tester.tap(
      find.byKey(const ValueKey('sidebar-account-action-change-password')),
    );
    await tester.pumpAndSettle();
    expect(find.text('Current password'), findsOneWidget);
    await tester.enterText(find.bySemanticsLabel('Current password'), 'old');
    await tester.enterText(
      find.bySemanticsLabel('New password'),
      'a-long-enough-password',
    );
    await tester.tap(find.widgetWithText(FButton, 'Change password'));
    await tester.pumpAndSettle();
    expect(currentPassword, 'old');
    expect(newPassword, 'a-long-enough-password');
  });

  testWidgets('account menu dismisses an open project action menu', (
    tester,
  ) async {
    _setWindow(tester);
    final auth = await _authenticatedAuth();
    addTearDown(auth.dispose);

    await _pumpSidebarAndProjectMenu(tester, auth);
    await tester.tap(
      find.byKey(const ValueKey('project-label-northstar-inventory')),
    );
    await tester.pump(const Duration(milliseconds: 300));

    final projectActions = find.byKey(
      const ValueKey('project-actions-northstar-inventory'),
    );
    expect(projectActions, findsOneWidget);
    await tester.tap(projectActions);
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(const ValueKey('project-action-pin')), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('sidebar-user-button')));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(const ValueKey('project-action-pin')), findsNothing);
    expect(
      find.byKey(const ValueKey('sidebar-account-action-logout')),
      findsOneWidget,
    );
  });

  testWidgets('account menu dismisses and flips in a short viewport', (
    tester,
  ) async {
    _setWindow(tester, const Size(880, 260));
    final auth = await _authenticatedAuth();
    addTearDown(auth.dispose);

    await _pumpShell(tester, auth);
    final trigger = find.byKey(const ValueKey('sidebar-user-button'));
    await tester.tap(trigger);
    await tester.pump(const Duration(milliseconds: 150));
    expect(find.text('Log out'), findsOneWidget);
    expect(tester.takeException(), isNull);

    await tester.tapAt(const Offset(700, 100));
    await tester.pump(const Duration(milliseconds: 150));
    expect(find.text('Log out'), findsNothing);
    expect(tester.takeException(), isNull);
  });
}

Future<DemoAuthRepository> _authenticatedAuth() async {
  final auth = DemoAuthRepository(username: 'Owner');
  await auth.login(username: 'Owner', password: 'test-password');
  return auth;
}

Future<void> _pumpShell(
  WidgetTester tester,
  AuthRepository auth, {
  bool withToaster = true,
  Future<void> Function()? onLogout,
  Future<void> Function()? onLogoutAll,
  Future<void> Function(String current, String next)? onChangePassword,
}) async {
  await tester.pumpWidget(
    FrankTestApp(
      home: OfficeShell(
        gateway: FixtureFrankGateway(latency: Duration.zero),
        authRepository: auth,
        onLogout: onLogout,
        onLogoutAll: onLogoutAll,
        onChangePassword: onChangePassword,
      ),
      withToaster: withToaster,
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
}

Future<void> _pumpSidebarAndProjectMenu(
  WidgetTester tester,
  AuthRepository auth,
) async {
  final workspace = (await tester.runAsync<OfficeWorkspace>(
    () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
  ))!;
  final searchFocusNode = FocusNode(debugLabel: 'account menu test search');
  addTearDown(searchFocusNode.dispose);

  await tester.pumpWidget(
    FrankTestApp(
      home: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            width: 360,
            child: MainSidebarContent(
              isFullscreen: false,
              workspace: workspace,
              workspaceName: workspace.name,
              activeView: WorkspaceView.settings,
              settingsSection: SettingsSection.organization,
              projects: workspace.projects,
              selectedProjectId: null,
              selectedMissionId: null,
              expandedProjectIds: const {},
              projectScope: null,
              pinnedMissionIds: const [],
              authRepository: auth,
              searchFocusNode: searchFocusNode,
              onSelectView: (_) {},
              onSelectSettingsSection: (_) {},
              onAddProject: () {},
              onToggleProject: (_) {},
              onSelectMission: (_, _) {},
              onCreateMission: (_) {},
              onPinProject: (_) {},
              onRenameProject: (_) {},
              onArchiveProject: (_) {},
              onRemoveProject: (_) {},
              onPinMission: (_, _) {},
              onRenameMission: (_, _) {},
              onArchiveMission: (_, _) {},
              onSelectProjectScope: (_) {},
              onTogglePinnedMission: (_) {},
              onReorderPinnedMissions: (_) {},
            ),
          ),
          Expanded(
            child: SingleChildScrollView(
              child: ProjectsTreePane(
                projects: workspace.projects,
                selectedProjectId: null,
                selectedMissionId: null,
                expandedProjectIds: const {},
                onToggleProject: (_) {},
                onSelectMission: (_, _) {},
                onCreateMission: (_) {},
                onPinProject: (_) {},
                onRenameProject: (_) {},
                onArchiveProject: (_) {},
                onRemoveProject: (_) {},
                onPinMission: (_, _) {},
                onRenameMission: (_, _) {},
                onArchiveMission: (_, _) {},
              ),
            ),
          ),
        ],
      ),
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
}

void _setWindow(WidgetTester tester, [Size size = const Size(1600, 1000)]) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}
