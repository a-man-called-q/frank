import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';

void main() {
  test('Material interactions do not animate a pressed-state overlay', () {
    final theme = buildFrankTheme(Brightness.dark);
    const pressed = {WidgetState.pressed};
    const focused = {WidgetState.focused};

    expect(theme.splashFactory, same(NoSplash.splashFactory));
    expect(theme.splashColor, Colors.transparent);
    expect(theme.highlightColor, Colors.transparent);
    expect(theme.focusColor, Colors.transparent);
    expect(
      theme.iconButtonTheme.style?.overlayColor?.resolve(pressed),
      Colors.transparent,
    );
    expect(
      theme.iconButtonTheme.style?.overlayColor?.resolve(focused),
      Colors.transparent,
    );
    expect(
      theme.textButtonTheme.style?.overlayColor?.resolve(pressed),
      Colors.transparent,
    );
    expect(theme.filledButtonTheme.style?.animationDuration, Duration.zero);
  });

  test('Frank uses aubergine accents and themed Material/Forui tooltips', () {
    final theme = buildFrankTheme(Brightness.dark);
    final materialDecoration = theme.tooltipTheme.decoration! as BoxDecoration;
    final forui = buildFrankForuiTheme(Brightness.dark);
    final foruiSidebarDecoration =
        forui.sidebarStyle.decoration as BoxDecoration;
    final foruiTooltipDecoration =
        forui.tooltipStyle.decoration as BoxDecoration;

    expect(FrankColors.aubergine, const Color(0xFF9A68A5));
    expect(FrankColors.aubergineSoft, const Color(0xFF302238));
    expect(FrankColors.warningAmber, const Color(0xFFE2A84B));
    expect(theme.colorScheme.primary, isNot(const Color(0xFFE2A84B)));
    expect(materialDecoration.color, FrankColors.tooltipPanel);
    expect(materialDecoration.border, isNull);
    expect(theme.tooltipTheme.constraints?.maxWidth, 340);
    expect(theme.tooltipTheme.preferBelow, isFalse);
    expect(theme.tooltipTheme.verticalOffset, 8);
    expect(
      theme.tooltipTheme.margin,
      const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
    );
    // Sidebar translucency is supplied by the native macOS visual-effect
    // subview.  The Flutter/Forui style stays solid for non-macOS and tests.
    expect(forui.sidebarStyle.backgroundFilter, isNull);
    expect(foruiSidebarDecoration.color, FrankColors.sidebarSolid);
    expect(forui.tooltipStyle.backgroundFilter, isNotNull);
    expect(foruiTooltipDecoration.color, FrankColors.tooltipPanel);
    expect(foruiTooltipDecoration.border, isNull);
    expect(forui.tooltipStyle.constraints.maxWidth, 340);
    expect(
      forui.tooltipStyle.hoverEnterDuration,
      const Duration(milliseconds: 350),
    );
    expect(
      forui.tooltipStyle.hoverExitDuration,
      const Duration(milliseconds: 100),
    );
  });
}
