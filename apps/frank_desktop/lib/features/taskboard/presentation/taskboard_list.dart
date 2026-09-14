part of 'taskboard_surface.dart';

class _TaskboardList extends StatelessWidget {
  const _TaskboardList({
    required this.tasks,
    required this.workspaceHasTasks,
    required this.profileFor,
    required this.onSelectTask,
    required this.focusNodeFor,
    required this.hasActiveFilters,
    required this.onClearFilters,
  });

  final List<TaskboardTask> tasks;
  final bool workspaceHasTasks;
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
    return SingleChildScrollView(
      key: const ValueKey('taskboard-list-scroll'),
      padding: EdgeInsets.fromLTRB(gutter, 0, gutter, gutter),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final missionId in missionIds)
            _MissionListSection(
              tasks: tasks
                  .where((task) => task.missionId == missionId)
                  .toList(growable: false),
              profileFor: profileFor,
              onSelectTask: onSelectTask,
              focusNodeFor: focusNodeFor,
            ),
        ],
      ),
    );
  }
}

class _MissionListSection extends StatelessWidget {
  const _MissionListSection({
    required this.tasks,
    required this.profileFor,
    required this.onSelectTask,
    required this.focusNodeFor,
  });

  final List<TaskboardTask> tasks;
  final TaskboardAgentLookup profileFor;
  final ValueChanged<String> onSelectTask;
  final FocusNode Function(String taskId) focusNodeFor;

  @override
  Widget build(BuildContext context) {
    final first = tasks.first;
    return Padding(
      padding: const EdgeInsets.only(bottom: 17),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
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
          for (final task in tasks)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: _TaskListRow(
                task: task,
                profile: profileFor(task.agentId),
                onPressed: () => onSelectTask(task.id),
                focusNode: focusNodeFor(task.id),
              ),
            ),
        ],
      ),
    );
  }
}

class _TaskListRow extends StatelessWidget {
  const _TaskListRow({
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
    final textScaleFactor = MediaQuery.textScalerOf(context).scale(1);
    final stackedLayout = textScaleFactor > 1.25;
    return FCard(
      key: ValueKey('taskboard-list-task-${task.id}'),
      child: FTappable.static(
        focusNode: focusNode,
        onPress: onPressed,
        semanticsLabel: task.title,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 10),
          child: stackedLayout
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Row(
                      children: [
                        SizedBox(
                          width: 64,
                          child: Text(
                            task.id,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(
                              color: FrankColors.muted,
                              fontFamily: FrankTypography.monoFontFamily,
                              fontSize: 10,
                            ),
                          ),
                        ),
                        _LaneDot(lane: task.lane),
                        const SizedBox(width: 7),
                        SizedBox(
                          width: 130,
                          child: Text(
                            task.lane.label,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              color: _laneColor(task.lane),
                              fontSize: 11,
                            ),
                          ),
                        ),
                        const Spacer(),
                        _InitialsAvatar(
                          initials: profile?.initials ?? task.agentInitials,
                        ),
                        const SizedBox(width: 6),
                        Flexible(
                          child: Text(
                            activity?.timeLabel ?? '',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            textAlign: TextAlign.end,
                            style: const TextStyle(
                              color: FrankColors.muted,
                              fontSize: 11,
                            ),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 4),
                    Text(
                      task.title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: FrankColors.ink,
                        fontSize: 12,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
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
                  ],
                )
              : Row(
                  children: [
                    SizedBox(
                      width: 53,
                      child: Text(
                        task.id,
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontFamily: FrankTypography.monoFontFamily,
                          fontSize: 10,
                        ),
                      ),
                    ),
                    _LaneDot(lane: task.lane),
                    const SizedBox(width: 7),
                    SizedBox(
                      width: 78,
                      child: Text(
                        task.lane.label,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          color: _laneColor(task.lane),
                          fontSize: 11,
                        ),
                      ),
                    ),
                    const SizedBox(width: 7),
                    Expanded(
                      child: Text(
                        task.title,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          color: FrankColors.ink,
                          fontSize: 12,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                    if (activity != null) ...[
                      const SizedBox(width: 10),
                      Flexible(
                        child: Text(
                          activity.message,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            color: task.needsAttention
                                ? FrankColors.warningAmber
                                : FrankColors.muted,
                            fontSize: 11,
                          ),
                        ),
                      ),
                      const SizedBox(width: 10),
                    ],
                    _InitialsAvatar(
                      initials: profile?.initials ?? task.agentInitials,
                    ),
                    const SizedBox(width: 6),
                    Text(
                      activity?.timeLabel ?? '',
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 11,
                      ),
                    ),
                  ],
                ),
        ),
      ),
    );
  }
}

class _TaskboardEmpty extends StatelessWidget {
  const _TaskboardEmpty({
    required this.workspaceHasTasks,
    required this.hasActiveFilters,
    required this.onClearFilters,
  });

  final bool workspaceHasTasks;
  final bool hasActiveFilters;
  final VoidCallback onClearFilters;

  @override
  Widget build(BuildContext context) {
    if (!workspaceHasTasks) {
      return const FrankEmptyState(
        title: 'No tasks in this workspace',
        message: 'This workspace has no tasks yet.',
        icon: FrankIcons.taskList,
      );
    }
    return FrankEmptyState(
      title: 'No tasks match these filters',
      message: 'Clear filters to see all tasks in this workspace.',
      icon: FrankIcons.filter,
      action: FButton(
        key: const ValueKey('taskboard-clear-filters'),
        onPress: hasActiveFilters ? onClearFilters : null,
        variant: FButtonVariant.outline,
        prefix: const Icon(FrankIcons.close, size: FrankUiTokens.iconSize),
        child: const Text('Clear filters'),
      ),
    );
  }
}
