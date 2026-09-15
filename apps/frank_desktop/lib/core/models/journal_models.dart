import 'package:flutter/foundation.dart';

enum JournalEntryKind {
  task,
  comment,
  approval,
  agentLifecycle,
  usage,
  toolchain,
  check,
  build,
  event,
}

extension JournalEntryKindMetadata on JournalEntryKind {
  String get wireName => switch (this) {
    JournalEntryKind.task => 'task',
    JournalEntryKind.comment => 'comment',
    JournalEntryKind.approval => 'approval',
    JournalEntryKind.agentLifecycle => 'agent_lifecycle',
    JournalEntryKind.usage => 'usage',
    JournalEntryKind.toolchain => 'toolchain',
    JournalEntryKind.check => 'check',
    JournalEntryKind.build => 'build',
    JournalEntryKind.event => 'event',
  };

  String get label => switch (this) {
    JournalEntryKind.task => 'Task',
    JournalEntryKind.comment => 'Comment',
    JournalEntryKind.approval => 'Approval',
    JournalEntryKind.agentLifecycle => 'Agent',
    JournalEntryKind.usage => 'Usage',
    JournalEntryKind.toolchain => 'Toolchain',
    JournalEntryKind.check => 'Check',
    JournalEntryKind.build => 'Build',
    JournalEntryKind.event => 'Event',
  };
}

enum JournalOutcome { info, pending, success, failure, blocked }

extension JournalOutcomeMetadata on JournalOutcome {
  String get wireName => name;

  String get label => switch (this) {
    JournalOutcome.info => 'Info',
    JournalOutcome.pending => 'Pending',
    JournalOutcome.success => 'Success',
    JournalOutcome.failure => 'Failure',
    JournalOutcome.blocked => 'Blocked',
  };
}

@immutable
class JournalEntry {
  const JournalEntry({
    required this.sequence,
    required this.occurredAt,
    required this.kind,
    required this.outcome,
    required this.summary,
    this.detail,
    this.projectId,
    this.missionId,
    this.taskId,
    this.agentId,
    this.checkRunId,
  });

  factory JournalEntry.fromJson(Map<String, dynamic> json) => JournalEntry(
    sequence: _int(json['sequence']),
    occurredAt: _date(json['occurred_at'] ?? json['occurredAt']),
    kind: _kind(json['kind']),
    outcome: _outcome(json['outcome']),
    summary: json['summary']?.toString() ?? 'Untitled event',
    detail: json['detail'],
    projectId: _string(json['project_id'] ?? json['projectId']),
    missionId: _string(json['mission_id'] ?? json['missionId']),
    taskId: _string(json['task_id'] ?? json['taskId']),
    agentId: _string(json['agent_id'] ?? json['agentId']),
    checkRunId: _string(json['check_run_id'] ?? json['checkRunId']),
  );

  final int sequence;
  final DateTime occurredAt;
  final JournalEntryKind kind;
  final JournalOutcome outcome;
  final String summary;
  final Object? detail;
  final String? projectId;
  final String? missionId;
  final String? taskId;
  final String? agentId;
  final String? checkRunId;
}

@immutable
class JournalPage {
  const JournalPage({required this.entries, this.nextBeforeSequence});

  factory JournalPage.fromJson(Map<String, dynamic> json) => JournalPage(
    entries: [
      for (final value
          in json['entries'] is List
              ? json['entries'] as List
              : const <Object?>[])
        if (value is Map)
          JournalEntry.fromJson(Map<String, dynamic>.from(value)),
    ],
    nextBeforeSequence: _nullableInt(
      json['next_before_sequence'] ?? json['nextBeforeSequence'],
    ),
  );

  final List<JournalEntry> entries;
  final int? nextBeforeSequence;
}

JournalEntryKind _kind(Object? value) => switch (value?.toString()) {
  'task' => JournalEntryKind.task,
  'comment' => JournalEntryKind.comment,
  'approval' => JournalEntryKind.approval,
  'agent_lifecycle' => JournalEntryKind.agentLifecycle,
  'usage' => JournalEntryKind.usage,
  'toolchain' => JournalEntryKind.toolchain,
  'check' => JournalEntryKind.check,
  'build' => JournalEntryKind.build,
  _ => JournalEntryKind.event,
};

JournalOutcome _outcome(Object? value) => switch (value?.toString()) {
  'pending' => JournalOutcome.pending,
  'success' => JournalOutcome.success,
  'failure' => JournalOutcome.failure,
  'blocked' => JournalOutcome.blocked,
  _ => JournalOutcome.info,
};

DateTime _date(Object? value) {
  final raw = value?.toString().trim() ?? '';
  final millis = int.tryParse(raw);
  if (millis != null) {
    return DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
  }
  return DateTime.tryParse(raw) ??
      DateTime.fromMillisecondsSinceEpoch(0, isUtc: true);
}

String? _string(Object? value) {
  final text = value?.toString().trim();
  return text == null || text.isEmpty ? null : text;
}

int _int(Object? value) => _nullableInt(value) ?? 0;

int? _nullableInt(Object? value) =>
    value is num ? value.toInt() : int.tryParse(value?.toString() ?? '');
