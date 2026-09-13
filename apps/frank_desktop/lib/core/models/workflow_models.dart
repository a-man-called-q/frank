import 'package:flutter/foundation.dart';

DateTime? _workflowDateTime(Object? value) {
  if (value is num) {
    return DateTime.fromMillisecondsSinceEpoch(value.toInt(), isUtc: true);
  }
  final raw = value?.toString();
  if (raw == null || raw.isEmpty) return null;
  final millis = int.tryParse(raw);
  if (millis != null) {
    return DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
  }
  return DateTime.tryParse(raw)?.toUtc();
}

/// The execution mode of a shared Taskboard. A board is a durable routing
/// surface; it is not a provider session and therefore keeps cards stable
/// while ownership changes.
enum WorkflowDispatchMode { auto, pull, manual }

extension WorkflowDispatchModeJson on WorkflowDispatchMode {
  String get wireName => name;

  static WorkflowDispatchMode fromWire(Object? value) => switch (value) {
    'auto' => WorkflowDispatchMode.auto,
    'manual' => WorkflowDispatchMode.manual,
    _ => WorkflowDispatchMode.pull,
  };
}

enum WorkflowWorkItemKind { task, humanQuestion, idea, note }

extension WorkflowWorkItemKindJson on WorkflowWorkItemKind {
  String get wireName => switch (this) {
    WorkflowWorkItemKind.task => 'task',
    WorkflowWorkItemKind.humanQuestion => 'human_question',
    WorkflowWorkItemKind.idea => 'idea',
    WorkflowWorkItemKind.note => 'note',
  };

  static WorkflowWorkItemKind fromWire(Object? value) => switch (value) {
    'human_question' || 'humanQuestion' => WorkflowWorkItemKind.humanQuestion,
    'idea' => WorkflowWorkItemKind.idea,
    'note' => WorkflowWorkItemKind.note,
    _ => WorkflowWorkItemKind.task,
  };
}

enum WorkflowOfferStatus { pending, accepted, declined, expired, cancelled }

extension WorkflowOfferStatusJson on WorkflowOfferStatus {
  static WorkflowOfferStatus fromWire(Object? value) => switch (value) {
    'accepted' => WorkflowOfferStatus.accepted,
    'declined' => WorkflowOfferStatus.declined,
    'expired' => WorkflowOfferStatus.expired,
    'cancelled' => WorkflowOfferStatus.cancelled,
    _ => WorkflowOfferStatus.pending,
  };
}

enum WorkflowHumanInputStatus { pending, answered, cancelled }

extension WorkflowHumanInputStatusJson on WorkflowHumanInputStatus {
  static WorkflowHumanInputStatus fromWire(Object? value) => switch (value) {
    'answered' => WorkflowHumanInputStatus.answered,
    'cancelled' => WorkflowHumanInputStatus.cancelled,
    _ => WorkflowHumanInputStatus.pending,
  };
}

enum WorkflowHumanInputKind { question, approval, clarification }

extension WorkflowHumanInputKindJson on WorkflowHumanInputKind {
  static WorkflowHumanInputKind fromWire(Object? value) => switch (value) {
    'approval' => WorkflowHumanInputKind.approval,
    'clarification' => WorkflowHumanInputKind.clarification,
    _ => WorkflowHumanInputKind.question,
  };
}

@immutable
class WorkflowTaskboard {
  const WorkflowTaskboard({
    required this.id,
    required this.name,
    this.projectId,
    this.workflowId,
    this.dispatchMode = WorkflowDispatchMode.pull,
    this.defaultRoleId,
    this.archived = false,
    this.createdAt,
    this.updatedAt,
  });

  final String id;
  final String name;
  final String? projectId;
  final String? workflowId;
  final WorkflowDispatchMode dispatchMode;
  final String? defaultRoleId;
  final bool archived;
  final DateTime? createdAt;
  final DateTime? updatedAt;

  factory WorkflowTaskboard.fromJson(Map<String, Object?> json) =>
      WorkflowTaskboard(
        id: json['id'] as String? ?? '',
        name: json['name'] as String? ?? 'Taskboard',
        projectId: json['project_id'] as String? ?? json['projectId'] as String?,
        workflowId:
            json['workflow_id'] as String? ?? json['workflowId'] as String?,
        dispatchMode: WorkflowDispatchModeJson.fromWire(
          json['dispatch_mode'] ?? json['dispatchMode'],
        ),
        defaultRoleId:
            json['default_role_id'] as String? ??
            json['defaultRoleId'] as String?,
        archived: json['archived'] as bool? ?? false,
        createdAt: _workflowDateTime(json['created_at']),
        updatedAt: _workflowDateTime(json['updated_at']),
      );
}

@immutable
class WorkflowOffer {
  const WorkflowOffer({
    required this.id,
    required this.taskId,
    required this.taskboardId,
    required this.agentId,
    this.roleId,
    this.status = WorkflowOfferStatus.pending,
    this.attempt = 0,
    this.createdAt,
    this.expiresAt,
    this.respondedAt,
  });

  final String id;
  final String taskId;
  final String taskboardId;
  final String agentId;
  final String? roleId;
  final WorkflowOfferStatus status;
  final int attempt;
  final DateTime? createdAt;
  final DateTime? expiresAt;
  final DateTime? respondedAt;

  bool get isOpen => status == WorkflowOfferStatus.pending;

