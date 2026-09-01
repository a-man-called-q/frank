import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

import 'support/fake_gateway.dart';

void main() {
  testWidgets('injected gateway failure can recover through retry', (
    tester,
  ) async {
    _setDesktopSize(tester);
    final gateway = FakeGateway(loadError: StateError('offline'));
    await tester.pumpWidget(FrankApp(gateway: gateway));
    await tester.pump(const Duration(milliseconds: 20));

    expect(
      find.textContaining('Could not load the local workspace.'),
      findsOneWidget,
    );
    expect(find.textContaining('offline'), findsOneWidget);

    gateway.loadError = null;
    await tester.tap(find.text('Retry'));
    await tester.pump(const Duration(milliseconds: 500));

    expect(find.bySemanticsLabel('Office view'), findsOneWidget);
    expect(find.text('OFFICE FLOOR · EMPTY FOR NOW'), findsOneWidget);
  });

  testWidgets('an empty injected workspace stays in the empty state', (
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
        color: 0xFFE2A84B,
      ),
    );
    await tester.pumpWidget(
      FrankApp(gateway: FakeGateway(workspace: workspace)),
    );
    await tester.pump(const Duration(milliseconds: 20));

    expect(find.text('No projects yet'), findsOneWidget);
    expect(find.textContaining('Frank Agency'), findsNothing);
  });
}

void _setDesktopSize(WidgetTester tester) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(1600, 1000);
  addTearDown(tester.view.reset);
}
