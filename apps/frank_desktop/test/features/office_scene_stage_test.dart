import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/floor/office_scene_floor.dart';

void main() {
  test('only live activity schedules scene ticks', () {
    expect(OfficeSceneActivity.static.shouldTick, isFalse);
    expect(OfficeSceneActivity.paused.shouldTick, isFalse);
    expect(OfficeSceneActivity.live.shouldTick, isTrue);
  });

  testWidgets('stage keeps glass wiring and retry above the foreground', (
    tester,
  ) async {
    var attempts = 0;

    Future<void> failToInitialize() async {
      attempts += 1;
      throw StateError('GPU unavailable in test');
    }

    await tester.pumpWidget(
      MaterialApp(
        home: OfficeSceneStage(
          key: const ValueKey('office-scene-stage'),
          activity: OfficeSceneActivity.paused,
          blurSigma: 12.0,
          scrimColor: const Color(0x47101113),
          foreground: const ColoredBox(
            key: ValueKey('office-scene-foreground'),
            color: Colors.transparent,
            child: Center(child: Text('Foreground remains usable')),
          ),
          initializeResources: failToInitialize,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    final stage = tester.widget<OfficeSceneStage>(
      find.byKey(const ValueKey('office-scene-stage')),
    );
    expect(stage.activity, OfficeSceneActivity.paused);
    expect(stage.blurSigma, 12.0);
    expect(stage.scrimColor, const Color(0x47101113));
    expect(
      tester.widget<ImageFiltered>(
        find.byKey(const ValueKey('office-scene-image-filter')),
      ),
      isA<ImageFiltered>().having((filter) => filter.enabled, 'enabled', true),
    );
    expect(find.text('Foreground remains usable'), findsOneWidget);
    expect(find.bySemanticsLabel('Office floor unavailable'), findsOneWidget);
    expect(find.text('Retry'), findsOneWidget);

    await tester.tap(find.text('Retry'));
    await tester.pump();
    await tester.pump();
    expect(attempts, 2);
    expect(find.text('Foreground remains usable'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('loading visual remains semantic and filtered without GPU', (
    tester,
  ) async {
    final resources = Completer<void>();
    await tester.pumpWidget(
      MaterialApp(
        home: OfficeSceneStage(
          blurSigma: 12.0,
          foreground: const SizedBox.expand(),
          initializeResources: () => resources.future,
        ),
      ),
    );
    await tester.pump();

    expect(find.bySemanticsLabel('Office floor loading'), findsOneWidget);
    expect(find.text('OFFICE SCENE · INITIALIZING'), findsOneWidget);
    final filter = tester.widget<ImageFiltered>(
      find.byKey(const ValueKey('office-scene-image-filter')),
    );
    expect(filter.enabled, isTrue);
    expect(filter.imageFilter, isA<ui.ImageFilter>());
  });

  testWidgets(
    'empty preset skips GPU initialization and keeps its foreground usable',
    (tester) async {
      var attempts = 0;
      var taps = 0;
      final controller = OfficeSceneController();
      addTearDown(controller.dispose);
      await tester.pumpWidget(
        MaterialApp(
          home: OfficeSceneStage(
            key: const ValueKey('login-scene-stage'),
            preset: OfficeScenePreset.empty,
            semanticLabel: 'Login background',
            controller: controller,
            initializeResources: () async {
              attempts++;
              throw StateError('GPU must not be initialized');
            },
            foreground: Center(
              child: TextButton(
                onPressed: () => taps++,
                child: const Text('Continue'),
              ),
            ),
          ),
        ),
      );
      await tester.pump();

      expect(find.bySemanticsLabel('Login background'), findsOneWidget);
      expect(find.byKey(const ValueKey('office-scene-view')), findsNothing);
      expect(find.text('OFFICE SCENE · INITIALIZING'), findsNothing);
      expect(find.text('Retry'), findsNothing);
      expect(attempts, 0);
      expect(controller.isReady, isFalse);
      await tester.tap(find.text('Continue'));
      expect(taps, 1);
      expect(tester.takeException(), isNull);
      expect(
        tester
            .widget<OfficeSceneStage>(
              find.byKey(const ValueKey('login-scene-stage')),
            )
            .preset,
        OfficeScenePreset.empty,
      );
    },
  );

  testWidgets('interactive controller stays disabled across GPU failures', (
    tester,
  ) async {
    final controller = OfficeSceneController();
    addTearDown(controller.dispose);
    var attempts = 0;

    Future<void> failToInitialize() async {
      attempts += 1;
      throw StateError('GPU unavailable in test');
    }

    await tester.pumpWidget(
      MaterialApp(
        home: OfficeSceneStage(
          controller: controller,
          initializeResources: failToInitialize,
        ),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(controller.isReady, isFalse);
    expect(controller.canReset, isFalse);
    expect(attempts, 1);

    await tester.tap(find.text('Retry'));
    await tester.pump();
    await tester.pump();
    expect(controller.isReady, isFalse);
    expect(controller.canReset, isFalse);
    expect(attempts, 2);
  });
}
