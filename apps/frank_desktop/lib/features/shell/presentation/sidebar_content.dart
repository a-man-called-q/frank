part of 'package:frank_desktop/features/shell/main_sidebar.dart';

class MainSidebarContent extends StatelessWidget {
  static const fixedWidth = SidebarLayout.defaultWidth;

  const MainSidebarContent({
    this.width = SidebarLayout.defaultWidth,
    this.nativeSidebarEffect = false,
    required this.isFullscreen,
    required this.workspace,
    required this.workspaceName,
    required this.activeView,
    required this.officeSection,
    required this.settingsSection,
    required this.projects,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.expandedProjectIds,
    required this.projectScope,
    required this.pinnedMissionIds,
    required this.searchFocusNode,
    this.onToggleSidebar,
    this.onDoubleTap,
    required this.onSelectView,
    required this.onSelectOfficeSection,
    required this.onOpenSettings,
    required this.onSelectSettings,
    required this.onToggleProject,
    required this.onSelectMission,
    required this.onCreateMission,
    required this.onPinProject,
    required this.onRenameProject,
    required this.onArchiveProject,
    required this.onRemoveProject,
    required this.onPinMission,
    required this.onRenameMission,
    required this.onArchiveMission,
    required this.onSelectProjectScope,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    super.key,
  });

  final double width;
  final bool nativeSidebarEffect;
  final OfficeWorkspace workspace;
  final bool isFullscreen;
  final String workspaceName;
  final WorkspaceView? activeView;
  final OfficeSection officeSection;
  final SettingsSection? settingsSection;
  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final Set<String> expandedProjectIds;
  final String? projectScope;
  final List<String> pinnedMissionIds;
  final FocusNode searchFocusNode;
  final VoidCallback? onToggleSidebar;
  final VoidCallback? onDoubleTap;
  final ValueChanged<WorkspaceView> onSelectView;
  final ValueChanged<OfficeSection> onSelectOfficeSection;
  final ValueChanged<SettingsSection> onOpenSettings;
  final ValueChanged<SettingsSection> onSelectSettings;
  final ValueChanged<String> onToggleProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final ValueChanged<String> onCreateMission;
  final ValueChanged<String> onPinProject;
  final ValueChanged<String> onRenameProject;
  final ValueChanged<String> onArchiveProject;
  final ValueChanged<String> onRemoveProject;
  final void Function(String projectId, String missionId) onPinMission;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;
  final ValueChanged<String?> onSelectProjectScope;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;

  @override
  Widget build(BuildContext context) {
    final content = switch (activeView) {
      WorkspaceView.office => <Widget>[
        _OfficeNavigation(
          selected: officeSection,
          onSelect: onSelectOfficeSection,
        ),
      ],
      WorkspaceView.projects => const <Widget>[],
      null => <Widget>[
        _SettingsNavigation(
          selected: settingsSection!,
          onSelect: onSelectSettings,
        ),
      ],
    };

    return LayoutBuilder(
      builder: (context, constraints) {
        final shortHeight =
            constraints.maxHeight.isFinite && constraints.maxHeight < 360;
        // There is no useful room for the inbox below the traffic-light/header
        // chrome. Hiding its scrollable body keeps tiny windows free of
        // a RenderFlex overflow while the shell remains navigable.
        final inbox = activeView == WorkspaceView.projects && !shortHeight
            ? WorkInboxPane(
                workspace: workspace,
                selectedProjectId: selectedProjectId,
                selectedMissionId: selectedMissionId,
                projectScope: projectScope,
                pinnedMissionIds: pinnedMissionIds,
                searchFocusNode: searchFocusNode,
                onProjectScopeChanged: onSelectProjectScope,
                onSelectProject: onSelectProjectScope,
                onSelectMission: onSelectMission,
                onSelectAgent: () => onSelectSettings(SettingsSection.team),
                onTogglePinnedMission: onTogglePinnedMission,
                onReorderPinnedMissions: onReorderPinnedMissions,
                onCreateMission: onCreateMission,
                onRenameMission: onRenameMission,
                onArchiveMission: onArchiveMission,
              )
            : null;
        final sidebar = inbox == null
            ? FSidebar(
                style: _frankSidebarStyle(
                  width,
                  nativeSidebarEffect: nativeSidebarEffect,
                ),
                header: _SidebarHeader(
                  dense: shortHeight,
                  isFullscreen: isFullscreen,
                  workspaceName: workspaceName,
                  activeView: activeView,
                  onSelectView: onSelectView,
                ),
                children: content,
                footer: _SidebarFooter(
                  dense: shortHeight,
                  settingsOpen: settingsSection != null,
                  onOpenSettings: () => onOpenSettings(SettingsSection.team),
                ),
              )
            : FSidebar.raw(
                style: _frankSidebarStyle(
                  width,
                  nativeSidebarEffect: nativeSidebarEffect,
                ),
                header: _SidebarHeader(
                  dense: shortHeight,
                  isFullscreen: isFullscreen,
                  workspaceName: workspaceName,
                  activeView: activeView,
                  onSelectView: onSelectView,
                ),
                child: inbox,
                footer: _SidebarFooter(
                  dense: shortHeight,
                  settingsOpen: settingsSection != null,
                  onOpenSettings: () => onOpenSettings(SettingsSection.team),
                ),
              );

        return SizedBox(width: width, child: sidebar);
      },
    );
  }
}

