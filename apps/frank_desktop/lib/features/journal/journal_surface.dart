import 'package:flutter/material.dart';

import '../../app/icons.dart';
import '../../app/layout/office_surface_frame.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';

/// Journal is intentionally read-only until the daemon exposes its event
/// projection to the desktop gateway. The empty treatment explains the
/// boundary and keeps the future filter affordance discoverable without
/// presenting a non-functional retry or create action.
class JournalSurface extends StatelessWidget {
  const JournalSurface({super.key});

  @override
  Widget build(BuildContext context) {
    return OfficeSurfaceFrame.page(
      key: const ValueKey('journal-page-frame'),
      scrollKey: const ValueKey('settings-journal-scroll'),
      fullWidth: true,
      header: const OfficePageHeader(
        title: 'Journal',
        description:
            'Review operational events globally or by agent and project.',
      ),
      slivers: const [SliverToBoxAdapter(child: JournalEmptyState())],
    );
  }
}

class JournalEmptyState extends StatelessWidget {
  const JournalEmptyState({super.key});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const SizedBox(height: 18),
        FrankInlineNotice(
          icon: FrankIcons.activity,
          message:
              'The event timeline will appear here once the server emits journal events.',
          tone: FrankStatusTone.neutral,
        ),
        const SizedBox(height: 14),
        FrankPanel(
          key: const ValueKey('journal-empty-panel'),
          child: FrankEmptyState(
            title: 'No events yet',
            description:
                'Frank keeps operational history here so handoffs and decisions can be reviewed later.',
            icon: FrankIcons.activity,
          ),
        ),
        const SizedBox(height: 14),
        Semantics(
          container: true,
          label: 'Journal filters unavailable until events arrive',
          child: Opacity(
            opacity: .55,
            child: IgnorePointer(
              child: OutlinedButton.icon(
                onPressed: null,
                icon: const Icon(
                  FrankIcons.filter,
                  size: FrankUiTokens.iconSize,
                ),
                label: const Text('Filter events'),
              ),
            ),
          ),
        ),
      ],
    );
  }
}
