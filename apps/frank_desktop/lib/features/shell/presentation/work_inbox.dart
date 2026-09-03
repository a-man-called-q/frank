import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../../app/icons.dart';
import '../../../app/theme.dart';
import '../../../core/models/workspace_models.dart';
import '../sidebar_projection.dart';
import 'frank_desktop_menu.dart';

class WorkInboxPane extends StatefulWidget {
  const WorkInboxPane({
    required this.workspace,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.projectScope,
    required this.pinnedMissionIds,
    required this.searchFocusNode,
    required this.onProjectScopeChanged,
    required this.onSelectProject,
    required this.onSelectMission,
    required this.onSelectAgent,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    required this.onCreateMission,
    required this.onRenameMission,
    required this.onArchiveMission,
    super.key,
  });

  final OfficeWorkspace workspace;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final String? projectScope;
  final List<String> pinnedMissionIds;
  final FocusNode searchFocusNode;
  final ValueChanged<String?> onProjectScopeChanged;
  final ValueChanged<String> onSelectProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final VoidCallback onSelectAgent;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;
  final ValueChanged<String> onCreateMission;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;

  @override
  State<WorkInboxPane> createState() => _WorkInboxPaneState();
}

class _WorkInboxPaneState extends State<WorkInboxPane> {
  late final TextEditingController _searchController;
  late final ScrollController _missionScrollController;
  late final ScrollController _searchScrollController;
  bool _showAllCompleted = false;
  double _missionScrollOffset = 0;
  bool _missionScrollRestoreScheduled = false;

  @override
  void initState() {
    super.initState();
    _searchController = TextEditingController();
    _missionScrollController = ScrollController(
      debugLabel: 'Frank mission shelves',
      keepScrollOffset: false,
    );
    _searchScrollController = ScrollController(
      debugLabel: 'Frank work inbox search',
      keepScrollOffset: false,
    );
    _missionScrollController.addListener(_rememberMissionScrollOffset);
  }

