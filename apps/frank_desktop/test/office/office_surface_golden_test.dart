import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';

import '../support/fake_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
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

  testWidgets('Taskboard and Journal use their production Settings frames', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(1600, 1000);
    addTearDown(tester.view.reset);
    final gateway = FakeGateway();
    await tester.pumpWidget(
      RepaintBoundary(
        key: const ValueKey('office-surface-golden-root'),
        child: FrankApp(gateway: gateway, showLogin: false),
      ),
    );
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tap(find.byKey(const ValueKey('settings-section-taskboard')));
    await tester.pump(const Duration(milliseconds: 220));
    await tester.pump();
    await tester.pump();
    expect(find.text('Taskboard'), findsNWidgets(2));
    expect(find.text('Loading taskboard…'), findsNothing);
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/taskboard.png'),
    );

    await tester.tap(find.byKey(const ValueKey('settings-section-journal')));
    await tester.pump(const Duration(milliseconds: 220));
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/journal.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('Taskboard List view uses the production canvas shell', (
    tester,
  ) async {
    await _pumpTaskboardShell(tester, const ui.Size(1600, 1000));
    await tester.tap(find.byKey(const ValueKey('taskboard-view-list')));
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/taskboard-list.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('Taskboard detail inspector uses the production canvas shell', (
    tester,
  ) async {
    await _pumpTaskboardShell(tester, const ui.Size(1600, 1000));
    await tester.tap(find.byKey(const ValueKey('taskboard-task-NS-03')));
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/taskboard-detail.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('Taskboard switches to List in the compact production shell', (
    tester,
  ) async {
    await _pumpTaskboardShell(tester, const ui.Size(680, 800));
    expect(find.byKey(const ValueKey('taskboard-list-scroll')), findsOneWidget);
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/taskboard-compact.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('Taskboard detail fills the compact production shell', (
    tester,
  ) async {
    await _pumpTaskboardShell(tester, const ui.Size(680, 800));
    await tester.tap(find.byKey(const ValueKey('taskboard-list-task-NS-03')));
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/taskboard-detail-compact.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('Taskboard detail supports 200% text scaling', (tester) async {
    await _pumpTaskboardShell(tester, const ui.Size(680, 800), textScale: 2);
    final task = find.byKey(const ValueKey('taskboard-list-task-NS-03'));
    await tester.ensureVisible(task);
    await tester.tap(task);
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('office-surface-golden-root')),
      matchesGoldenFile('goldens/taskboard-detail-compact-text-scale-200.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}

Future<void> _pumpTaskboardShell(
  WidgetTester tester,
  ui.Size size, {
  double textScale = 1,
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
  final gateway = FakeGateway();
  await tester.pumpWidget(
    RepaintBoundary(
      key: const ValueKey('office-surface-golden-root'),
      child: MediaQuery(
        data: MediaQueryData(textScaler: TextScaler.linear(textScale)),
        child: FrankApp(gateway: gateway, showLogin: false),
      ),
    ),
  );
  // The shell's collapsed-logo asset is loaded asynchronously. Precache it
  // before capturing any taskboard frame so the result does not depend on
  // which earlier golden test happened to populate Flutter's image cache.
  await tester.runAsync(
    () => precacheImage(
      const AssetImage('assets/branding/frank-logo.png'),
      tester.element(find.byType(FrankApp)),
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
  await tester.tap(find.byKey(const ValueKey('settings-section-taskboard')));
  await tester.pump(const Duration(milliseconds: 220));
  // The fixture gateway completes through a microtask after the route and
  // AnimatedSwitcher have settled.
  await tester.pump();
  await tester.pump();
}
