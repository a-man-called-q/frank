part of 'taskboard_surface.dart';

class _TaskboardInspector extends StatelessWidget {
  const _TaskboardInspector({
    required this.task,
    required this.profile,
    required this.profiles,
    required this.compact,
    required this.controller,
    required this.commentController,
    required this.decisionStatus,
    required this.decisionError,
    required this.mutationStatus,
    required this.mutationError,
    required this.onClose,
    required this.onSubmit,
    required this.onClaim,
    required this.onRelease,
    required this.onComment,
  });

  final TaskboardTask task;
  final TeamAgentProfile? profile;
  final List<TeamAgentProfile> profiles;
  final bool compact;
  final TextEditingController controller;
  final TextEditingController commentController;
  final TaskboardDecisionStatus decisionStatus;
  final String? decisionError;
  final TaskboardMutationStatus mutationStatus;
  final String? mutationError;
  final VoidCallback onClose;
  final ValueChanged<TaskboardDecisionInput> onSubmit;
  final ValueChanged<String> onClaim;
  final VoidCallback onRelease;
  final ValueChanged<String> onComment;

  @override
  Widget build(BuildContext context) {
    final metrics = OfficeLayoutMetricsScope.maybeOf(context);
    final gutter = metrics?.gutter ?? FrankUiTokens.inset;
    final inspector = Material(
      color: FrankColors.panel,
      child: SafeArea(
        top: false,
        bottom: false,
        child: SingleChildScrollView(
          padding: EdgeInsets.fromLTRB(gutter, 18, gutter, gutter),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 460),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        task.id,
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontFamily: FrankTypography.monoFontFamily,
                          fontSize: 10,
                        ),
                      ),
                    ),
                    IconButton(
                      key: const ValueKey('taskboard-close-detail'),
                      onPressed: onClose,
                      tooltip: compact
                          ? 'Back to taskboard'
                          : 'Close task details',
                      icon: Icon(
                        compact ? FrankIcons.back : FrankIcons.close,
                        size: FrankUiTokens.iconSize,
                      ),
                      color: FrankColors.muted,
                      padding: EdgeInsets.zero,
                      constraints: const BoxConstraints.tightFor(
                        width: FrankUiTokens.controlHeight,
                        height: FrankUiTokens.controlHeight,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                Text(
                  task.title,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontSize: 19,
                    height: 1.3,
                    fontWeight: FontWeight.w500,
                  ),
                ),
                const SizedBox(height: 5),
                Text(
                  '${task.projectName}  /  ${task.missionName}',
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 11,
                  ),
                ),
                const SizedBox(height: 18),
                _TaskStatusLine(lane: task.lane),
                const Divider(height: 25, color: FrankColors.border),
                _DetailMetadata(task: task, profile: profile, compact: compact),
                const SizedBox(height: 20),
                _ClaimPanel(
                  task: task,
                  profiles: profiles,
                  status: mutationStatus,
                  error: mutationError,
                  onClaim: onClaim,
                  onRelease: onRelease,
                ),
                if (task.decision != null)
                  _DecisionPanel(
                    task: task,
                    controller: controller,
                    status: decisionStatus,
                    error: decisionError,
                    onSubmit: onSubmit,
                  ),
                const _DetailSectionTitle('Brief'),
                Text(
                  task.objective,
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 12,
                    height: 1.55,
                  ),
                ),
                const _DetailSectionTitle('Latest activity'),
                for (final activity in task.activities.take(4))
                  Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: _ActivityLine(activity: activity),
                  ),
                const SizedBox(height: 8),
                _CommentComposer(
                  controller: commentController,
                  submitting:
                      mutationStatus == TaskboardMutationStatus.submitting,
                  onSubmit: onComment,
                ),
              ],
            ),
          ),
        ),
      ),
    );
    final decoratedInspector = DecoratedBox(
      decoration: const BoxDecoration(
        border: Border(left: BorderSide(color: FrankColors.border)),
      ),
      child: inspector,
    );
    if (compact) {
      return decoratedInspector;
    }
    return decoratedInspector;
  }
}

class _ClaimPanel extends StatefulWidget {
  const _ClaimPanel({
    required this.task,
    required this.profiles,
    required this.status,
    required this.error,
    required this.onClaim,
    required this.onRelease,
  });

