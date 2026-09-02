import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

abstract final class FrankTypography {
  /// Use the macOS system UI face to match Codex's native text rendering.
  /// Other platforms fall back to Forui's bundled Inter font.
  static const uiFontFamily = '.AppleSystemUIFont';
  static const uiFontFallback = ['packages/forui/Inter'];
  static const monoFontFamily = 'GeistMono';
}

ThemeData buildFrankTheme(Brightness brightness) {
  final dark = brightness == Brightness.dark;
  final scheme = ColorScheme.fromSeed(
    seedColor: FrankColors.aubergine,
    brightness: brightness,
    surface: dark ? const Color(0xFF101113) : const Color(0xFFF7F7F4),
  );
  final immediateButtonStyle = ButtonStyle(
    overlayColor: WidgetStateProperty.resolveWith((states) {
      if (states.contains(WidgetState.pressed)) return Colors.transparent;
      if (states.contains(WidgetState.focused)) {
        return Colors.transparent;
      }
      if (states.contains(WidgetState.hovered)) {
        return scheme.onSurface.withValues(alpha: 0.08);
      }
      return Colors.transparent;
    }),
    splashFactory: NoSplash.splashFactory,
    animationDuration: Duration.zero,
  );

  return ThemeData(
    brightness: brightness,
    colorScheme: scheme,
    useMaterial3: true,
    // Frank is a desktop-first surface. Pointer clicks should feel immediate,
    // without Material's expanding splash or pressed-state wash.
    splashFactory: NoSplash.splashFactory,
    splashColor: Colors.transparent,
    highlightColor: Colors.transparent,
    focusColor: Colors.transparent,
    iconButtonTheme: IconButtonThemeData(style: immediateButtonStyle),
    textButtonTheme: TextButtonThemeData(style: immediateButtonStyle),
    filledButtonTheme: FilledButtonThemeData(style: immediateButtonStyle),
    elevatedButtonTheme: ElevatedButtonThemeData(style: immediateButtonStyle),
    outlinedButtonTheme: OutlinedButtonThemeData(style: immediateButtonStyle),
    menuButtonTheme: MenuButtonThemeData(style: immediateButtonStyle),
    inputDecorationTheme: const InputDecorationTheme(
      focusedBorder: UnderlineInputBorder(
        borderSide: BorderSide(color: FrankColors.border),
      ),
    ),
    fontFamily: FrankTypography.uiFontFamily,
    fontFamilyFallback: FrankTypography.uiFontFallback,
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(
        color: FrankColors.tooltipPanel,
        borderRadius: BorderRadius.circular(9),
        boxShadow: const [
          BoxShadow(
            color: Color(0x66000000),
            blurRadius: 18,
            offset: Offset(0, 8),
          ),
        ],
      ),
      textStyle: const TextStyle(
        color: FrankColors.ink,
        fontSize: 12,
        height: 16 / 12,
      ),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      constraints: const BoxConstraints(maxWidth: 340),
      // Keep the overlay clear of the trigger and prefer the free space above
      // controls so it covers less of the sidebar content.
      margin: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      verticalOffset: 8,
      preferBelow: false,
      waitDuration: const Duration(milliseconds: 350),
      exitDuration: const Duration(milliseconds: 100),
    ),
    scaffoldBackgroundColor: dark
        ? const Color(0xFF101113)
        : const Color(0xFFF7F7F4),
    dividerColor: dark ? const Color(0xFF2B2D31) : const Color(0xFFE0E0DA),
    cardTheme: CardThemeData(
      color: dark ? const Color(0xFF17191C) : Colors.white,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(14),
        side: BorderSide(
          color: dark ? const Color(0xFF2B2D31) : const Color(0xFFE0E0DA),
        ),
      ),
    ),
  );
}

/// The Forui surface configuration used by Frank's desktop shell.
///
/// Keep this alongside the Material theme so the two tooltip implementations
/// share the same timing and contrast. Sidebar translucency is supplied by the
/// macOS window effect at the shell boundary; this global style is the solid
/// fallback used by tests and non-macOS platforms.
FThemeData buildFrankForuiTheme(Brightness brightness) {
  final base = brightness == Brightness.dark
      ? FTheme.neutral.dark.desktop
      : FTheme.neutral.light.desktop;
  return base.copyWith(
    sidebarStyle: FSidebarStyleDelta.delta(
      decoration: DecorationDelta.value(
        BoxDecoration(
          color: FrankColors.sidebarSolid,
          border: Border(
            right: BorderSide(
              color: FrankColors.border.withValues(alpha: 0.82),
            ),
          ),
        ),
      ),
    ),
    tooltipStyle: FTooltipStyleDelta.delta(
      decoration: DecorationDelta.value(
        BoxDecoration(
          color: FrankColors.tooltipPanel,
          borderRadius: BorderRadius.circular(9),
          boxShadow: const [
            BoxShadow(
              color: Color(0x66000000),
              blurRadius: 18,
              offset: Offset(0, 8),
            ),
          ],
        ),
      ),
      backgroundFilter: ui.ImageFilter.blur(sigmaX: 10, sigmaY: 10),
      padding: EdgeInsetsDelta.value(
        const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      ),
      constraints: const BoxConstraints(maxWidth: 340),
      textStyle: TextStyleDelta.value(
        const TextStyle(
          color: FrankColors.ink,
          fontFamily: FrankTypography.uiFontFamily,
          fontFamilyFallback: FrankTypography.uiFontFallback,
          fontSize: 12,
          height: 16 / 12,
        ),
      ),
      hoverEnterDuration: const Duration(milliseconds: 350),
      hoverExitDuration: const Duration(milliseconds: 100),
    ),
  );
}

abstract final class FrankColors {
  static const aubergine = Color(0xFF9A68A5);
  static const aubergineSoft = Color(0xFF302238);
  static const warningAmber = Color(0xFFE2A84B);
  static const warningAmberSoft = Color(0xFF3A2E1C);
  static const ink = Color(0xFFE8E9E7);
  static const muted = Color(0xFF9A9D9B);
  static const canvas = Color(0xFF101113);
  static const panel = Color(0xFF17191C);
  static const panelRaised = Color(0xFF1D2024);
  static const sidebarGlass = Color(0x401A1821);
  static const sidebarSolid = Color(0xFF1A1821);
  static const tooltipPanel = Color(0xE61D1724);
  static const border = Color(0xFF2B2D31);
  static const green = Color(0xFF77C69B);
  static const blue = Color(0xFF82B7E8);
}
