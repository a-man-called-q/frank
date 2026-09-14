import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import '../../../app/icons.dart';
import '../../../app/theme.dart';
import '../../../core/models/workspace_models.dart';
import '../../../core/models/connection_models.dart';
import '../sidebar_layout.dart';

String _contextSubtitle(OfficeMission? mission, OfficeProject? project) {
  if (mission == null) {
    if (project == null) return 'Workspace';
    return project.client;
  }
  return project?.name ?? 'Mission';
}

double _attentionReserved({required bool attention, required bool showLabel}) {
  if (!attention) return 0;
  return showLabel ? 140 : 40;
}

class ShellContextBar extends StatelessWidget {
  static const height = 32.0;
  static const _toggleHitboxSize = 32.0;
  static const _glyphSize = 18.0;
  static const _logoSize = 18.0;
  static const _collapsedVisualGap = 8.0;
  static const _openTitleGap = 14.0;

  // The toggle is a 32px hitbox but its visible glyph is 18px centered inside
  // it. Position the logo from the glyph's painted right edge so the optical
  // gap matches the logo-to-title gap below.
  static const _collapsedLogoOffset =
      (_toggleHitboxSize - _glyphSize) / 2 + _glyphSize + _collapsedVisualGap;
  static const _collapsedTitleOffset =
      _collapsedLogoOffset + _logoSize + _collapsedVisualGap;

  const ShellContextBar({
    required this.workspace,
    required this.project,
    required this.mission,
    required this.sidebarVisible,
    this.sidebarWidth = SidebarLayout.defaultWidth,
    required this.isFullscreen,
    required this.onToggleSidebar,
    this.onDoubleTap,
    this.connectionStatus,
    this.isFixture = false,
    super.key,
  });

  final OfficeWorkspace workspace;
  final OfficeProject? project;
  final OfficeMission? mission;
  final bool sidebarVisible;
  final double sidebarWidth;
  final bool isFullscreen;
  final VoidCallback onToggleSidebar;
  final VoidCallback? onDoubleTap;
  final FrankConnectionStatus? connectionStatus;
  final bool isFixture;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final platform = defaultTargetPlatform;
        final usesWindowedMacChrome =
            platform == TargetPlatform.macOS && !isFullscreen;
        final inset = usesWindowedMacChrome ? 76.0 : 12.0;
        final showSubtitle = constraints.maxWidth >= 620;
        final showAttentionLabel = constraints.maxWidth >= 760;
        final showConnectionLabel = constraints.maxWidth >= 520;
        final connection = connectionStatus;
        final connectionLabel = isFixture
            ? 'Demo'
            : connection?.label ?? FrankConnectionPhase.checking.label;
        final connectionColor = isFixture
            ? FrankColors.aubergineAccent
            : switch (connection?.phase) {
                FrankConnectionPhase.reconnecting => FrankColors.warningAmber,
                FrankConnectionPhase.offline ||
                FrankConnectionPhase.incompatible => FrankColors.failure,
                FrankConnectionPhase.checking => FrankColors.warningAmber,
                FrankConnectionPhase.connected => FrankColors.green,
                null => FrankColors.warningAmber,
              };
        final connectionIcon = isFixture
            ? FrankIcons.infoOutline
            : switch (connection?.phase) {
                FrankConnectionPhase.reconnecting => FrankIcons.refresh,
                FrankConnectionPhase.offline ||
                FrankConnectionPhase.incompatible => FrankIcons.cloudOffOutlined,
                FrankConnectionPhase.checking => FrankIcons.hourglassEmpty,
                FrankConnectionPhase.connected => FrankIcons.circleCheck,
                null => FrankIcons.hourglassEmpty,
              };
        final contextTitle = mission?.title ?? project?.name ?? workspace.name;
        final contextSubtitle = _contextSubtitle(mission, project);
        final attention =
            project?.missions.any((candidate) => candidate.needsAttention) ??
            false;
        final rightReserved =
            _attentionReserved(
              attention: attention,
              showLabel: showAttentionLabel,
            ) +
            (showConnectionLabel ? 120.0 : 40.0) +
            24.0;

        return SizedBox(
          height: height,
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
              // Bottom border — only from sidebar right edge to window right
              AnimatedPositioned(
                duration: SidebarLayout.animationDuration,
                curve: SidebarLayout.animationCurve,
                left: sidebarVisible ? sidebarWidth : 0,
                right: 0,
                bottom: 0,
                height: 1,
                child: const ColoredBox(color: Color(0x55363940)),
              ),
              // Fixed Toggle Button (Always at x = inset, never vanishes or jumps)
              Positioned(
                left: inset,
                top: 0,
                bottom: 0,
                child: Center(
                  child: SizedBox(
                    width: _toggleHitboxSize,
                    height: _toggleHitboxSize,
                    child: FButton.icon(
                      onPress: onToggleSidebar,
                      semanticsLabel: sidebarVisible
                          ? 'Hide the workspace sidebar'
                          : 'Show the workspace sidebar',
                      semanticsTooltip: sidebarVisible
                          ? 'Hide the workspace sidebar'
                          : 'Show the workspace sidebar',
                      size: FButtonSizeVariant.sm,
                      child: Icon(
                        sidebarVisible
                            ? FrankIcons.panelClose
                            : FrankIcons.panelOpen,
                      ),
                    ),
                  ),
                ),
              ),
              // Mini Frank Logo (fades in next to toggle when sidebar collapses)
              Positioned(
                left: inset + _collapsedLogoOffset,
                top: 0,
                bottom: 0,
                child: Center(
                  child: AnimatedOpacity(
                    opacity: sidebarVisible ? 0.0 : 1.0,
                    duration: const Duration(milliseconds: 150),
                    child: Semantics(
                      image: true,
                      label: 'Frank',
                      child: ClipRRect(
                        borderRadius: const BorderRadius.all(
                          Radius.circular(4),
                        ),
                        child: Image.asset(
                          'assets/branding/frank-logo.png',
                          width: _logoSize,
                          height: _logoSize,
                          fit: BoxFit.cover,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              // Sliding Context Information (Title & Subtitle)
              AnimatedPositioned(
                duration: SidebarLayout.animationDuration,
                curve: SidebarLayout.animationCurve,
                left: sidebarVisible
                    ? (sidebarWidth + _openTitleGap)
                    : (inset + _collapsedTitleOffset),
                right: rightReserved,
                top: 0,
                bottom: 0,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
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
              ),
              // Fixed Right Status Badges
              Positioned(
                right: 14,
                top: 0,
                bottom: 0,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
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
                              color: FrankColors.warningAmber,
                            ),
                            if (showAttentionLabel) ...[
                              const SizedBox(width: 5),
                              const Text(
                                'Needs attention',
                                style: TextStyle(
                                  color: FrankColors.warningAmber,
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
                      value: connectionLabel,
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            connectionIcon,
                            size: 15,
                            color: connectionColor,
                            semanticLabel: connectionLabel,
                          ),
                          if (showConnectionLabel) ...[
                            const SizedBox(width: 5),
                            Text(
                              connectionLabel,
                              style: TextStyle(
                                color: connectionColor,
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
            ],
          ),
        );
      },
    );
  }
}
