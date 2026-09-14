import 'package:flutter/widgets.dart';
import 'package:frank_desktop/app/theme.dart';

/// Focus-aware surface shared by Frank's chat and prompt composers.
class BaseComposer extends StatefulWidget {
  const BaseComposer({
    required this.input,
    this.attachmentPreview,
    this.toolbarLeading,
    this.toolbarTrailing,
    this.focusNode,
    this.onTapBackground,
    this.decoration,
    this.padding = const EdgeInsets.fromLTRB(16, 14, 12, 10),
    super.key,
  });

  /// Optional attachment preview rendered inside the card, above the input.
  final Widget? attachmentPreview;

  /// Main text input widget (e.g. [TextField]).
  final Widget input;

  /// Quiet context shown on the left of the bottom row.
  final Widget? toolbarLeading;

  /// Primary actions shown on the right of the bottom row.
  final Widget? toolbarTrailing;

  /// Focus node of the internal input for focus delegation.
  final FocusNode? focusNode;

  /// Callback when user clicks the container background outside input.
  final VoidCallback? onTapBackground;

  /// Custom box decoration for the composer card.
  final BoxDecoration? decoration;

  /// Padding inside the composer card.
  final EdgeInsetsGeometry padding;

  @override
  State<BaseComposer> createState() => _BaseComposerState();
}

class _BaseComposerState extends State<BaseComposer> {
  bool _focused = false;

  @override
  void initState() {
    super.initState();
    _attachFocusNode(widget.focusNode);
  }

  @override
  void didUpdateWidget(covariant BaseComposer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.focusNode != widget.focusNode) {
      oldWidget.focusNode?.removeListener(_handleFocusChanged);
      _attachFocusNode(widget.focusNode);
    }
  }

  @override
  void dispose() {
    widget.focusNode?.removeListener(_handleFocusChanged);
    super.dispose();
  }

  void _attachFocusNode(FocusNode? focusNode) {
    focusNode?.addListener(_handleFocusChanged);
    _focused = focusNode?.hasFocus ?? false;
  }

  void _handleFocusChanged() {
    final focused = widget.focusNode?.hasFocus ?? false;
    if (focused != _focused && mounted) {
      setState(() => _focused = focused);
    }
  }

  void _handleContainerTap() {
    widget.onTapBackground?.call();
    widget.focusNode?.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    final defaultDecoration = BoxDecoration(
      color: FrankColors.panel.withValues(alpha: 0.97),
      borderRadius: BorderRadius.circular(19),
      border: Border.all(
        color: _focused
            ? FrankColors.aubergineAccent.withValues(alpha: 0.72)
            : FrankColors.border.withValues(alpha: 0.92),
      ),
      boxShadow: [
        BoxShadow(
          color: const Color(
            0xFF000000,
          ).withValues(alpha: _focused ? 0.42 : 0.32),
          blurRadius: _focused ? 24 : 18,
          spreadRadius: -6,
          offset: const Offset(0, 8),
        ),
      ],
    );

    final hasToolbar =
        widget.toolbarLeading != null || widget.toolbarTrailing != null;

    final content = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.attachmentPreview != null) ...[
          widget.attachmentPreview!,
          const SizedBox(height: 6),
        ],
        widget.input,
        if (hasToolbar) ...[
          const SizedBox(height: 12),
          Row(
            children: [
              if (widget.toolbarLeading != null)
                Expanded(
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: widget.toolbarLeading!,
                  ),
                )
              else
                const Spacer(),
              if (widget.toolbarTrailing != null) widget.toolbarTrailing!,
            ],
          ),
        ],
      ],
    );

    return Listener(
      behavior: HitTestBehavior.translucent,
      onPointerDown: (_) => _handleContainerTap(),
      child: AnimatedContainer(
        key: const ValueKey('composer-surface'),
        duration: const Duration(milliseconds: 140),
        curve: Curves.easeOutCubic,
        constraints: const BoxConstraints(minHeight: 98),
        decoration: widget.decoration ?? defaultDecoration,
        padding: widget.padding,
        child: content,
      ),
    );
  }
}
