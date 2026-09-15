part of 'taskboard_surface.dart';

class _TaskboardContent extends StatelessWidget {
  const _TaskboardContent({
    required this.workspace,
    required this.profileFor,
    required this.state,
    required this.visibleTasks,
    required this.compact,
    required this.onProjectChanged,
    required this.onAttentionChanged,
    required this.onViewChanged,
    required this.onRetry,
    required this.onClearFilters,
    required this.onSelectTask,
    required this.focusNodeFor,
  });

  final OfficeWorkspace workspace;
  final TaskboardAgentLookup profileFor;
  final TaskboardState state;
  final List<TaskboardTask> visibleTasks;
  final bool compact;
  final ValueChanged<String?> onProjectChanged;
  final ValueChanged<bool> onAttentionChanged;
  final ValueChanged<TaskboardView> onViewChanged;
  final VoidCallback onRetry;
  final VoidCallback onClearFilters;
  final ValueChanged<String> onSelectTask;
  final FocusNode Function(String taskId) focusNodeFor;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0x00000000),
      child: switch (state.loadStatus) {
        TaskboardLoadStatus.initial ||
        TaskboardLoadStatus.loading => const _TaskboardLoading(),
        TaskboardLoadStatus.failure => _TaskboardFailure(
          message: frankFriendlyError(
            state.error,
            fallback: 'The taskboard could not be loaded.',
          ),
          onRetry: onRetry,
        ),
        TaskboardLoadStatus.ready => _TaskboardReady(
          workspace: workspace,
          profileFor: profileFor,
          state: state,
          visibleTasks: visibleTasks,
          compact: compact,
          onProjectChanged: onProjectChanged,
          onAttentionChanged: onAttentionChanged,
          onViewChanged: onViewChanged,
          onClearFilters: onClearFilters,
          onSelectTask: onSelectTask,
          focusNodeFor: focusNodeFor,
        ),
      },
    );
  }
}

class _TaskboardLoading extends StatelessWidget {
  const _TaskboardLoading();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            FrankSkeleton(lines: 5, height: 14),
            const SizedBox(height: 14),
            const Text(
              'Loading taskboard…',
              style: TextStyle(color: FrankColors.muted, fontSize: 13),
            ),
          ],
        ),
      ),
    );
  }
}

class _TaskboardFailure extends StatelessWidget {
  const _TaskboardFailure({required this.message, required this.onRetry});

  final String message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return FrankUnavailableState(
      title: 'Taskboard unavailable',
      message: message,
      icon: FrankIcons.circleAlert,
      onRetry: onRetry,
    );
  }
}

class _TaskboardReady extends StatelessWidget {
  const _TaskboardReady({
    required this.workspace,
    required this.profileFor,
    required this.state,
    required this.visibleTasks,
    required this.compact,
    required this.onProjectChanged,
    required this.onAttentionChanged,
    required this.onViewChanged,
    required this.onClearFilters,
    required this.onSelectTask,
    required this.focusNodeFor,
  });

  final OfficeWorkspace workspace;
  final TaskboardAgentLookup profileFor;
  final TaskboardState state;
  final List<TaskboardTask> visibleTasks;
  final bool compact;
  final ValueChanged<String?> onProjectChanged;
  final ValueChanged<bool> onAttentionChanged;
  final ValueChanged<TaskboardView> onViewChanged;
  final VoidCallback onClearFilters;
  final ValueChanged<String> onSelectTask;
  final FocusNode Function(String taskId) focusNodeFor;

  @override
  Widget build(BuildContext context) {
    final effectiveView = compact ? TaskboardView.list : state.view;
    final boardLanes = TaskboardLaneContract.valuesFor(
      fixture: state.snapshot?.isFixture ?? true,
    );
    final projectOptions = <String, String>{};
    for (final task in state.snapshot?.tasks ?? const <TaskboardTask>[]) {
      projectOptions.putIfAbsent(task.projectId, () => task.projectName);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _TaskboardToolbar(
          taskCount: visibleTasks.length,
          missionCount: _missionCount(visibleTasks),
          projectId: state.projectId,
          projectOptions: projectOptions,
          attentionOnly: state.attentionOnly,
          attentionCount: visibleTasks
              .where((task) => task.needsAttention)
              .length,
          view: effectiveView,
          boardEnabled: !compact,
          onProjectChanged: onProjectChanged,
          onAttentionChanged: onAttentionChanged,
          onViewChanged: onViewChanged,
        ),
        Expanded(
          child: effectiveView == TaskboardView.board
              ? _TaskboardBoard(
                  tasks: visibleTasks,
                  workspaceHasTasks: state.hasTasks,
                  lanes: boardLanes,
                  profileFor: profileFor,
                  onSelectTask: onSelectTask,
                  focusNodeFor: focusNodeFor,
                  hasActiveFilters: state.hasActiveFilters,
                  onClearFilters: onClearFilters,
                )
              : _TaskboardList(
                  tasks: visibleTasks,
                  workspaceHasTasks: state.hasTasks,
                  profileFor: profileFor,
                  onSelectTask: onSelectTask,
                  focusNodeFor: focusNodeFor,
                  hasActiveFilters: state.hasActiveFilters,
                  onClearFilters: onClearFilters,
                ),
        ),
      ],
    );
  }

  int _missionCount(List<TaskboardTask> tasks) {
    final ids = <String>{};
    for (final task in tasks) {
      ids.add(task.missionId);
    }
    return ids.length;
  }
}

