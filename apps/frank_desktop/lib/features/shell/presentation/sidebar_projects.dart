part of 'package:frank_desktop/features/shell/main_sidebar.dart';

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

class _ProjectTreeState extends State<_ProjectTree> {
  late final FocusNode _focusNode;
  bool _hovered = false;
  bool _focused = false;
  bool _actionsOpen = false;
  bool _showAllMissions = false;
  FPopoverController? _actionsController;

  bool get _projectSelected =>
      widget.containsSelection && widget.selectedMissionId == null;

  bool get _showActions =>
      _projectSelected || _hovered || _focused || _actionsOpen;

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'Project ${widget.project.name}');
  }

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  void _toggleActions() {
    _actionsController?.toggle();
    setState(() => _actionsOpen = !_actionsOpen);
  }

  void _selectProject() {
    widget.onToggle();
    if (mounted) setState(() => _focused = true);
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

  List<FItemGroupMixin> _actionMenu() {
    return [
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('project-action-pin'),
            title: const Text('Pin'),
            prefix: const Icon(FrankIcons.pin),
            semanticsLabel: 'Pin project',
            onPress: widget.onPin,
          ),
          FItem(
            key: const ValueKey('project-action-rename'),
            title: const Text('Rename'),
            prefix: const Icon(FrankIcons.edit),
            semanticsLabel: 'Rename project',
            onPress: widget.onRename,
          ),
        ],
      ),
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('project-action-reveal'),
            title: const Text('Reveal in Finder'),
            prefix: const Icon(FrankIcons.folderOpen),
            enabled: false,
            semanticsLabel: 'Reveal project in Finder',
          ),
        ],
      ),
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('project-action-archive'),
            title: const Text('Archive project'),
            prefix: const Icon(FrankIcons.archive),
            semanticsLabel: 'Archive project',
            onPress: widget.onArchive,
          ),
          FItem(
            key: const ValueKey('project-action-remove'),
            title: const Text('Remove project'),
            prefix: const Icon(FrankIcons.close),
            variant: FItemVariant.destructive,
            semanticsLabel: 'Remove project',
            onPress: widget.onRemove,
          ),
        ],
      ),
    ];
  }

  Widget _actionsButton({VoidCallback? onPress}) {
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
            child: FButton.icon(
              key: ValueKey('project-actions-${widget.project.id}'),
              onPress: onPress ?? _toggleActions,
              semanticsLabel: 'Project actions for ${widget.project.name}',
              semanticsTooltip: 'Project actions for ${widget.project.name}',
              size: FButtonSizeVariant.sm,
              child: const Icon(FrankIcons.more, size: 17),
            ),
          ),
        ),
      ),
    );
  }

  Widget _projectRow(VoidCallback toggleActions) {
    return InteractiveTreeRow(
      key: ValueKey('project-row-${widget.project.id}'),
      selected: _projectSelected,
      hovered: _hovered,
      menuOpen: _actionsOpen,
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
        children: [
          _actionsButton(onPress: toggleActions),
          _createMissionAction(),
        ],
      ),
    );
  }

  Widget _actionsPopover() {
    return FPopoverMenu(
      key: ValueKey('project-popover-${widget.project.id}'),
      groupId: 'project-tree-menu',
      style: const FPopoverMenuStyleDelta.delta(motion: FPopoverMotion.none),
      menu: _actionMenu(),
      semanticsLabel: 'Project actions for ${widget.project.name}',
      builder: (_, controller, _) {
        _actionsController = controller;
        return _projectRow(() {
          controller.toggle();
          if (mounted) setState(() => _actionsOpen = !_actionsOpen);
        });
      },
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
              child: FContextMenu(
                groupId: 'project-tree-menu',
                semanticsLabel: 'Actions for ${widget.project.name}',
                menu: _actionMenu(),
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
                              'No missions yet',
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
            child: FButton.icon(
              onPress: widget.onCreateMission,
              semanticsLabel: 'New mission in ${widget.project.name}',
              semanticsTooltip: 'New mission in ${widget.project.name}',
              size: FButtonSizeVariant.sm,
              child: const Icon(FrankIcons.editNote, size: 14),
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
      child: FButton(
        onPress: onPressed,
        variant: FButtonVariant.ghost,
        size: FButtonSizeVariant.sm,
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

class _MissionTreeRowState extends State<_MissionTreeRow> {
  late final FocusNode _focusNode;
  bool _hovered = false;
  bool _focused = false;
  bool _actionsOpen = false;
  FPopoverController? _actionsController;

  bool get _showActions =>
      widget.selected || _hovered || _focused || _actionsOpen;

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'Mission ${widget.mission.title}');
  }

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  void _toggleActions() {
    _actionsController?.toggle();
    setState(() => _actionsOpen = !_actionsOpen);
  }

  void _selectMission() {
    widget.onTap();
    if (mounted) setState(() => _focused = true);
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

  List<FItemGroupMixin> _actionMenu() {
    return [
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('mission-action-pin'),
            title: const Text('Pin'),
            prefix: const Icon(FrankIcons.pin),
            semanticsLabel: 'Pin mission',
            onPress: widget.onPin,
          ),
          FItem(
            key: const ValueKey('mission-action-rename'),
            title: const Text('Rename'),
            prefix: const Icon(FrankIcons.edit),
            semanticsLabel: 'Rename mission',
            onPress: widget.onRename,
          ),
        ],
      ),
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('mission-action-archive'),
            title: const Text('Archive mission'),
            prefix: const Icon(FrankIcons.archive),
            semanticsLabel: 'Archive mission',
            onPress: widget.onArchive,
          ),
        ],
      ),
    ];
  }

  Widget _actionsButton({VoidCallback? onPress}) {
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
            child: FButton.icon(
              onPress: onPress ?? _toggleActions,
              semanticsLabel: 'Mission actions for ${widget.mission.title}',
              semanticsTooltip: 'Mission actions for ${widget.mission.title}',
              size: FButtonSizeVariant.sm,
              child: const Icon(FrankIcons.more, size: 16),
            ),
          ),
        ),
      ),
    );
  }

  Widget _missionRow(VoidCallback toggleActions) {
    return InteractiveTreeRow(
      key: ValueKey('mission-surface-${widget.mission.id}'),
      selected: widget.selected,
      hovered: _hovered,
      menuOpen: _actionsOpen,
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
        children: [
          _pinAction(),
          _actionsButton(onPress: toggleActions),
        ],
      ),
    );
  }

  Widget _actionsPopover() {
    return FPopoverMenu(
      key: ValueKey('mission-popover-${widget.mission.id}'),
      groupId: 'project-tree-menu',
      style: const FPopoverMenuStyleDelta.delta(motion: FPopoverMotion.none),
      menu: _actionMenu(),
      semanticsLabel: 'Mission actions for ${widget.mission.title}',
      builder: (_, controller, _) {
        _actionsController = controller;
        return _missionRow(() {
          controller.toggle();
          if (mounted) setState(() => _actionsOpen = !_actionsOpen);
        });
      },
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
            child: FButton.icon(
              onPress: widget.onPin,
              semanticsLabel:
                  'Pin mission ${widget.mission.title} to the pinned list',
              semanticsTooltip:
                  'Pin mission ${widget.mission.title} to the pinned list',
              size: FButtonSizeVariant.sm,
              child: const Icon(FrankIcons.pin, size: 16),
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
          child: FContextMenu(
            groupId: 'project-tree-menu',
            semanticsLabel: 'Actions for ${widget.mission.title}',
            menu: _actionMenu(),
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
    final background = (selected || menuOpen)
        ? FrankColors.ink.withValues(alpha: 0.08)
        : hovered
        ? FrankColors.ink.withValues(alpha: 0.06)
        : const Color(0x00000000);

    return FTappable.static(
      onPressDown: (_) => onPointerDown(),
      onPress: onTap,
      excludeSemantics: true,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: background,
          borderRadius: BorderRadius.circular(8),
        ),
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

// Kept for source compatibility with the first shell implementation; the
// visible navigation uses [_GlobalNavigation].
// ignore: unused_element
