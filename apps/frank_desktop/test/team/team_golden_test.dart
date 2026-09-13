import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_team.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/team/presentation/team_surface.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('team roster desktop golden', (tester) async {
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    _setSize(tester, const ui.Size(1600, 1000));
    await tester.pumpWidget(
      _goldenApp(
        TeamSurface(
          workspace: workspace!,
          profiles: fixtureTeamProfiles(workspace),
        ),
      ),
    );
    await _precacheTeamPortraits(tester);
    await tester.pump(const Duration(milliseconds: 250));
    await expectLater(
      find.byKey(const ValueKey('team-golden-root')),
      matchesGoldenFile('goldens/team-roster.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('Nia character profile golden', (tester) async {
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    _setSize(tester, const ui.Size(1600, 1000));
    await tester.pumpWidget(
      _goldenApp(
        TeamSurface(
          workspace: workspace!,
          profiles: fixtureTeamProfiles(workspace),
        ),
      ),
    );
    await _precacheTeamPortraits(tester);
    await tester.pump(const Duration(milliseconds: 250));
    await tester.tap(
      find.byKey(const ValueKey('team-agent-card-programmer-nia')),
    );
    await tester.pump(const Duration(milliseconds: 250));
    await expectLater(
      find.byKey(const ValueKey('team-golden-root')),
      matchesGoldenFile('goldens/team-profile-nia.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('team roster compact golden', (tester) async {
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    _setSize(tester, const ui.Size(680, 800));
    await tester.pumpWidget(
      _goldenApp(
        TeamSurface(
          workspace: workspace!,
          profiles: fixtureTeamProfiles(workspace),
        ),
      ),
    );
    await _precacheTeamPortraits(tester);
    await tester.pump(const Duration(milliseconds: 250));
    await expectLater(
      find.byKey(const ValueKey('team-golden-root')),
      matchesGoldenFile('goldens/team-roster-compact.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}

Widget _goldenApp(Widget child) {
  return MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: buildFrankTheme(Brightness.dark),
    home: RepaintBoundary(
      key: const ValueKey('team-golden-root'),
      child: child,
    ),
  );
}

void _setSize(WidgetTester tester, ui.Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}

Future<void> _precacheTeamPortraits(WidgetTester tester) async {
  final context = tester.element(
    find.byKey(const ValueKey('team-golden-root')),
  );
  await tester.runAsync(() async {
    for (final asset in _teamPortraitAssets) {
      await precacheImage(AssetImage(asset), context);
    }
  });
  await tester.pump();
}

const _teamPortraitAssets = <String>[
  'assets/characters/team/maya-chen.png',
  'assets/characters/team/budi-santoso.png',
  'assets/characters/team/nia-alvarez.png',
  'assets/characters/team/dimas-pratama.png',
];
