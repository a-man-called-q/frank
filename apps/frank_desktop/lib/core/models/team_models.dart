import 'dart:collection';

import 'package:flutter/widgets.dart';

import '../../app/icons.dart';

/// The presentation status shown by the Team roster.
///
/// This is deliberately separate from [OfficeEmployee.status]. The current
/// workspace DTO is a remote-facing compatibility type, while the Team
/// surface needs a small, typed vocabulary for chips, icons, and filtering.
enum TeamAgentStatus { available, working, idle, reviewing, blocked, offline }

extension TeamAgentStatusMetadata on TeamAgentStatus {
  String get label => switch (this) {
    TeamAgentStatus.available => 'Available',
    TeamAgentStatus.working => 'Working',
    TeamAgentStatus.idle => 'Idle',
    TeamAgentStatus.reviewing => 'Reviewing',
    TeamAgentStatus.blocked => 'Blocked',
    TeamAgentStatus.offline => 'Offline',
  };

  Color get color => switch (this) {
    TeamAgentStatus.available => const Color(0xFF77C69B),
    TeamAgentStatus.working => const Color(0xFF82B7E8),
    TeamAgentStatus.idle => const Color(0xFF9A9D9B),
    TeamAgentStatus.reviewing => const Color(0xFFE2A84B),
    TeamAgentStatus.blocked => const Color(0xFFE47B7B),
    TeamAgentStatus.offline => const Color(0xFF65686C),
  };

  IconData get icon => switch (this) {
    TeamAgentStatus.available => FrankIcons.checkCircleOutline,
    TeamAgentStatus.working => FrankIcons.boltOutlined,
    TeamAgentStatus.idle => FrankIcons.pauseCircleOutline,
    TeamAgentStatus.reviewing => FrankIcons.rateReviewOutlined,
    TeamAgentStatus.blocked => FrankIcons.errorOutline,
    TeamAgentStatus.offline => FrankIcons.cloudOffOutlined,
  };
}

/// A deterministic, presentation-only profile for the first Team mockup.
///
/// The model intentionally lives outside [OfficeWorkspace] and the gateway
/// contract. When the remote agent DTO arrives, the fixture can be replaced
/// without changing the Team surface's rendering API.
class TeamAgentProfile {
  static const Object _unset = Object();

  const TeamAgentProfile({
    required this.employeeId,
    this.roleId,
    this.roleRevision = 0,
    required this.name,
    required this.role,
    required this.initials,
    required this.status,
    required this.accentColor,
    required this.imageAsset,
    required this.tagline,
    required this.currentProject,
    required this.assignment,
    required this.model,
    this.modelOverride,
    this.modelSource = 'role',
    this.roleDefaultModel,
    this.pendingModelOverride,
    this.pendingModelChange = false,
    this.revision = 0,
    this.archived = false,
    required this.capabilities,
    required this.activity,
  });

  final String employeeId;

  /// Stable role identity from the remote Team projection. A null value is
  /// retained for legacy/fixture agents that predate role-backed members.
  final String? roleId;

  /// The role template revision materialized into this agent at its last idle
  /// boundary. It lets the UI explain why a working agent may lag a template
  /// edit without implying that a live provider was reconfigured mid-task.
  final int roleRevision;
  final String name;
  final String role;

  final String initials;
  final TeamAgentStatus status;
  final int accentColor;
  final String imageAsset;
  final String tagline;
  final String currentProject;
  final String assignment;
  final String model;
  final String? modelOverride;
  final String modelSource;
  final String? roleDefaultModel;
  final String? pendingModelOverride;
  final bool pendingModelChange;
  final int revision;
  final bool archived;
  final List<TeamCapability> capabilities;
  final List<TeamActivityEvent> activity;

