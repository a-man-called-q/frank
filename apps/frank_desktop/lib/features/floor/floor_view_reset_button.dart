import 'package:flutter/material.dart';

import '../../app/icons.dart';
import '../../app/theme.dart';

/// Compact camera control rendered over the floor, away from chat input.
class FloorViewResetButton extends StatelessWidget {
  const FloorViewResetButton({
    required this.enabled,
    required this.onPressed,
    super.key,
  });

  final bool enabled;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      enabled: enabled,
      label: 'Reset floor view',
      child: IconButton(
        key: const ValueKey('floor-reset-view-button'),
        onPressed: enabled ? onPressed : null,
        tooltip: 'Reset floor view',
        constraints: const BoxConstraints.tightFor(width: 36, height: 36),
        padding: EdgeInsets.zero,
        style: ButtonStyle(
          minimumSize: const WidgetStatePropertyAll(Size(36, 36)),
          maximumSize: const WidgetStatePropertyAll(Size(36, 36)),
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          backgroundColor: const WidgetStatePropertyAll(
            FrankColors.panelRaised,
          ),
          side: const WidgetStatePropertyAll(
            BorderSide(color: FrankColors.border),
          ),
          shape: WidgetStatePropertyAll(
            RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          ),
          overlayColor: WidgetStateProperty.resolveWith((states) {
            if (states.contains(WidgetState.hovered) ||
                states.contains(WidgetState.focused)) {
              return FrankColors.aubergineSoft;
            }
            return Colors.transparent;
          }),
          foregroundColor: WidgetStateProperty.resolveWith((states) {
            if (!enabled) return FrankColors.muted.withValues(alpha: 0.4);
            if (states.contains(WidgetState.hovered) ||
                states.contains(WidgetState.focused)) {
              return FrankColors.ink;
            }
            return FrankColors.muted;
          }),
          splashFactory: NoSplash.splashFactory,
          animationDuration: Duration.zero,
        ),
        icon: const Icon(FrankIcons.recenter, size: 16),
      ),
    );
  }
}
