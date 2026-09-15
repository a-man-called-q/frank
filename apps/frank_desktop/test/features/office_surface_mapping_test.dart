import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/app/layout/office_surface_frame.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/office_shell.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('presentation mapping keeps Canvas and Page explicit', () {
    expect(
      settingsSurfaceModeForSection(SettingsSection.organization),
      OfficeSurfaceMode.canvas,
    );
    expect(
      settingsSurfaceModeForSection(SettingsSection.taskboard),
      OfficeSurfaceMode.canvas,
    );
    for (final section in [
      SettingsSection.team,
      SettingsSection.ledger,
      SettingsSection.journal,
      SettingsSection.toolchains,
    ]) {
      expect(settingsSurfaceModeForSection(section), OfficeSurfaceMode.page);
    }
  });

  testWidgets('Settings sections select the explicit canvas/page frame', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(1280, 800);
    addTearDown(tester.view.reset);
    await tester.pumpWidget(const FrankApp(showLogin: false));
    await tester.pump(const Duration(milliseconds: 500));
    const modes = <String, bool>{
      'organization': false,
      'team': true,
      'ledger': true,
      'taskboard': false,
      'journal': true,
    };
    for (final entry in modes.entries) {
      final workspacePage = entry.key == 'taskboard' || entry.key == 'journal';
      await tester.tap(
        find.bySemanticsLabel(workspacePage ? 'Office view' : 'Settings view'),
      );
      await tester.pump(const Duration(milliseconds: 220));
      await tester.tap(find.byKey(ValueKey('settings-section-${entry.key}')));
      await tester.pump(const Duration(milliseconds: 220));
      if (entry.value) {
        expect(find.byType(CustomScrollView), findsOneWidget);
        expect(
          find.byKey(const ValueKey('office-surface-frame-page')),
          findsOneWidget,
        );
        expect(
          find.byKey(const ValueKey('office-surface-frame-canvas')),
          findsNothing,
        );
      } else {
        expect(find.byType(CustomScrollView), findsNothing);
        expect(
          find.byKey(const ValueKey('office-surface-frame-canvas')),
          findsOneWidget,
        );
        expect(
          find.byKey(const ValueKey('office-surface-frame-page')),
          findsNothing,
        );
      }
    }
  });
}
