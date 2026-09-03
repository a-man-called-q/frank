import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flow_ui/flow_ui.dart';
import 'package:material_ui/material_ui.dart' as mui;

import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';
import '../floor/floor_view_reset_button.dart';
import '../floor/office_scene_floor.dart';
import 'presentation/focusable_composer.dart';

class AccountExecutiveChat extends StatefulWidget {
  const AccountExecutiveChat({
    required this.executive,
    required this.project,
    required this.mission,
    required this.officeView,
    required this.messages,
    required this.generating,
    required this.onSend,
    required this.onStop,
    this.renderFloor = true,
    super.key,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;
  final bool officeView;
  final List<OfficeMessage> messages;
  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;
  final bool renderFloor;

  @override
  State<AccountExecutiveChat> createState() => _AccountExecutiveChatState();
}

class _AccountExecutiveChatState extends State<AccountExecutiveChat> {
  late final OfficeSceneController _sceneController = OfficeSceneController();

  @override
  void dispose() {
    _sceneController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: _conversationLabel,
      child: LayoutBuilder(
        builder: (context, constraints) {
          final availableWidth = constraints.maxWidth.isFinite
              ? constraints.maxWidth
              : 760.0;
          final availableHeight = constraints.maxHeight.isFinite
              ? constraints.maxHeight
              : 640.0;
          final railWidth = math.min(760.0, math.max(1.0, availableWidth - 32));
          final railHeight = math.min(
            560.0,
            math.max(260.0, availableHeight * 0.50),
          );
          final showConversationRail =
              availableWidth >= 360 && availableHeight >= 340;

          return Stack(
            fit: StackFit.expand,
            children: [
              if (widget.renderFloor) ...[
                Positioned.fill(
                  child: OfficeSceneFloor(controller: _sceneController),
                ),
                Positioned(
                  top: 64,
                  right: 16,
                  child: AnimatedBuilder(
                    animation: _sceneController,
                    builder: (context, child) => FloorViewResetButton(
                      enabled: _sceneController.canReset,
                      onPressed: _sceneController.reset,
                    ),
                  ),
                ),
              ],
              if (showConversationRail)
                Positioned.fill(
                  child: Align(
                    alignment: Alignment.bottomCenter,
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(16, 16, 16, 18),
                      child: SizedBox(
                        width: railWidth,
                        height: railHeight,
                        child: _ConversationRail(
                          executive: widget.executive,
                          project: widget.project,
                          mission: widget.mission,
                          officeView: widget.officeView,
                          messages: widget.messages,
                          generating: widget.generating,
                          onSend: widget.onSend,
                          onStop: widget.onStop,
                        ),
                      ),
                    ),
                  ),
                )
              else
                Positioned(
                  left: 16,
                  right: 16,
                  bottom: 16,
                  child: _CompactChatHint(
                    executive: widget.executive,
                    project: widget.project,
                    mission: widget.mission,
                  ),
                ),
            ],
          );
        },
      ),
    );
  }

  String get _conversationLabel {
    final contextLabel = widget.mission == null
        ? widget.project.name
        : widget.mission!.title;
    return 'Conversation with ${widget.executive.name} for $contextLabel';
  }
}

class _CompactChatHint extends StatelessWidget {
  const _CompactChatHint({
    required this.executive,
    required this.project,
    required this.mission,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;

  @override
  Widget build(BuildContext context) {
    final shortExecutiveName = executive.name.split(' ').first;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
      decoration: BoxDecoration(
        color: FrankColors.panel.withValues(alpha: 0.92),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: FrankColors.border),
      ),
      child: Text(
        '$shortExecutiveName · ${mission?.title ?? project.name} · Resize to chat',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        textAlign: TextAlign.center,
        style: const TextStyle(color: FrankColors.muted, fontSize: 11),
      ),
    );
  }
}

class _ConversationRail extends StatelessWidget {
  const _ConversationRail({
    required this.executive,
    required this.project,
    required this.mission,
    required this.officeView,
    required this.messages,
    required this.generating,
    required this.onSend,
    required this.onStop,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;
  final bool officeView;
  final List<OfficeMessage> messages;
  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    return Localizations(
      locale: const Locale('en', 'US'),
      delegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        mui.DefaultMaterialLocalizations.delegate,
      ],
      child: mui.Material(
        // A canvas Material absorbs hit tests even with a transparent color.
        // Transparency keeps Flow UI's inherited Material context without
        // turning the whole rail into an interaction shield over the floor.
        type: mui.MaterialType.transparency,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (!officeView && project.missions.isEmpty) ...[
              Align(
                alignment: Alignment.centerLeft,
                child: Semantics(
                  container: true,
                  label: 'No missions yet',
                  child: Text(
                    'No missions yet',
                    style: TextStyle(color: FrankColors.muted, fontSize: 12),
                  ),
                ),
              ),
            ],
            Expanded(
              child: _PassiveConversationLog(
                executive: executive,
                messages: messages,
              ),
            ),
            if (messages.isEmpty) ...[
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8),
                child: FlowSuggestionGroup(
                  layout: FlowSuggestionLayout.column,
                  suggestions: [
                    FlowSuggestion(
                      label: 'Turn a rough idea into a project brief',
                      icon: FrankIcons.editNote,
                      onTap: () => onSend(
                        'Help me turn this rough idea into a project brief.',
                      ),
                    ),
                    FlowSuggestion(
                      label: 'Suggest a team for my next project',
                      icon: FrankIcons.users,
                      onTap: () => onSend(
                        'Suggest the smallest team for my next project.',
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 8),
            ],
            FocusableComposer(
              generating: generating,
              onSend: onSend,
              onStop: onStop,
              executive: executive,
              project: project,
              mission: mission,
            ),
          ],
        ),
      ),
    );
  }
}

