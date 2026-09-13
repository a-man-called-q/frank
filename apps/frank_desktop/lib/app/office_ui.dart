import 'package:flutter/material.dart';

import 'theme.dart';

/// Shared status tones used by the Office surfaces. The label remains part of
/// the control so status is never communicated by color alone.
enum FrankStatusTone { neutral, success, working, attention, failure }

extension FrankStatusToneMetadata on FrankStatusTone {
  Color get color => switch (this) {
    FrankStatusTone.neutral => FrankColors.muted,
    FrankStatusTone.success => FrankColors.green,
    FrankStatusTone.working => FrankColors.blue,
    FrankStatusTone.attention => FrankColors.warningAmber,
    FrankStatusTone.failure => FrankColors.failure,
  };

  Color get softColor => color.withValues(alpha: .12);
}

/// Maps transport and server failures to copy that can be acted on by a
/// person. Feature surfaces may still provide a more specific fallback, but
/// no raw exception should be shown in the reading UI.
String frankFriendlyError(
  Object? error, {
  String fallback = 'Something went wrong.',
}) {
  final raw = error?.toString().trim() ?? '';
  final normalized = raw.toLowerCase();
  if (normalized.contains('organization api') ||
      normalized.contains('organization editor') ||
      normalized.contains('not part of the') ||
      normalized.contains('unsupported')) {
    return 'Organization editor isn’t available on this server build.';
  }
  if (normalized.contains('offline') ||
      normalized.contains('socket') ||
      normalized.contains('connection refused') ||
      normalized.contains('network')) {
    return 'The server is offline right now. Check the connection and try again.';
  }
  if (normalized.contains('timeout') || normalized.contains('timed out')) {
    return 'The server took too long to respond. Try again in a moment.';
  }
  if (normalized.contains('unauthorized') ||
      normalized.contains('forbidden') ||
      normalized.contains('authentication')) {
    return 'Frank could not authenticate with this server. Check the connection.';
  }
  if (raw.isEmpty) return fallback;

  final cleaned = raw
      .replaceFirst(RegExp(r'^(Bad state|StateError|Exception|Error):\s*'), '')
      .trim();
  if (cleaned.isEmpty ||
      cleaned.startsWith('Instance of ') ||
      cleaned.contains('Exception:') ||
      cleaned.contains('Error:')) {
    return fallback;
  }
  // Short gateway messages such as "Catalog unavailable" are already
  // human-readable; preserve them while keeping stack-shaped values hidden.
  return cleaned.length > 180 ? fallback : cleaned;
}

bool frankIsUnsupportedError(Object? error) {
  final normalized = error?.toString().toLowerCase() ?? '';
  return normalized.contains('organization api') ||
      normalized.contains('organization editor') ||
      normalized.contains('not part of the') ||
      normalized.contains('unsupported');
}

/// A small, consistent surface for cards, inspectors, and panels.
class FrankPanel extends StatelessWidget {
  const FrankPanel({
    required this.child,
    this.padding = const EdgeInsets.all(FrankUiTokens.inset),
    this.raised = false,
    this.header,
    super.key,
  });

  final Widget child;
  final EdgeInsetsGeometry padding;
  final bool raised;
  final Widget? header;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: raised ? FrankColors.panelRaised : FrankColors.panel,
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        border: Border.all(
          color: FrankColors.border,
          width: FrankUiTokens.borderWidth,
        ),
      ),
      child: Padding(
        padding: padding,
        child: header == null
            ? child
            : Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [header!, const SizedBox(height: 12), child],
              ),
      ),
    );
  }
}

/// Marks fixture-backed content without conflating it with the shell's
/// connection status.
class FrankSampleDataBadge extends StatelessWidget {
  const FrankSampleDataBadge({super.key});

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: 'Sample data',
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 7),
        decoration: BoxDecoration(
          color: FrankColors.panel,
          borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
          border: Border.all(color: FrankColors.border),
        ),
        child: const Text(
          'Sample data',
          style: TextStyle(
            color: FrankColors.muted,
            fontSize: FrankUiTokens.metadataTextSize,
          ),
        ),
      ),
    );
  }
}

/// The shared status badge primitive for agents, tasks, and connection state.
class FrankStatusBadge extends StatelessWidget {
  const FrankStatusBadge({
    required this.label,
    required this.tone,
    this.icon,
    this.compact = false,
    super.key,
  });