class _SidebarHeader extends StatelessWidget {
  const _SidebarHeader({
    required this.dense,
    required this.isFullscreen,
    required this.workspaceName,
    required this.activeView,
    required this.onSelectView,
  });

  final bool dense;
  final bool isFullscreen;
  final String workspaceName;
  final WorkspaceView? activeView;
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
                  ],
                ),
              ),
            ],
          ),
          SizedBox(height: dense ? 12 : 16),
          _WorkspaceViewToggle(selected: activeView, onSelected: onSelectView),
        ],
      ),
    );
  }
}

class _WorkspaceViewToggle extends StatelessWidget {
  const _WorkspaceViewToggle({
    required this.selected,
    required this.onSelected,
  });

  final WorkspaceView? selected;
  final ValueChanged<WorkspaceView> onSelected;

  @override
  Widget build(BuildContext context) {
    final selectedIndex = selected == WorkspaceView.projects ? 1 : 0;
    const trackHeight = 32.0;

    return Semantics(
      container: true,
      label: 'Workspace view',
      value: selected == null ? 'Settings' : _viewLabel(selected!),
      child: Container(
        width: double.infinity,
        height: trackHeight,
        decoration: BoxDecoration(
          color: FrankColors.canvas.withValues(alpha: 0.5),
          borderRadius: BorderRadius.circular(8),
          border: Border.all(
            color: FrankColors.border.withValues(alpha: 0.4),
            width: 1,
          ),
        ),
        child: Stack(
          fit: StackFit.expand,
          children: [
            AnimatedAlign(
              alignment: Alignment(selectedIndex == 0 ? -1 : 1, 0),
              duration: const Duration(milliseconds: 160),
              curve: Curves.easeOutCubic,
              child: IgnorePointer(
                child: AnimatedOpacity(
                  opacity: selected == null ? 0 : 1,
                  duration: const Duration(milliseconds: 120),
                  child: FractionallySizedBox(
                    widthFactor: 0.5,
                    heightFactor: 1,
                    child: Padding(
                      padding: const EdgeInsets.all(2),
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          color: FrankColors.panelRaised,
                          borderRadius: BorderRadius.circular(6),
                          border: Border.all(
                            color: Colors.white.withValues(alpha: 0.07),
                            width: 0.8,
                          ),
                          boxShadow: [
                            BoxShadow(
                              color: Colors.black.withValues(alpha: 0.18),
                              blurRadius: 3,
                              offset: const Offset(0, 1),
                            ),
                          ],
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
                    onMove: () => onSelected(WorkspaceView.projects),
                  ),
                ),
                Expanded(
                  child: _ToggleSegment(
                    view: WorkspaceView.projects,
                    selected: selected == WorkspaceView.projects,
                    onPressed: () => onSelected(WorkspaceView.projects),
                    onMove: () => onSelected(WorkspaceView.office),
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
    WorkspaceView.projects => 'Projects',
  };
}

class _ToggleSegment extends StatefulWidget {
  const _ToggleSegment({
    required this.view,
    required this.selected,
    required this.onPressed,
    required this.onMove,
  });

  final WorkspaceView view;
  final bool selected;
  final VoidCallback onPressed;
  final VoidCallback onMove;

  @override
  State<_ToggleSegment> createState() => _ToggleSegmentState();
}

class _ToggleSegmentState extends State<_ToggleSegment> {
  bool _hovered = false;

  @override
  Widget build(BuildContext context) {
    final label = widget.view == WorkspaceView.office ? 'Office' : 'Projects';
    final icon = widget.view == WorkspaceView.office
        ? FrankIcons.dashboard
        : FrankIcons.folder;

    return Focus(
      onKeyEvent: _handleKeyEvent,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovered = true),
        onExit: (_) => setState(() => _hovered = false),
        child: Tooltip(
          message: widget.view == WorkspaceView.office
              ? 'Open Office view to see the workspace floor'
              : 'Open Projects view to browse projects and tasks',
          child: Semantics(
            button: true,
            toggled: widget.selected,
            label: '$label view',
            excludeSemantics: true,
            child: Material(
              color: Colors.transparent,
              child: InkWell(
                onTap: widget.onPressed,
                splashColor: Colors.transparent,
                highlightColor: Colors.transparent,
                hoverColor: Colors.transparent,
                borderRadius: BorderRadius.circular(6),
                child: AnimatedContainer(
                  duration: const Duration(milliseconds: 120),
                  margin: const EdgeInsets.all(2),
                  decoration: BoxDecoration(
                    color: _hovered && !widget.selected
                        ? FrankColors.ink.withValues(alpha: 0.04)
                        : Colors.transparent,
                    borderRadius: BorderRadius.circular(6),
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

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.space) {
      widget.onPressed();
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

class ProjectsTreePane extends StatefulWidget {
  const ProjectsTreePane({
    required this.projects,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.expandedProjectIds,
    required this.onToggleProject,
    required this.onSelectMission,
    required this.onCreateMission,
    required this.onPinProject,
    required this.onRenameProject,
    required this.onArchiveProject,
    required this.onRemoveProject,
    required this.onPinMission,
    required this.onRenameMission,
    required this.onArchiveMission,
    super.key,
  });

  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final Set<String> expandedProjectIds;
  final ValueChanged<String> onToggleProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final ValueChanged<String> onCreateMission;
  final ValueChanged<String> onPinProject;
  final ValueChanged<String> onRenameProject;
  final ValueChanged<String> onArchiveProject;
  final ValueChanged<String> onRemoveProject;
  final void Function(String projectId, String missionId) onPinMission;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;

  @override
  State<ProjectsTreePane> createState() => _ProjectsTreePaneState();
}

class _ProjectsTreePaneState extends State<ProjectsTreePane> {
  late final _TreeInputModality _inputModality;

  @override
  void initState() {
    super.initState();
    _inputModality = _TreeInputModality();
  }

  @override
  void dispose() {
    _inputModality.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: _inputModality,
      builder: (context, _) => Listener(
        behavior: HitTestBehavior.translucent,
        onPointerDown: (_) => _inputModality.pointerDown(),
        child: Focus(
          canRequestFocus: false,
          skipTraversal: true,
          includeSemantics: false,
          onKeyEvent: (_, event) {
            if (event is KeyDownEvent) _inputModality.keyDown();
            return KeyEventResult.ignored;
          },
          child: FSidebarGroup(
            style: FSidebarGroupStyleDelta.delta(
              padding: const EdgeInsetsDelta.value(
                EdgeInsets.symmetric(horizontal: 8),
              ),
              headerPadding: const EdgeInsetsGeometryDelta.value(
                EdgeInsets.symmetric(horizontal: 2),
              ),
              labelStyle: const TextStyleDelta.value(
                TextStyle(
                  fontFamily: FrankTypography.uiFontFamily,
                  fontFamilyFallback: FrankTypography.uiFontFallback,
                  fontSize: 13,
                  fontWeight: FontWeight.w400,
                  height: 20 / 13,
                ),
              ),
              childrenSpacing: 2,
              childrenPadding: const EdgeInsetsGeometryDelta.value(
                EdgeInsets.only(bottom: 12),
              ),
            ),
            label: const Text('Projects'),
            children: [
              for (final project in widget.projects)
                _ProjectTree(
                  project: project,
                  expanded: widget.expandedProjectIds.contains(project.id),
                  containsSelection: project.id == widget.selectedProjectId,
                  selectedMissionId: project.id == widget.selectedProjectId
                      ? widget.selectedMissionId
                      : null,
                  inputModality: _inputModality,
                  onToggle: () => widget.onToggleProject(project.id),
                  onCreateMission: () => widget.onCreateMission(project.id),
                  onSelectMission: (missionId) =>
                      widget.onSelectMission(project.id, missionId),
                  onPin: () => widget.onPinProject(project.id),
                  onRename: () => widget.onRenameProject(project.id),
                  onArchive: () => widget.onArchiveProject(project.id),
                  onRemove: () => widget.onRemoveProject(project.id),
                  onPinMission: (missionId) =>
                      widget.onPinMission(project.id, missionId),
                  onRenameMission: (missionId) =>
                      widget.onRenameMission(project.id, missionId),
                  onArchiveMission: (missionId) =>
                      widget.onArchiveMission(project.id, missionId),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ProjectTree extends StatefulWidget {
  const _ProjectTree({
    required this.project,
    required this.expanded,
    required this.containsSelection,
    required this.selectedMissionId,
    required this.inputModality,
    required this.onToggle,
    required this.onSelectMission,
    required this.onCreateMission,
    required this.onPin,
    required this.onRename,
    required this.onArchive,
    required this.onRemove,
    required this.onPinMission,
    required this.onRenameMission,
    required this.onArchiveMission,
  });

  final OfficeProject project;
  final bool expanded;
  final bool containsSelection;
  final String? selectedMissionId;
  final _TreeInputModality inputModality;
  final VoidCallback onToggle;
  final ValueChanged<String> onSelectMission;
  final VoidCallback onCreateMission;
  final VoidCallback onPin;
  final VoidCallback onRename;
  final VoidCallback onArchive;
  final VoidCallback onRemove;
  final ValueChanged<String> onPinMission;
  final ValueChanged<String> onRenameMission;
  final ValueChanged<String> onArchiveMission;

  @override
  State<_ProjectTree> createState() => _ProjectTreeState();
}

class _ProjectTreeState extends State<_ProjectTree>
    with TickerProviderStateMixin {
  late final FocusNode _focusNode;
  late final FPopoverController _actionsController;
  late final FPopoverController _contextMenuController;
  final Object _actionsMenuOwner = Object();
  final Object _contextMenuOwner = Object();
  bool _hovered = false;
  bool _focused = false;
  bool _actionsOpen = false;
  bool _contextMenuOpen = false;
  bool _showAllMissions = false;

  bool get _projectSelected =>
      widget.containsSelection && widget.selectedMissionId == null;

  bool get _showActions =>
      _projectSelected ||
      _hovered ||
      _focused ||
      _actionsOpen ||
      _contextMenuOpen;

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'Project ${widget.project.name}');
    _actionsController = FPopoverController(vsync: this);
    _contextMenuController = FPopoverController(vsync: this);
    _actionsController.addListener(_handleActionsChanged);
  }

  @override
  void dispose() {
    FrankDesktopMenuDismissScope.release(context, _actionsMenuOwner);
    FrankDesktopMenuDismissScope.release(context, _contextMenuOwner);
    _actionsController.removeListener(_handleActionsChanged);
    _actionsController.dispose();
    _contextMenuController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _handleActionsChanged() {
    final open = _actionsController.status.isForwardOrCompleted;
    if (open) {
      FrankDesktopMenuDismissScope.register(
        context,
        _actionsMenuOwner,
        () => unawaited(_actionsController.hide(animated: false)),
      );
    } else {
      FrankDesktopMenuDismissScope.release(context, _actionsMenuOwner);
    }
    if (open != _actionsOpen && mounted) {
      setState(() => _actionsOpen = open);
    }
  }

  void _handleContextMenuChanged(bool open) {
    if (open) {
      FrankDesktopMenuDismissScope.register(
        context,
        _contextMenuOwner,
        () => unawaited(_contextMenuController.hide(animated: false)),
      );
    } else {
      FrankDesktopMenuDismissScope.release(context, _contextMenuOwner);
    }
    if (open != _contextMenuOpen && mounted) {
      setState(() => _contextMenuOpen = open);
    }
  }

  void _toggleActions() {
    if (_actionsController.status.isForwardOrCompleted) {
      unawaited(_actionsController.hide());
      return;
    }
    FrankDesktopMenuDismissScope.dismissAll(context, restoreFocus: false);
    unawaited(_actionsController.show());
  }

  void _selectProject() {
    widget.onToggle();
    _focusNode.requestFocus();
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    widget.inputModality.keyDown();
    final contextMenuPressed =
        event.logicalKey == LogicalKeyboardKey.contextMenu ||
        (event.logicalKey == LogicalKeyboardKey.f10 &&
            HardwareKeyboard.instance.isShiftPressed);
    if (contextMenuPressed) {
      _toggleActions();
      return KeyEventResult.handled;
    }

    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.space) {
      widget.onToggle();
      return KeyEventResult.handled;
    }

    if (event.logicalKey == LogicalKeyboardKey.arrowRight && !widget.expanded) {
      widget.onToggle();
      return KeyEventResult.handled;
    }

    if (event.logicalKey == LogicalKeyboardKey.arrowLeft && widget.expanded) {
      widget.onToggle();
      return KeyEventResult.handled;
    }

    return KeyEventResult.ignored;
  }

  List<FTileGroupMixin> _actionMenu(FPopoverController controller) {
    VoidCallback invoke(VoidCallback action) => () {
      unawaited(controller.hide());
      action();
    };

    return [
      FTileGroup(
        key: const ValueKey('project-actions-group-primary'),
        divider: FItemDivider.none,
        children: [
          FTile(
            key: const ValueKey('project-action-pin'),
            title: _frankMenuTitle('Pin'),
            prefix: const Icon(FrankIcons.pin, size: 16),
            semanticsLabel: 'Pin project',
            onPress: invoke(widget.onPin),
          ),
          FTile(
            key: const ValueKey('project-action-rename'),
            title: _frankMenuTitle('Rename'),
            prefix: const Icon(FrankIcons.edit, size: 16),
            semanticsLabel: 'Rename project',
            onPress: invoke(widget.onRename),
          ),
        ],
      ),
      FTileGroup(
        key: const ValueKey('project-actions-group-reveal'),
        divider: FItemDivider.none,
        children: [
          FTile(
            key: const ValueKey('project-action-reveal'),
            title: _frankMenuTitle('Reveal in Finder'),
            prefix: const Icon(FrankIcons.folderOpen, size: 16),
            enabled: false,
            semanticsLabel: 'Reveal project in Finder',
            semanticsTooltip: 'No local project location is available yet',
          ),
        ],
      ),
      FTileGroup(
        key: const ValueKey('project-actions-group-destructive'),
        divider: FItemDivider.none,
        children: [
          FTile(
            key: const ValueKey('project-action-archive'),
            title: _frankMenuTitle('Archive project'),
            prefix: const Icon(FrankIcons.archive, size: 16),
            semanticsLabel: 'Archive project',
            onPress: invoke(widget.onArchive),
          ),
          FTile(
            key: const ValueKey('project-action-remove'),
            variant: FItemVariant.destructive,
            title: _frankMenuTitle('Remove project'),
            prefix: const Icon(FrankIcons.close, size: 16),
            semanticsLabel: 'Remove project',
            onPress: invoke(widget.onRemove),
          ),
        ],
      ),
    ];
  }

  Widget _actionsButton(FPopoverController controller) {
    return SizedBox(
      width: 28,
      height: 28,
      child: ExcludeSemantics(
        excluding: !_showActions,
        child: IgnorePointer(
          ignoring: !_showActions,
          child: AnimatedOpacity(
            opacity: _showActions ? 1 : 0,
            duration: const Duration(milliseconds: 120),
            child: IconButton(
              onPressed: _toggleActions,
              tooltip: 'Project actions for ${widget.project.name}',
              icon: const Icon(FrankIcons.more, size: 17),
              color: _projectSelected ? FrankColors.ink : FrankColors.muted,
              padding: EdgeInsets.zero,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints.tightFor(width: 28, height: 28),
            ),
          ),
        ),
      ),
    );
  }

  Widget _projectRow(FPopoverController controller) {
    return InteractiveTreeRow(
      key: ValueKey('project-row-${widget.project.id}'),
      selected: _projectSelected,
      hovered: _hovered,
      menuOpen: _actionsOpen || _contextMenuOpen,
      onPointerDown: widget.inputModality.pointerDown,
      onTap: _selectProject,
      prefix: Icon(
        FrankIcons.folder,
        size: 16,
        color: _projectSelected ? FrankColors.ink : FrankColors.muted,
      ),
      label: Semantics(
        container: true,
        excludeSemantics: true,
        button: true,
        focusable: true,
        selected: _projectSelected,
        label: '${widget.project.name}, ${widget.project.statusLabel}',
        expanded: widget.expanded,
        onTap: _selectProject,
        child: Text(
          key: ValueKey('project-label-${widget.project.id}'),
          widget.project.name,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            color: _projectSelected
                ? FrankColors.ink.withValues(alpha: _treeSelectedInkOpacity)
                : FrankColors.ink.withValues(alpha: _treeMissionInkOpacity),
            fontSize: 14,
            fontWeight: FontWeight.w400,
            height: 20 / 14,
          ),
        ),
      ),
      suffix: Row(
        mainAxisSize: MainAxisSize.min,
        children: [_actionsButton(controller), _createMissionAction()],
      ),
    );
  }

  Widget _actionsPopover() {
    return FPopoverMenu.tiles(
      key: ValueKey('project-popover-${widget.project.id}'),
      control: FPopoverControl.managed(
        controller: _actionsController,
        onChange: (shown) {
          if (mounted) setState(() => _actionsOpen = shown);
        },
      ),
      intrinsicWidth: false,
      menuBuilder: (_, controller, _) => _actionMenu(controller),
      style: _frankMenuStyle,
      menuAnchor: Alignment.topLeft,
      childAnchor: Alignment.topRight,
      spacing: const FPortalSpacing.spacing(4),
      offset: Offset.zero,
      overflow: FPortalOverflow.flip,
      onTapHide: widget.inputModality.pointerDown,
      semanticsLabel: 'Project actions for ${widget.project.name}',
      child: const SizedBox.shrink(),
      builder: (_, controller, _) => _projectRow(controller),
    );
  }

  @override
  Widget build(BuildContext context) {
    final visibleMissions =
        (_showAllMissions
                ? widget.project.missions
                : widget.project.missions.take(5))
            .toList(growable: false);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Focus(
          focusNode: _focusNode,
          onFocusChange: (value) => setState(() => _focused = value),
          onKeyEvent: _handleKeyEvent,
          child: Listener(
            behavior: HitTestBehavior.opaque,
            onPointerDown: (_) => widget.inputModality.pointerDown(),
            child: MouseRegion(
              onEnter: (_) => setState(() => _hovered = true),
              onExit: (_) => setState(() => _hovered = false),
              child: FContextMenu.tiles(
                control: FPopoverControl.managed(
                  controller: _contextMenuController,
                  onChange: _handleContextMenuChanged,
                ),
                intrinsicWidth: false,
                style: _frankMenuStyle,
                onTapHide: widget.inputModality.pointerDown,
                semanticsLabel: 'Actions for ${widget.project.name}',
                secondaryPress: true,
                longPress: false,
                menuBuilder: (_, controller, _) => _actionMenu(controller),
                child: _actionsPopover(),
              ),
            ),
          ),
        ),
        AnimatedSize(
          duration: const Duration(milliseconds: 180),
          curve: Curves.easeOutCubic,
          child: widget.expanded
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: widget.project.missions.isEmpty
                      ? [
                          const Padding(
                            padding: EdgeInsets.symmetric(
                              horizontal: 8,
                              vertical: 7,
                            ),
                            child: Text(
                              'No tasks yet',
                              style: TextStyle(
                                color: FrankColors.muted,
                                fontSize: 14,
                                fontWeight: FontWeight.w400,
                                height: 20 / 14,
                              ),
                            ),
                          ),
                        ]
                      : [
                          for (
                            var index = 0;
                            index < visibleMissions.length;
                            index++
                          ) ...[
                            if (index > 0) const SizedBox(height: 2),
                            _MissionTreeRow(
                              key: ValueKey(
                                'mission-row-${visibleMissions[index].id}',
                              ),
                              mission: visibleMissions[index],
                              selected:
                                  visibleMissions[index].id ==
                                  widget.selectedMissionId,
                              inputModality: widget.inputModality,
                              onTap: () => widget.onSelectMission(
                                visibleMissions[index].id,
                              ),
                              onPin: () => widget.onPinMission(
                                visibleMissions[index].id,
                              ),
                              onRename: () => widget.onRenameMission(
                                visibleMissions[index].id,
                              ),
                              onArchive: () => widget.onArchiveMission(
                                visibleMissions[index].id,
                              ),
                            ),
                          ],
                          if (!_showAllMissions &&
                              widget.project.missions.length > 5)
                            _ShowMoreMissions(
                              onPressed: () =>
                                  setState(() => _showAllMissions = true),
                            ),
                        ],
                )
              : const SizedBox.shrink(),
        ),
      ],
    );
  }

  Widget _createMissionAction() {
    return SizedBox(
      width: 28,
      height: 28,
      child: ExcludeSemantics(
        excluding: !_showActions,
        child: IgnorePointer(
          ignoring: !_showActions,
          child: AnimatedOpacity(
            opacity: _showActions ? 1 : 0,
            duration: const Duration(milliseconds: 120),
            child: IconButton(
              onPressed: widget.onCreateMission,
              tooltip: 'Create a new task in ${widget.project.name}',
              icon: const Icon(FrankIcons.editNote, size: 16),
              padding: EdgeInsets.zero,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints.tightFor(width: 28, height: 28),
            ),
          ),
        ),
      ),
    );
  }
}

class _ShowMoreMissions extends StatelessWidget {
  const _ShowMoreMissions({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 32,
      child: TextButton(
        onPressed: onPressed,
        style: TextButton.styleFrom(
          alignment: Alignment.centerLeft,
          padding: const EdgeInsets.symmetric(horizontal: 32),
          foregroundColor: FrankColors.muted,
          textStyle: const TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w400,
            height: 20 / 14,
          ),
        ),
        child: const Text('Show more'),
      ),
    );
  }
}

class _MissionTreeRow extends StatefulWidget {
  const _MissionTreeRow({
    super.key,
    required this.mission,
    required this.selected,
    required this.inputModality,
    required this.onTap,
    required this.onPin,
    required this.onRename,
    required this.onArchive,
  });

  final OfficeMission mission;
  final bool selected;
  final _TreeInputModality inputModality;
  final VoidCallback onTap;
  final VoidCallback onPin;
  final VoidCallback onRename;
  final VoidCallback onArchive;

  @override
  State<_MissionTreeRow> createState() => _MissionTreeRowState();
}

class _MissionTreeRowState extends State<_MissionTreeRow>
    with TickerProviderStateMixin {
  late final FocusNode _focusNode;
  late final FPopoverController _actionsController;
  late final FPopoverController _contextMenuController;
  final Object _actionsMenuOwner = Object();
  final Object _contextMenuOwner = Object();
  bool _hovered = false;
  bool _focused = false;
  bool _actionsOpen = false;
  bool _contextMenuOpen = false;

  bool get _showActions =>
      widget.selected ||
      _hovered ||
      _focused ||
      _actionsOpen ||
      _contextMenuOpen;

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'Mission ${widget.mission.title}');
    _actionsController = FPopoverController(vsync: this);
    _contextMenuController = FPopoverController(vsync: this);
    _actionsController.addListener(_handleActionsChanged);
  }

  @override
  void dispose() {
    FrankDesktopMenuDismissScope.release(context, _actionsMenuOwner);
    FrankDesktopMenuDismissScope.release(context, _contextMenuOwner);
    _actionsController.removeListener(_handleActionsChanged);
    _actionsController.dispose();
    _contextMenuController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _handleActionsChanged() {
    final open = _actionsController.status.isForwardOrCompleted;
    if (open) {
      FrankDesktopMenuDismissScope.register(
        context,
        _actionsMenuOwner,
        () => unawaited(_actionsController.hide(animated: false)),
      );
    } else {
      FrankDesktopMenuDismissScope.release(context, _actionsMenuOwner);
    }
    if (open != _actionsOpen && mounted) {
      setState(() => _actionsOpen = open);
    }
  }

  void _handleContextMenuChanged(bool open) {
    if (open) {
      FrankDesktopMenuDismissScope.register(
        context,
        _contextMenuOwner,
        () => unawaited(_contextMenuController.hide(animated: false)),
      );
    } else {
      FrankDesktopMenuDismissScope.release(context, _contextMenuOwner);
    }
    if (open != _contextMenuOpen && mounted) {
      setState(() => _contextMenuOpen = open);
    }
  }

  void _toggleActions() {
    if (_actionsController.status.isForwardOrCompleted) {
      unawaited(_actionsController.hide());
      return;
    }
    FrankDesktopMenuDismissScope.dismissAll(context, restoreFocus: false);
    unawaited(_actionsController.show());
  }

  void _selectMission() {
    widget.onTap();
    _focusNode.requestFocus();
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    widget.inputModality.keyDown();
    final contextMenuPressed =
        event.logicalKey == LogicalKeyboardKey.contextMenu ||
        (event.logicalKey == LogicalKeyboardKey.f10 &&
            HardwareKeyboard.instance.isShiftPressed);
    if (contextMenuPressed) {
      _toggleActions();
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.space) {
      _selectMission();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  List<FTileGroupMixin> _actionMenu(FPopoverController controller) {
    VoidCallback invoke(VoidCallback action) => () {
      unawaited(controller.hide());
      action();
    };

    return [
      FTileGroup(
        key: const ValueKey('mission-actions-group-primary'),
        divider: FItemDivider.none,
        children: [
          FTile(
            key: const ValueKey('mission-action-pin'),
            title: _frankMenuTitle('Pin'),
            prefix: const Icon(FrankIcons.pin, size: 16),
            semanticsLabel: 'Pin task',
            onPress: invoke(widget.onPin),
          ),
          FTile(
            key: const ValueKey('mission-action-rename'),
            title: _frankMenuTitle('Rename'),
            prefix: const Icon(FrankIcons.edit, size: 16),
            semanticsLabel: 'Rename task',
            onPress: invoke(widget.onRename),
          ),
        ],
      ),
      FTileGroup(
        key: const ValueKey('mission-actions-group-archive'),
        divider: FItemDivider.none,
        children: [
          FTile(
            key: const ValueKey('mission-action-archive'),
            title: _frankMenuTitle('Archive task'),
            prefix: const Icon(FrankIcons.archive, size: 16),
            semanticsLabel: 'Archive task',
            onPress: invoke(widget.onArchive),
          ),
        ],
      ),
    ];
  }

  Widget _actionsButton(FPopoverController controller) {
    return SizedBox(
      width: 28,
      height: 28,
      child: ExcludeSemantics(
        excluding: !_showActions,
        child: IgnorePointer(
          ignoring: !_showActions,
          child: AnimatedOpacity(
            opacity: _showActions ? 1 : 0,
            duration: const Duration(milliseconds: 120),
            child: IconButton(
              onPressed: _toggleActions,
              tooltip: 'Task actions for ${widget.mission.title}',
              icon: const Icon(FrankIcons.more, size: 16),
              color: widget.selected ? FrankColors.ink : FrankColors.muted,
              padding: EdgeInsets.zero,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints.tightFor(width: 28, height: 28),
            ),
          ),
        ),
      ),
    );
  }

  Widget _missionRow(FPopoverController controller) {
    return InteractiveTreeRow(
      key: ValueKey('mission-surface-${widget.mission.id}'),
      selected: widget.selected,
      hovered: _hovered,
      menuOpen: _actionsOpen || _contextMenuOpen,
      onPointerDown: widget.inputModality.pointerDown,
      onTap: _selectMission,
      label: Semantics(
        container: true,
        excludeSemantics: true,
        button: true,
        focusable: true,
        selected: widget.selected,
        label: '${widget.mission.title}, ${widget.mission.statusLabel}',
        onTap: _selectMission,
        child: Text(
          key: ValueKey('mission-label-${widget.mission.id}'),
          widget.mission.title,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(
            color: widget.selected
                ? FrankColors.ink.withValues(alpha: _treeSelectedInkOpacity)
                : FrankColors.ink.withValues(alpha: _treeMissionInkOpacity),
            fontSize: 14,
            fontWeight: FontWeight.w400,
            height: 20 / 14,
          ),
        ),
      ),
      suffix: Row(
        mainAxisSize: MainAxisSize.min,
        children: [_pinAction(), _actionsButton(controller)],
      ),
    );
  }

  Widget _actionsPopover() {
    return FPopoverMenu.tiles(
      key: ValueKey('mission-popover-${widget.mission.id}'),
      control: FPopoverControl.managed(
        controller: _actionsController,
        onChange: (shown) {
          if (mounted) setState(() => _actionsOpen = shown);
        },
      ),
      intrinsicWidth: false,
      menuBuilder: (_, controller, _) => _actionMenu(controller),
      style: _frankMenuStyle,
      menuAnchor: Alignment.topLeft,
      childAnchor: Alignment.topRight,
      spacing: const FPortalSpacing.spacing(4),
      offset: Offset.zero,
      overflow: FPortalOverflow.flip,
      onTapHide: widget.inputModality.pointerDown,
      semanticsLabel: 'Task actions for ${widget.mission.title}',
      child: const SizedBox.shrink(),
      builder: (_, controller, _) => _missionRow(controller),
    );
  }

  Widget _pinAction() {
    return SizedBox(
      width: 28,
      height: 28,
      child: ExcludeSemantics(
        excluding: !_showActions,
        child: IgnorePointer(
          ignoring: !_showActions,
          child: AnimatedOpacity(
            opacity: _showActions ? 1 : 0,
            duration: const Duration(milliseconds: 120),
            child: IconButton(
              onPressed: widget.onPin,
              tooltip: 'Pin task ${widget.mission.title} to the pinned list',
              icon: const Icon(FrankIcons.pin, size: 16),
              padding: EdgeInsets.zero,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints.tightFor(width: 28, height: 28),
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Focus(
      focusNode: _focusNode,
      onFocusChange: (value) => setState(() => _focused = value),
      onKeyEvent: _handleKeyEvent,
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: (_) => widget.inputModality.pointerDown(),
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          onEnter: (_) => setState(() => _hovered = true),
          onExit: (_) => setState(() => _hovered = false),
          child: FContextMenu.tiles(
            control: FPopoverControl.managed(
              controller: _contextMenuController,
              onChange: _handleContextMenuChanged,
            ),
            intrinsicWidth: false,
            style: _frankMenuStyle,
            onTapHide: widget.inputModality.pointerDown,
            semanticsLabel: 'Actions for ${widget.mission.title}',
            secondaryPress: true,
            longPress: false,
            menuBuilder: (_, controller, _) => _actionMenu(controller),
            child: _actionsPopover(),
          ),
        ),
      ),
    );
  }
}

/// Shared interaction surface for project and mission rows.
///
/// The row-specific popover/menu remains in each row widget; this primitive
/// owns the common hover, keyboard focus state, tap target, and 32px geometry.
class InteractiveTreeRow extends StatelessWidget {
  const InteractiveTreeRow({
    super.key,
    required this.selected,
    required this.hovered,
    required this.menuOpen,
    required this.onPointerDown,
    required this.onTap,
    this.prefix,
    required this.label,
    required this.suffix,
  });

  final bool selected;
  final bool hovered;
  final bool menuOpen;
  final VoidCallback onPointerDown;
  final VoidCallback onTap;
  final Widget? prefix;
  final Widget label;
  final Widget suffix;

  @override
  Widget build(BuildContext context) {
    final background = selected
        ? FrankColors.ink.withValues(alpha: 0.08)
        : menuOpen
        ? FrankColors.ink.withValues(alpha: 0.08)
        : hovered
        ? FrankColors.ink.withValues(alpha: 0.06)
        : Colors.transparent;

    return Material(
      color: background,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTapDown: (_) => onPointerDown(),
        onTap: onTap,
        excludeFromSemantics: true,
        focusColor: Colors.transparent,
        borderRadius: BorderRadius.circular(8),
        child: SizedBox(
          width: double.infinity,
          height: 32,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8),
            child: Row(
              children: [
                SizedBox(
                  width: 16,
                  height: 16,
                  child: prefix == null ? null : Center(child: prefix),
                ),
                const SizedBox(width: 8),
                Expanded(child: label),
                suffix,
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _OfficeNavigation extends StatelessWidget {
  const _OfficeNavigation({required this.selected, required this.onSelect});

  final OfficeSection selected;
  final ValueChanged<OfficeSection> onSelect;

  @override
  Widget build(BuildContext context) {
    return FSidebarGroup(
      key: const ValueKey('office-navigation'),
      style: _frankOfficeNavigationStyle,
      label: const Text('Office'),
      children: [
        for (final section in OfficeSection.values)
          FSidebarItem(
            key: ValueKey('office-section-${section.name}'),
            selected: selected == section,
            icon: Icon(_iconFor(section), size: 16),
            label: Text(section.label),
            onPress: () => onSelect(section),
          ),
      ],
    );
  }

  IconData _iconFor(OfficeSection section) => switch (section) {
    OfficeSection.organization => FrankIcons.gitBranch,
    OfficeSection.team => FrankIcons.users,
    OfficeSection.ledger => FrankIcons.ledger,
    OfficeSection.taskboard => FrankIcons.dashboard,
    OfficeSection.journal => FrankIcons.activity,
  };
}

class _SettingsNavigation extends StatelessWidget {
  const _SettingsNavigation({required this.selected, required this.onSelect});

  final SettingsSection selected;
  final ValueChanged<SettingsSection> onSelect;

  @override
  Widget build(BuildContext context) {
    return FSidebarGroup(
      label: const Text('Settings'),
      children: [
        _settingsItem(
          section: SettingsSection.projects,
          icon: FrankIcons.folder,
          label: 'Projects',
        ),
        _settingsItem(
          section: SettingsSection.team,
          icon: FrankIcons.users,
          label: 'Team',
        ),
        _settingsItem(
          section: SettingsSection.activity,
          icon: FrankIcons.activity,
          label: 'Activity',
        ),
        _settingsItem(
          section: SettingsSection.ledger,
          icon: FrankIcons.ledger,
          label: 'Ledger',
        ),
      ],
    );
  }

  Widget _settingsItem({
    required SettingsSection section,
    required IconData icon,
    required String label,
  }) {
    return FSidebarItem(
      selected: selected == section,
      icon: Icon(icon, size: 18),
      label: Text(label),
      onPress: () => onSelect(section),
    );
  }
}

class _SidebarFooter extends StatelessWidget {
  const _SidebarFooter({
    required this.dense,
    required this.settingsOpen,
    required this.onOpenSettings,
  });

  final bool dense;
  final bool settingsOpen;
  final VoidCallback onOpenSettings;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: EdgeInsets.all(dense ? 10 : 12),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.start,
        children: [
          IconButton(
            onPressed: onOpenSettings,
            tooltip: 'Open workspace settings',
            icon: Icon(
              FrankIcons.settings,
              size: 18,
              color: settingsOpen ? FrankColors.aubergine : null,
            ),
            padding: dense ? EdgeInsets.zero : null,
            visualDensity: dense ? VisualDensity.compact : null,
            constraints: dense
                ? const BoxConstraints.tightFor(width: 32, height: 32)
                : null,
          ),
          const Expanded(
            child: Text(
              'Prototype workspace',
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: FrankColors.muted, fontSize: 11),
            ),
          ),
        ],
      ),
    );
  }
}

class FrankLogo extends StatelessWidget {
  const FrankLogo({
    required this.size,
    this.semanticLabel = 'Frank',
    super.key,
  });

  final double size;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(size * 0.22),
      child: Image.asset(
        'assets/branding/frank-logo.png',
        width: size,
        height: size,
        fit: BoxFit.cover,
        semanticLabel: semanticLabel,
        errorBuilder: (context, error, stackTrace) {
          return Container(
            width: size,
            height: size,
            alignment: Alignment.center,
            color: FrankColors.aubergine.withValues(alpha: 0.14),
            child: const Text(
              'F',
              style: TextStyle(color: FrankColors.aubergine),
            ),
          );
        },
      ),
    );
  }
}
