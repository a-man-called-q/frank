import 'package:flutter/material.dart';
import 'package:flow_ui/flow_ui.dart';
import 'package:material_ui/material_ui.dart' as mui;

import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';

class AccountExecutiveChat extends StatelessWidget {
  const AccountExecutiveChat({
    required this.executive,
    required this.project,
    required this.messages,
    required this.generating,
    required this.onSend,
    required this.onStop,
    super.key,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final List<FlowMessageData> messages;
  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: 'Conversation with ${executive.name}, ${executive.role}',
      child: Column(
        children: [
          _ConversationHeader(executive: executive, project: project),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 22),
              child: Localizations(
                locale: const Locale('en', 'US'),
                delegates: const [
                  DefaultWidgetsLocalizations.delegate,
                  mui.DefaultMaterialLocalizations.delegate,
                ],
                child: mui.Material(
                  color: Colors.transparent,
                  child: FlowChatView(
                  empty: messages.isEmpty,
                  greeting: FlowGreeting(
                    icon: Icons.handshake_outlined,
                    text: 'How can the office help?',
                  ),
                  suggestions: FlowSuggestionGroup(
                    layout: FlowSuggestionLayout.column,
                    suggestions: [
                      FlowSuggestion(
                        label: 'Turn a rough idea into a project brief',
                        icon: Icons.edit_note_outlined,
                        onTap: () => onSend(
                          'Help me turn this rough idea into a project brief.',
                        ),
                      ),
                      FlowSuggestion(
                        label: 'Suggest a team for my next project',
                        icon: Icons.groups_outlined,
                        onTap: () => onSend(
                          'Suggest the smallest team for my next project.',
                        ),
                      ),
                    ],
                  ),
                  thread: FlowThread(
                    messages: messages,
                    thinkingLabel: 'Maya is thinking…',
                  ),
                  composer: FlowComposer(
                    placeholder: 'Brief Maya about what your company needs…',
                    isStreaming: generating,
                    onSend: onSend,
                    onStop: onStop,
                  ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ConversationHeader extends StatelessWidget {
  const _ConversationHeader({required this.executive, required this.project});

  final OfficeEmployee executive;
  final OfficeProject project;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 14),
      child: Row(
        children: [
          CircleAvatar(
            radius: 21,
            backgroundColor: Color(executive.color),
            child: Text(
              executive.initials,
              style: const TextStyle(
                color: Color(0xFF17191C),
                fontSize: 12,
                fontWeight: FontWeight.w800,
              ),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  executive.name,
                  style: const TextStyle(
                    fontSize: 14,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                Text(
                  '${executive.role} · ${project.client}',
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: colors.onSurfaceVariant,
                    fontSize: 12,
                  ),
                ),
              ],
            ),
          ),
          const _OnlineDot(),
          const SizedBox(width: 7),
          Text(
            executive.status,
            style: const TextStyle(color: FrankColors.green, fontSize: 11),
          ),
        ],
      ),
    );
  }
}

class _OnlineDot extends StatelessWidget {
  const _OnlineDot();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 8,
      height: 8,
      decoration: const BoxDecoration(
        color: FrankColors.green,
        shape: BoxShape.circle,
      ),
    );
  }
}
