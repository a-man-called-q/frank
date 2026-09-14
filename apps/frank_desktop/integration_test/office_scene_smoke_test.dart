import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_scene/scene.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/features/chat/presentation/focusable_composer.dart';
import 'package:frank_desktop/features/floor/office_scene_floor.dart';
import 'package:frank_desktop/features/shell/sidebar_layout.dart';
import 'package:vector_math/vector_math.dart' as vm;

void main() {
  testWidgets('macOS renders the scene and survives sidebar changes', (
    tester,
  ) async {
    await tester.pumpWidget(const FrankApp(showLogin: false));

    // GPU shader compilation is asynchronous on a real host. Give the scene
    // a bounded window to reveal; a failed initialization must surface in the
    // floor's nonfatal error state rather than as a Flutter exception.
    for (var attempt = 0; attempt < 40; attempt++) {
      if (find
          .byKey(const ValueKey('office-scene-view'))
          .evaluate()
          .isNotEmpty) {
        break;
      }
      await tester.pump(const Duration(milliseconds: 250));
    }

    expect(find.byKey(const ValueKey('office-scene-view')), findsOneWidget);
    final sceneView = tester.widget<SceneView>(
      find.byKey(const ValueKey('office-scene-view')),
    );
    expect(sceneView.autoTick, isFalse);
    final retainedScene = sceneView.scene;
    final retainedCamera = sceneView.camera;
    final stage = find.byKey(const ValueKey('office-scene-stage'));
    final stageElement = tester.element(stage);
    final sceneViewFinder = find.byKey(const ValueKey('office-scene-view'));
    final sceneViewElement = tester.element(sceneViewFinder);

    // The host app keeps sidebar visibility in shared preferences. A previous
    // smoke run may have left it collapsed, so normalize the starting state
    // before exercising section navigation, resize, and collapse.
    final showSidebar = find.bySemanticsLabel('Show the workspace sidebar');
    if (showSidebar.evaluate().isNotEmpty) {
      await tester.tap(showSidebar);
      await tester.pump(const Duration(milliseconds: 320));
    }

    await tester.tap(find.bySemanticsLabel('Settings view'));
    await tester.pump(const Duration(milliseconds: 220));
    await tester.tap(find.byKey(const ValueKey('settings-section-team')));
    await tester.pump(const Duration(milliseconds: 220));
    expect(identical(stageElement, tester.element(stage)), isTrue);
    expect(
      identical(sceneViewElement, tester.element(sceneViewFinder)),
      isTrue,
    );
    final teamSceneView = tester.widget<SceneView>(sceneViewFinder);
    expect(identical(retainedScene, teamSceneView.scene), isTrue);
    expect(identical(retainedCamera, teamSceneView.camera), isTrue);
    expect(
      find.byKey(const ValueKey('settings-section-content-team')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);

    final openSurface = tester.getRect(
      find.byKey(const ValueKey('main-surface')),
    );
    final sidebarWidth = tester
        .getRect(find.byKey(const ValueKey('sidebar-slot')))
        .width;
    final resizeHandle = find.bySemanticsLabel('Resize sidebar');
    expect(resizeHandle, findsOneWidget);
    final resizeGesture = await tester.startGesture(
      tester.getCenter(resizeHandle),
    );
    // Pick the direction with room so persisted preferences at either width
    // bound still exercise a real viewport resize.
    final resizeDelta = sidebarWidth >= SidebarLayout.maxWidth - 1
        ? -48.0
        : 48.0;
    await resizeGesture.moveBy(Offset(resizeDelta, 0));
    await resizeGesture.moveBy(Offset.zero);
    await tester.pump();
    final resizedWidth = tester
        .getRect(find.byKey(const ValueKey('main-surface')))
        .width;
    // Verify a real viewport change without assuming which direction was used.
    expect((resizedWidth - openSurface.width).abs(), greaterThan(1));
    await resizeGesture.up();
    await tester.pump(const Duration(milliseconds: 220));
    expect(identical(stageElement, tester.element(stage)), isTrue);
    expect(
      identical(sceneViewElement, tester.element(sceneViewFinder)),
      isTrue,
    );
    final resizedSceneView = tester.widget<SceneView>(sceneViewFinder);
    expect(identical(retainedScene, resizedSceneView.scene), isTrue);
    expect(identical(retainedCamera, resizedSceneView.camera), isTrue);

    await tester.tap(find.bySemanticsLabel('Hide the workspace sidebar'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.bySemanticsLabel('Show the workspace sidebar'), findsOneWidget);
    expect(find.byKey(const ValueKey('office-scene-view')), findsOneWidget);
    expect(identical(stageElement, tester.element(stage)), isTrue);
    expect(
      identical(sceneViewElement, tester.element(sceneViewFinder)),
      isTrue,
    );
    final collapsedSceneView = tester.widget<SceneView>(sceneViewFinder);
    expect(identical(retainedScene, collapsedSceneView.scene), isTrue);
    expect(identical(retainedCamera, collapsedSceneView.camera), isTrue);
    expect(tester.takeException(), isNull);

    // Office reuses the shell's retained floor and controller. Verify the
    // desktop mouse contract against that same camera: the floor remains
    // interactive outside chat, the transcript owns wheel input, and reset
    // restores the initial view.
    await tester.tap(find.bySemanticsLabel('Show the workspace sidebar'));
    await tester.pump(const Duration(milliseconds: 420));
    final officeSelector = find.bySemanticsLabel('Office view');
    await tester.tap(officeSelector, warnIfMissed: false);
    await tester.pump(const Duration(milliseconds: 420));
    final conversation = find.bySemanticsLabel(RegExp(r'^Conversation with '));
    final chatSceneFinder = find.byKey(const ValueKey('office-scene-view'));
    for (var attempt = 0; attempt < 40; attempt++) {
      final chatReady = conversation.evaluate().isNotEmpty;
      final sceneReady = chatSceneFinder.evaluate().isNotEmpty;
      final resetReady = find
          .bySemanticsLabel('Reset floor view')
          .evaluate()
          .isNotEmpty;
      if (chatReady && sceneReady && resetReady) {
        break;
      }
      // The shell's sidebar animation and BLoC dispatch can finish on
      // different frames on a real macOS host. Re-send the idempotent view
      // selection until the chat surface and its floor are both mounted.
      if (officeSelector.evaluate().isNotEmpty) {
        await tester.tap(officeSelector, warnIfMissed: false);
      }
      await tester.pump(const Duration(milliseconds: 250));
    }

    expect(conversation, findsOneWidget);
    expect(chatSceneFinder, findsOneWidget);
    expect(find.bySemanticsLabel('Reset floor view'), findsOneWidget);
    final chatScene = tester.widget<SceneView>(chatSceneFinder);
    final chatInitialPosition = chatScene.camera!.position;
    final chatInitialForward = chatScene.camera!.forward;
    final chatRect = tester.getRect(chatSceneFinder);
    final transcriptFinder = find.byKey(
      const ValueKey('passive-chat-transcript'),
    );
    expect(transcriptFinder, findsOneWidget);
    final transcriptRect = tester.getRect(transcriptFinder);
    final floorPoint = Offset(chatRect.left + 24, chatRect.top + 24);

    final composerFinder = find.byType(FocusableComposer);
    expect(composerFinder, findsOneWidget);
    final beforeComposerTap = tester
        .widget<SceneView>(chatSceneFinder)
        .camera!
        .position;
    await tester.tap(composerFinder, warnIfMissed: false);
    await tester.pump();
    _expectVectorClose(
      tester.widget<SceneView>(chatSceneFinder).camera!.position,
      beforeComposerTap,
    );

    final panGesture = await tester.startGesture(
      floorPoint,
      kind: PointerDeviceKind.mouse,
      buttons: kPrimaryMouseButton,
    );
    await panGesture.moveBy(const Offset(80, 24));
    await panGesture.up();
    await tester.pump();
    final pannedPosition = tester
        .widget<SceneView>(chatSceneFinder)
        .camera!
        .position;
    expect(
      _distanceBetween(pannedPosition, chatInitialPosition),
      greaterThan(0.01),
    );
    expect(
      tester.widget<SceneView>(chatSceneFinder).camera!.forward.y,
      closeTo(chatInitialForward.y, 0.01),
    );

    final orbitGesture = await tester.startGesture(
      floorPoint,
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await orbitGesture.moveBy(const Offset(80, 60));
    await orbitGesture.up();
    await tester.pump();
    final orbitedForward = tester
        .widget<SceneView>(chatSceneFinder)
        .camera!
        .forward;
    expect(orbitedForward.y, closeTo(chatInitialForward.y, 0.01));
    expect(
      _distanceBetween(orbitedForward, chatInitialForward),
      greaterThan(0.01),
    );

    final chatProjection = chatScene.camera!.projection;
    final initialProjectionScale = chatProjection
        .getProjectionMatrix(chatRect.width / chatRect.height)
        .storage[5];
    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: transcriptRect.center,
        scrollDelta: const Offset(0, -120),
      ),
    );
    await tester.pump();
    final projectionScaleAfterTranscriptScroll = tester
        .widget<SceneView>(chatSceneFinder)
        .camera!
        .projection
        .getProjectionMatrix(chatRect.width / chatRect.height)
        .storage[5];
    expect(
      projectionScaleAfterTranscriptScroll,
      closeTo(initialProjectionScale, 1e-6),
    );

    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: floorPoint,
        scrollDelta: const Offset(0, -120),
      ),
    );
    await tester.pump();
    final zoomedProjectionScale = tester
        .widget<SceneView>(chatSceneFinder)
        .camera!
        .projection
        .getProjectionMatrix(chatRect.width / chatRect.height)
        .storage[5];
    expect(zoomedProjectionScale, greaterThan(initialProjectionScale));

    await tester.tap(
      find.byKey(const ValueKey('floor-reset-view-button')),
      warnIfMissed: false,
    );
    await tester.pump();
    final resetScene = tester.widget<SceneView>(chatSceneFinder);
    _expectVectorClose(resetScene.camera!.position, chatInitialPosition);
    _expectVectorClose(resetScene.camera!.forward, chatInitialForward);
    expect(
      resetScene.camera!.projection
          .getProjectionMatrix(chatRect.width / chatRect.height)
          .storage[5],
      closeTo(initialProjectionScale, 1e-6),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('login uses an empty scene and fades into the office', (
    tester,
  ) async {
    await tester.pumpWidget(const FrankApp());
    await tester.pump();

    expect(find.bySemanticsLabel('Frank login'), findsOneWidget);
    // The empty login preset skips the renderer until the office is revealed.
    expect(find.byType(SceneView), findsNothing);
    final loginStage = tester.widget<OfficeSceneStage>(
      find.byKey(const ValueKey('login-scene-stage')),
    );
    expect(loginStage.preset, OfficeScenePreset.empty);
    expect(loginStage.semanticLabel, 'Login background');

    await tester.enterText(
      find.byKey(const ValueKey('login-username-field')),
      'demo_owner',
    );
    await tester.enterText(
      find.byKey(const ValueKey('login-password-field')),
      'demo-password',
    );
    await tester.tap(find.byKey(const ValueKey('login-submit-button')));
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pump(const Duration(milliseconds: 720));
    await tester.pump();

    expect(find.byKey(const ValueKey('login-scene-stage')), findsNothing);
    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

double _distanceBetween(vm.Vector3 first, vm.Vector3 second) {
  final dx = first.x - second.x;
  final dy = first.y - second.y;
  final dz = first.z - second.z;
  return math.sqrt(dx * dx + dy * dy + dz * dz);
}

void _expectVectorClose(vm.Vector3 actual, vm.Vector3 expected) {
  expect(actual.x, closeTo(expected.x, 0.01));
  expect(actual.y, closeTo(expected.y, 0.01));
  expect(actual.z, closeTo(expected.z, 0.01));
}