  final TaskboardTask task;
  final List<TeamAgentProfile> profiles;
  final TaskboardMutationStatus status;
  final String? error;
  final ValueChanged<String> onClaim;
  final VoidCallback onRelease;

  @override
  State<_ClaimPanel> createState() => _ClaimPanelState();
}

class _ClaimPanelState extends State<_ClaimPanel> {
  String? _selectedAgentId;

  List<TeamAgentProfile> get _eligibleProfiles => widget.profiles
      .where(
        (profile) =>
            (profile.status == TeamAgentStatus.available ||
                profile.status == TeamAgentStatus.idle ||
                profile.status == TeamAgentStatus.offline) &&
            (widget.task.requiredRoleId == null ||
                profile.roleId == widget.task.requiredRoleId),
      )
      .toList(growable: false);

  @override
  void didUpdateWidget(covariant _ClaimPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    final eligible = _eligibleProfiles;
    if (_selectedAgentId != null &&
        eligible.any((profile) => profile.employeeId == _selectedAgentId)) {
      return;
    }
    _selectedAgentId = eligible.firstOrNull?.employeeId;
  }

  @override
  Widget build(BuildContext context) {
    final eligible = _eligibleProfiles;
    final selectedAgentId =
        eligible.any((profile) => profile.employeeId == _selectedAgentId)
        ? _selectedAgentId
        : eligible.firstOrNull?.employeeId;
    final mutating = widget.status == TaskboardMutationStatus.submitting;
    final assigned = widget.task.isClaimed;
    final assignedProfile = widget.profiles
        .where((profile) => profile.employeeId == widget.task.agentId)
        .firstOrNull;
    return Padding(
      padding: const EdgeInsets.only(bottom: 19),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: FrankColors.panelRaised.withValues(alpha: .55),
          border: Border.all(color: FrankColors.border),
          borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        ),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  const Icon(
                    FrankIcons.taskboard,
                    size: FrankUiTokens.iconSize,
                    color: FrankColors.aubergineAccent,
                  ),
                  const SizedBox(width: 7),
                  const Expanded(
                    child: Text(
                      'Role queue',
                      style: TextStyle(
                        color: FrankColors.ink,
                        fontSize: 12,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ),
                  if (assigned)
                    TextButton(
                      key: ValueKey('taskboard-release-${widget.task.id}'),
                      onPressed: mutating ? null : widget.onRelease,
                      child: const Text('Release'),
                    ),
                ],
              ),
              const SizedBox(height: 7),
              Text(
                assigned
                    ? 'Claimed by ${assignedProfile?.name ?? widget.task.agentName}'
                    : widget.task.requiredRoleId == null
                    ? 'Waiting for an eligible role member.'
                    : 'Waiting for an idle or offline member in the required role.',
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: 11,
                  height: 1.4,
                ),
              ),
              if (!assigned && eligible.isNotEmpty) ...[
                const SizedBox(height: 10),
                FrankDesktopSelectField<String?>(
                  key: ValueKey('taskboard-claim-agent-${widget.task.id}'),
                  fieldKey: ValueKey(
                    'taskboard-claim-agent-trigger-${widget.task.id}',
                  ),
                  value: selectedAgentId,
                  label: 'Eligible member',
                  hint: 'Choose a member',
                  enabled: !mutating,
                  options: [
                    for (final profile in eligible)
                      FrankDesktopSelectOption<String?>(
                        value: profile.employeeId,
                        label: profile.name,
                      ),
                  ],
                  onChanged: (value) =>
                      setState(() => _selectedAgentId = value),
                ),
                const SizedBox(height: 9),
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.icon(
                    key: ValueKey('taskboard-claim-${widget.task.id}'),
                    onPressed: mutating || selectedAgentId == null
                        ? null
                        : () => widget.onClaim(selectedAgentId),
                    style: FilledButton.styleFrom(
                      minimumSize: const Size(0, FrankUiTokens.controlHeight),
                      padding: const EdgeInsets.symmetric(horizontal: 11),
                      backgroundColor: FrankColors.aubergine,
                      foregroundColor: Colors.white,
                    ),
                    icon: mutating
                        ? const SizedBox(
                            width: 14,
                            height: 14,
                            child: CircularProgressIndicator(
                              strokeWidth: 2,
                              color: Colors.white,
                            ),
                          )
                        : const Icon(
                            FrankIcons.check,
                            size: FrankUiTokens.iconSize,
                          ),
                    label: const Text('Claim task'),
                  ),
                ),
              ],
              if (!assigned && eligible.isEmpty)
                const Padding(
                  padding: EdgeInsets.only(top: 8),
                  child: Text(
                    'No eligible idle or offline member is available yet.',
                    style: TextStyle(color: FrankColors.muted, fontSize: 11),
                  ),
                ),
              if (widget.error != null)
                Padding(
                  padding: const EdgeInsets.only(top: 8),
                  child: Text(
                    widget.error!,
                    key: const ValueKey('taskboard-mutation-error'),
                    style: const TextStyle(
                      color: FrankColors.warningAmber,
                      fontSize: 11,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _CommentComposer extends StatelessWidget {
  const _CommentComposer({
    required this.controller,
    required this.submitting,
    required this.onSubmit,
  });

  final TextEditingController controller;
  final bool submitting;
  final ValueChanged<String> onSubmit;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _DetailSectionTitle('Add to task feed'),
        TextField(
          key: const ValueKey('taskboard-comment-input'),
          controller: controller,
          enabled: !submitting,
          minLines: 2,
          maxLines: 5,
          style: const TextStyle(color: FrankColors.ink, fontSize: 12),
          decoration: InputDecoration(
            hintText: 'Document the handoff for the next role member…',
            hintStyle: const TextStyle(color: FrankColors.muted, fontSize: 12),
            filled: true,
            fillColor: FrankColors.panelRaised.withValues(alpha: .45),
            contentPadding: const EdgeInsets.all(10),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
          ),
        ),
        const SizedBox(height: 8),
        Align(
          alignment: Alignment.centerLeft,
          child: OutlinedButton.icon(
            key: const ValueKey('taskboard-comment-submit'),
            onPressed: submitting
                ? null
                : () {
                    final body = controller.text.trim();
                    if (body.isEmpty) return;
                    onSubmit(body);
                    controller.clear();
                  },
            icon: const Icon(FrankIcons.send, size: FrankUiTokens.iconSize),
            label: const Text('Post note'),
          ),
        ),
      ],
    );
  }
}

