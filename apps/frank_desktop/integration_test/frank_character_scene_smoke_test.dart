import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import '../test/support/frank_test_app.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_scene/scene.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:vector_math/vector_math.dart' as vm;

void main() {
  testWidgets('flutter_scene renders the Frank GLB and green material', (
    tester,
  ) async {
    await Scene.initializeStaticResources();
    final scene = Scene();
    final model = await loadScene(
      'assets/character_model/frank.glb',
      applyStageTo: scene,
    );
    scene.add(model);
    scene.directionalLight = DirectionalLight(
      direction: vm.Vector3(-0.4, -0.8, -0.6),
      color: vm.Vector3(1, 0.92, 0.82),
      intensity: 2.4,
    );

    final repaintKey = GlobalKey();
    final camera = PerspectiveCamera(
      position: vm.Vector3(0, 0.75, -3.2),
      target: vm.Vector3(0, 0.75, 0),
    );
    await tester.pumpWidget(
      FrankTestApp(
        home: Center(
          child: SizedBox.square(
            dimension: 512,
            child: RepaintBoundary(
              key: repaintKey,
              child: SceneView(scene, camera: camera, warmUp: true),
            ),
          ),
        ),
      ),
    );

    for (var frame = 0; frame < 24; frame++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(tester.takeException(), isNull);

    final boundary =
        repaintKey.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 1);
    final raw = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
    final bytes = raw!.buffer.asUint8List();
    var greenPixels = 0;
    for (var offset = 0; offset < bytes.length; offset += 4) {
      final red = bytes[offset];
      final green = bytes[offset + 1];
      final blue = bytes[offset + 2];
      final alpha = bytes[offset + 3];
      if (alpha > 180 && green > red * 1.15 && green > blue * 1.25) {
        greenPixels++;
      }
    }
    expect(greenPixels, greaterThan(1000));

    final png = await image.toByteData(format: ui.ImageByteFormat.png);
    final qaFile = File(
      '${Directory.systemTemp.path}/frank_flutter_scene_runtime.png',
    );
    await qaFile.writeAsBytes(png!.buffer.asUint8List());
    debugPrint('FRANK_QA_SCREENSHOT=${qaFile.path}');
    image.dispose();
  }, timeout: const Timeout(Duration(minutes: 2)));
}