  final String label;
  final FrankStatusTone tone;
  final IconData? icon;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final iconSize = compact ? 12.0 : FrankUiTokens.iconSize;
    final stacked = MediaQuery.textScalerOf(context).scale(1) > 1.25;
    final content = stacked
        ? Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (icon != null) Icon(icon, size: iconSize, color: tone.color),
              Text(
                label,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: TextStyle(
                  color: tone.color,
                  fontSize: compact ? 11 : FrankUiTokens.textSize,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          )
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (icon != null) ...[
                Icon(icon, size: iconSize, color: tone.color),
                const SizedBox(width: 5),
              ],
              Text(
                label,
                style: TextStyle(
                  color: tone.color,
                  fontSize: compact ? 11 : FrankUiTokens.textSize,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          );
    return Semantics(
      label: label,
      child: Container(
        padding: EdgeInsets.symmetric(
          horizontal: compact ? 7 : 9,
          vertical: compact ? 4 : 6,
        ),
        decoration: BoxDecoration(
          color: tone.softColor,
          borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
          border: Border.all(color: tone.color.withValues(alpha: .72)),
        ),
        child: content,
      ),
    );
  }
}

/// Shared agent avatar with a deterministic initials fallback.
class FrankAgentAvatar extends StatelessWidget {
  const FrankAgentAvatar({
    required this.initials,
    this.imageAsset,
    this.color = FrankColors.accent,
    this.size = 32,
    super.key,
  });

  final String initials;
  final String? imageAsset;
  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final image = imageAsset;
    return Container(
      width: size,
      height: size,
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: color.withValues(alpha: .16),
        shape: BoxShape.circle,
        border: Border.all(color: color.withValues(alpha: .55)),
      ),
      child: image == null
          ? Center(
              child: Text(
                initials,
                style: TextStyle(
                  color: color,
                  fontSize: size * .32,
                  fontWeight: FontWeight.w700,
                ),
              ),
            )
          : Image.asset(
              image,
              fit: BoxFit.cover,
              errorBuilder: (_, _, _) => Center(
                child: Text(
                  initials,
                  style: TextStyle(
                    color: color,
                    fontSize: size * .32,
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
            ),
    );
  }
}

/// A compact segmented control used for view and period switches.
class FrankSegmentedControl<T> extends StatelessWidget {
  const FrankSegmentedControl({
    required this.value,
    required this.items,
    required this.onChanged,
    this.itemKeyBuilder,
    this.enabledBuilder,
    super.key,
  });

  final T value;
  final List<(T, String, IconData?)> items;
  final ValueChanged<T> onChanged;
  final Key Function(T value)? itemKeyBuilder;
  final bool Function(T value)? enabledBuilder;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: FrankColors.panel,
        border: Border.all(color: FrankColors.border),
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
      ),
      child: Padding(
        padding: const EdgeInsets.all(3),
        child: Wrap(
          spacing: 2,
          runSpacing: 2,
          children: [
            for (final item in items)
              Builder(
                builder: (context) {
                  final enabled = enabledBuilder?.call(item.$1) ?? true;
                  final selected = value == item.$1;
                  return Semantics(
                    button: true,
                    selected: selected,
                    enabled: enabled,
                    label: item.$2,
                    child: TextButton.icon(
                      key: itemKeyBuilder?.call(item.$1),
                      onPressed: enabled ? () => onChanged(item.$1) : null,
                      style: TextButton.styleFrom(
                        minimumSize: const Size(0, FrankUiTokens.controlHeight),
                        padding: const EdgeInsets.symmetric(horizontal: 10),
                        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        foregroundColor: !enabled
                            ? FrankColors.muted.withValues(alpha: .45)
                            : selected
                            ? FrankColors.ink
                            : FrankColors.muted,
                        backgroundColor: selected
                            ? FrankColors.aubergineSelection.withValues(
                                alpha: .34,
                              )
                            : Colors.transparent,
                        shape: RoundedRectangleBorder(
                          borderRadius: BorderRadius.circular(
                            FrankUiTokens.controlRadius,
                          ),
                        ),
                        textStyle: const TextStyle(
                          fontSize: FrankUiTokens.metadataTextSize,
                        ),
                      ),
                      icon: item.$3 == null
                          ? null
                          : Icon(item.$3, size: FrankUiTokens.iconSize),
                      label: Text(item.$2),
                    ),
                  );
                },
              ),
          ],
        ),
      ),
    );
  }
}

