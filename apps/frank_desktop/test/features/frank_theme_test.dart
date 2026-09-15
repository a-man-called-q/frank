import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';

void main() {
  test('buildFrankTheme is a dark desktop ForUI theme', () {
    final theme = buildFrankTheme();

    expect(theme.colors.background, FrankColors.canvas);
    expect(theme.colors.foreground, FrankColors.ink);
    expect(theme.colors.primary, FrankColors.brandPrimary);
    expect(theme.colors.primaryForeground, FrankColors.canvas);
    expect(theme.colors.secondary, FrankColors.panelRaised);
    expect(theme.colors.mutedForeground, FrankColors.muted);
    expect(theme.colors.card, FrankColors.panel);
    expect(theme.colors.border, FrankColors.border);
    expect(theme.colors.destructive, FrankColors.failure);
    expect(theme.colors.error, FrankColors.failure);
    expect(theme.colors.brightness, Brightness.dark);
  });

  test('Frank keeps the native desktop tooltip and sidebar treatment', () {
    final theme = buildFrankTheme();
    final sidebarDecoration = theme.sidebarStyle.decoration as BoxDecoration;
    final tooltipDecoration = theme.tooltipStyle.decoration as BoxDecoration;

    expect(sidebarDecoration.color, FrankColors.sidebarSolid);
    expect(sidebarDecoration.border, isNotNull);
    expect(tooltipDecoration.color, FrankColors.tooltipPanel);
    expect(tooltipDecoration.border, isNull);
    expect(theme.tooltipStyle.backgroundFilter, isNotNull);
    expect(theme.tooltipStyle.constraints.maxWidth, 340);
    expect(
      theme.tooltipStyle.hoverEnterDuration,
      const Duration(milliseconds: 350),
    );
    expect(
      theme.tooltipStyle.hoverExitDuration,
      const Duration(milliseconds: 100),
    );
  });
}