/// A transcript that is visually present but never wins pointer hit testing.
///
/// The reversed, non-scrollable list keeps the newest messages at the bottom
/// of the viewport. Once the history is taller than the available space, the
/// older rows remain in state but are clipped above the visible log, just like
/// a compact MMORPG chat window.
class _PassiveConversationLog extends StatefulWidget {
  const _PassiveConversationLog({
    required this.executive,
    required this.messages,
  });

  final OfficeEmployee executive;
  final List<OfficeMessage> messages;

  @override
  State<_PassiveConversationLog> createState() =>
      _PassiveConversationLogState();
}

class _PassiveConversationLogState extends State<_PassiveConversationLog> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final hasMessages = widget.messages.isNotEmpty;
    return MouseRegion(
      opaque: false,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: Padding(
        // Leave a small breathing rail at either side; the lower edge stays
        // flush and square so the transcript can sit on the composer cleanly.
        padding: const EdgeInsets.symmetric(horizontal: 18),
        child: ClipRRect(
          key: const ValueKey('passive-chat-transcript-clip'),
          borderRadius: const BorderRadius.only(
            topLeft: Radius.circular(14),
            topRight: Radius.circular(14),
          ),
          child: IgnorePointer(
            key: const ValueKey('passive-chat-transcript'),
            child: AnimatedContainer(
              key: const ValueKey('passive-chat-transcript-background'),
              duration: const Duration(milliseconds: 120),
              curve: Curves.easeOut,
              decoration: BoxDecoration(
                color: hasMessages && _hovered
                    ? Colors.black.withValues(alpha: .07)
                    : Colors.transparent,
              ),
              child: ListView.builder(
                key: const ValueKey('chat-transcript-list'),
                reverse: true,
                primary: false,
                physics: const NeverScrollableScrollPhysics(),
                padding: const EdgeInsets.fromLTRB(8, 8, 8, 8),
                itemCount: widget.messages.length,
                itemBuilder: (context, index) {
                  // A reversed list starts with the newest message at the bottom.
                  final message =
                      widget.messages[widget.messages.length - 1 - index];
                  return Padding(
                    key: ValueKey('chat-log-message-${message.id}'),
                    padding: EdgeInsets.only(
                      top: index == widget.messages.length - 1 ? 0 : 12,
                    ),
                    child: _ChatLogMessage(
                      executive: widget.executive,
                      message: message,
                    ),
                  );
                },
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ChatLogMessage extends StatelessWidget {
  const _ChatLogMessage({required this.executive, required this.message});

  final OfficeEmployee executive;
  final OfficeMessage message;

  @override
  Widget build(BuildContext context) {
    final assistant = message.role == ChatRole.assistant;
    final speaker = assistant ? executive.name.split(' ').first : 'You';
    final nameColor = assistant ? Color(executive.color) : FrankColors.muted;
    final bodyColor = switch (message.status) {
      OfficeMessageStatus.error => Theme.of(context).colorScheme.error,
      OfficeMessageStatus.stopped => FrankColors.muted,
      _ => FrankColors.ink,
    };
    final isThinking =
        message.status == OfficeMessageStatus.pending &&
        message.text.trim().isEmpty;

    return Semantics(
      container: true,
      label: '$speaker: ${isThinking ? 'is thinking…' : message.text}',
      child: Row(
        key: ValueKey('chat-log-row-${message.id}'),
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '$speaker:',
            style: TextStyle(
              color: nameColor,
              fontSize: 12,
              fontWeight: FontWeight.w700,
              height: 1.45,
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: isThinking
                ? const Text(
                    'is thinking…',
                    style: TextStyle(
                      color: FrankColors.muted,
                      fontSize: 13,
                      height: 1.45,
                    ),
                  )
                : message.text.isEmpty
                ? const SizedBox.shrink()
                : FlowMarkdown(
                    text: message.text,
                    isStreaming:
                        message.status == OfficeMessageStatus.streaming,
                    style: TextStyle(
                      color: bodyColor,
                      fontSize: 13,
                      height: 1.45,
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}