class _TaskStatusLine extends StatelessWidget {
  const _TaskStatusLine({required this.lane});

  final TaskboardLane lane;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        _LaneDot(lane: lane),
        const SizedBox(width: 8),
        Text(
          lane.label,
          style: TextStyle(color: _laneColor(lane), fontSize: 12),
        ),
      ],
    );
  }
}

class _DetailMetadata extends StatelessWidget {
  const _DetailMetadata({
    required this.task,
    required this.profile,
    required this.compact,
  });

  final TaskboardTask task;
  final TeamAgentProfile? profile;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final rows = <(String, String)>[
      ('Assigned to', profile?.name ?? task.agentName),
      if (task.requiredRoleId != null)
        ('Required role', task.requiredRoleName ?? task.requiredRoleId!),
      ('Supervisor', task.supervisorName),
      (
        'Depends on',
        task.dependencies.isEmpty
            ? 'None'
            : task.dependencies
                  .map((dependency) => dependency.label)
                  .join(', '),
      ),
    ];
    return Column(
      children: [
        for (final row in rows)
          Padding(
            padding: const EdgeInsets.only(bottom: 11),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SizedBox(
                  width: compact ? 84 : 88,
                  child: Text(
                    row.$1,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 11,
                    ),
                  ),
                ),
                Expanded(
                  child: Text(
                    row.$2,
                    style: const TextStyle(
                      color: FrankColors.ink,
                      fontSize: 12,
                    ),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _DecisionPanel extends StatelessWidget {
  const _DecisionPanel({
    required this.task,
    required this.controller,
    required this.status,
    required this.error,
    required this.onSubmit,
  });

  final TaskboardTask task;
  final TextEditingController controller;
  final TaskboardDecisionStatus status;
  final String? error;
  final ValueChanged<TaskboardDecisionInput> onSubmit;

  @override
  Widget build(BuildContext context) {
    final decision = task.decision!;
    final submitting = status == TaskboardDecisionStatus.submitting;
    return Padding(
      padding: const EdgeInsets.only(bottom: 19),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: FrankColors.warningAmberSoft,
          border: Border.all(
            color: FrankColors.warningAmber.withValues(alpha: .5),
          ),
          borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        ),
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Row(
                children: [
                  Icon(
                    FrankIcons.circleAlert,
                    size: FrankUiTokens.iconSize,
                    color: FrankColors.warningAmber,
                  ),
                  SizedBox(width: 7),
                  Text(
                    'Your decision',
                    style: TextStyle(
                      color: FrankColors.warningAmber,
                      fontSize: 12,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Text(
                decision.prompt,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 12,
                  height: 1.45,
                ),
              ),
              if (decision.requiresInput) ...[
                const SizedBox(height: 12),
                Text(
                  decision.inputLabel ?? 'Value',
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 11,
                  ),
                ),
                const SizedBox(height: 6),
                TextField(
                  key: ValueKey('taskboard-input-${task.id}'),
                  controller: controller,
                  enabled: !submitting,
                  keyboardType: TextInputType.number,
                  inputFormatters: [FilteringTextInputFormatter.digitsOnly],
                  style: const TextStyle(color: FrankColors.ink, fontSize: 12),
                  decoration: InputDecoration(
                    isDense: true,
                    filled: true,
                    fillColor: FrankColors.panel,
                    hintText: 'e.g. 5000',
                    hintStyle: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 12,
                    ),
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 10,
                      vertical: 9,
                    ),
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(
                        FrankUiTokens.controlRadius,
                      ),
                      borderSide: const BorderSide(color: FrankColors.border),
                    ),
                    enabledBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(
                        FrankUiTokens.controlRadius,
                      ),
                      borderSide: const BorderSide(color: FrankColors.border),
                    ),
                  ),
                ),
              ],
              if (error != null) ...[
                const SizedBox(height: 8),
                Text(
                  error!,
                  key: const ValueKey('taskboard-decision-error'),
                  style: const TextStyle(
                    color: FrankColors.warningAmber,
                    fontSize: 11,
                  ),
                ),
              ],
              const SizedBox(height: 12),
              Align(
                alignment: Alignment.centerLeft,
                child: FilledButton.icon(
                  key: ValueKey('taskboard-decision-${task.id}'),
                  onPressed: submitting
                      ? null
                      : () {
                          final value = decision.requiresInput
                              ? int.tryParse(controller.text.trim())
                              : null;
                          onSubmit(
                            decision.requiresInput
                                ? TaskboardDecisionInput.positiveInteger(
                                    value ?? 0,
                                  )
                                : const TaskboardDecisionInput.approve(),
                          );
                        },
                  style: FilledButton.styleFrom(
                    minimumSize: const Size(0, FrankUiTokens.controlHeight),
                    padding: const EdgeInsets.symmetric(horizontal: 11),
                    backgroundColor: FrankColors.aubergine,
                    foregroundColor: Colors.white,
                    disabledBackgroundColor: FrankColors.aubergine.withValues(
                      alpha: .55,
                    ),
                  ),
                  icon: submitting
                      ? const SizedBox(
                          width: 14,
                          height: 14,
                          child: CircularProgressIndicator(
                            strokeWidth: 2,
                            color: Colors.white,
                          ),
                        )
                      : const Icon(
                          FrankIcons.check,
                          size: FrankUiTokens.iconSize,
                        ),
                  label: Text(
                    error == null ? decision.actionLabel : 'Try again',
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _DetailSectionTitle extends StatelessWidget {
  const _DetailSectionTitle(this.title);

  final String title;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8, top: 1),
      child: Text(
        title,
        style: const TextStyle(
          color: FrankColors.ink,
          fontSize: 12,
          fontWeight: FontWeight.w500,
        ),
      ),
    );
  }
}

class _ActivityLine extends StatelessWidget {
  const _ActivityLine({required this.activity});

  final TaskboardActivity activity;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Padding(
          padding: EdgeInsets.only(top: 4),
          child: Icon(FrankIcons.activity, size: 13, color: FrankColors.muted),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                activity.message,
                style: const TextStyle(color: FrankColors.ink, fontSize: 12),
              ),
              const SizedBox(height: 2),
              Text(
                '${activity.actor} · ${activity.timeLabel}',
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
