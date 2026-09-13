part of 'package:frank_desktop/features/shell/main_sidebar.dart';

class _SidebarHeader extends StatelessWidget {
  const _SidebarHeader({
    required this.dense,
    required this.isFullscreen,
    required this.workspaceName,
    required this.activeView,
    required this.showDemoBanner,
    required this.connectedServer,
    required this.onSelectView,
  });

  final bool dense;
  final bool isFullscreen;
  final String workspaceName;
  final WorkspaceView activeView;
  final bool showDemoBanner;
  final String? connectedServer;
  final ValueChanged<WorkspaceView> onSelectView;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.fromLTRB(
        dense ? 8 : 12,
        dense ? 34 : 38,
        10,
        dense ? 6 : 8,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const FrankLogo(size: 32),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text(
                      'FRANK',
                      style: TextStyle(
                        color: FrankColors.ink,
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        letterSpacing: 1.1,
                      ),
                    ),
                    Text(
                      workspaceName,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 11,
                      ),
                    ),
                    if (connectedServer case final server?)
                      Tooltip(
                        message: server,
                        child: Semantics(
                          container: true,
                          label: 'Server identity',
                          value: server,
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              const Icon(
                                Icons.lock_outline,
                                size: 11,
                                color: FrankColors.green,
                              ),
                              const SizedBox(width: 4),
                              const Text(
                                'Secure server',
                                style: TextStyle(
                                  color: FrankColors.muted,
                                  fontSize: 9,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
          SizedBox(height: dense ? 12 : 16),
          _WorkspaceViewToggle(selected: activeView, onSelected: onSelectView),
          if (showDemoBanner) ...[
            SizedBox(height: dense ? 8 : 10),
            const _DemoDataBanner(),
          ],
        ],
      ),
    );
  }
}

/// The broad Office/Settings switch remains a useful quick jump even though
/// the sidebar also exposes every destination in one global navigation map.
/// Its value intentionally follows the internal [WorkspaceView] routing model
/// so existing deep links and keyboard contracts keep the same semantics.
class _WorkspaceViewToggle extends StatelessWidget {
  const _WorkspaceViewToggle({
    required this.selected,
    required this.onSelected,
  });

  final WorkspaceView selected;
  final ValueChanged<WorkspaceView> onSelected;

