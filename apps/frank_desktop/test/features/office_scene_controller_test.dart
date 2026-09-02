import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
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

  test('reset restores the authored target and yaw', () {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);

    controller.panByPixels(const Offset(80, 20), const Size(800, 400));
    controller.rotateByPixels(const Offset(-140, 0), const Size(800, 400));
    expect(controller.canReset, isTrue);

    controller.reset();
    expect(controller.target.x, closeTo(0.0, 1e-9));
    expect(controller.target.y, closeTo(0.8, 1e-6));
    expect(controller.target.z, closeTo(0.0, 1e-9));
    expect(controller.yaw, closeTo(OfficeSceneController.initialYaw, 1e-9));
    expect(controller.canReset, isFalse);
  });

  testWidgets('interaction surface maps left pan and right horizontal orbit', (
    tester,
  ) async {
    final controller = OfficeSceneController()..setReady(true);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 240,
            height: 180,
            child: OfficeSceneInteractionSurface(
              controller: controller,
              child: const ColoredBox(color: Colors.black),
            ),
          ),
        ),
      ),
    );

    final initialYaw = controller.yaw;
    final leftDrag = await tester.startGesture(
      const Offset(100, 80),
      kind: PointerDeviceKind.mouse,
      buttons: kPrimaryMouseButton,
    );
    await leftDrag.moveBy(const Offset(40, 20));
    await leftDrag.up();
    await tester.pump();

    expect(controller.canReset, isTrue);
    expect(controller.yaw, closeTo(initialYaw, 1e-9));

    controller.reset();
    final rightDrag = await tester.startGesture(
      const Offset(100, 80),
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await rightDrag.moveBy(const Offset(40, 20));
    await rightDrag.up();
    await tester.pump();

    expect(controller.yaw, greaterThan(initialYaw));
    expect(controller.target.x, closeTo(0.0, 1e-9));
    expect(controller.target.z, closeTo(0.0, 1e-9));

    controller.reset();
    final clicked = await tester.startGesture(
      const Offset(100, 80),
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