  factory WorkflowOffer.fromJson(Map<String, Object?> json) => WorkflowOffer(
    id: json['id'] as String? ?? '',
    taskId: json['task_id'] as String? ?? json['taskId'] as String? ?? '',
    taskboardId:
        json['taskboard_id'] as String? ?? json['taskboardId'] as String? ?? '',
    agentId: json['agent_id'] as String? ?? json['agentId'] as String? ?? '',
    roleId: json['role_id'] as String? ?? json['roleId'] as String?,
    status: WorkflowOfferStatusJson.fromWire(json['status']),
    attempt: (json['attempt'] as num?)?.toInt() ?? 0,
    createdAt: _workflowDateTime(json['created_at']),
    expiresAt: _workflowDateTime(json['expires_at']),
    respondedAt: _workflowDateTime(json['responded_at']),
  );
}

@immutable
class WorkflowHumanInput {
  const WorkflowHumanInput({
    required this.id,
    required this.taskId,
    required this.prompt,
    this.missionId,
    this.requestedBy,
    this.kind = WorkflowHumanInputKind.question,
    this.status = WorkflowHumanInputStatus.pending,
    this.answer,
    this.answeredBy,
    this.createdAt,
    this.updatedAt,
  });

  final String id;
  final String taskId;
  final String? missionId;
  final String? requestedBy;
  final WorkflowHumanInputKind kind;
  final String prompt;
  final WorkflowHumanInputStatus status;
  final String? answer;
  final String? answeredBy;
  final DateTime? createdAt;
  final DateTime? updatedAt;

  bool get isPending => status == WorkflowHumanInputStatus.pending;

  factory WorkflowHumanInput.fromJson(Map<String, Object?> json) =>
      WorkflowHumanInput(
        id: json['id'] as String? ?? '',
        taskId: json['task_id'] as String? ?? json['taskId'] as String? ?? '',
        missionId:
            json['mission_id'] as String? ?? json['missionId'] as String?,
        requestedBy:
            json['requested_by'] as String? ?? json['requestedBy'] as String?,
        kind: WorkflowHumanInputKindJson.fromWire(json['kind']),
        prompt: json['prompt'] as String? ?? '',
        status: WorkflowHumanInputStatusJson.fromWire(json['status']),
        answer: json['answer'] as String?,
        answeredBy:
            json['answered_by'] as String? ?? json['answeredBy'] as String?,
        createdAt: _workflowDateTime(json['created_at']),
        updatedAt: _workflowDateTime(json['updated_at']),
      );
}

enum WorkflowDrainStatus { running, draining, paused }

extension WorkflowDrainStatusJson on WorkflowDrainStatus {
  static WorkflowDrainStatus fromWire(Object? value) => switch (value) {
    'draining' => WorkflowDrainStatus.draining,
    'paused' => WorkflowDrainStatus.paused,
    _ => WorkflowDrainStatus.running,
  };
}

@immutable
class WorkflowRuntime {
  const WorkflowRuntime({
    this.activeRevision = 0,
    this.status = WorkflowDrainStatus.running,
    this.drainRequestedRevision,
    this.pausedRevision,
    this.pendingRelocationCount = 0,
  });

  final int activeRevision;
  final WorkflowDrainStatus status;
  final int? drainRequestedRevision;
  final int? pausedRevision;
  final int pendingRelocationCount;

  bool get isDraining => status == WorkflowDrainStatus.draining;
  bool get isPaused => status == WorkflowDrainStatus.paused;

  factory WorkflowRuntime.fromJson(Map<String, Object?> json) => WorkflowRuntime(
    activeRevision: (json['active_revision'] as num?)?.toInt() ??
        (json['activeRevision'] as num?)?.toInt() ??
        0,
    status: WorkflowDrainStatusJson.fromWire(json['status']),
    drainRequestedRevision:
        (json['drain_requested_revision'] as num?)?.toInt() ??
        (json['drainRequestedRevision'] as num?)?.toInt(),
    pausedRevision: (json['paused_revision'] as num?)?.toInt() ??
        (json['pausedRevision'] as num?)?.toInt(),
    pendingRelocationCount:
        (json['pending_relocation_count'] as num?)?.toInt() ??
        (json['pendingRelocationCount'] as num?)?.toInt() ??
        0,
  );
}

@immutable
class WorkflowProjection {
  const WorkflowProjection({
    this.boards = const [],
    this.offers = const [],
    this.humanInputs = const [],
    this.runtime = const WorkflowRuntime(),
  });

  final List<WorkflowTaskboard> boards;
  final List<WorkflowOffer> offers;
  final List<WorkflowHumanInput> humanInputs;
  final WorkflowRuntime runtime;

  WorkflowTaskboard? boardById(String? id) {
    if (id == null) return null;
    for (final board in boards) {
      if (board.id == id) return board;
    }
    return null;
  }

  List<WorkflowOffer> offersForTask(String taskId) => [
    for (final offer in offers)
      if (offer.taskId == taskId) offer,
  ];

  List<WorkflowHumanInput> inputsForTask(String taskId) => [
    for (final input in humanInputs)
      if (input.taskId == taskId) input,
  ];

  factory WorkflowProjection.fromSnapshot(Map<String, Object?> json) {
    List<Map<String, Object?>> maps(Object? value) => value is List
        ? value.whereType<Map>().map(Map<String, Object?>.from).toList()
        : const [];
    final runtime = json['organization_runtime'] ?? json['organizationRuntime'];
    return WorkflowProjection(
      boards: maps(json['taskboards'])
          .map(WorkflowTaskboard.fromJson)
          .toList(growable: false),
      offers: maps(json['work_offers'])
          .map(WorkflowOffer.fromJson)
          .toList(growable: false),
      humanInputs: maps(json['human_inputs'])
          .map(WorkflowHumanInput.fromJson)
          .toList(growable: false),
      runtime: runtime is Map
          ? WorkflowRuntime.fromJson(Map<String, Object?>.from(runtime))
          : const WorkflowRuntime(),
    );
  }
}