  TeamAgentProfile copyWith({
    String? model,
    String? modelOverride,
    bool clearModelOverride = false,
    String? modelSource,
    String? roleDefaultModel,
    Object? pendingModelOverride = _unset,
    bool? pendingModelChange,
    int? revision,
    bool? archived,
  }) {
    final nextOverride = clearModelOverride
        ? null
        : (modelOverride ?? this.modelOverride);
    final nextPendingModelOverride = identical(pendingModelOverride, _unset)
        ? this.pendingModelOverride
        : pendingModelOverride as String?;
    return TeamAgentProfile(
      employeeId: employeeId,
      roleId: roleId,
      roleRevision: roleRevision,
      name: name,
      role: role,
      initials: initials,
      status: status,
      accentColor: accentColor,
      imageAsset: imageAsset,
      tagline: tagline,
      currentProject: currentProject,
      assignment: assignment,
      model: model ?? this.model,
      modelOverride: nextOverride,
      modelSource: modelSource ?? this.modelSource,
      roleDefaultModel: roleDefaultModel ?? this.roleDefaultModel,
      pendingModelOverride: nextPendingModelOverride,
      pendingModelChange: pendingModelChange ?? this.pendingModelChange,
      revision: revision ?? this.revision,
      archived: archived ?? this.archived,
      capabilities: capabilities,
      activity: activity,
    );
  }

  Color get accent => Color(accentColor);

  /// Keeps an incomplete model configuration honest in compact surfaces.
  /// A missing model is not silently replaced by a default.
  String get modelSummary {
    final modelLabel = model.trim();
    if (modelLabel.isEmpty || modelLabel.toLowerCase() == 'unconfigured') {
      return 'Unconfigured';
    }
    final nativeOpenAi = modelLabel.startsWith('openai/');
    return nativeOpenAi
        ? 'OpenAI · ${modelLabel.substring('openai/'.length)}'
        : 'OpenRouter · $modelLabel';
  }
}

class TeamRoleSummary {
  const TeamRoleSummary({
    required this.id,
    required this.name,
    this.defaultModel,
    this.instructions = '',
    this.policy = const <String, Object?>{},
    this.budget = const <String, Object?>{},
    this.revision = 0,
    this.archived = false,
  });

  final String id;
  final String name;
  final String? defaultModel;
  final String instructions;
  final Map<String, Object?> policy;
  final Map<String, Object?> budget;
  final int revision;
  final bool archived;
}

class TeamCapability {
  const TeamCapability({required this.label, required this.icon});

  final String label;
  final IconData icon;
}

class TeamActivityEvent {
  const TeamActivityEvent({
    required this.label,
    required this.detail,
    required this.timeLabel,
    required this.icon,
  });

  final String label;
  final String detail;
  final String timeLabel;
  final IconData icon;
}

/// The three states a mutation field can carry over the wire.
///
/// A missing field leaves the server value untouched. [set] writes a value and
/// [clear] deliberately writes JSON null. Keeping these states separate avoids
/// turning an omitted nullable value into an accidental destructive update.
enum TeamPatchFieldState { unset, set, clear }

class TeamPatchField<T> {
  const TeamPatchField.unset()
    : state = TeamPatchFieldState.unset,
      value = null;

  const TeamPatchField.set(this.value) : state = TeamPatchFieldState.set;

  const TeamPatchField.clear()
    : state = TeamPatchFieldState.clear,
      value = null;

  final TeamPatchFieldState state;
  final T? value;

  bool get isUnset => state == TeamPatchFieldState.unset;
  bool get isSet => state == TeamPatchFieldState.set;
  bool get isCleared => state == TeamPatchFieldState.clear;
}

void _writeTeamPatchField<T>(
  Map<String, Object?> json,
  String key,
  TeamPatchField<T> field, {
  String? clearKey,
  Object? Function(T value)? encode,
}) {
  if (field.isUnset) return;
  final value = field.isCleared
      ? null
      : (field.value == null
            ? null
            : encode?.call(field.value as T) ?? field.value);
  json[key] = value;
  if (clearKey != null) json[clearKey] = field.isCleared;
}

/// Typed updates accepted by the Team agent mutation command.
///
/// This remains a read-only Map view for compatibility with the pre-typed
/// gateway entry point. New code should use the named fields and [toJson].
class TeamAgentPatch extends MapBase<String, Object?> {
  const TeamAgentPatch({
    this.roleId = const TeamPatchField<String>.unset(),
    this.displayName = const TeamPatchField<String>.unset(),
    this.model = const TeamPatchField<String>.unset(),
    this.modelOverride = const TeamPatchField<String>.unset(),
    this.instructions = const TeamPatchField<String>.unset(),
    this.policy = const TeamPatchField<Map<String, Object?>>.unset(),
    this.budget = const TeamPatchField<Map<String, Object?>>.unset(),
  });

  final TeamPatchField<String> roleId;
  final TeamPatchField<String> displayName;
  final TeamPatchField<String> model;
  final TeamPatchField<String> modelOverride;
  final TeamPatchField<String> instructions;
  final TeamPatchField<Map<String, Object?>> policy;
  final TeamPatchField<Map<String, Object?>> budget;

