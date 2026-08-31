import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';

void main() {
  test('fixture gateway exposes an agency roster and projects', () async {
    final workspace = await FixtureFrankGateway().loadWorkspace();

    expect(workspace.name, 'Frank Agency');
    expect(workspace.accountExecutive.name, 'Maya Chen');
    expect(workspace.employees, hasLength(4));
    expect(workspace.projects, hasLength(3));
    expect(workspace.projects.first.name, 'Northstar Inventory');
  });

  testWidgets('renders the office shell with both navigation surfaces', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1600, 1000);
    addTearDown(tester.view.reset);
    await tester.pumpWidget(const FrankApp());
    await tester.pump(const Duration(milliseconds: 500));

    expect(find.text('Office'), findsWidgets);
    expect(find.text('Maya Chen'), findsOneWidget);
    expect(find.text('Projects'), findsOneWidget);
    expect(find.byTooltip('Hide projects'), findsOneWidget);
    expect(find.text('OFFICE FLOOR · EMPTY FOR NOW'), findsOneWidget);
  });

  testWidgets('projects drawer can be closed and reopened', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1600, 1000);
    addTearDown(tester.view.reset);
    await tester.pumpWidget(const FrankApp());
    await tester.pump(const Duration(milliseconds: 500));

    await tester.tap(find.byTooltip('Hide projects'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.byTooltip('Show projects'), findsOneWidget);

    await tester.tap(find.byTooltip('Show projects'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.byTooltip('Hide projects'), findsOneWidget);
    expect(find.text('Northstar Inventory'), findsWidgets);
  });
}
