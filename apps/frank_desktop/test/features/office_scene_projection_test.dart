import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/floor/office_scene_floor.dart';
import 'package:vector_math/vector_math.dart' as vm;

void main() {
  test('keeps the vertical world height stable across aspect ratios', () {
    const verticalSize = 12.0;
    final projection = OfficeOrthographicProjection(
      verticalSize: verticalSize,
      near: 0.5,
      far: 80.0,
    );
    final square = projection.getProjectionMatrix(1.0);
    final wide = projection.getProjectionMatrix(2.0);
    final tall = projection.getProjectionMatrix(0.5);

    final expectedYScale = 2.0 / verticalSize;
    expect(square.storage[5], closeTo(expectedYScale, 1e-6));
    expect(wide.storage[5], closeTo(expectedYScale, 1e-6));
    expect(tall.storage[5], closeTo(expectedYScale, 1e-6));
    expect(wide.storage[0], closeTo(square.storage[0] / 2.0, 1e-6));
    expect(tall.storage[0], closeTo(square.storage[0] * 2.0, 1e-6));
  });

  test('maps the configured depth range to Flutter Scene clip depth', () {
    final projection = OfficeOrthographicProjection(
      verticalSize: 10.0,
      near: 2.0,
      far: 42.0,
    );
    final matrix = projection.getProjectionMatrix(16.0 / 9.0);

    expect(matrix.storage[10], closeTo(1.0 / 40.0, 1e-6));
    expect(matrix.storage[14], closeTo(-2.0 / 40.0, 1e-6));
    expect(matrix.storage[15], 1.0);
  });

  test('resize cannot introduce a stretch or a degenerate matrix', () {
    final projection = OfficeOrthographicProjection();
    for (final aspect in <double>[0.25, 0.75, 1.0, 1.7, 3.0]) {
      final matrix = projection.getProjectionMatrix(aspect);
      final horizontalWorldSpan = 2.0 / matrix.storage[0];
      final verticalWorldSpan = 2.0 / matrix.storage[5];
      expect(horizontalWorldSpan / verticalWorldSpan, closeTo(aspect, 1e-6));
      expect(matrix.storage.every((entry) => entry.isFinite), isTrue);
    }
    final invalid = projection.getProjectionMatrix(double.nan);
    expect(
      invalid.storage[0],
      closeTo(projection.getProjectionMatrix(1).storage[0], 1e-6),
    );
  });

  test(
    'clip-space jitter is a constant translation for an orthographic lens',
    () {
      final projection = OfficeOrthographicProjection();
      final base = projection.getProjectionMatrix(1.5);
      final jittered = projection.getProjectionMatrix(
        1.5,
        jitter: vm.Vector2(0.02, -0.03),
      );

      expect(jittered.storage[12], closeTo(0.02, 1e-6));
      expect(jittered.storage[13], closeTo(-0.03, 1e-6));
      expect(jittered.storage[0], base.storage[0]);
      expect(jittered.storage[5], base.storage[5]);
      expect(jittered.storage[10], base.storage[10]);
    },
  );
}
