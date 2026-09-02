import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flow_ui/flow_ui.dart';
import 'package:material_ui/material_ui.dart' as mui;

import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';
import '../floor/office_scene_floor.dart';
import 'presentation/focusable_composer.dart';
import 'presentation/flow_message_mapper.dart';

class AccountExecutiveChat extends StatelessWidget {
  const AccountExecutiveChat({
    required this.executive,
    required this.project,
    required this.mission,
    required this.officeView,
    required this.messages,
    required this.generating,
    required this.onSend,
    required this.onStop,
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

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
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
            480.0,
            math.max(170.0, availableHeight * 0.44),
          );
          final showConversationRail =
              availableWidth >= 360 && availableHeight >= 340;

          return Stack(
            fit: StackFit.expand,
            children: [
              const Positioned.fill(child: OfficeSceneFloor()),
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
                          executive: executive,
                          project: project,
                          mission: mission,
                          officeView: officeView,
                          messages: messages,
                          generating: generating,
                          onSend: onSend,
                          onStop: onStop,
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
                    executive: executive,
                    project: project,
                    mission: mission,
                  ),
                ),
            ],
          );
        },
      ),
    );
  }

  String get _conversationLabel {
    final contextLabel = mission == null ? project.name : mission!.title;
    return 'Conversation with ${executive.name} for $contextLabel';
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
    final flowMessages = messages.map(toFlowMessage).toList(growable: false);
    return Localizations(
      locale: const Locale('en', 'US'),
      delegates: const [
        DefaultWidgetsLocalizations.delegate,
        mui.DefaultMaterialLocalizations.delegate,
      ],
      child: mui.Material(
        color: Colors.transparent,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Align(
              alignment: Alignment.centerLeft,
              child: _ContextChip(
                executive: executive,
                project: project,
                mission: mission,
                officeView: officeView,
              ),
            ),
            if (!officeView && project.missions.isEmpty) ...[
              const SizedBox(height: 10),
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
            const SizedBox(height: 8),
            Expanded(
              child: FlowChatView(
                // The rail owns the empty state so the floor is never
                // replaced by FlowChatView's full-surface zero state.
                empty: false,
                thread: FlowThread(
                  messages: flowMessages,
                  padding: const EdgeInsets.fromLTRB(8, 8, 8, 8),
                  itemSpacing: 12,
                  messageBuilder: (context, message) =>
                      _FloatingMessage(executive: executive, message: message),
                  thinkingLabel: 'Maya is thinking…',
                ),
                aboveComposer: messages.isEmpty
                    ? FlowSuggestionGroup(
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
                      )
                    : null,
                composer: FocusableComposer(
                  generating: generating,
                  onSend: onSend,
                  onStop: onStop,
                ),
                maxContentWidth: 760,
                padding: EdgeInsets.zero,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ContextChip extends StatelessWidget {
  const _ContextChip({
    required this.executive,
    required this.project,
    required this.mission,
    required this.officeView,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;
  final bool officeView;

  @override
  Widget build(BuildContext context) {
    final shortExecutiveName = executive.name.split(' ').first;
    final title = officeView || mission == null
        ? '$shortExecutiveName · ${project.name}'
        : '${project.name} · ${mission!.title} · $shortExecutiveName';
    return Container(
      constraints: const BoxConstraints(maxWidth: 560),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
      decoration: BoxDecoration(
        color: FrankColors.panel.withValues(alpha: 0.92),
        borderRadius: BorderRadius.circular(99),
        border: Border.all(color: FrankColors.border),
        boxShadow: const [
          BoxShadow(color: Colors.black38, blurRadius: 14, spreadRadius: -5),
        ],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          CircleAvatar(
            radius: 10,
            backgroundColor: Color(executive.color),
            child: Text(
              executive.initials,
              style: const TextStyle(color: Color(0xFF17191C), fontSize: 7),
            ),
          ),
          const SizedBox(width: 7),
          Flexible(
            child: Text(
              title,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(color: FrankColors.ink, fontSize: 11),
            ),
          ),
          const SizedBox(width: 7),
          const Icon(FrankIcons.circle, size: 6, color: FrankColors.green),
        ],
      ),
    );
  }
}

class _FloatingMessage extends StatelessWidget {
  const _FloatingMessage({required this.executive, required this.message});

  final OfficeEmployee executive;
  final FlowMessageData message;

  @override
  Widget build(BuildContext context) {
    final assistant = message.role == FlowMessageRole.assistant;
    final content = FlowMessage(
      message,
      markdown: true,
      maxBubbleWidthFraction: 1,
      bubbleRadius: BorderRadius.circular(12),
      bubblePadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
      thinkingLabel: 'Maya is thinking…',
      leading: assistant
          ? CircleAvatar(
              radius: 13,
              backgroundColor: Color(executive.color),
              child: Text(
                executive.initials,
                style: const TextStyle(color: Color(0xFF17191C), fontSize: 8),
              ),
            )
          : null,
    );

    final bubble = assistant
        ? Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
            decoration: BoxDecoration(
              color: FrankColors.panel.withValues(alpha: 0.92),
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: FrankColors.border),
              boxShadow: const [
                BoxShadow(
                  color: Colors.black38,
                  blurRadius: 14,
                  spreadRadius: -5,
                ),
              ],
            ),
            child: content,
          )
        : content;

    return Align(
      alignment: assistant ? Alignment.centerLeft : Alignment.centerRight,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 600),
        child: bubble,
      ),
    );
  }
}
