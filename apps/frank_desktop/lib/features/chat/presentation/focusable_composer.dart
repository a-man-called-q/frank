import 'package:flutter/material.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

import 'composer/frank_composer.dart';

/// Presentation-only composer boundary.
///
/// Focus ownership belongs to the widget because it is transient UI state;
/// message submission and streaming state remain owned by [ChatBloc].
class FocusableComposer extends StatefulWidget {
  const FocusableComposer({
    required this.generating,
    required this.onSend,
    required this.onStop,
    required this.executive,
    required this.project,
    required this.mission,
    super.key,
  });

  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;
  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;

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
    return FrankComposer(
      focusNode: _focusNode,
      generating: widget.generating,
      onSend: widget.onSend,
      onStop: widget.onStop,
      executive: widget.executive,
      project: widget.project,
      mission: widget.mission,
    );
  }
}