  @override
  void dispose() {
    _searchController.dispose();
    _missionScrollController.dispose();
    _searchScrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = _searchController.text.trim();
    final searchResults = SidebarProjection.search(
      workspace: widget.workspace,
      projectId: widget.projectScope,
      query: query,
    );
    final groups = SidebarProjection.shelves(
      projects: widget.workspace.projects,
      projectId: widget.projectScope,
      pinnedMissionIds: widget.pinnedMissionIds,
      showAllCompleted: _showAllCompleted,
    );
    final scopeProject = _scopeProject;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _SearchField(
                controller: _searchController,
                focusNode: widget.searchFocusNode,
                onChanged: _onSearchChanged,
                onClear: _clearSearch,
              ),
              const SizedBox(height: 10),
              Row(
                children: [
                  Expanded(
                    child: _ScopeSelector(
                      projects: widget.workspace.projects,
                      selectedProjectId: widget.projectScope,
                      onChanged: widget.onProjectScopeChanged,
                    ),
                  ),
                  const SizedBox(width: 6),
                  Tooltip(
                    message: scopeProject == null
                        ? 'Select a project to enable task creation'
                        : 'Create a new task in ${scopeProject.name}',
                    child: IconButton(
                      onPressed: scopeProject == null
                          ? null
                          : () => widget.onCreateMission(scopeProject.id),
                      icon: const Icon(FrankIcons.plus, size: 17),
                      color: FrankColors.muted,
                      visualDensity: VisualDensity.compact,
                      constraints: const BoxConstraints.tightFor(
                        width: 30,
                        height: 30,
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(height: 12),
        Expanded(
          child: query.isNotEmpty
              ? _SearchResults(
                  controller: _searchScrollController,
                  results: searchResults,
                  selectedProjectId: widget.selectedProjectId,
                  selectedMissionId: widget.selectedMissionId,
                  onSelectProject: (projectId) {
                    _clearSearch();
                    widget.onSelectProject(projectId);
                  },
                  onSelectMission: (projectId, missionId) {
                    _clearSearch();
                    widget.onSelectMission(projectId, missionId);
                  },
                  onSelectAgent: () {
                    _clearSearch();
                    widget.onSelectAgent();
                  },
                )
              : _ShelfList(
                  controller: _missionScrollController,
                  employees: widget.workspace.employees,
                  groups: groups,
                  selectedProjectId: widget.selectedProjectId,
                  selectedMissionId: widget.selectedMissionId,
                  onSelectMission: widget.onSelectMission,
                  onTogglePinnedMission: widget.onTogglePinnedMission,
                  onReorderPinnedMissions: widget.onReorderPinnedMissions,
                  onShowMoreCompleted: () =>
                      setState(() => _showAllCompleted = true),
                  onRenameMission: widget.onRenameMission,
                  onArchiveMission: widget.onArchiveMission,
                ),
        ),
      ],
    );
  }

  OfficeProject? get _scopeProject {
    for (final project in widget.workspace.projects) {
      if (project.id == widget.projectScope) return project;
    }
    return null;
  }

  void _clearSearch() {
    if (_searchController.text.isEmpty) return;
    _searchController.clear();
    _resetSearchScroll();
    setState(() {});
    _restoreMissionScrollPosition();
  }

  void _onSearchChanged(String value) {
    _resetSearchScroll();
    setState(() {});
    if (value.trim().isEmpty) {
      _restoreMissionScrollPosition();
    }
  }

  void _resetSearchScroll() {
    if (_searchScrollController.hasClients &&
        _searchScrollController.offset != 0) {
      _searchScrollController.jumpTo(0);
    }
  }

  void _rememberMissionScrollOffset() {
    if (_missionScrollController.hasClients) {
      _missionScrollOffset = _missionScrollController.offset;
    }
  }

  void _restoreMissionScrollPosition() {
    if (_missionScrollOffset <= 0 || _missionScrollRestoreScheduled) return;
    _missionScrollRestoreScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _missionScrollRestoreScheduled = false;
      if (!mounted || !_missionScrollController.hasClients) return;
      final position = _missionScrollController.position;
      final offset = _missionScrollOffset.clamp(0.0, position.maxScrollExtent);
      if (position.pixels != offset) {
        _missionScrollController.jumpTo(offset);
      }
    });
  }
}

class _SearchField extends StatelessWidget {
  const _SearchField({
    required this.controller,
    required this.focusNode,
    required this.onChanged,
    required this.onClear,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final ValueChanged<String> onChanged;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    return Focus(
      onKeyEvent: (_, event) {
        if (event is! KeyDownEvent ||
            event.logicalKey != LogicalKeyboardKey.escape) {
          return KeyEventResult.ignored;
        }
        if (controller.text.isNotEmpty) {
          onClear();
          return KeyEventResult.handled;
        }
        focusNode.unfocus();
        return KeyEventResult.handled;
      },
      child: Semantics(
        textField: true,
        label: 'Search workspace',
        child: TextField(
          controller: controller,
          focusNode: focusNode,
          onChanged: onChanged,
          style: const TextStyle(color: FrankColors.ink, fontSize: 12),
          decoration: InputDecoration(
            isDense: true,
            filled: true,
            fillColor: FrankColors.panelRaised,
            prefixIcon: const Icon(FrankIcons.search, size: 16),
            prefixIconConstraints: const BoxConstraints.tightFor(width: 34),
            suffixIcon: controller.text.isEmpty
                ? null
                : Semantics(
                    button: true,
                    label: 'Clear the workspace search',
                    child: IconButton(
                      onPressed: onClear,
                      tooltip: 'Clear the workspace search',
                      icon: const Icon(FrankIcons.close, size: 15),
                      color: FrankColors.muted,
                      visualDensity: VisualDensity.compact,
                    ),
                  ),
            hintText: 'Search workspace',
            hintStyle: const TextStyle(color: FrankColors.muted, fontSize: 12),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 10,
              vertical: 9,
            ),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
          ),
        ),
      ),
    );
  }
}

class _ScopeSelector extends StatelessWidget {
  const _ScopeSelector({
    required this.projects,
    required this.selectedProjectId,
    required this.onChanged,
  });

  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final ValueChanged<String?> onChanged;

