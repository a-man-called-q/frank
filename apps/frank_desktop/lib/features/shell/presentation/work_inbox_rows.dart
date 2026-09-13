part of 'work_inbox.dart';

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
    required this.isLast,
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
  final bool isLast;
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
      padding: EdgeInsets.only(bottom: isLast ? 0 : 16),
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
              width: 248,
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
