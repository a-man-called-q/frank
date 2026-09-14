import 'package:flutter/widgets.dart';
import 'package:frank_desktop/app/theme.dart';

/// Compact context label for the composer's quiet metadata row.
class ComposerContextPill extends StatefulWidget {
  const ComposerContextPill({
    required this.label,
    this.icon,
    this.tooltip,
    this.onTap,
    super.key,
  });

  final String label;
  final IconData? icon;
  final String? tooltip;
  final VoidCallback? onTap;

  @override
  State<ComposerContextPill> createState() => _ComposerContextPillState();
}

class _ComposerContextPillState extends State<ComposerContextPill> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final content = Row(
      mainAxisSize: MainAxisSize.max,
      children: [
        if (widget.icon != null) ...[
          Icon(widget.icon, size: 13, color: FrankColors.muted),
          const SizedBox(width: 6),
        ],
        Flexible(
          child: Text(
            widget.label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: FrankColors.muted,
              fontSize: 12,
              fontWeight: FontWeight.w500,
            ),
          ),
        ),
      ],
    );

    return Semantics(
      button: widget.onTap != null,
      label: widget.tooltip ?? widget.label,
      child: MouseRegion(
        onEnter: widget.onTap == null
            ? null
            : (_) => setState(() => _hovered = true),
        onExit: widget.onTap == null
            ? null
            : (_) => setState(() => _hovered = false),
        cursor: widget.onTap != null
            ? SystemMouseCursors.click
            : SystemMouseCursors.basic,
        child: GestureDetector(
          onTap: widget.onTap,
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 150),
            constraints: const BoxConstraints(maxWidth: 360),
            padding: EdgeInsets.symmetric(
              horizontal: widget.onTap == null ? 2 : 8,
              vertical: 4,
            ),
            decoration: BoxDecoration(
              color: widget.onTap != null && _hovered
                  ? FrankColors.aubergineSoft
                  : const Color(0x00000000),
              borderRadius: BorderRadius.circular(99),
            ),
            child: content,
          ),
        ),
      ),
    );
  }
}
