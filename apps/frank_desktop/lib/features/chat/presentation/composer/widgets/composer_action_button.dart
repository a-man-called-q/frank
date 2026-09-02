import 'package:flutter/material.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/app/theme.dart';

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
        child: IconButton(
          onPressed: onStop,
          tooltip: 'Stop generation',
          constraints: const BoxConstraints.tightFor(width: 32, height: 32),
          padding: EdgeInsets.zero,
          style: ButtonStyle(
            minimumSize: const WidgetStatePropertyAll(Size(32, 32)),
            maximumSize: const WidgetStatePropertyAll(Size(32, 32)),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            backgroundColor: const WidgetStatePropertyAll(
              FrankColors.aubergine,
            ),
            shape: WidgetStatePropertyAll(
              RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
            ),
            foregroundColor: const WidgetStatePropertyAll(Colors.white),
            splashFactory: NoSplash.splashFactory,
            animationDuration: Duration.zero,
          ),
          icon: const Icon(FrankIcons.square, size: 13),
        ),
      );
    }

    return Semantics(
      button: true,
      enabled: canSend,
      label: 'Send message',
      child: IconButton(
        onPressed: canSend ? onSend : null,
        tooltip: 'Send message',
        constraints: const BoxConstraints.tightFor(width: 32, height: 32),
        padding: EdgeInsets.zero,
        style: ButtonStyle(
          minimumSize: const WidgetStatePropertyAll(Size(32, 32)),
          maximumSize: const WidgetStatePropertyAll(Size(32, 32)),
          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
          backgroundColor: WidgetStateProperty.resolveWith((states) {
            if (!canSend) return FrankColors.muted.withValues(alpha: 0.2);
            return FrankColors.aubergine;
          }),
          shape: WidgetStatePropertyAll(
            RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
          ),
          foregroundColor: WidgetStateProperty.resolveWith((states) {
            if (!canSend) return FrankColors.muted.withValues(alpha: 0.5);
            return Colors.white;
          }),
          splashFactory: NoSplash.splashFactory,
          animationDuration: Duration.zero,
        ),
        icon: const Icon(FrankIcons.arrowUp, size: 15),
      ),
    );
  }
}
