import 'dart:ui' as ui;

import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

abstract final class FrankTypography {
  /// Use the macOS system UI face to match Codex's native text rendering.
  /// Other platforms fall back to Forui's bundled Inter font.
  static const uiFontFamily = '.AppleSystemUIFont';
  static const uiFontFallback = ['packages/forui/Inter'];
  static const monoFontFamily = 'GeistMono';
}

/// Shared geometry and interaction values for Frank's compact desktop UI.
///
/// The sidebar is the visual reference for feature surfaces. Keeping these
/// values in the app theme lets Organization (and future Office surfaces) use
/// the same quiet rhythm without coupling feature code to Forui internals.
abstract final class FrankUiTokens {
  static const pageTitleSize = 22.0;
  static const pageTitleLineHeight = 28.0;
  static const cardTitleSize = 14.0;
  static const bodyTextSize = 13.0;
  static const metadataTextSize = 12.0;
  static const controlRadius = 7.0;
  static const panelRadius = 8.0;
  static const controlHeight = 32.0;
  static const toolbarHeight = 36.0;
  static const inspectorRailWidth = 320.0;
  static const iconSize = 16.0;
  static const textSize = 12.0;
  static const hoverInkOpacity = 0.06;
  static const selectedInkOpacity = 0.08;
  static const borderWidth = 1.0;
  static const inset = 12.0;
  static const panelPadding = 16.0;
  static const drawerPadding = 16.0;
  static const dialogBodyPadding = 24.0;
  static const footerVerticalPadding = 16.0;
  static const footerHorizontalPadding = 24.0;
  static const rowHorizontalPadding = 16.0;
  static const rowVerticalPadding = 12.0;
  static const actionGap = 8.0;
  static const pageGutter = 24.0;
  static const compactPageGutter = 16.0;
  static const motionFast = Duration(milliseconds: 120);
  static const motionStandard = Duration(milliseconds: 160);
  static const motionSlow = Duration(milliseconds: 180);
}

/// The single dark desktop surface configuration used by Frank.
///
/// Sidebar translucency is supplied by the macOS window effect at the shell
/// boundary; this global style is the solid fallback used by tests and
/// non-macOS platforms.
FThemeData buildFrankTheme() {
  final base = FTheme.neutral.dark.desktop;
  final colors = base.colors.copyWith(
    background: FrankColors.canvas,
    foreground: FrankColors.ink,
    primary: FrankColors.brandPrimary,
    primaryForeground: FrankColors.canvas,
    secondary: FrankColors.panelRaised,
    secondaryForeground: FrankColors.ink,
    muted: FrankColors.panel,
    mutedForeground: FrankColors.muted,
    destructive: FrankColors.failure,
    destructiveForeground: FrankColors.canvas,
    error: FrankColors.failure,
    errorForeground: FrankColors.canvas,
    card: FrankColors.panel,
    border: FrankColors.border,
  );
  final theme = FThemeData(
    colors: colors,
    touch: false,
    debugLabel: 'Frank Dark Desktop',
    breakpoints: base.breakpoints,
    typography: base.typography,
    icons: base.icons,
    style: base.style,
    hapticFeedback: base.hapticFeedback,
  );
  return theme.copyWith(
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
  /// Base brand hue. Keep this dark enough to read as a surface tint rather
  /// than a second accent color.
  static const aubergine = Color(0xFF4A263D);
  static const aubergineSoft = Color(0xFF241921);
  static const aubergineAccent = Color(0xFF9B708D);
  // Canonical operations-lab tokens. The older aubergine names above remain
  // as compatibility aliases for the login and organization integrations;
  // new surfaces should use these explicit tokens so the hierarchy is easy to
  // audit against the visual system.
  static const aubergineSelection = Color(0xFF6B3A58);
  static const accent = Color(0xFFC394B4);
  static const greenSoft = Color(0xFF1E2A20);
  // Semantic action/text/boundary tokens. Keep these names stable so feature
  // surfaces do not encode the current hue directly and so contrast checks
  // can audit one palette in isolation.
  static const brandPrimary = Color(0xFF7A3D68);
  static const secondaryAction = panelRaised;
  static const textPrimary = Color(0xFFF2F1EB);
  static const textMuted = Color(0xFFA4A8A3);
  static const secondaryTextOpaque = textMuted;
  static const panelBorder = Color(0xFF303338);
  static const strongBorder = Color(0xFF4A4E55);
  static const focusRing = accent;
  static const warningAmber = Color(0xFFE2A84B);
  static const warningAmberSoft = Color(0xFF3A2E1C);
  static const ink = textPrimary;
  static const muted = textMuted;
  static const canvas = Color(0xFF101113);
  static const panel = Color(0xFF17191C);
  static const panelRaised = Color(0xFF1D2024);
  static const sidebarGlass = Color(0x240C0D10);
  static const sidebarSolid = Color(0xFF17191C);
  static const tooltipPanel = Color(0xE61D1724);
  static const border = panelBorder;
  static const statusSuccess = Color(0xFFA8C97E);
  static const blue = Color(0xFF82B7E8);
  static const failure = Color(0xFFE47B7B);
}
