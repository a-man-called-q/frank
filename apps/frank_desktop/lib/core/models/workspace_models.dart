/// The two top-level destinations in the desktop shell.
///
/// Office owns the project and mission inbox, chat, and retained floor.
/// Settings owns the configuration and operational surfaces.
enum WorkspaceView { office, settings }

/// The sections available from the Settings view.
///
/// Settings sections are intentionally separate from [WorkspaceView]. The
/// segmented control chooses the broad workspace destination, while this
/// value chooses the current Settings area without introducing a conversation
/// context.
enum SettingsSection {
  models,
  organization,
  team,
  ledger,
  taskboard,
  journal,
}

extension SettingsSectionMetadata on SettingsSection {
  String get label => switch (this) {
    SettingsSection.models => 'Models & OpenRouter',
    SettingsSection.organization => 'Organization',
    SettingsSection.team => 'Team',
    SettingsSection.ledger => 'Ledger',
    SettingsSection.taskboard => 'Taskboard',
    SettingsSection.journal => 'Journal',
  };

  String get description => switch (this) {
    SettingsSection.models =>
      'Connect OpenRouter, browse models, and choose the supervisor model.',
    SettingsSection.organization =>
      'Configure agents, connections, and taskboard assignments.',
    SettingsSection.team =>
      'Configure each agent’s model, prompt, identity, and role.',
    SettingsSection.ledger =>
      'Review input/output token usage and totals by agent.',
    SettingsSection.taskboard => 'Track project tasks in a Kanban board.',
    SettingsSection.journal =>
      'Review operational events globally or by agent and project.',
  };
}

enum ProjectStatus { planning, active, review, delivered }

enum MissionStatus {
  planned,
  draft,
  active,
  paused,
  blocked,
  complete,
  completed,
  failed,
  cancelled,
}

enum ChatRole { user, assistant }

enum OfficeMessageStatus { pending, streaming, complete, error, stopped }

/// Identifies the conversation shown by the chat surface.
///
/// A project conversation is used by Office mode when no mission is selected;
/// a mission conversation is used when a mission is selected. Settings has no
/// conversation context. Keeping this as a value object prevents the UI and
/// gateway layers from passing stringly-typed context keys around.
sealed class ConversationContext {
  const ConversationContext({required this.projectId});

  final String projectId;

  String get key;
}

final class ProjectConversationContext extends ConversationContext {
  const ProjectConversationContext({required super.projectId});

  @override
  String get key => 'project:$projectId';

  @override
  bool operator ==(Object other) =>
      other is ProjectConversationContext && other.projectId == projectId;

  @override
  int get hashCode => Object.hash(runtimeType, projectId);
}

final class MissionConversationContext extends ConversationContext {
  const MissionConversationContext({
    required super.projectId,
    required this.missionId,
  });

  final String missionId;

  @override
  String get key => 'mission:$projectId:$missionId';

  @override
  bool operator ==(Object other) =>
      other is MissionConversationContext &&
      other.projectId == projectId &&
      other.missionId == missionId;

  @override
  int get hashCode => Object.hash(runtimeType, projectId, missionId);
}

class OfficeMission {
  const OfficeMission({
    required this.id,
    required this.title,
    required this.status,
    required this.messages,
    this.updatedAt,
    this.assignedAgentIds = const [],
    this.pendingApprovalCount = 0,
  });

  final String id;
  final String title;
  final MissionStatus status;
  final List<OfficeMessage> messages;
  final DateTime? updatedAt;
  final List<String> assignedAgentIds;
  final int pendingApprovalCount;

  String get statusLabel => switch (status) {
    MissionStatus.planned || MissionStatus.draft => 'Draft',
    MissionStatus.active => 'Active',
    MissionStatus.paused => 'Paused',
    MissionStatus.blocked => 'Blocked',
    MissionStatus.complete || MissionStatus.completed => 'Completed',
    MissionStatus.failed => 'Failed',
    MissionStatus.cancelled => 'Cancelled',
  };

  bool get isDraft =>
      status == MissionStatus.planned || status == MissionStatus.draft;

  bool get isActive =>
      status == MissionStatus.active || status == MissionStatus.paused;

  bool get isCompleted =>
      status == MissionStatus.complete || status == MissionStatus.completed;

  bool get needsAttention =>
      pendingApprovalCount > 0 ||
      status == MissionStatus.blocked ||
      status == MissionStatus.failed;
}

class OfficeProject {
  const OfficeProject({
    required this.id,
    required this.name,
    required this.client,
    required this.status,
    required this.progress,
    required this.team,
    required this.summary,
    required this.messages,
    required this.missions,
  });

  final String id;
  final String name;
  final String client;
  final ProjectStatus status;
  final double progress;
  final List<String> team;
  final String summary;

  /// Conversation used by Office mode for this project's latest engagement.
  final List<OfficeMessage> messages;
  final List<OfficeMission> missions;

  String get statusLabel => switch (status) {
    ProjectStatus.planning => 'Planning',
    ProjectStatus.active => 'Active',
    ProjectStatus.review => 'Review',
    ProjectStatus.delivered => 'Delivered',
  };
}

class OfficeEmployee {
  const OfficeEmployee({
    required this.id,
    required this.name,
    required this.role,
    required this.status,
    required this.initials,
    required this.color,
  });

  final String id;
  final String name;
  final String role;
  final String status;
  final String initials;
  final int color;
}

class OfficeMessage {
  const OfficeMessage({
    required this.id,
    required this.role,
    required this.text,
    this.status = OfficeMessageStatus.complete,
  });

  final String id;
  final ChatRole role;
  final String text;
  final OfficeMessageStatus status;

  OfficeMessage copyWith({
    String? id,
    ChatRole? role,
    String? text,
    OfficeMessageStatus? status,
  }) {
    return OfficeMessage(
      id: id ?? this.id,
      role: role ?? this.role,
      text: text ?? this.text,
      status: status ?? this.status,
    );
  }
}

class OfficeWorkspace {
  const OfficeWorkspace({
    required this.name,
    required this.projects,
    required this.employees,
    required this.accountExecutive,
  });

  final String name;
  final List<OfficeProject> projects;
  final List<OfficeEmployee> employees;
  final OfficeEmployee accountExecutive;
}