  @override
  Widget build(BuildContext context) {
    var selectedName = 'All projects';
    for (final project in projects) {
      if (project.id == selectedProjectId) {
        selectedName = project.name;
        break;
      }
    }
    return FrankDesktopSelect<String?>(
      value: selectedProjectId,
      options: [
        const FrankDesktopSelectOption<String?>(
          value: null,
          label: 'All projects',
        ),
        ...projects.map(
          (project) => FrankDesktopSelectOption<String?>(
            value: project.id,
            label: project.name,
          ),
        ),
      ],
      onChanged: onChanged,
      semanticsLabel: 'Project scope',
      child: Container(
        height: 30,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: FrankColors.panelRaised,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: FrankColors.border),
        ),
        child: Row(
          children: [
            Expanded(
              child: Text(
                selectedName,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.ink, fontSize: 11),
              ),
            ),
            const SizedBox(width: 8),
            const Icon(FrankIcons.chevronDown, size: 15),
          ],
        ),
      ),
    );
  }
}

class _ShelfList extends StatelessWidget {
  const _ShelfList({
    required this.controller,
    required this.employees,
    required this.groups,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.onSelectMission,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    required this.onShowMoreCompleted,
    required this.onRenameMission,
    required this.onArchiveMission,
  });

  final ScrollController controller;
  final List<OfficeEmployee> employees;
  final List<SidebarShelfGroup> groups;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final void Function(String projectId, String missionId) onSelectMission;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;
  final VoidCallback onShowMoreCompleted;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;

  @override
  Widget build(BuildContext context) {
    if (groups.isEmpty) {
      return const Center(
        child: Text(
          'No tasks yet',
          style: TextStyle(color: FrankColors.muted, fontSize: 12),
        ),
      );
    }
    return _WorkInboxScrollRegion(
      controller: controller,
      child: ListView(
        key: const ValueKey('mission-shelf-scroll-view'),
        controller: controller,
        primary: false,
        padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
        children: [
          for (final group in groups)
            _ShelfSection(
              employees: employees,
              group: group,
              selectedProjectId: selectedProjectId,
              selectedMissionId: selectedMissionId,
              onSelectMission: onSelectMission,
              onTogglePinnedMission: onTogglePinnedMission,
              onReorderPinnedMissions: onReorderPinnedMissions,
              onShowMoreCompleted: onShowMoreCompleted,
              onRenameMission: onRenameMission,
              onArchiveMission: onArchiveMission,
            ),
        ],
      ),
    );
  }
}

class _WorkInboxScrollRegion extends StatefulWidget {
  const _WorkInboxScrollRegion({required this.controller, required this.child});

  final ScrollController controller;
  final Widget child;

  @override
  State<_WorkInboxScrollRegion> createState() => _WorkInboxScrollRegionState();
}