/// A quiet, readable empty state shared by feature pages.
///
/// The optional action is deliberately supplied by the caller. This keeps
/// read-only pages from growing an accidental create flow while allowing
/// setup pages to expose one obvious next step.
class FrankEmptyState extends StatelessWidget {
  const FrankEmptyState({
    required this.title,
    this.description,
    this.message,
    this.icon,
    this.showLogo = false,
    this.action,
    this.actionLabel,
    this.onAction,
    this.alignment = Alignment.center,
    super.key,
  });

  final String title;
  final String? description;
  final String? message;
  final IconData? icon;
  final bool showLogo;
  final Widget? action;
  final String? actionLabel;
  final VoidCallback? onAction;
  final Alignment alignment;

  @override
  Widget build(BuildContext context) {
    final details = [?message, ?description];
    final resolvedAction =
        action ??
        (actionLabel != null && onAction != null
            ? FrankPrimaryAction(label: actionLabel!, onPressed: onAction)
            : null);
    return Align(
      alignment: alignment,
      child: Semantics(
        container: true,
        label: title,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Padding(
            padding: const EdgeInsets.all(28),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (showLogo)
                  ClipRRect(
                    borderRadius: BorderRadius.circular(9),
                    child: Image.asset(
                      'assets/branding/frank-logo.png',
                      width: 52,
                      height: 52,
                      fit: BoxFit.cover,
                      errorBuilder: (_, _, _) => const Icon(
                        Icons.workspaces_outline,
                        size: 36,
                        color: FrankColors.accent,
                      ),
                    ),
                  )
                else if (icon != null)
                  Icon(icon, size: 28, color: FrankColors.accent),
                if (showLogo || icon != null) const SizedBox(height: 14),
                Text(
                  title,
                  textAlign: TextAlign.center,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontSize: 17,
                    height: 1.3,
                    fontWeight: FontWeight.w500,
                  ),
                ),
                if (details.isNotEmpty) ...[
                  const SizedBox(height: 7),
                  for (final detail in details)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 3),
                      child: Text(
                        detail,
                        textAlign: TextAlign.center,
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontSize: 12,
                          height: 1.5,
                        ),
                      ),
                    ),
                ],
                if (resolvedAction != null) ...[
                  const SizedBox(height: 16),
                  resolvedAction,
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// A friendly unavailable state. A retry is opt-in so unsupported server
/// capabilities can explain themselves without presenting a dead button.
class FrankUnavailableState extends StatelessWidget {
  const FrankUnavailableState({
    required this.title,
    required this.message,
    this.icon = Icons.cloud_off_outlined,
    this.onRetry,
    this.retryLabel = 'Retry',
    super.key,
  });

  final String title;
  final String message;
  final IconData icon;
  final VoidCallback? onRetry;
  final String retryLabel;

  @override
  Widget build(BuildContext context) {
    return FrankEmptyState(
      title: title,
      description: message,
      icon: icon,
      action: onRetry == null
          ? null
          : OutlinedButton.icon(
              onPressed: onRetry,
              icon: const Icon(Icons.refresh, size: FrankUiTokens.iconSize),
              label: Text(retryLabel),
            ),
    );
  }
}

/// Inline evidence or warning treatment for a page that should remain usable.
class FrankInlineNotice extends StatelessWidget {
  const FrankInlineNotice({
    this.message,
    this.child,
    this.icon,
    this.tone = FrankStatusTone.neutral,
    super.key,
  });

  final String? message;
  final Widget? child;
  final IconData? icon;
  final FrankStatusTone tone;

  @override
  Widget build(BuildContext context) {
    final content =
        child ??
        (message == null
            ? const SizedBox.shrink()
            : Text(
                message!,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: FrankUiTokens.metadataTextSize,
                  height: 1.4,
                ),
              ));
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 9),
      decoration: BoxDecoration(
        color: tone == FrankStatusTone.success
            ? FrankColors.greenSoft
            : tone.softColor,
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        border: Border.all(color: tone.color.withValues(alpha: .45)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            icon ??
                (tone == FrankStatusTone.failure
                    ? Icons.error_outline
                    : Icons.info_outline),
            size: FrankUiTokens.iconSize,
            color: tone.color,
          ),
          const SizedBox(width: 8),
          Expanded(child: content),
        ],
      ),
    );
  }
}

