import 'package:flutter/animation.dart';

/// Shared geometry and normalization rules for the desktop sidebar.
///
/// Keeping these values outside the widget tree means persisted preferences and
/// transient drag previews cannot drift apart.
abstract final class SidebarLayout {
  static const defaultWidth = 264.0;
  static const minWidth = 240.0;
  static const maxWidth = 420.0;
  static const collapseThreshold = 200.0;
  static const resizeStep = 16.0;
  static const resizeHandleWidth = 8.0;
  static const animationDuration = Duration(milliseconds: 300);
  static const animationCurve = Curves.easeInOutCubic;

  /// Normalizes a stored or committed width to the visible sidebar range.
  static double normalizeWidth(double? width) {
    if (width == null || !width.isFinite) return defaultWidth;
    return width.clamp(minWidth, maxWidth).toDouble();
  }

  /// Keeps a drag preview inside the slot's physical range, including the
  /// partially-clipped area used immediately before collapse.
  static double clampPreview(double width) {
    if (!width.isFinite) return 0;
    return width.clamp(0, maxWidth).toDouble();
  }

  /// The content remains usable at the visible minimum while its viewport can
  /// be narrower during a collapse preview.
  static double contentWidthForPreview(double width) {
    return normalizeWidth(clampPreview(width));
  }

  static bool shouldCollapse(double width) =>
      width.isFinite && width < collapseThreshold;
}