class _WorkInboxScrollRegionState extends State<_WorkInboxScrollRegion> {
  static const _edgeFadeExtent = 22.0;
  static const _edgeFadeActivationExtent = 10.0;
  double _topFadeStrength = 0;
  double _bottomFadeStrength = 0;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_handleControllerChange);
    _scheduleControllerSync();
  }

  @override
  void didUpdateWidget(covariant _WorkInboxScrollRegion oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller == widget.controller) return;
    oldWidget.controller.removeListener(_handleControllerChange);
    widget.controller.addListener(_handleControllerChange);
    _scheduleControllerSync();
  }

  @override
  void dispose() {
    widget.controller.removeListener(_handleControllerChange);
    super.dispose();
  }

  void _handleControllerChange() {
    if (!mounted || !widget.controller.hasClients) return;
    _syncFromMetrics(widget.controller.position);
  }

  void _syncFromController() {
    if (!mounted || !widget.controller.hasClients) return;
    _syncFromMetrics(widget.controller.position);
  }

  void _scheduleControllerSync() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (!widget.controller.hasClients ||
          !widget.controller.position.hasContentDimensions) {
        // RawScrollbar attaches its controller after the first layout pass in
        // some hosts, and ScrollPosition receives content dimensions after
        // attachment. Retry on the next frame so an initially scrollable list
        // still exposes its directional edge fade without user input.
        _scheduleControllerSync();
        return;
      }
      _syncFromController();
    });
  }

  void _syncFromMetrics(ScrollMetrics metrics) {
    final topDistance = (metrics.pixels - metrics.minScrollExtent).clamp(
      0.0,
      _edgeFadeActivationExtent,
    );
    final bottomDistance = (metrics.maxScrollExtent - metrics.pixels).clamp(
      0.0,
      _edgeFadeActivationExtent,
    );
    final topStrength = _smoothStep(topDistance / _edgeFadeActivationExtent);
    final bottomStrength = _smoothStep(
      bottomDistance / _edgeFadeActivationExtent,
    );
    if ((topStrength - _topFadeStrength).abs() < 0.001 &&
        (bottomStrength - _bottomFadeStrength).abs() < 0.001) {
      return;
    }
    setState(() {
      _topFadeStrength = topStrength;
      _bottomFadeStrength = bottomStrength;
    });
  }

  double _smoothStep(double value) {
    final t = value.clamp(0.0, 1.0);
    return t * t * (3 - 2 * t);
  }

  bool _handleNotification(ScrollNotification notification) {
    if (notification.metrics.axis != Axis.vertical) return false;
    if (notification is ScrollStartNotification ||
        notification is ScrollUpdateNotification ||
        notification is OverscrollNotification ||
        notification is ScrollMetricsNotification) {
      FrankDesktopMenuDismissScope.dismissAll(context, restoreFocus: false);
    }
    _syncFromMetrics(notification.metrics);
    if (notification is ScrollMetricsNotification) {
      // Content dimensions can be reported before the sliver finishes its
      // layout pass. Recheck on the following frame so the initial bottom
      // fade reflects the settled maxScrollExtent without requiring a drag.
      _scheduleControllerSync();
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    return ScrollConfiguration(
      behavior: ScrollConfiguration.of(context).copyWith(scrollbars: false),
      child: NotificationListener<ScrollNotification>(
        onNotification: _handleNotification,
        child: RawScrollbar(
          controller: widget.controller,
          thumbVisibility: false,
          trackVisibility: false,
          thickness: 3,
          radius: const Radius.circular(999),
          thumbColor: FrankColors.muted.withValues(alpha: 0.42),
          minThumbLength: 32,
          fadeDuration: const Duration(milliseconds: 150),
          timeToFade: const Duration(milliseconds: 600),
          mainAxisMargin: 4,
          crossAxisMargin: 2,
          interactive: true,
          // Fade the list pixels themselves instead of painting a dark panel
          // over them. Keeping the masks inside the scrollbar's content child
          // leaves its thumb crisp and fully interactive.
          child: Stack(
            fit: StackFit.expand,
            children: [
              _ScrollEdgeFadeMask(
                topStrength: _topFadeStrength,
                bottomStrength: _bottomFadeStrength,
                extent: _edgeFadeExtent,
                child: widget.child,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ScrollEdgeFadeMask extends StatelessWidget {
  const _ScrollEdgeFadeMask({
    required this.topStrength,
    required this.bottomStrength,
    required this.extent,
    required this.child,
  });

  final double topStrength;
  final double bottomStrength;
  final double extent;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ShaderMask(
      key: const ValueKey('work-inbox-edge-fade-mask'),
      blendMode: BlendMode.dstIn,
      shaderCallback: (bounds) {
        final fadeExtent = bounds.height == 0
            ? 0.5
            : (extent / bounds.height).clamp(0.0, 0.5);
        final top = _fadeColor(topStrength);
        return LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [
            top,
            _fadeColor(topStrength, 0.12),
            _fadeColor(topStrength, 0.5),
            _fadeColor(topStrength, 0.88),
            Colors.white,
            Colors.white,
            _fadeColor(bottomStrength, 0.88),
            _fadeColor(bottomStrength, 0.5),
            _fadeColor(bottomStrength, 0.12),
            _fadeColor(bottomStrength),
          ],
          stops: [
            0,
            fadeExtent * 0.24,
            fadeExtent * 0.50,
            fadeExtent * 0.76,
            fadeExtent,
            1 - fadeExtent,
            1 - fadeExtent * 0.76,
            1 - fadeExtent * 0.50,
            1 - fadeExtent * 0.24,
            1,
          ],
        ).createShader(bounds);
      },
      child: child,
    );
  }

  Color _fadeColor(double strength, [double visibleAlpha = 0]) {
    final alpha = 1 - strength * (1 - visibleAlpha);
    return Colors.white.withValues(alpha: alpha);
  }
}

class _ShelfSection extends StatelessWidget {
  const _ShelfSection({
    required this.employees,
    required this.group,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.onSelectMission,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    required this.onShowMoreCompleted,
    required this.onRenameMission,
    required this.onArchiveMission,
  });

  final List<OfficeEmployee> employees;
  final SidebarShelfGroup group;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final void Function(String projectId, String missionId) onSelectMission;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;
  final VoidCallback onShowMoreCompleted;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;

  @override
  Widget build(BuildContext context) {
    final label = switch (group.shelf) {
      SidebarShelf.attention => 'Needs attention',
      SidebarShelf.pinned => 'Pinned',
      SidebarShelf.draft => 'Draft',
      SidebarShelf.active => 'Active',
      SidebarShelf.completed => 'Completed',
    };
    final icon = switch (group.shelf) {
      SidebarShelf.attention => FrankIcons.circleAlert,
      SidebarShelf.pinned => FrankIcons.pin,
      SidebarShelf.draft => FrankIcons.circleDashed,
      SidebarShelf.active => FrankIcons.activity,
      SidebarShelf.completed => FrankIcons.circleCheck,
    };

    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 6),
            child: Row(
              children: [
                Icon(icon, size: 14, color: FrankColors.muted),
                const SizedBox(width: 7),
                Expanded(
                  child: Text(
                    label,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 10,
                      letterSpacing: 0.8,
                    ),
                  ),
                ),
                const SizedBox(width: 4),
                Text(
                  '${group.totalCount}',
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 10,
                  ),
                ),
              ],
            ),
          ),
          if (group.shelf == SidebarShelf.pinned)
            ReorderableListView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              buildDefaultDragHandles: false,
              itemCount: group.entries.length,
              onReorderItem: (oldIndex, newIndex) {
                final ids = group.entries
                    .map((entry) => entry.mission.id)
                    .toList();
                final id = ids.removeAt(oldIndex);
                ids.insert(newIndex, id);
                onReorderPinnedMissions(ids);
              },
              itemBuilder: (context, index) {
                final entry = group.entries[index];
                return ReorderableDragStartListener(
                  key: ValueKey('pinned-${entry.mission.id}'),
                  index: index,
                  child: _MissionRow(
                    employees: employees,
                    entry: entry,
                    selected:
                        entry.project.id == selectedProjectId &&
                        entry.mission.id == selectedMissionId,
                    onSelect: () =>
                        onSelectMission(entry.project.id, entry.mission.id),
                    onTogglePinned: () =>
                        onTogglePinnedMission(entry.mission.id),
                    onRename: () =>
                        onRenameMission(entry.project.id, entry.mission.id),
                    onArchive: () =>
                        onArchiveMission(entry.project.id, entry.mission.id),
                  ),
                );
              },
            )
          else
            for (final entry in group.entries)
              _MissionRow(
                employees: employees,
                key: ValueKey('${group.shelf.name}-${entry.mission.id}'),
                entry: entry,
                selected:
                    entry.project.id == selectedProjectId &&
                    entry.mission.id == selectedMissionId,
                onSelect: () =>
                    onSelectMission(entry.project.id, entry.mission.id),
                onTogglePinned: () => onTogglePinnedMission(entry.mission.id),
                onRename: () =>
                    onRenameMission(entry.project.id, entry.mission.id),
                onArchive: () =>
                    onArchiveMission(entry.project.id, entry.mission.id),
              ),
          if (group.hasMore)
            TextButton(
              onPressed: onShowMoreCompleted,
              style: TextButton.styleFrom(
                foregroundColor: FrankColors.muted,
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
                alignment: Alignment.centerLeft,
                textStyle: const TextStyle(fontSize: 11),
              ),
              child: Text('Show all ${group.totalCount}'),
            ),
        ],
      ),
    );
  }
}

