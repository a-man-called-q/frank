import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import '../support/frank_test_app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/floor/office_scene_floor.dart';

void main() {
  test('starts at the authored framing and keeps elevation fixed', () {
    final controller = OfficeSceneController();
    addTearDown(controller.dispose);

    expect(controller.target.x, closeTo(0.0, 1e-9));
    expect(controller.target.y, closeTo(0.8, 1e-6));
    expect(controller.target.z, closeTo(0.0, 1e-9));
    expect(controller.eye.x, closeTo(10.0, 1e-9));
    expect(controller.eye.y, closeTo(8.5, 1e-6));
    expect(controller.eye.z, closeTo(12.0, 1e-9));
    expect(controller.canReset, isFalse);
  });

  test('primary pan changes ground position only and clamps to the floor', () {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);

    controller.panByPixels(const Offset(80, 40), const Size(800, 400));
    expect(controller.target.y, closeTo(0.8, 1e-6));
    expect(controller.target.x, isNot(closeTo(0.0, 1e-9)));
    expect(controller.target.z, isNot(closeTo(0.0, 1e-9)));
    expect(controller.canReset, isTrue);

    controller.panByPixels(const Offset(100000, 100000), const Size(800, 400));
    expect(controller.target.x, inInclusiveRange(-6.0, 6.0));
    expect(controller.target.z, inInclusiveRange(-4.0, 4.0));
  });

  test('primary pan follows the pointer in both screen directions', () {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);

    controller.panByPixels(const Offset(80, 0), const Size(800, 400));
    expect(controller.target.x, greaterThan(0.0));

    controller.reset();
    controller.panByPixels(const Offset(0, 80), const Size(800, 400));
    // The camera's screen-up axis points toward -Z at the authored angle;
    // moving the target there makes the floor follow a downward drag.
    expect(controller.target.z, lessThan(0.0));
  });

  test('secondary rotation changes yaw but ignores vertical movement', () {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);
    final initialYaw = controller.yaw;
    final initialTarget = controller.target;

    controller.rotateByPixels(const Offset(0, 120), const Size(800, 400));
    expect(controller.yaw, closeTo(initialYaw, 1e-9));

    controller.rotateByPixels(const Offset(200, 80), const Size(800, 400));
    expect(controller.yaw, closeTo(initialYaw + mathPi / 4, 1e-9));
    expect(controller.target.x, closeTo(initialTarget.x, 1e-9));
    expect(controller.target.y, closeTo(initialTarget.y, 1e-6));
    expect(controller.target.z, closeTo(initialTarget.z, 1e-9));
    expect(controller.eye.y, closeTo(8.5, 1e-6));
  });

  test(
    'scroll zoom changes the lens scale without moving the camera target',
    () {
      final controller = OfficeSceneController()..setReady(true);
      addTearDown(controller.dispose);
      final initialTarget = controller.target;
      final initialYaw = controller.yaw;

      controller.zoomByScroll(-120.0);

      expect(controller.zoom, greaterThan(OfficeSceneController.initialZoom));
      expect(controller.target.x, closeTo(initialTarget.x, 1e-9));
      expect(controller.target.y, closeTo(initialTarget.y, 1e-6));
      expect(controller.target.z, closeTo(initialTarget.z, 1e-9));
      expect(controller.yaw, closeTo(initialYaw, 1e-9));

      controller.reset();
      controller.zoomByScale(1.25);
      expect(controller.zoom, closeTo(1.25, 1e-9));

      controller.zoomByScroll(-100000.0);
      expect(controller.zoom, closeTo(OfficeSceneController.maxZoom, 1e-9));
      controller.zoomByScroll(100000.0);
      expect(controller.zoom, closeTo(OfficeSceneController.minZoom, 1e-9));
    },
  );

  test('reset restores the authored target and yaw', () {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);

    controller.panByPixels(const Offset(80, 20), const Size(800, 400));
    controller.rotateByPixels(const Offset(-140, 0), const Size(800, 400));
    controller.zoomByScroll(-120.0);
    expect(controller.canReset, isTrue);

    controller.reset();
    expect(controller.target.x, closeTo(0.0, 1e-9));
    expect(controller.target.y, closeTo(0.8, 1e-6));
    expect(controller.target.z, closeTo(0.0, 1e-9));
    expect(controller.yaw, closeTo(OfficeSceneController.initialYaw, 1e-9));
    expect(controller.zoom, closeTo(OfficeSceneController.initialZoom, 1e-9));
    expect(controller.canReset, isFalse);
  });

  testWidgets('interaction surface maps left pan and right horizontal orbit', (
    tester,
  ) async {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: SizedBox(
            width: 240,
            height: 180,
            child: OfficeSceneInteractionSurface(
              controller: controller,
              child: const ColoredBox(color: Color(0xFF000000)),
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    final initialYaw = controller.yaw;
    final leftDrag = await tester.startGesture(
      const Offset(380, 80),
      kind: PointerDeviceKind.mouse,
      buttons: kPrimaryMouseButton,
    );
    await leftDrag.moveBy(const Offset(40, 20));
    await leftDrag.up();
    await tester.pump(const Duration(milliseconds: 150));

    expect(controller.canReset, isTrue);
    expect(controller.yaw, closeTo(initialYaw, 1e-9));

    controller.reset();
    final rightDrag = await tester.startGesture(
      const Offset(380, 80),
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await rightDrag.moveBy(const Offset(40, 20));
    await rightDrag.up();
    await tester.pump(const Duration(milliseconds: 150));

    expect(controller.yaw, greaterThan(initialYaw));
    expect(controller.target.x, closeTo(0.0, 1e-9));
    expect(controller.target.z, closeTo(0.0, 1e-9));

    controller.reset();
    await tester.sendEventToBinding(
      const PointerScrollEvent(
        kind: PointerDeviceKind.trackpad,
        position: Offset(380, 80),
        scrollDelta: Offset(0, -120),
      ),
    );
    await tester.pump();
    expect(controller.zoom, greaterThan(OfficeSceneController.initialZoom));

    controller.reset();
    await tester.sendEventToBinding(
      const PointerPanZoomStartEvent(position: Offset(380, 80)),
    );
    await tester.sendEventToBinding(
      const PointerPanZoomUpdateEvent(position: Offset(380, 80), scale: 1.25),
    );
    await tester.sendEventToBinding(
      const PointerPanZoomEndEvent(position: Offset(380, 80)),
    );
    await tester.pump();
    expect(controller.zoom, closeTo(1.25, 1e-9));

    controller.reset();
    final clicked = await tester.startGesture(
      const Offset(380, 80),
      kind: PointerDeviceKind.mouse,
      buttons: kPrimaryMouseButton,
    );
    await clicked.up();
    await tester.pump();
    expect(controller.target.x, closeTo(0.0, 1e-9));
    expect(controller.target.z, closeTo(0.0, 1e-9));
    expect(controller.yaw, closeTo(initialYaw, 1e-9));
  });
}

const mathPi = 3.141592653589793;
