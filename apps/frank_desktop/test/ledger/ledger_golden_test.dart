import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import '../support/frank_test_app.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_ledger.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/ledger/presentation/ledger_surface.dart';

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

  testWidgets('Ledger effectiveness and operational tabs use the page frame', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(1600, 1000);
    addTearDown(tester.view.reset);
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );

    await tester.pumpWidget(
      FrankTestApp(
        debugShowCheckedModeBanner: false,
        theme: buildFrankTheme(),
        home: RepaintBoundary(
          key: const ValueKey('ledger-golden-root'),
          child: LedgerSurface(
            workspace: workspace!,
            data: fixtureLedgerDashboard(workspace),
          ),
        ),
      ),
    );
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('ledger-golden-root')),
      matchesGoldenFile('goldens/ledger-effectiveness.png'),
    );

    await tester.tap(find.byKey(const ValueKey('ledger-tab-operational')));
    await tester.pump(const Duration(milliseconds: 150));
    await expectLater(
      find.byKey(const ValueKey('ledger-golden-root')),
      matchesGoldenFile('goldens/ledger-operational.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}
