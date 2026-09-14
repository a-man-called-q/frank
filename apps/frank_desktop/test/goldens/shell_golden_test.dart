import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/core/auth/auth_repository.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

import '../support/fake_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    // The production theme uses this platform alias. Loading Geist into the
    // same alias makes the image contract deterministic on every CI runner.
    final font = await rootBundle.load('assets/fonts/Geist-Variable.ttf');
    final loader = FontLoader('Geist')..addFont(Future<ByteData>.value(font));
    await loader.load();

    final lucideFont = await rootBundle.load(
      'packages/forui_assets/assets/lucide.ttf',
    );
    final lucideLoader = FontLoader('ForuiLucideIcons')
      ..addFont(Future<ByteData>.value(lucideFont));
    await lucideLoader.load();
  });

  testWidgets('open office and Lucide shell golden', (tester) async {
    await _pumpShell(tester);
    expect(FrankIcons.search.fontFamily, 'ForuiLucideIcons');
    expect(FrankIcons.search.fontPackage, 'forui_assets');
    expect(find.byIcon(FrankIcons.floor), findsWidgets);
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/office.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('settings mode grouped sidebar golden', (tester) async {
    await _pumpShell(tester);
    await tester.runAsync(
      () => precacheImage(
        const AssetImage('assets/branding/frank-logo.png'),
        tester.element(find.byType(FrankApp)),
      ),
    );
    await tester.pump();
    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(
      find.byKey(const ValueKey('global-navigation-workspace')),
      findsNothing,
    );
    for (final group in ['agency', 'insights', 'system']) {
      expect(find.byKey(ValueKey('global-navigation-$group')), findsOneWidget);
    }
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/settings-sidebar.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('authenticated account menu golden', (tester) async {
    final auth = DemoAuthRepository(username: 'Owner');
    await auth.login(username: 'Owner', password: 'test-password');
    addTearDown(auth.dispose);

    await _pumpShell(tester, authRepository: auth);
    await tester.tap(find.byKey(const ValueKey('sidebar-user-button')));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.text('Change password'), findsOneWidget);
    expect(find.text('Log out all devices'), findsOneWidget);
    expect(find.text('Log out'), findsOneWidget);
    await expectLater(
      find.byType(Overlay).first,
      matchesGoldenFile('goldens/account-menu.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('project scope select menu golden', (tester) async {
    await _pumpShell(tester);
    await tester.tap(find.byKey(const ValueKey('project-scope-selector')));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.text('All projects'), findsWidgets);
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/project-scope-menu.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('attention inbox golden', (tester) async {
    await _pumpShell(tester);
    await _openOffice(tester);
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/attention-inbox.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('hidden sidebar full-width golden', (tester) async {
    await _pumpShell(tester);
    await tester.tap(find.bySemanticsLabel('Hide the workspace sidebar'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 220));
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/sidebar-hidden.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('search results golden', (tester) async {
    await _pumpShell(tester);
    await _openOffice(tester);
    await tester.enterText(_searchField(), 'warehouse');
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/search-results.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('completed shelf expanded golden', (tester) async {
    await _pumpShell(
      tester,
      gateway: FakeGateway(workspace: _completedWorkspace()),
    );
    await _openOffice(tester);
    await tester.tap(find.text('Show all 6'));
    await tester.pump(const Duration(milliseconds: 150));
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/completed-expanded.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('scrolled work inbox alpha fade golden', (tester) async {
    await _pumpShell(
      tester,
      gateway: FakeGateway(workspace: _scrollableWorkspace()),
      size: const ui.Size(880, 640),
    );
    await _openOffice(tester);
    await tester.runAsync(
      () => precacheImage(
        const AssetImage('assets/branding/frank-logo.png'),
        tester.element(find.byType(FrankApp)),
      ),
    );
    await tester.pump();
    final list = find.byKey(const ValueKey('mission-shelf-scroll-view'));
    await tester.drag(list, const Offset(0, -280));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.pump(const Duration(milliseconds: 200));
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/work-inbox-scrolled.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('minimum sidebar width golden', (tester) async {
    await _pumpShell(tester);
    final handle = find.bySemanticsLabel('Resize sidebar');
    final gesture = await tester.startGesture(tester.getCenter(handle));
    await gesture.moveBy(const Offset(-24, 0));
    await gesture.moveBy(Offset.zero);
    await tester.pump();
    await gesture.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 220));
    await expectLater(
      find.byKey(const ValueKey('golden-root')),
      matchesGoldenFile('goldens/sidebar-min-width.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}

Future<void> _pumpShell(
  WidgetTester tester, {
  FakeGateway? gateway,
  AuthRepository? authRepository,
  ui.Size size = const ui.Size(1600, 1000),
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    RepaintBoundary(
      key: const ValueKey('golden-root'),
      child: FrankApp(
        gateway: gateway,
        authRepository: authRepository,
        showLogin: false,
      ),
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
}

Future<void> _openOffice(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('global-nav-office')));
  await tester.pump(const Duration(milliseconds: 220));
}

Finder _searchField() => find.byKey(const ValueKey('work-inbox-search-field'));

OfficeWorkspace _completedWorkspace() {
  const ae = OfficeEmployee(
    id: 'ae',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
  );
  final project = OfficeProject(
    id: 'completed-project',
    name: 'Completed Project',
    client: 'Acme',
    status: ProjectStatus.delivered,
    progress: 1,
    team: const [],
    summary: 'Completed fixture missions.',
    messages: const [],
    missions: List<OfficeMission>.generate(
      6,
      (index) => OfficeMission(
        id: 'completed-$index',
        title: 'Completed mission ${index + 1}',
        status: MissionStatus.completed,
        updatedAt: DateTime.utc(2026, 1, index + 1),
        messages: const [],
      ),
    ),
  );
  return OfficeWorkspace(
    name: 'Completed Agency',
    projects: [project],
    employees: const [ae],
    accountExecutive: ae,
  );
}

OfficeWorkspace _scrollableWorkspace() {
  const ae = OfficeEmployee(
    id: 'ae',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
  );
  final project = OfficeProject(
    id: 'scrollable-project',
    name: 'Scrollable Project',
    client: 'Acme',
    status: ProjectStatus.active,
    progress: 0.42,
    team: const ['Maya Chen'],
    summary: 'A fixture with enough missions to activate the inbox scrollbar.',
    messages: const [],
    missions: List<OfficeMission>.generate(
      32,
      (index) => OfficeMission(
        id: 'scrollable-$index',
        title: 'Mission ${index + 1}',
        status: MissionStatus.draft,
        updatedAt: DateTime.utc(2026, 1, 1).add(Duration(days: index)),
        messages: const [],
      ),
    ),
  );
  return OfficeWorkspace(
    name: 'Scrollable Agency',
    projects: [project],
    employees: const [ae],
    accountExecutive: ae,
  );
}
