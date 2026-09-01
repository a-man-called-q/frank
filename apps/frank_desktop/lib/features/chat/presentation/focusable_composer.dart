import 'package:flutter/material.dart';
import 'package:flow_ui/flow_ui.dart';

/// Presentation-only composer boundary.
///
/// Focus ownership belongs to the widget because it is transient UI state;
/// message submission and streaming state remain owned by [ChatBloc].
class FocusableComposer extends StatefulWidget {
  const FocusableComposer({
    required this.generating,
    required this.onSend,
    required this.onStop,
    super.key,
  });

  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;

  @override
  State<FocusableComposer> createState() => _FocusableComposerState();
}

class _FocusableComposerState extends State<FocusableComposer> {
  late final FocusNode _focusNode = FocusNode(debugLabel: 'Frank composer');

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: 'Message Maya',
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: (_) => _focusNode.requestFocus(),
        child: FlowComposer(
          focusNode: _focusNode,
          placeholder: 'Brief Maya about what your company needs…',
          isStreaming: widget.generating,
          onSend: widget.onSend,
          onStop: widget.onStop,
        ),
      ),
    );
  }
}
