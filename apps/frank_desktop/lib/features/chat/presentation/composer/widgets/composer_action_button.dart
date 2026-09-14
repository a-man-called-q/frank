import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import 'package:frank_desktop/app/icons.dart';

/// Primary action button for the composer: Send or Stop generation.
class ComposerActionButton extends StatelessWidget {
  const ComposerActionButton({
    required this.isGenerating,
    required this.canSend,
    required this.onSend,
    required this.onStop,
    super.key,
  });

  final bool isGenerating;
  final bool canSend;
  final VoidCallback onSend;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    if (isGenerating) {
      return Semantics(
        button: true,
        label: 'Stop generation',
        child: FButton.icon(
          onPress: onStop,
          semanticsTooltip: 'Stop generation',
          size: FButtonSizeVariant.sm,
          variant: FButtonVariant.destructive,
          child: const Icon(FrankIcons.square, size: 13),
        ),
      );
    }

    return Semantics(
      button: true,
      enabled: canSend,
      label: 'Send message',
      child: FButton.icon(
        onPress: canSend ? onSend : null,
        semanticsTooltip: 'Send message',
        size: FButtonSizeVariant.sm,
        variant: FButtonVariant.primary,
        child: const Icon(FrankIcons.arrowUp, size: 15),
      ),
    );
  }
}
