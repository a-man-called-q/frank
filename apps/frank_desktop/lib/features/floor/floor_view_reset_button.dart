import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import '../../app/icons.dart';

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
      child: FButton.icon(
        key: const ValueKey('floor-reset-view-button'),
        onPress: enabled ? onPressed : null,
        semanticsTooltip: 'Reset floor view',
        size: FButtonSizeVariant.sm,
        child: const Icon(FrankIcons.recenter, size: 16),
      ),
    );
  }
}
