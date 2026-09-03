import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/ledger_models.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/ledger/ledger_surface.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('exposes operational and effectiveness tabs', (tester) async {
    _setSize(tester, const ui.Size(1400, 900));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );

    await tester.pumpWidget(_app(LedgerSurface(workspace: workspace!)));
    await tester.pump();

    expect(
      find.byKey(const ValueKey('ledger-tab-operational')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('ledger-tab-effectiveness')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('ledger-period-session')), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('ledger-tab-operational')));
    await tester.pump();

    expect(
      find.byKey(const ValueKey('ledger-operational-view')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('ledger-operational-filters')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('ledger-operational-trend')),
      findsOneWidget,
    );
    expect(find.text('Unattributed'), findsWidgets);
    expect(find.text('Subagent sidechain'), findsOneWidget);
    expect(find.byKey(const ValueKey('ledger-period-session')), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('lifetime view leads with honest evidence status', (
    tester,
  ) async {
    _setSize(tester, const ui.Size(1600, 1000));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );

    await tester.pumpWidget(_app(LedgerSurface(workspace: workspace!)));
    await tester.pump();

    expect(find.byKey(const ValueKey('ledger-surface')), findsOneWidget);
    expect(find.text('Not enough data yet'), findsOneWidget);
    expect(find.text('12 / 20'), findsOneWidget);
    expect(find.text('124 / 200'), findsOneWidget);
    expect(find.text('1,266,610'), findsOneWidget);
    expect(find.text('286,420'), findsOneWidget);
    expect(find.text('104,800 B'), findsOneWidget);
    expect(find.text('Unattributed'), findsOneWidget);
    expect(find.text('Subagent sidechain'), findsOneWidget);
    expect(find.byKey(const ValueKey('ledger-trend-chart')), findsOneWidget);
    expect(find.bySemanticsLabel('Ledger attribution table'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('period toggle switches to session evidence', (tester) async {
    _setSize(tester, const ui.Size(1200, 900));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );

    await tester.pumpWidget(_app(LedgerSurface(workspace: workspace!)));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('ledger-period-session')));
    await tester.pump();

    expect(find.text('Session evidence'), findsOneWidget);
    expect(find.text('Not enough data yet'), findsNothing);
    expect(find.text('155,480'), findsOneWidget);
    expect(find.text('28,940'), findsOneWidget);
    expect(find.text('11,420 B'), findsOneWidget);
    expect(find.text('18'), findsOneWidget);
    expect(find.text('Claude · default'), findsOneWidget);
    expect(find.byKey(const ValueKey('ledger-period-session')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('compact layout keeps the chart and table usable', (
    tester,
  ) async {
    _setSize(tester, const ui.Size(680, 800));
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );

    await tester.pumpWidget(_app(LedgerSurface(workspace: workspace!)));
    await tester.pump();

    expect(find.byKey(const ValueKey('ledger-measures')), findsOneWidget);
    expect(find.byKey(const ValueKey('ledger-trend-panel')), findsOneWidget);
    expect(
      find.byKey(const ValueKey('ledger-attribution-scroll')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('empty data does not manufacture usage or savings', (
    tester,
  ) async {
    _setSize(tester, const ui.Size(1000, 800));
    const emptyPeriod = LedgerPeriodData(
      period: LedgerPeriod.lifetime,
      sessionCount: 0,
      turnCount: 0,
      totals: LedgerTotals(),
      trend: [],
      attribution: [],
    );
    const emptyData = LedgerDashboardData(
      session: LedgerPeriodData(
        period: LedgerPeriod.session,
        sessionCount: 0,
        turnCount: 0,
        totals: LedgerTotals(),
        trend: [],
        attribution: [],
      ),
      lifetime: emptyPeriod,
    );

    final workspace = await tester.runAsync(_workspace);
    final app = _app(LedgerSurface(workspace: workspace!, data: emptyData));
    await tester.pumpWidget(app);
    await tester.pump();

    expect(find.text('No measured usage yet'), findsOneWidget);
    expect(find.text('Not enough data yet'), findsOneWidget);
    expect(find.text('Estimated saved range'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

Future<OfficeWorkspace> _workspace() =>
    FixtureFrankGateway(latency: Duration.zero).loadWorkspace();

Widget _app(Widget child) {
  return MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: buildFrankTheme(Brightness.dark),
    home: child,
  );
}

void _setSize(WidgetTester tester, ui.Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}
