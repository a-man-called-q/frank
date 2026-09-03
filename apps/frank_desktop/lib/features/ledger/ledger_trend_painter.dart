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

/// Compact token count for axis ticks and ranges: 1.2k, 3m, 940.
///
/// Lives beside the painters because they are its primary caller; the Ledger
/// surface reuses it so a tick and a table cell never disagree about how a
/// number is abbreviated.
String formatCompactCount(int value) {
  if (value >= 1000000) {
    return '${(value / 1000000).toStringAsFixed(value % 1000000 == 0 ? 0 : 1)}m';
  }
  if (value >= 1000) {
    return '${(value / 1000).toStringAsFixed(value % 1000 == 0 ? 0 : 1)}k';
  }
  return value.toString();
}

/// Trend renderer with a labelled value axis and an explicit empty state.
///
/// The Effectiveness tab needs its axis to read a measured token count off the
/// chart, which is why it does not share [LedgerTrendPainter]'s bare-grid
/// treatment. Both live here so the overlap between them stays visible; folding
/// them into one parameterised painter is a deliberate change to make against
/// the goldens, not an incidental cleanup.
class LedgerAxisTrendPainter extends CustomPainter {
  LedgerAxisTrendPainter(this.points);

  final List<LedgerTrendPoint> points;

  @override
  void paint(Canvas canvas, Size size) {
    if (points.isEmpty) {
      _paintEmpty(canvas, size);
      return;
    }
    const left = 42.0;
    const right = 10.0;
    const top = 12.0;
    const bottom = 31.0;
    final plot = Rect.fromLTRB(
      left,
      top,
      math.max(left + 20, size.width - right),
      math.max(top + 20, size.height - bottom),
    );
    final maxValue = points.fold<double>(
      1,
      (maximum, point) => math.max(
        maximum,
        math.max(
          point.measuredOutputTokens.toDouble(),
          point.estimatedSaved?.high.toDouble() ?? 0,
        ),
      ),
    );
    final chartMax = _niceCeiling(maxValue);
    double y(double value) => plot.bottom - (value / chartMax) * plot.height;
    double x(int index) => points.length == 1
        ? plot.center.dx
        : plot.left + (plot.width * index / (points.length - 1));

    final gridPaint = Paint()
      ..color = FrankColors.border.withValues(alpha: .74)
      ..strokeWidth = 1;
    final faintGridPaint = Paint()
      ..color = FrankColors.border.withValues(alpha: .42)
      ..strokeWidth = 1;
    final textStyle = const TextStyle(color: FrankColors.muted, fontSize: 10);
    for (var tick = 0; tick <= 3; tick++) {
      final value = chartMax * tick / 3;
      final dy = y(value);
      canvas.drawLine(
        Offset(plot.left, dy),
        Offset(plot.right, dy),
        tick == 0 ? gridPaint : faintGridPaint,
      );
      _paintText(
        canvas,
        formatCompactCount(value.round()),
        Offset(plot.left - 8, dy - 6),
        textStyle,
        alignRight: true,
      );
    }

    if (points.length > 1) {
      final band = Path();
      for (var index = 0; index < points.length; index++) {
        final range = points[index].estimatedSaved;
        final point = Offset(x(index), y(range?.high.toDouble() ?? 0));
        if (index == 0) {
          band.moveTo(point.dx, point.dy);
        } else {
          band.lineTo(point.dx, point.dy);
        }
      }
      for (var index = points.length - 1; index >= 0; index--) {
        final range = points[index].estimatedSaved;
        band.lineTo(x(index), y(range?.low.toDouble() ?? 0));
      }
      band.close();
      canvas.drawPath(
        band,
        Paint()
          ..color = FrankColors.aubergineAccent.withValues(alpha: .14)
          ..style = PaintingStyle.fill,
      );
      final bandOutline = Paint()
        ..color = FrankColors.aubergineAccent.withValues(alpha: .68)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round;
      final high = Path();
      final low = Path();
      for (var index = 0; index < points.length; index++) {
        final range = points[index].estimatedSaved;
        final highPoint = Offset(x(index), y(range?.high.toDouble() ?? 0));
        final lowPoint = Offset(x(index), y(range?.low.toDouble() ?? 0));
        if (index == 0) {
          high.moveTo(highPoint.dx, highPoint.dy);
          low.moveTo(lowPoint.dx, lowPoint.dy);
        } else {
          high.lineTo(highPoint.dx, highPoint.dy);
          low.lineTo(lowPoint.dx, lowPoint.dy);
        }
      }
      canvas.drawPath(high, bandOutline);
      canvas.drawPath(low, bandOutline);
    }

    final measured = Path();
    for (var index = 0; index < points.length; index++) {
      final point = Offset(
        x(index),
        y(points[index].measuredOutputTokens.toDouble()),
      );
      if (index == 0) {
        measured.moveTo(point.dx, point.dy);
      } else {
        measured.lineTo(point.dx, point.dy);
      }
    }
    canvas.drawPath(
      measured,
      Paint()
        ..color = FrankColors.green
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2.5
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round,
    );
    final pointPaint = Paint()..color = FrankColors.green;
    for (var index = 0; index < points.length; index++) {
      canvas.drawCircle(
        Offset(x(index), y(points[index].measuredOutputTokens.toDouble())),
        3,
        pointPaint,
      );
    }

    final labelEvery = size.width < 470 ? 2 : 1;
    for (var index = 0; index < points.length; index++) {
      if (index % labelEvery != 0 && index != points.length - 1) continue;
      final label = points[index].label;
      final textWidth = _measureText(label, textStyle).width;
      final centered = x(index) - textWidth / 2;
      final leftOffset = centered
          .clamp(plot.left - 10, plot.right - textWidth + 10)
          .toDouble();
      _paintText(canvas, label, Offset(leftOffset, plot.bottom + 9), textStyle);
    }
  }

  void _paintEmpty(Canvas canvas, Size size) {
    _paintText(
      canvas,
      'No measured usage yet',
      Offset(size.width / 2 - 58, size.height / 2 - 6),
      const TextStyle(color: FrankColors.muted, fontSize: 12),
    );
  }

  double _niceCeiling(double value) {
    final magnitude = math
        .pow(10, (math.log(value) / math.ln10).floor())
        .toDouble();
    final normalized = value / magnitude;
    final step = normalized <= 1
        ? 1
        : normalized <= 2
        ? 2
        : normalized <= 5
        ? 5
        : 10;
    return step * magnitude;
  }

  void _paintText(
    Canvas canvas,
    String text,
    Offset offset,
    TextStyle style, {
    bool alignRight = false,
  }) {
    final painter = _measureText(text, style);
    painter.paint(
      canvas,
      alignRight ? Offset(offset.dx - painter.width, offset.dy) : offset,
    );
  }

  TextPainter _measureText(String text, TextStyle style) {
    return TextPainter(
      text: TextSpan(text: text, style: style),
      textDirection: TextDirection.ltr,
    )..layout();
  }

  @override
  bool shouldRepaint(covariant LedgerAxisTrendPainter oldDelegate) =>
      oldDelegate.points != points;
}
