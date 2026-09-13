import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';

void main() {
  testWidgets('Office pages survive a short, scaled desktop viewport', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(560, 560);
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(
          disableAnimations: true,
          textScaler: TextScaler.linear(1.5),
        ),
        child: const FrankApp(showLogin: false),
      ),
    );
    await tester.pump(const Duration(milliseconds: 500));
    expect(tester.takeException(), isNull, reason: 'organization');
    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 220));

    for (final section in ['organization', 'team', 'ledger', 'models']) {
      await tester.tap(find.byKey(ValueKey('settings-section-$section')));
      await tester.pump(const Duration(milliseconds: 220));
      expect(tester.takeException(), isNull, reason: section);

      if (section == 'team') {
        await tester.tap(find.byKey(const ValueKey('team-agent-card-ae-maya')));
        await tester.pump(const Duration(milliseconds: 220));
        expect(tester.takeException(), isNull, reason: 'team profile');
        await tester.tap(find.byKey(const ValueKey('team-profile-back')));
        await tester.pump(const Duration(milliseconds: 220));
        expect(tester.takeException(), isNull, reason: 'team roster restore');
      }
    }

    await tester.tap(find.bySemanticsLabel('Office view'));
    await tester.pump(const Duration(milliseconds: 220));
    for (final section in ['taskboard', 'journal']) {
      await tester.tap(find.byKey(ValueKey('settings-section-$section')));
      await tester.pump(const Duration(milliseconds: 220));
      expect(tester.takeException(), isNull, reason: section);
    }
  });
}