/// Frank's primary action treatment: green surface, dark ink, and immediate
/// pointer feedback without Material's splash animation.
class FrankPrimaryAction extends StatelessWidget {
  const FrankPrimaryAction({
    required this.label,
    required this.onPressed,
    this.icon,
    super.key,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final button = icon == null
        ? FilledButton(onPressed: onPressed, style: _style, child: Text(label))
        : FilledButton.icon(
            onPressed: onPressed,
            style: _style,
            icon: Icon(icon, size: FrankUiTokens.iconSize),
            label: Text(label),
          );
    return button;
  }

  static final _style = FilledButton.styleFrom(
    minimumSize: const Size(0, FrankUiTokens.controlHeight),
    padding: const EdgeInsets.symmetric(horizontal: 13),
    tapTargetSize: MaterialTapTargetSize.shrinkWrap,
    backgroundColor: FrankColors.primaryAction,
    foregroundColor: FrankColors.canvas,
    disabledBackgroundColor: FrankColors.primaryAction.withValues(alpha: .35),
    disabledForegroundColor: FrankColors.canvas.withValues(alpha: .65),
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
    ),
  );
}

/// Shared compact metric card for headers and evidence summaries.
class FrankMetricCard extends StatelessWidget {
  const FrankMetricCard({
    required this.label,
    required this.value,
    this.detail,
    this.accent = FrankColors.primaryAction,
    super.key,
  });

  final String label;
  final String value;
  final String? detail;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 11),
      decoration: BoxDecoration(
        color: FrankColors.panelRaised,
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 6,
                height: 6,
                decoration: BoxDecoration(
                  color: accent,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 7),
              Flexible(
                child: Text(
                  label,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: FrankUiTokens.metadataTextSize,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            value,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: FrankColors.ink,
              fontFamily: FrankTypography.monoFontFamily,
              fontSize: 17,
              fontWeight: FontWeight.w500,
            ),
          ),
          if (detail != null) ...[
            const SizedBox(height: 3),
            Text(
              detail!,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(
                color: FrankColors.muted,
                fontSize: 11,
                height: 1.3,
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// Low-cost loading placeholder used in reading panels. It intentionally has
/// no texture or network image and becomes static when reduced motion is on.
class FrankSkeleton extends StatelessWidget {
  const FrankSkeleton({
    this.width = double.infinity,
    this.height = 12,
    this.radius = FrankUiTokens.controlRadius,
    this.lines = 1,
    super.key,
  });

  final double width;
  final double height;
  final double radius;
  final int lines;

  @override
  Widget build(BuildContext context) {
    final count = lines.clamp(1, 8);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var index = 0; index < count; index++) ...[
          FractionallySizedBox(
            widthFactor: index == count - 1 && count > 1 ? .62 : 1,
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: FrankColors.panelRaised,
                borderRadius: BorderRadius.circular(radius),
                border: Border.all(
                  color: FrankColors.border.withValues(alpha: .7),
                ),
              ),
              child: SizedBox(width: width, height: height),
            ),
          ),
          if (index != count - 1) const SizedBox(height: 8),
        ],
      ],
    );
  }
}

/// Subtle radial + grid treatment for canvas surfaces. Reading panels remain
/// solid and therefore preserve text contrast.
class FrankBackdrop extends StatelessWidget {
  const FrankBackdrop({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: FrankColors.canvas,
        gradient: RadialGradient(
          center: Alignment(-.7, -.85),
          radius: 1.15,
          colors: [Color(0x241F2A22), Colors.transparent],
        ),
      ),
      child: CustomPaint(painter: _FrankGridPainter(), child: child),
    );
  }
}

class _FrankGridPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = FrankColors.border.withValues(alpha: .17)
      ..strokeWidth = .5;
    const step = 32.0;
    for (var x = 0.0; x <= size.width; x += step) {
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), paint);
    }
    for (var y = 0.0; y <= size.height; y += step) {
      canvas.drawLine(Offset(0, y), Offset(size.width, y), paint);
    }
  }

  @override
  bool shouldRepaint(covariant _FrankGridPainter oldDelegate) => false;
}