class _TaskboardToolbar extends StatelessWidget {
  const _TaskboardToolbar({
    required this.taskCount,
    required this.missionCount,
    required this.projectId,
    required this.projectOptions,
    required this.attentionOnly,
    required this.attentionCount,
    required this.view,
    required this.boardEnabled,
    required this.onProjectChanged,
    required this.onAttentionChanged,
    required this.onViewChanged,
  });

  final int taskCount;
  final int missionCount;
  final String? projectId;
  final Map<String, String> projectOptions;
  final bool attentionOnly;
  final int attentionCount;
  final TaskboardView view;
  final bool boardEnabled;
  final ValueChanged<String?> onProjectChanged;
  final ValueChanged<bool> onAttentionChanged;
  final ValueChanged<TaskboardView> onViewChanged;

  @override
  Widget build(BuildContext context) {
    final metrics = OfficeLayoutMetricsScope.maybeOf(context);
    final gutter = metrics?.gutter ?? FrankUiTokens.inset;
    return Padding(
      padding: EdgeInsets.fromLTRB(gutter, 20, gutter, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 8,
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _ProjectSelect(
                    projectId: projectId,
                    options: projectOptions,
                    onChanged: onProjectChanged,
                  ),
                  _AttentionToggle(
                    selected: attentionOnly,
                    count: attentionCount,
                    onChanged: onAttentionChanged,
                  ),
                ],
              ),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _ViewToggle(
                    view: view,
                    boardEnabled: boardEnabled,
                    onChanged: onViewChanged,
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 2),
          Semantics(
            liveRegion: true,
            child: Text(
              '$taskCount tasks · $missionCount missions',
              style: const TextStyle(color: FrankColors.muted, fontSize: 11),
            ),
          ),
        ],
      ),
    );
  }
}

class _ProjectSelect extends StatelessWidget {
  const _ProjectSelect({
    required this.projectId,
    required this.options,
    required this.onChanged,
  });

  final String? projectId;
  final Map<String, String> options;
  final ValueChanged<String?> onChanged;

  @override
  Widget build(BuildContext context) {
    final textScaleFactor = MediaQuery.textScalerOf(context).scale(1);
    // FSelect's trigger keeps its label at the scaled font size. Give it a
    // real responsive measure instead of letting the compact 250px default
    // overflow at 200% accessibility text scaling.
    final expandedWidth = textScaleFactor >= 1.5
        ? 360.0
        : textScaleFactor > 1.25
        ? 300.0
        : null;
    return SizedBox(
      width: expandedWidth,
      child: Semantics(
        label: 'Project filter',
        child: FSelect<String?>.rich(
          key: const ValueKey('taskboard-project-filter'),
          format: (value) =>
              value == null ? 'All projects' : options[value] ?? 'All projects',
          control: FSelectControl<String?>.lifted(
            value: projectId,
            onChange: onChanged,
          ),
          hint: 'All projects',
          children: [
            FSelectItem<String?>.item(
              value: null,
              title: const Text('All projects'),
            ),
            for (final entry in options.entries)
              FSelectItem<String?>.item(
                value: entry.key,
                title: Text(entry.value),
              ),
          ],
        ),
      ),
    );
  }
}

class _AttentionToggle extends StatelessWidget {
  const _AttentionToggle({
    required this.selected,
    required this.count,
    required this.onChanged,
  });

  final bool selected;
  final int count;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final largeText = MediaQuery.textScalerOf(context).scale(1) >= 1.5;
    return LayoutBuilder(
      builder: (context, constraints) {
        final maxWidth = constraints.hasBoundedWidth
            ? constraints.maxWidth
            : 240.0;
        final textMaxWidth = (maxWidth - (largeText ? 24 : 48)).clamp(
          40.0,
          maxWidth,
        );
        final label = Text(
          largeText ? 'Attention · $count' : 'Needs attention  $count',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        );
        if (largeText) {
          return FButton.raw(
            key: const ValueKey('taskboard-attention-filter'),
            onPress: () => onChanged(!selected),
            variant: selected ? FButtonVariant.primary : FButtonVariant.outline,
            size: FButtonSizeVariant.sm,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
              child: ConstrainedBox(
                constraints: BoxConstraints(maxWidth: textMaxWidth),
                child: Center(
                  child: FittedBox(fit: BoxFit.scaleDown, child: label),
                ),
              ),
            ),
          );
        }
        return FButton(
          key: const ValueKey('taskboard-attention-filter'),
          onPress: () => onChanged(!selected),
          variant: selected ? FButtonVariant.primary : FButtonVariant.outline,
          size: FButtonSizeVariant.sm,
          mainAxisSize: MainAxisSize.min,
          prefix: const Icon(
            FrankIcons.circleAlert,
            size: FrankUiTokens.iconSize,
          ),
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: textMaxWidth),
            child: label,
          ),
        );
      },
    );
  }
}

