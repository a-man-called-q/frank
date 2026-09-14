import 'workspace_models.dart';

/// Server-side directory entry used by the project picker. It intentionally
/// contains names only; the native client never scans the user's filesystem.
class ProjectDirectoryEntry {
  const ProjectDirectoryEntry({required this.name, required this.directory});

  final String name;
  final bool directory;
}

class ProjectRegistrationDraft {
  const ProjectRegistrationDraft({
    required this.name,
    required this.path,
    this.baseBranch = 'main',
  });

  final String name;
  final String path;
  final String baseBranch;
}

class ProjectCloneDraft {
  const ProjectCloneDraft({required this.url, required this.destination});

  final String url;
  final String destination;
}

class ProjectCloneReceipt {
  const ProjectCloneReceipt({
    required this.operationId,
    required this.destination,
  });

  final String operationId;
  final String destination;
}

enum ProjectOperationStatus {
  queued,
  running,
  waiting,
  succeeded,
  failed,
  cancelled,
  recovering,
  unknown,
}

class ProjectOperation {
  const ProjectOperation({
    required this.id,
    required this.status,
    required this.phase,
    this.error,
  });

  final String id;
  final ProjectOperationStatus status;
  final String phase;
  final String? error;

  bool get finished => switch (status) {
    ProjectOperationStatus.succeeded ||
    ProjectOperationStatus.failed ||
    ProjectOperationStatus.cancelled => true,
    _ => false,
  };

  bool get failed =>
      status == ProjectOperationStatus.failed ||
      status == ProjectOperationStatus.cancelled;
}

/// A small result object for a server-persisted mission command.
class MissionCreationReceipt {
  const MissionCreationReceipt({required this.project, required this.mission});

  final OfficeProject project;
  final OfficeMission mission;
}