  @override
  Widget build(BuildContext context) {
    final disableAnimations =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    final selectedIndex = selected == WorkspaceView.settings ? 1 : 0;
    final duration = disableAnimations
        ? Duration.zero
        : FrankUiTokens.motionStandard;

    return Semantics(
      container: true,
      label: 'Workspace view',
      value: _viewLabel(selected),
      child: Container(
        key: const ValueKey('workspace-view-toggle'),
        height: FrankUiTokens.controlHeight,
        decoration: BoxDecoration(
          color: FrankColors.canvas.withValues(alpha: 0.5),
          borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
          border: Border.all(color: FrankColors.border),
        ),
        child: Stack(
          fit: StackFit.expand,
          children: [
            AnimatedAlign(
              alignment: Alignment(selectedIndex == 0 ? -1 : 1, 0),
              duration: duration,
              curve: Curves.easeOutCubic,
              child: IgnorePointer(
                child: FractionallySizedBox(
                  widthFactor: 0.5,
                  heightFactor: 1,
                  child: Padding(
                    padding: const EdgeInsets.all(2),
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: FrankColors.aubergineSelection,
                        borderRadius: BorderRadius.circular(
                          FrankUiTokens.controlRadius - 1,
                        ),
                        border: Border.all(
                          color: FrankColors.accent.withValues(alpha: 0.35),
                          width: 0.8,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
            Row(
              children: [
                Expanded(
                  child: _ToggleSegment(
                    view: WorkspaceView.office,
                    selected: selected == WorkspaceView.office,
                    onPressed: () => onSelected(WorkspaceView.office),
                    onMove: () => onSelected(WorkspaceView.settings),
                    disableAnimations: disableAnimations,
                  ),
                ),
                Expanded(
                  child: _ToggleSegment(
                    view: WorkspaceView.settings,
                    selected: selected == WorkspaceView.settings,
                    onPressed: () => onSelected(WorkspaceView.settings),
                    onMove: () => onSelected(WorkspaceView.office),
                    disableAnimations: disableAnimations,
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  String _viewLabel(WorkspaceView view) => switch (view) {
    WorkspaceView.office => 'Office',
    WorkspaceView.settings => 'Settings',
  };
}

class _ToggleSegment extends StatefulWidget {
  const _ToggleSegment({
    required this.view,
    required this.selected,
    required this.onPressed,
    required this.onMove,
    required this.disableAnimations,
  });

  final WorkspaceView view;
  final bool selected;
  final VoidCallback onPressed;
  final VoidCallback onMove;
  final bool disableAnimations;

  @override
  State<_ToggleSegment> createState() => _ToggleSegmentState();
}

class _ToggleSegmentState extends State<_ToggleSegment> {
  late final FocusNode _focusNode;
  bool _hovered = false;

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'workspace-${widget.view.name}-toggle');
  }

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final label = widget.view == WorkspaceView.office ? 'Office' : 'Settings';
    final icon = widget.view == WorkspaceView.office
        ? FrankIcons.office
        : FrankIcons.settings;

    return Focus(
      focusNode: _focusNode,
      onKeyEvent: _handleKeyEvent,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: Tooltip(
          message: widget.view == WorkspaceView.office
              ? 'Open Office to work with projects and missions'
              : 'Open Settings to configure the workspace',
          child: Semantics(
            button: true,
            toggled: widget.selected,
            label: '$label view',
            excludeSemantics: true,
            child: Material(
              color: Colors.transparent,
              child: InkWell(
                onTap: _activate,
                splashColor: Colors.transparent,
                highlightColor: Colors.transparent,
                hoverColor: Colors.transparent,
                borderRadius: BorderRadius.circular(
                  FrankUiTokens.controlRadius,
                ),
                child: AnimatedContainer(
                  duration: widget.disableAnimations
                      ? Duration.zero
                      : FrankUiTokens.motionFast,
                  margin: const EdgeInsets.all(2),
                  decoration: BoxDecoration(
                    color: _hovered && !widget.selected
                        ? FrankColors.ink.withValues(alpha: 0.04)
                        : Colors.transparent,
                    borderRadius: BorderRadius.circular(
                      FrankUiTokens.controlRadius - 1,
                    ),
                  ),
                  child: Center(
                    child: FittedBox(
                      fit: BoxFit.scaleDown,
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            icon,
                            size: 15,
                            color: widget.selected
                                ? FrankColors.ink
                                : FrankColors.muted,
                          ),
                          const SizedBox(width: 6),
                          Text(
                            label,
                            style: TextStyle(
                              color: widget.selected
                                  ? FrankColors.ink
                                  : FrankColors.muted,
                              fontSize: 11.5,
                              fontWeight: widget.selected
                                  ? FontWeight.w500
                                  : FontWeight.w400,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _activate() {
    _focusNode.requestFocus();
    widget.onPressed();
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.space) {
      _activate();
      return KeyEventResult.handled;
    }

    final forward = event.logicalKey == LogicalKeyboardKey.arrowRight;
    final backward = event.logicalKey == LogicalKeyboardKey.arrowLeft;
    if (forward || backward) {
      widget.onMove();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }
}

class _DemoDataBanner extends StatelessWidget {
  const _DemoDataBanner();

  @override
  Widget build(BuildContext context) => Container(
    key: const ValueKey('demo-data-banner'),
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
    decoration: BoxDecoration(
      color: FrankColors.warningAmber.withValues(alpha: .09),
      borderRadius: BorderRadius.circular(6),
      border: Border.all(
        color: FrankColors.warningAmber.withValues(alpha: .28),
      ),
    ),
    child: const Text(
      'Demo data — fitur belum terhubung ke server',
      style: TextStyle(color: FrankColors.warningAmber, fontSize: 10),
    ),
  );
}

/// Mode-scoped destination navigation. Workspace pages live under Office;
/// configuration and operational administration live under Settings.
/// Taskboard and Journal keep their existing internal route types, while the
/// sidebar projects them into the Office side of the product.
class _GlobalNavigation extends StatelessWidget {
  const _GlobalNavigation({
    required this.view,
    required this.activeView,
    required this.settingsSection,
    required this.onSelectView,
    required this.onSelectSettingsSection,
  });

  final WorkspaceView view;
  final WorkspaceView activeView;
  final SettingsSection settingsSection;
  final ValueChanged<WorkspaceView> onSelectView;
  final ValueChanged<SettingsSection> onSelectSettingsSection;

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('global-navigation'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: view == WorkspaceView.office
          ? [_workspaceGroup()]
          : [_agencyGroup(), _insightsGroup(), _systemGroup()],
    );
  }

  FSidebarGroup _workspaceGroup() => FSidebarGroup(
    key: const ValueKey('global-navigation-workspace'),
    style: _frankSettingsNavigationStyle,
    label: const Text('Office'),
    children: [
      FSidebarItem(
        key: const ValueKey('global-nav-office'),
        selected: activeView == WorkspaceView.office,
        icon: const Icon(FrankIcons.floor, size: 16),
        label: const Text('Floor'),
        onPress: () => onSelectView(WorkspaceView.office),
      ),
      _sectionItem(
        SettingsSection.taskboard,
        icon: FrankIcons.taskboard,
        label: 'Taskboard',
      ),
      _sectionItem(
        SettingsSection.journal,
        icon: FrankIcons.journal,
        label: 'Journal',
      ),
    ],
  );

  FSidebarGroup _agencyGroup() => FSidebarGroup(
    key: const ValueKey('global-navigation-agency'),
    style: _frankSettingsNavigationStyle,
    label: const Text('Agency'),
    children: [
      _sectionItem(SettingsSection.team, icon: FrankIcons.users, label: 'Team'),
      _sectionItem(
        SettingsSection.organization,
        icon: FrankIcons.gitBranch,
        label: 'Organization',
      ),
    ],
  );

  FSidebarGroup _insightsGroup() => FSidebarGroup(
    key: const ValueKey('global-navigation-insights'),
    style: _frankSettingsNavigationStyle,
    label: const Text('Insights'),
    children: [
      _sectionItem(
        SettingsSection.ledger,
        icon: FrankIcons.ledger,
        label: 'Ledger',
      ),
    ],
  );

  FSidebarGroup _systemGroup() => FSidebarGroup(
    key: const ValueKey('global-navigation-system'),
    style: _frankSettingsNavigationStyle,
    label: const Text('System'),
    children: [
      _sectionItem(
        SettingsSection.models,
        icon: FrankIcons.cloud,
        label: 'Models & OpenRouter',
      ),
    ],
  );

  FSidebarItem _sectionItem(
    SettingsSection section, {
    required IconData icon,
    required String label,
  }) => FSidebarItem(
    key: ValueKey('settings-section-${section.name}'),
    // Taskboard and Journal are projected into the Office group, but their
    // internal routes remain SettingsDestination(section). Checking the
    // internal view keeps a stale section from appearing selected after the
    // user returns to the Floor.
    selected:
        activeView == WorkspaceView.settings && settingsSection == section,
    icon: Icon(icon, size: 16),
    label: Text(label),
    onPress: () => onSelectSettingsSection(section),
  );
}