  Map<String, Object?> toJson() {
    final json = <String, Object?>{};
    _writeTeamPatchField(json, 'role_id', roleId);
    _writeTeamPatchField(json, 'display_name', displayName);
    _writeTeamPatchField(json, 'model', model);
    _writeTeamPatchField(
      json,
      'model_override',
      modelOverride,
      clearKey: 'clear_model_override',
    );
    _writeTeamPatchField(json, 'instructions', instructions);
    _writeTeamPatchField(json, 'policy', policy);
    _writeTeamPatchField(json, 'budget', budget);
    return json;
  }

  @override
  Iterable<String> get keys => toJson().keys;

  @override
  Object? operator [](Object? key) => toJson()[key];

  @override
  void operator []=(String key, Object? value) =>
      throw UnsupportedError('Team patches are immutable.');

  @override
  void clear() => throw UnsupportedError('Team patches are immutable.');

  @override
  Object? remove(Object? key) =>
      throw UnsupportedError('Team patches are immutable.');
}

/// Typed updates accepted by the Team role mutation command.
///
/// As with [TeamAgentPatch], the read-only Map view is only a compatibility
/// bridge for callers that still expose the original gateway method.
class TeamRolePatch extends MapBase<String, Object?> {
  const TeamRolePatch({
    this.name = const TeamPatchField<String>.unset(),
    this.defaultModel = const TeamPatchField<String>.unset(),
    this.instructions = const TeamPatchField<String>.unset(),
    this.policy = const TeamPatchField<Map<String, Object?>>.unset(),
    this.budget = const TeamPatchField<Map<String, Object?>>.unset(),
  });

  final TeamPatchField<String> name;
  final TeamPatchField<String> defaultModel;
  final TeamPatchField<String> instructions;
  final TeamPatchField<Map<String, Object?>> policy;
  final TeamPatchField<Map<String, Object?>> budget;

  Map<String, Object?> toJson() {
    final json = <String, Object?>{};
    _writeTeamPatchField(json, 'name', name);
    _writeTeamPatchField(
      json,
      'default_model',
      defaultModel,
      clearKey: 'clear_model',
    );
    _writeTeamPatchField(json, 'instructions', instructions);
    _writeTeamPatchField(json, 'policy', policy);
    _writeTeamPatchField(json, 'budget', budget);
    return json;
  }

  @override
  Iterable<String> get keys => toJson().keys;

  @override
  Object? operator [](Object? key) => toJson()[key];

  @override
  void operator []=(String key, Object? value) =>
      throw UnsupportedError('Team patches are immutable.');

  @override
  void clear() => throw UnsupportedError('Team patches are immutable.');

  @override
  Object? remove(Object? key) =>
      throw UnsupportedError('Team patches are immutable.');
}

/// Owner-facing role template input. The daemon remains authoritative for
/// validation, revisioning, and provider availability; this value object only
/// defines the JSON shape shared by the Team editor and remote gateway.
class TeamRoleDraft {
  const TeamRoleDraft({
    required this.name,
    this.defaultModel,
    this.instructions = '',
    this.policy = const <String, Object?>{
      'filesystem': 'workspace-write',
      'shell': 'ask',
      'network': 'ask',
      'approval': 'ask',
    },
    this.budget = const <String, Object?>{
      'time_seconds': null,
      'turns': null,
      'measured_tokens': null,
      'cost_micros': null,
    },
  });

  final String name;
  final String? defaultModel;
  final String instructions;
  final Map<String, Object?> policy;
  final Map<String, Object?> budget;

  Map<String, Object?> toJson() => {
    'name': name,
    'default_model': defaultModel,
    'instructions': instructions,
    'policy': policy,
    'budget': budget,
  };
}

/// Minimal member creation input. Role-owned fields are intentionally not
/// duplicated here; the server materializes them from [roleId].
class TeamAgentDraft {
  const TeamAgentDraft({
    required this.displayName,
    required this.roleId,
    this.modelOverride,
  });

  final String displayName;
  final String roleId;
  final String? modelOverride;

  Map<String, Object?> toJson() => {
    'role_id': roleId,
    'display_name': displayName,
    if (modelOverride != null && modelOverride!.trim().isNotEmpty)
      'model_override': modelOverride,
  };
}
