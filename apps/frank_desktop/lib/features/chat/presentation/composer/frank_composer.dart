import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:forui/forui.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/chat/presentation/composer/base_composer.dart';
import 'package:frank_desktop/features/chat/presentation/composer/widgets/composer_action_button.dart';
import 'package:frank_desktop/features/chat/presentation/composer/widgets/composer_context_pill.dart';

/// Full-featured, desktop-first Frank chat composer.
///
/// Implemented via [BaseComposer] and modular atomic composer widgets.
class FrankComposer extends StatefulWidget {
  const FrankComposer({
    required this.generating,
    required this.onSend,
    required this.onStop,
    required this.project,
    required this.executive,
    this.mission,
    this.placeholder,
    this.environmentLabel = 'Local',
    this.focusNode,
    this.controller,
    super.key,
  });

  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;
  final OfficeProject project;
  final OfficeEmployee executive;
  final OfficeMission? mission;
  final String? placeholder;
  final String environmentLabel;
  final FocusNode? focusNode;
  final TextEditingController? controller;

  @override
  State<FrankComposer> createState() => _FrankComposerState();
}

class _FrankComposerState extends State<FrankComposer> {
  late final FocusNode _internalFocusNode = FocusNode(
    debugLabel: 'Frank composer',
    onKeyEvent: _handleKeyEvent,
  );
  late final TextEditingController _internalController =
      TextEditingController();

  FocusNode get _effectiveFocusNode => widget.focusNode ?? _internalFocusNode;
  TextEditingController get _effectiveController =>
      widget.controller ?? _internalController;

  bool _hasText = false;

  @override
  void initState() {
    super.initState();
    _effectiveController.addListener(_onTextChanged);
    _hasText = _effectiveController.text.trim().isNotEmpty;
  }

  @override
  void didUpdateWidget(FrankComposer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      (oldWidget.controller ?? _internalController).removeListener(
        _onTextChanged,
      );
      _effectiveController.addListener(_onTextChanged);
      _hasText = _effectiveController.text.trim().isNotEmpty;
    }
  }

  @override
  void dispose() {
    _effectiveController.removeListener(_onTextChanged);
    _internalController.dispose();
    _internalFocusNode.dispose();
    super.dispose();
  }

  void _onTextChanged() {
    final hasText = _effectiveController.text.trim().isNotEmpty;
    if (hasText != _hasText && mounted) {
      setState(() {
        _hasText = hasText;
      });
    }
  }

  void _handleSend() {
    final text = _effectiveController.text.trim();
    if (text.isEmpty || widget.generating) return;

    widget.onSend(text);
    _effectiveController.clear();
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;

    if (event.logicalKey == LogicalKeyboardKey.enter) {
      final isShift = HardwareKeyboard.instance.isShiftPressed;
      if (!isShift) {
        // Enter without Shift triggers send
        if (_hasText && !widget.generating) {
          _handleSend();
          return KeyEventResult.handled;
        }
        // Swallow Enter if empty to avoid trailing newline
        return KeyEventResult.handled;
      }
      // Shift+Enter inserts newline normally
      return KeyEventResult.ignored;
    }

    if (event.logicalKey == LogicalKeyboardKey.escape && widget.generating) {
      widget.onStop();
      return KeyEventResult.handled;
    }

    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final shortExecutiveName = widget.executive.name.split(' ').first;
    final contextName = widget.mission?.title ?? widget.project.name;
    final semanticLabel = 'Message $shortExecutiveName';

    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: semanticLabel,
      child: BaseComposer(
        focusNode: _effectiveFocusNode,
        input: Focus(
          onKeyEvent: _handleKeyEvent,
          child: ExcludeSemantics(
            child: FTextField(
              control: FTextFieldControl.managed(
                controller: _effectiveController,
              ),
              focusNode: _effectiveFocusNode,
              minLines: 2,
              maxLines: 6,
              keyboardType: TextInputType.multiline,
              hint:
                  widget.placeholder ??
                  'Brief $shortExecutiveName about what you need…',
              textInputAction: TextInputAction.newline,
            ),
          ),
        ),
        toolbarLeading: ComposerContextPill(
          key: const ValueKey('composer-context'),
          label: '$shortExecutiveName · $contextName',
          icon: FrankIcons.bot,
          tooltip:
              'Working with ${widget.executive.name} in ${widget.project.name}',
        ),
        toolbarTrailing: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              widget.environmentLabel,
              style: const TextStyle(
                color: FrankColors.muted,
                fontSize: 11,
                fontWeight: FontWeight.w500,
              ),
            ),
            const SizedBox(width: 10),
            ComposerActionButton(
              isGenerating: widget.generating,
              canSend: _hasText,
              onSend: _handleSend,
              onStop: widget.onStop,
            ),
          ],
        ),
      ),
    );
  }
}
