import 'package:flutter/foundation.dart';

/// The fixed runtime lanes used by every shared Taskboard.
///
/// The three legacy aliases remain readable for fixture/golden compatibility;
/// new remote projections always use the seven canonical values below. The
/// board chooses [canonicalValues] for remote projections and
/// [legacyValues] only for the historical fixture presentation.
enum TaskboardLane {
  backlog,
  ready,
  running,
  blocked,
  review,
  done,
  cancelled,

  /// Deprecated four-lane fixture aliases. Use [canonical] at boundaries.
  @Deprecated('Use backlog')
  queued,
  @Deprecated('Use running')
  working,
  @Deprecated('Use blocked')
  attention,
}

extension TaskboardLaneContract on TaskboardLane {
  static const canonicalValues = <TaskboardLane>[
    TaskboardLane.backlog,
    TaskboardLane.ready,
    TaskboardLane.running,
    TaskboardLane.blocked,
    TaskboardLane.review,
    TaskboardLane.done,
    TaskboardLane.cancelled,
  ];

  static const legacyValues = <TaskboardLane>[
    TaskboardLane.queued,
    TaskboardLane.working,
    TaskboardLane.attention,
    TaskboardLane.done,
  ];

  static List<TaskboardLane> valuesFor({required bool fixture}) =>
      fixture ? legacyValues : canonicalValues;

  TaskboardLane get canonical => switch (this) {
    TaskboardLane.queued => TaskboardLane.backlog,
    TaskboardLane.working => TaskboardLane.running,
    TaskboardLane.attention => TaskboardLane.blocked,
    _ => this,
  };

  bool get isTerminal => canonical == TaskboardLane.done ||
      canonical == TaskboardLane.cancelled;
}

extension TaskboardLaneMetadata on TaskboardLane {
  String get label => switch (this) {
    TaskboardLane.backlog => 'Backlog',
    TaskboardLane.ready => 'Ready',
    TaskboardLane.running => 'Running',
    TaskboardLane.blocked => 'Blocked',
    TaskboardLane.review => 'Review',
    TaskboardLane.cancelled => 'Cancelled',
    TaskboardLane.queued => 'Queued',
    TaskboardLane.working => 'Working',
    TaskboardLane.attention => 'Needs attention',
    TaskboardLane.done => 'Done',
  };
}

enum TaskboardView { board, list }

enum TaskboardDecisionKind { approval, positiveInteger }

@immutable
class TaskboardDependency {
  const TaskboardDependency({required this.taskId, required this.label});

  final String taskId;
  final String label;
}

@immutable
class TaskboardActivity {
  const TaskboardActivity({
    required this.message,
    required this.actor,
    required this.timeLabel,
    this.kind = 'activity',
    this.artifactIds = const [],
  });

  final String message;
  final String actor;
  final String timeLabel;
  final String kind;
  final List<String> artifactIds;
}

@immutable
class TaskboardDecision {
  const TaskboardDecision({
    required this.kind,
    required this.prompt,
    required this.actionLabel,
    this.inputLabel,
  });

  final TaskboardDecisionKind kind;
  final String prompt;
  final String actionLabel;
  final String? inputLabel;

  bool get requiresInput => kind == TaskboardDecisionKind.positiveInteger;
}

@immutable
class TaskboardDecisionInput {
  const TaskboardDecisionInput.approve() : value = null;

  const TaskboardDecisionInput.positiveInteger(this.value);

  final int? value;
}

@immutable
class TaskboardTask {
  const TaskboardTask({
    required this.id,
    required this.projectId,
    required this.projectName,
    required this.missionId,
    required this.missionName,
    required this.agentId,
    required this.agentName,
    required this.agentInitials,
    required this.supervisorName,
    required this.title,
    required this.objective,
    required this.lane,
    required this.dependencies,
    required this.activities,
    this.requiredRoleId,
    this.requiredRoleName,
    this.claimedAt,
    this.claimSource,
    this.reviewerAgentId,
    this.reviewerName,
    this.decision,
  });

  final String id;
  final String projectId;
  final String projectName;
  final String missionId;
  final String missionName;
  final String agentId;
  final String agentName;
  final String agentInitials;
  final String supervisorName;
  final String title;
  final String objective;
  final TaskboardLane lane;
  final List<TaskboardDependency> dependencies;
  final List<TaskboardActivity> activities;
  final String? requiredRoleId;
  final String? requiredRoleName;
  final DateTime? claimedAt;
  final String? claimSource;
  final String? reviewerAgentId;
  final String? reviewerName;
  final TaskboardDecision? decision;

  bool get isClaimed => agentId.isNotEmpty && agentId != 'unassigned';

  TaskboardActivity? get latestActivity =>
      activities.isEmpty ? null : activities.first;

  bool get needsAttention =>
      lane.canonical == TaskboardLane.blocked ||
      lane.canonical == TaskboardLane.review ||
      lane == TaskboardLane.attention;

  TaskboardTask copyWith({
    String? agentId,
    String? agentName,
    String? agentInitials,
    TaskboardLane? lane,
    List<TaskboardActivity>? activities,
    TaskboardDecision? decision,
    bool clearDecision = false,
    DateTime? claimedAt,
    String? claimSource,
    bool clearClaim = false,
  }) {
    return TaskboardTask(
      id: id,
      projectId: projectId,
      projectName: projectName,
      missionId: missionId,
      missionName: missionName,
      agentId: agentId ?? this.agentId,
      agentName: agentName ?? this.agentName,
      agentInitials: agentInitials ?? this.agentInitials,
      supervisorName: supervisorName,
      title: title,
      objective: objective,
      lane: lane ?? this.lane,
      dependencies: dependencies,
      activities: activities ?? this.activities,
      requiredRoleId: requiredRoleId,
      requiredRoleName: requiredRoleName,
      claimedAt: clearClaim ? null : claimedAt ?? this.claimedAt,
      claimSource: clearClaim ? null : claimSource ?? this.claimSource,
      reviewerAgentId: reviewerAgentId,
      reviewerName: reviewerName,
      decision: clearDecision ? null : decision ?? this.decision,
    );
  }
}

@immutable
class TaskboardSnapshot {
  const TaskboardSnapshot({required this.tasks, this.isFixture = true});

  final List<TaskboardTask> tasks;
  final bool isFixture;

  List<String> get projectIds {
    final ids = <String>[];
    final seen = <String>{};
    for (final task in tasks) {
      if (seen.add(task.projectId)) ids.add(task.projectId);
    }
    return ids;
  }

  TaskboardTask? taskById(String? id) {
    if (id == null) return null;
    for (final task in tasks) {
      if (task.id == id) return task;
    }
    return null;
  }

  TaskboardSnapshot replaceTask(TaskboardTask replacement) => TaskboardSnapshot(
    tasks: [
      for (final task in tasks) task.id == replacement.id ? replacement : task,
    ],
    isFixture: isFixture,
  );
}
