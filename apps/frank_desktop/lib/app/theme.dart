import 'package:flutter/material.dart';

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
    seedColor: const Color(0xFFE2A84B),
    brightness: brightness,
    surface: dark ? const Color(0xFF101113) : const Color(0xFFF7F7F4),
  );
  final immediateButtonStyle = ButtonStyle(
    overlayColor: WidgetStateProperty.resolveWith((states) {
      if (states.contains(WidgetState.pressed)) return Colors.transparent;
      if (states.contains(WidgetState.focused)) {
        return scheme.primary.withValues(alpha: 0.10);
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
    iconButtonTheme: IconButtonThemeData(style: immediateButtonStyle),
    textButtonTheme: TextButtonThemeData(style: immediateButtonStyle),
    filledButtonTheme: FilledButtonThemeData(style: immediateButtonStyle),
    elevatedButtonTheme: ElevatedButtonThemeData(style: immediateButtonStyle),
    outlinedButtonTheme: OutlinedButtonThemeData(style: immediateButtonStyle),
    menuButtonTheme: MenuButtonThemeData(style: immediateButtonStyle),
    fontFamily: FrankTypography.uiFontFamily,
    fontFamilyFallback: FrankTypography.uiFontFallback,
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

abstract final class FrankColors {
  static const amber = Color(0xFFE2A84B);
  static const amberSoft = Color(0xFF3A2E1C);
  static const ink = Color(0xFFE8E9E7);
  static const muted = Color(0xFF9A9D9B);
  static const canvas = Color(0xFF101113);
  static const panel = Color(0xFF17191C);
  static const panelRaised = Color(0xFF1D2024);
  static const border = Color(0xFF2B2D31);
  static const green = Color(0xFF77C69B);
  static const blue = Color(0xFF82B7E8);
}