class _ViewToggle extends StatelessWidget {
  const _ViewToggle({
    required this.view,
    required this.boardEnabled,
    required this.onChanged,
  });

  final TaskboardView view;
  final bool boardEnabled;
  final ValueChanged<TaskboardView> onChanged;

  @override
  Widget build(BuildContext context) {
    final largeText = MediaQuery.textScalerOf(context).scale(1) >= 1.5;
    return FrankSegmentedControl<TaskboardView>(
      value: view,
      items: [
        (TaskboardView.board, 'Board', largeText ? null : FrankIcons.taskboard),
        (TaskboardView.list, 'List', largeText ? null : FrankIcons.taskList),
      ],
      itemKeyBuilder: (item) =>
          ValueKey('taskboard-view-${item.name.toLowerCase()}'),
      enabledBuilder: (item) => item == TaskboardView.list || boardEnabled,
      onChanged: onChanged,
    );
  }
}

class _TaskboardBoard extends StatelessWidget {
  const _TaskboardBoard({
    required this.tasks,
    required this.workspaceHasTasks,
    required this.lanes,
    required this.profileFor,
    required this.onSelectTask,
    required this.focusNodeFor,
    required this.hasActiveFilters,
    required this.onClearFilters,
  });

  final List<TaskboardTask> tasks;
  final bool workspaceHasTasks;
  final List<TaskboardLane> lanes;
  final TaskboardAgentLookup profileFor;
  final ValueChanged<String> onSelectTask;
  final FocusNode Function(String taskId) focusNodeFor;
  final bool hasActiveFilters;
  final VoidCallback onClearFilters;

