import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../app/theme.dart';
import '../../core/models/ledger_models.dart';

/// A tiny dependency-free trend renderer for the presentation prototype.
///
/// The painter has no animation or time-based state, which keeps goldens
/// stable. The surrounding Ledger card provides the textual/semantic version
/// of the chart because a canvas itself is not an accessible control.
class LedgerTrendPainter extends CustomPainter {
  const LedgerTrendPainter({required this.points});

  final List<LedgerTrendPoint> points;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty) return;

    final chart = Rect.fromLTWH(
      0,
      8,
      math.max(0, size.width),
      math.max(0, size.height - 16),
    );
    final gridPaint = Paint()
      ..color = FrankColors.border.withValues(alpha: 0.7)
      ..strokeWidth = 1;
    for (var index = 0; index < 4; index++) {
      final y = chart.top + chart.height * index / 3;
      canvas.drawLine(Offset(chart.left, y), Offset(chart.right, y), gridPaint);
    }

    if (points.length < 2 || chart.width <= 0 || chart.height <= 0) return;

    final inputValues = points
        .map((point) => point.measuredInputTokens)
        .whereType<int>()
        .toList(growable: false);
    final outputValues = points.map((point) => point.measuredOutputTokens);
    final allValues = <int>[...inputValues, ...outputValues];
    if (allValues.isEmpty) return;
    final maximum = math.max(1, allValues.reduce(math.max)).toDouble();

    _drawSeries(
      canvas,
      chart,
      points,
      (point) => point.measuredInputTokens,
      maximum,
      FrankColors.blue,
    );
    _drawSeries(
      canvas,
      chart,
      points,
      (point) => point.measuredOutputTokens,
      maximum,
      FrankColors.aubergineAccent,
    );
  }

  void _drawSeries(
    Canvas canvas,
    Rect chart,
    List<LedgerTrendPoint> values,
    int? Function(LedgerTrendPoint point) select,
    double maximum,
    Color color,
  ) {
    final linePaint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final dotPaint = Paint()..color = color;
    final path = Path();
    var hasPoint = false;

    for (var index = 0; index < values.length; index++) {
      final value = select(values[index]);
      if (value == null) {
        hasPoint = false;
        continue;
      }
      final x = values.length == 1
          ? chart.center.dx
          : chart.left + chart.width * index / (values.length - 1);
      final y = chart.bottom - chart.height * value / maximum;
      final point = Offset(x, y.clamp(chart.top, chart.bottom));
      if (hasPoint) {
        path.lineTo(point.dx, point.dy);
      } else {
        path.moveTo(point.dx, point.dy);
      }
      canvas.drawCircle(point, 2.75, dotPaint);
      hasPoint = true;
    }
    canvas.drawPath(path, linePaint);
  }

  @override
  bool shouldRepaint(covariant LedgerTrendPainter oldDelegate) =>
      !_samePoints(oldDelegate.points, points);

  bool _samePoints(
    List<LedgerTrendPoint> first,
    List<LedgerTrendPoint> second,
  ) {
    if (identical(first, second)) return true;
    if (first.length != second.length) return false;
    for (var index = 0; index < first.length; index++) {
      final left = first[index];
      final right = second[index];
      if (left.label != right.label ||
          left.measuredInputTokens != right.measuredInputTokens ||
          left.measuredOutputTokens != right.measuredOutputTokens) {
        return false;
      }
    }
    return true;
  }
}