class _MissionRow extends StatefulWidget {
  const _MissionRow({
    required this.employees,
    required this.entry,
    required this.selected,
    required this.onSelect,
    required this.onTogglePinned,
    required this.onRename,
    required this.onArchive,
    super.key,
  });

  final List<OfficeEmployee> employees;
  final SidebarMissionEntry entry;
  final bool selected;
  final VoidCallback onSelect;
  final VoidCallback onTogglePinned;
  final VoidCallback onRename;
  final VoidCallback onArchive;

  @override
  State<_MissionRow> createState() => _MissionRowState();
}

class _MissionRowState extends State<_MissionRow> {
  // Keep the visual slot wide enough for two native-sized controls. The
  // passive state occupies the same width so titles never reflow on hover.
  static const _trailingSlotWidth = 60.0;

  bool _hovered = false;
  bool _focused = false;
  bool? _lastShowActions;
  var _transitionGeneration = 0;
  late final FrankDesktopMenuController _menuController;
  late final FocusNode _rowFocusNode;

  @override
  void initState() {
    super.initState();
    _menuController = FrankDesktopMenuController();
    _rowFocusNode = FocusNode(
      debugLabel: 'Mission ${widget.entry.mission.title}',
    );
  }

  @override
  void dispose() {
    _menuController.close();
    _rowFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final entry = widget.entry;
    final mission = entry.mission;
    final metadata = '${entry.project.name} · ${mission.statusLabel}';
    final assignment = _agentAssignment(mission);
    final pinPosition = entry.pinnedPosition;
    final details = [
      mission.title,
      'Project: ${entry.project.name}',
      'Status: ${mission.statusLabel}',
      'Agent assignment: $assignment',
      'Approval count: ${mission.pendingApprovalCount}',
      if (pinPosition != null) 'Pinned position: $pinPosition',
    ].join('\n');
    final semanticsLabel = [
      mission.title,
      'Project ${entry.project.name}',
      'Status ${mission.statusLabel}',
      'Agent assignment $assignment',
      mission.pendingApprovalCount == 0
          ? 'No pending approvals'
          : '${mission.pendingApprovalCount} approvals pending',
      if (pinPosition != null) 'Pinned position $pinPosition',
    ].join('. ');
    final showActions = widget.selected || _hovered || _focused;
    if (_lastShowActions != showActions) {
      _lastShowActions = showActions;
      _transitionGeneration++;
    }

    return Focus(
      focusNode: _rowFocusNode,
      onFocusChange: (value) {
        if (_focused == value) return;
        setState(() => _focused = value);
      },
      onKeyEvent: (_, event) {
        if (event is! KeyDownEvent) return KeyEventResult.ignored;
        if (event.logicalKey == LogicalKeyboardKey.enter ||
            event.logicalKey == LogicalKeyboardKey.space) {
          widget.onSelect();
          return KeyEventResult.handled;
        }
        final contextMenuPressed =
            event.logicalKey == LogicalKeyboardKey.contextMenu ||
            (event.logicalKey == LogicalKeyboardKey.f10 &&
                HardwareKeyboard.instance.isShiftPressed);
        if (contextMenuPressed) {
          _menuController.open();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) {
          if (_hovered) return;
          setState(() => _hovered = true);
        },
        onExit: (_) {
          if (!_hovered) return;
          setState(() => _hovered = false);
        },
        child: Semantics(
          container: true,
          button: true,
          selected: widget.selected,
          label: semanticsLabel,
          child: Tooltip(
            message: details,
            child: FrankDesktopMenu(
              controller: _menuController,
              openOnSecondaryTap: true,
              returnFocusNode: _rowFocusNode,
              semanticsLabel: 'Task actions for ${mission.title}',
              width: 150,
              groups: [
                FrankMenuGroup([
                  FrankMenuItem(
                    label: entry.pinned ? 'Unpin' : 'Pin',
                    icon: FrankIcons.pin,
                    onPressed: widget.onTogglePinned,
                  ),
                  FrankMenuItem(
                    label: 'Rename',
                    icon: FrankIcons.edit,
                    onPressed: widget.onRename,
                  ),
                  FrankMenuItem(
                    label: 'Archive',
                    icon: FrankIcons.archive,
                    onPressed: widget.onArchive,
                  ),
                ]),
              ],
              child: Material(
                color: widget.selected
                    ? FrankColors.ink.withValues(alpha: 0.08)
                    : Colors.transparent,
                borderRadius: BorderRadius.circular(7),
                child: InkWell(
                  onTap: widget.onSelect,
                  borderRadius: BorderRadius.circular(7),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 6,
                      vertical: 9,
                    ),
                    child: Row(
                      children: [
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                mission.title,
                                maxLines: 2,
                                softWrap: true,
                                overflow: TextOverflow.fade,
                                style: TextStyle(
                                  color: widget.selected
                                      ? FrankColors.ink
                                      : FrankColors.muted,
                                  fontSize: 12,
                                ),
                              ),
                              const SizedBox(height: 2),
                              Text(
                                metadata,
                                maxLines: 1,
                                overflow: TextOverflow.fade,
                                style: const TextStyle(
                                  color: FrankColors.muted,
                                  fontSize: 10,
                                ),
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(width: 4),
                        SizedBox(
                          width: _trailingSlotWidth,
                          height: 28,
                          child: AnimatedSwitcher(
                            duration: const Duration(milliseconds: 140),
                            reverseDuration: Duration.zero,
                            switchInCurve: Curves.easeOutCubic,
                            switchOutCurve: Curves.easeOutCubic,
                            child: showActions
                                ? _MissionActions(
                                    key: ValueKey(
                                      'mission-actions-$_transitionGeneration',
                                    ),
                                    entry: entry,
                                    onTogglePinned: widget.onTogglePinned,
                                    onOpenMenu: _menuController.open,
                                  )
                                : _PassiveMissionSignals(
                                    key: ValueKey(
                                      'mission-signals-$_transitionGeneration',
                                    ),
                                    entry: entry,
                                  ),
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
    );
  }

  String _agentAssignment(OfficeMission mission) {
    if (mission.assignedAgentIds.isEmpty) return 'Unassigned';
    final names = [
      for (final id in mission.assignedAgentIds)
        for (final employee in widget.employees)
          if (employee.id == id) employee.name,
    ];
    if (names.isNotEmpty) return names.join(', ');
    final count = mission.assignedAgentIds.length;
    return '$count assigned agent${count == 1 ? '' : 's'}';
  }
}

class _MissionActions extends StatelessWidget {
  const _MissionActions({
    required this.entry,
    required this.onTogglePinned,
    required this.onOpenMenu,
    super.key,
  });

  final SidebarMissionEntry entry;
  final VoidCallback onTogglePinned;
  final VoidCallback onOpenMenu;

  @override
  Widget build(BuildContext context) {
    final mission = entry.mission;
    return Align(
      alignment: Alignment.centerRight,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 28,
            height: 28,
            child: IconButton(
              onPressed: onTogglePinned,
              tooltip: entry.pinned
                  ? 'Unpin task ${mission.title} from the pinned list'
                  : 'Pin task ${mission.title} to the pinned list',
              icon: Icon(
                FrankIcons.pin,
                size: 14,
                color: entry.pinned
                    ? FrankColors.aubergineAccent
                    : FrankColors.muted.withValues(alpha: 0.6),
              ),
              padding: EdgeInsets.zero,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints.tightFor(width: 28, height: 28),
            ),
          ),
          SizedBox(
            width: 28,
            height: 28,
            child: Semantics(
              button: true,
              label: 'Task actions for ${mission.title}',
              child: IconButton(
                onPressed: onOpenMenu,
                tooltip: 'Open actions for ${mission.title}',
                icon: const Icon(FrankIcons.more, size: 15),
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints.tightFor(
                  width: 28,
                  height: 28,
                ),
                visualDensity: VisualDensity.compact,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _PassiveMissionSignals extends StatelessWidget {
  const _PassiveMissionSignals({required this.entry, super.key});

  final SidebarMissionEntry entry;

  @override
  Widget build(BuildContext context) {
    final signals = <Widget>[];
    final mission = entry.mission;
    if (mission.pendingApprovalCount > 0) {
      signals.add(
        Semantics(
          container: true,
          label: '${mission.pendingApprovalCount} approvals pending',
          excludeSemantics: true,
          child: Container(
            constraints: const BoxConstraints(minWidth: 20, minHeight: 20),
            padding: const EdgeInsets.symmetric(horizontal: 5),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: FrankColors.warningAmberSoft,
              borderRadius: BorderRadius.circular(6),
            ),
            child: Text(
              '${mission.pendingApprovalCount}',
              style: const TextStyle(
                color: FrankColors.warningAmber,
                fontSize: 10,
              ),
            ),
          ),
        ),
      );
    }
    if (entry.pinned) {
      if (signals.isNotEmpty) signals.add(const SizedBox(width: 4));
      signals.add(
        Semantics(
          container: true,
          label: 'Pinned position ${entry.pinnedPosition ?? 'unknown'}',
          excludeSemantics: true,
          child: const Icon(
            FrankIcons.pin,
            size: 14,
            color: FrankColors.aubergineAccent,
          ),
        ),
      );
    }
    if (signals.isEmpty) return const SizedBox.shrink();
    return Align(
      alignment: Alignment.centerRight,
      child: Row(mainAxisSize: MainAxisSize.min, children: signals),
    );
  }
}

class _SearchResults extends StatelessWidget {
  const _SearchResults({
    required this.controller,
    required this.results,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.onSelectProject,
    required this.onSelectMission,
    required this.onSelectAgent,
  });

  final ScrollController controller;
  final List<SidebarSearchItem> results;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final ValueChanged<String> onSelectProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final VoidCallback onSelectAgent;

  @override
  Widget build(BuildContext context) {
    if (results.isEmpty) {
      return const Center(
        child: Text(
          'No matches found.',
          style: TextStyle(color: FrankColors.muted, fontSize: 12),
        ),
      );
    }
    return _WorkInboxScrollRegion(
      controller: controller,
      child: ListView.separated(
        key: const ValueKey('search-results-scroll-view'),
        controller: controller,
        primary: false,
        padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
        itemCount: results.length,
        separatorBuilder: (_, _) =>
            const Divider(height: 1, color: FrankColors.border),
        itemBuilder: (context, index) {
          final result = results[index];
          final icon = switch (result.kind) {
            SidebarSearchKind.project => FrankIcons.folder,
            SidebarSearchKind.mission => FrankIcons.briefcase,
            SidebarSearchKind.agent => FrankIcons.user,
            SidebarSearchKind.message => FrankIcons.message,
          };
          return Material(
            color: Colors.transparent,
            child: ListTile(
              dense: true,
              contentPadding: const EdgeInsets.symmetric(horizontal: 4),
              leading: Icon(icon, size: 16, color: FrankColors.muted),
              title: Text(
                result.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.ink, fontSize: 12),
              ),
              subtitle: Text(
                result.subtitle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.muted, fontSize: 10),
              ),
              selected:
                  result.projectId == selectedProjectId &&
                  result.missionId == selectedMissionId,
              onTap: () {
                if (result.missionId case final missionId?) {
                  onSelectMission(result.projectId!, missionId);
                } else if (result.projectId case final projectId?) {
                  onSelectProject(projectId);
                } else if (result.kind == SidebarSearchKind.agent) {
                  onSelectAgent();
                }
              },
            ),
          );
        },
      ),
    );
  }
}