  @override
  Widget build(BuildContext context) {
    final metrics = OfficeLayoutMetricsScope.maybeOf(context);
    final gutter = metrics?.gutter ?? FrankUiTokens.inset;
    if (tasks.isEmpty) {
      return _TaskboardEmpty(
        workspaceHasTasks: workspaceHasTasks,
        hasActiveFilters: hasActiveFilters,
        onClearFilters: onClearFilters,
      );
    }
    final missionIds = <String>[];
    for (final task in tasks) {
      if (!missionIds.contains(task.missionId)) missionIds.add(task.missionId);
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        final availableWidth = constraints.hasBoundedWidth
            ? constraints.maxWidth
            : metrics?.availableWidth ?? double.infinity;
        final availableContentWidth = availableWidth.isFinite
            ? math.max(0.0, availableWidth - gutter * 2)
            : 1000.0;
        final minBoardWidth =
            lanes.length * 220.0 + math.max(0, lanes.length - 1) * 10.0;
        final boardWidth = math.max(minBoardWidth, availableContentWidth);
        return SizedBox(
          height: constraints.maxHeight,
          child: SingleChildScrollView(
            key: const ValueKey('taskboard-board-scroll'),
            padding: EdgeInsets.fromLTRB(gutter, 0, gutter, gutter),
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: SizedBox(
                key: const ValueKey('taskboard-board-content'),
                width: boardWidth,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _BoardColumnHeader(tasks: tasks, lanes: lanes),
                    for (final missionId in missionIds)
                      _MissionBoardSection(
                        tasks: tasks
                            .where((task) => task.missionId == missionId)
                            .toList(growable: false),
                        lanes: lanes,
                        profileFor: profileFor,
                        onSelectTask: onSelectTask,
                        focusNodeFor: focusNodeFor,
                      ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _BoardColumnHeader extends StatelessWidget {
  const _BoardColumnHeader({required this.tasks, required this.lanes});

  final List<TaskboardTask> tasks;
  final List<TaskboardLane> lanes;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        for (final lane in lanes)
          Expanded(
            child: Padding(
              padding: const EdgeInsets.only(right: 10),
              child: DecoratedBox(
                decoration: const BoxDecoration(
                  border: Border(top: BorderSide(color: FrankColors.border)),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 10),
                  child: Row(
                    children: [
                      _LaneDot(lane: lane),
                      const SizedBox(width: 6),
                      Text(
                        lane.label,
                        style: const TextStyle(
                          color: FrankColors.ink,
                          fontSize: 12,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                      const Spacer(),
                      Text(
                        '${tasks.where((task) => task.lane.canonical == lane.canonical).length}',
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontSize: 11,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _MissionBoardSection extends StatelessWidget {
  const _MissionBoardSection({
    required this.tasks,
    required this.lanes,
    required this.profileFor,
    required this.onSelectTask,
    required this.focusNodeFor,
  });

  final List<TaskboardTask> tasks;
  final List<TaskboardLane> lanes;
  final TaskboardAgentLookup profileFor;
  final ValueChanged<String> onSelectTask;
  final FocusNode Function(String taskId) focusNodeFor;

  @override
  Widget build(BuildContext context) {
    final first = tasks.first;
    return Padding(
      padding: const EdgeInsets.only(bottom: 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: Row(
              children: [
                const Icon(
                  FrankIcons.chevronDown,
                  size: 15,
                  color: FrankColors.muted,
                ),
                const SizedBox(width: 5),
                Flexible(
                  child: Text(
                    first.projectName,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.ink,
                      fontSize: 12,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(
                    '/ ${first.missionName}',
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 11,
                    ),
                  ),
                ),
              ],
            ),
          ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (final lane in lanes)
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.only(right: 10),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        for (final task in tasks.where(
                          (task) => task.lane.canonical == lane.canonical,
                        ))
                          Padding(
                            padding: const EdgeInsets.only(bottom: 8),
                            child: _TaskCard(
                              task: task,
                              profile: profileFor(task.agentId),
                              onPressed: () => onSelectTask(task.id),
                              focusNode: focusNodeFor(task.id),
                            ),
                          ),
                        if (!tasks.any(
                          (task) => task.lane.canonical == lane.canonical,
                        ))
                          const _EmptyLane(),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _EmptyLane extends StatelessWidget {
  const _EmptyLane();

  @override
  Widget build(BuildContext context) {
    return const Padding(
      padding: EdgeInsets.symmetric(vertical: 18),
      child: Text(
        '—',
        style: TextStyle(color: FrankColors.muted, fontSize: 12),
      ),
    );
  }
}

class _TaskCard extends StatelessWidget {
  const _TaskCard({
    required this.task,
    required this.profile,
    required this.onPressed,
    required this.focusNode,
  });

  final TaskboardTask task;
  final TeamAgentProfile? profile;
  final VoidCallback onPressed;
  final FocusNode focusNode;

  @override
  Widget build(BuildContext context) {
    final activity = task.latestActivity;
    return FCard(
      key: ValueKey('taskboard-task-${task.id}'),
      child: FTappable.static(
        focusNode: focusNode,
        onPress: onPressed,
        semanticsLabel: task.title,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                task.id,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontFamily: FrankTypography.monoFontFamily,
                  fontSize: 10,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                task.title,
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                ),
              ),
              const SizedBox(height: 7),
              if (activity != null)
                Text(
                  activity.message,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: task.needsAttention
                        ? FrankColors.warningAmber
                        : FrankColors.muted,
                    fontSize: 11,
                  ),
                ),
              const SizedBox(height: 10),
              Row(
                children: [
                  _InitialsAvatar(
                    initials: profile?.initials ?? task.agentInitials,
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      profile?.name ?? task.agentName,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 11,
                      ),
                    ),
                  ),
                  if (activity != null)
                    Text(
                      activity.timeLabel,
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 11,
                      ),
                    ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _InitialsAvatar extends StatelessWidget {
  const _InitialsAvatar({required this.initials});

  final String initials;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: FrankColors.panelRaised,
        borderRadius: BorderRadius.circular(6),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 3),
        child: Text(
          initials,
          style: const TextStyle(
            color: FrankColors.ink,
            fontSize: 10,
            fontWeight: FontWeight.w500,
          ),
        ),
      ),
    );
  }
}

class _LaneDot extends StatelessWidget {
  const _LaneDot({required this.lane});

  final TaskboardLane lane;

  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(color: _laneColor(lane), shape: BoxShape.circle),
    child: const SizedBox(width: 7, height: 7),
  );
}

Color _laneColor(TaskboardLane lane) => switch (lane.canonical) {
  TaskboardLane.backlog => FrankColors.muted,
  TaskboardLane.ready => FrankColors.aubergineAccent,
  TaskboardLane.running => FrankColors.blue,
  TaskboardLane.blocked => FrankColors.warningAmber,
  TaskboardLane.review => FrankColors.warningAmber,
  TaskboardLane.done => FrankColors.statusSuccess,
  TaskboardLane.cancelled => FrankColors.failure,
  // canonical is exhaustive above; aliases are normalized by the getter.
  TaskboardLane.queued ||
  TaskboardLane.working ||
  TaskboardLane.attention => FrankColors.muted,
};
