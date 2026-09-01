import 'package:flutter/material.dart';

import '../../../app/icons.dart';
import '../../../app/theme.dart';
import '../../../core/models/workspace_models.dart';

class ShellContextBar extends StatelessWidget {
  static const height = 32.0;

  const ShellContextBar({
    required this.workspace,
    required this.project,
    required this.mission,
    required this.sidebarVisible,
    required this.isFullscreen,
    required this.onToggleSidebar,
    this.onDoubleTap,
    super.key,
  });

  final OfficeWorkspace workspace;
  final OfficeProject? project;
  final OfficeMission? mission;
  final bool sidebarVisible;
  final bool isFullscreen;
  final VoidCallback onToggleSidebar;
  final VoidCallback? onDoubleTap;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final platform = Theme.of(context).platform;
        final usesWindowedMacChrome =
            platform == TargetPlatform.macOS && !isFullscreen;
        final inset = usesWindowedMacChrome && !sidebarVisible ? 76.0 : 12.0;
        final showSubtitle = constraints.maxWidth >= 620;
        final showAttentionLabel = constraints.maxWidth >= 760;
        final showConnectionLabel = constraints.maxWidth >= 520;
        final contextTitle = mission?.title ?? project?.name ?? workspace.name;
        final contextSubtitle = mission == null
            ? (project == null ? 'Workspace' : project!.client)
            : project?.name ?? 'Mission';
        final attention =
            project?.missions.any((candidate) => candidate.needsAttention) ??
            false;

        return Container(
          height: height,
          decoration: const BoxDecoration(
            color: FrankColors.panel,
            border: Border(bottom: BorderSide(color: FrankColors.border)),
          ),
          child: Stack(
            fit: StackFit.expand,
            children: [
              if (onDoubleTap != null)
                Positioned.fill(
                  child: GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onDoubleTap: onDoubleTap,
                  ),
                ),
              Align(
                // Native macOS traffic lights sit on a 16 px centerline. A
                // 32 px bar keeps the custom content on that same line without
                // leaving an extra row of empty titlebar space below it.
                alignment: Alignment.topCenter,
                child: SizedBox(
                  height: height,
                  child: Padding(
                    padding: EdgeInsets.only(left: inset, right: 14),
                    child: Row(
                      children: [
                        SizedBox(
                          width: 32,
                          height: 32,
                          child: IconButton(
                            onPressed: onToggleSidebar,
                            tooltip: sidebarVisible
                                ? 'Hide sidebar'
                                : 'Show sidebar',
                            icon: Icon(
                              sidebarVisible
                                  ? FrankIcons.panelClose
                                  : FrankIcons.panelOpen,
                            ),
                            iconSize: 18,
                            color: FrankColors.muted,
                            padding: EdgeInsets.zero,
                            constraints: const BoxConstraints.tightFor(
                              width: 32,
                              height: 32,
                            ),
                          ),
                        ),
                        if (!sidebarVisible) ...[
                          const SizedBox(width: 8),
                          Semantics(
                            image: true,
                            label: 'Frank',
                            child: ClipRRect(
                              borderRadius: BorderRadius.all(
                                Radius.circular(4),
                              ),
                              child: Image.asset(
                                'assets/branding/frank-logo.png',
                                width: 18,
                                height: 18,
                                fit: BoxFit.cover,
                              ),
                            ),
                          ),
                        ],
                        const SizedBox(width: 8),
                        Expanded(
                          child: Row(
                            children: [
                              Flexible(
                                child: Text(
                                  contextTitle,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                    color: FrankColors.ink,
                                    fontSize: 12,
                                  ),
                                ),
                              ),
                              if (showSubtitle) ...[
                                const SizedBox(width: 8),
                                Flexible(
                                  child: Text(
                                    contextSubtitle,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: const TextStyle(
                                      color: FrankColors.muted,
                                      fontSize: 11,
                                    ),
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),
                        if (attention) ...[
                          const SizedBox(width: 8),
                          Semantics(
                            container: true,
                            label: 'Needs attention',
                            child: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                const Icon(
                                  FrankIcons.circleAlert,
                                  size: 15,
                                  color: FrankColors.amber,
                                ),
                                if (showAttentionLabel) ...[
                                  const SizedBox(width: 5),
                                  const Text(
                                    'Needs attention',
                                    style: TextStyle(
                                      color: FrankColors.amber,
                                      fontSize: 11,
                                    ),
                                  ),
                                ],
                              ],
                            ),
                          ),
                        ],
                        const SizedBox(width: 12),
                        Semantics(
                          container: true,
                          label: 'Connection status',
                          value: 'Connected',
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              const Icon(
                                FrankIcons.circleCheck,
                                size: 15,
                                color: FrankColors.green,
                              ),
                              if (showConnectionLabel) ...[
                                const SizedBox(width: 5),
                                const Text(
                                  'Connected',
                                  style: TextStyle(
                                    color: FrankColors.muted,
                                    fontSize: 11,
                                  ),
                                ),
                              ],
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}
