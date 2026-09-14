import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/project_models.dart';
import '../../../core/models/workspace_models.dart';

enum ProjectsStatus { uninitialized, ready }

enum ProjectsMutationStatus {
  idle,
  registering,
  cloning,
  creatingMission,
  failure,
}

enum _ProjectMutationKind { register, clone, mission }

class ProjectsNotice {
  const ProjectsNotice({required this.id, required this.message});

  final int id;
  final String message;
}

class ProjectsState {
  const ProjectsState({
    this.status = ProjectsStatus.uninitialized,
    this.projects = const [],
    this.selectedProjectId,
    this.selectedMissionId,
    this.expandedProjectIds = const {},
    this.lastMissionByProject = const {},
    this.notice,
    this.mutationStatus = ProjectsMutationStatus.idle,
    this.mutationError,
    this.activeOperation,
  });

  static const _unset = Object();

  final ProjectsStatus status;
  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final Set<String> expandedProjectIds;
  final Map<String, String?> lastMissionByProject;
  final ProjectsNotice? notice;
  final ProjectsMutationStatus mutationStatus;
  final String? mutationError;
  final ProjectOperation? activeOperation;

  bool get isReady => status == ProjectsStatus.ready;

  OfficeProject? projectById(String? id) {
    if (id == null) return null;
    for (final project in projects) {
      if (project.id == id) return project;
    }
    return null;
  }

  OfficeMission? missionById(OfficeProject? project, String? id) {
    if (project == null || id == null) return null;
    for (final mission in project.missions) {
      if (mission.id == id) return mission;
    }
    return null;
  }

  OfficeMission? missionForProject(OfficeProject project) {
    if (project.missions.isEmpty) return null;
    final lastMissionId = lastMissionByProject[project.id];
    return missionById(project, lastMissionId) ?? project.missions.first;
  }

  ProjectsState copyWith({
    ProjectsStatus? status,
    List<OfficeProject>? projects,
    Object? selectedProjectId = _unset,
    Object? selectedMissionId = _unset,
    Set<String>? expandedProjectIds,
    Map<String, String?>? lastMissionByProject,
    Object? notice = _unset,
    ProjectsMutationStatus? mutationStatus,
    Object? mutationError = _unset,
    Object? activeOperation = _unset,
  }) {
    return ProjectsState(
      status: status ?? this.status,
      projects: projects ?? this.projects,
      selectedProjectId: identical(selectedProjectId, _unset)
          ? this.selectedProjectId
          : selectedProjectId as String?,
      selectedMissionId: identical(selectedMissionId, _unset)
          ? this.selectedMissionId
          : selectedMissionId as String?,
      expandedProjectIds: expandedProjectIds ?? this.expandedProjectIds,
      lastMissionByProject: lastMissionByProject ?? this.lastMissionByProject,
      notice: identical(notice, _unset)
          ? this.notice
          : notice as ProjectsNotice?,
      mutationStatus: mutationStatus ?? this.mutationStatus,
      mutationError: identical(mutationError, _unset)
          ? this.mutationError
          : mutationError as String?,
      activeOperation: identical(activeOperation, _unset)
          ? this.activeOperation
          : activeOperation as ProjectOperation?,
    );
  }
}

sealed class ProjectsEvent {
  const ProjectsEvent();
}

final class ProjectsInitialized extends ProjectsEvent {
  const ProjectsInitialized(this.workspace);

  final OfficeWorkspace workspace;
}

final class OfficeViewEntered extends ProjectsEvent {
  const OfficeViewEntered();
}

final class ProjectToggled extends ProjectsEvent {
  const ProjectToggled(this.projectId);

  final String projectId;
}

final class ProjectActivated extends ProjectsEvent {
  const ProjectActivated(this.projectId);

  final String projectId;
}

final class MissionSelected extends ProjectsEvent {
  const MissionSelected({required this.projectId, required this.missionId});

  final String projectId;
  final String missionId;
}

final class ProjectPinRequested extends ProjectsEvent {
  const ProjectPinRequested(this.projectId);

  final String projectId;
}

final class MissionPinRequested extends ProjectsEvent {
  const MissionPinRequested({required this.projectId, required this.missionId});

  final String projectId;
  final String missionId;
}

final class MissionCreationRequested extends ProjectsEvent {
  const MissionCreationRequested(this.projectId);

  final String projectId;
}

final class ProjectRegisterSubmitted extends ProjectsEvent {
  const ProjectRegisterSubmitted(this.draft);

  final ProjectRegistrationDraft draft;
}

final class ProjectCloneSubmitted extends ProjectsEvent {
  const ProjectCloneSubmitted(this.draft);

  final ProjectCloneDraft draft;
}

final class ProjectOperationChanged extends ProjectsEvent {
  const ProjectOperationChanged(this.operation);

  final ProjectOperation operation;
}

final class MissionCreationSubmitted extends ProjectsEvent {
  const MissionCreationSubmitted({
    required this.projectId,
    required this.objective,
  });

  final String projectId;
  final String objective;
}

final class ProjectsRefreshRequested extends ProjectsEvent {
  const ProjectsRefreshRequested();
}

final class ProjectMutationRetryRequested extends ProjectsEvent {
  const ProjectMutationRetryRequested();
}

final class ProjectRenameConfirmed extends ProjectsEvent {
  const ProjectRenameConfirmed({required this.projectId, required this.name});

  final String projectId;
  final String name;
}

final class MissionRenameConfirmed extends ProjectsEvent {
  const MissionRenameConfirmed({
    required this.projectId,
    required this.missionId,
    required this.name,
  });

  final String projectId;
  final String missionId;
  final String name;
}

final class ProjectArchiveConfirmed extends ProjectsEvent {
  const ProjectArchiveConfirmed(this.projectId);

  final String projectId;
}

final class MissionArchiveConfirmed extends ProjectsEvent {
  const MissionArchiveConfirmed({
    required this.projectId,
    required this.missionId,
  });

  final String projectId;
  final String missionId;
}

final class ProjectRemoveConfirmed extends ProjectsEvent {
  const ProjectRemoveConfirmed(this.projectId);

  final String projectId;
}

final class ProjectsNoticeConsumed extends ProjectsEvent {
  const ProjectsNoticeConsumed(this.noticeId);

  final int noticeId;
}

class ProjectsBloc extends Bloc<ProjectsEvent, ProjectsState> {
  ProjectsBloc({ProjectGateway? gateway})
    : _gateway = gateway,
      super(const ProjectsState()) {
    on<ProjectsInitialized>(_initialize);
    on<OfficeViewEntered>(_enterOfficeView);
    on<ProjectToggled>(_toggleProject);
    on<ProjectActivated>(_activateProject);
    on<MissionSelected>(_selectMission);
    on<ProjectPinRequested>(_pinProject);
    on<MissionPinRequested>(_pinMission);
    on<MissionCreationRequested>(_createMission);
    on<ProjectRegisterSubmitted>(_registerProject);
    on<ProjectCloneSubmitted>(_cloneProject);
    on<ProjectOperationChanged>(_operationChanged);
    on<MissionCreationSubmitted>(_createMissionPersisted);
    on<ProjectsRefreshRequested>(_refresh);
    on<ProjectMutationRetryRequested>(_retryMutation);
    on<ProjectRenameConfirmed>(_renameProject);
    on<MissionRenameConfirmed>(_renameMission);
    on<ProjectArchiveConfirmed>(_archiveProject);
    on<MissionArchiveConfirmed>(_archiveMission);
    on<ProjectRemoveConfirmed>(_removeProject);
    on<ProjectsNoticeConsumed>(_consumeNotice);
  }

  int _nextNoticeId = 0;
  final ProjectGateway? _gateway;
  ProjectRegistrationDraft? _lastRegisterDraft;
  ProjectCloneDraft? _lastCloneDraft;
  MissionCreationSubmitted? _lastMission;
  _ProjectMutationKind? _lastMutationKind;

  void _initialize(ProjectsInitialized event, Emitter<ProjectsState> emit) {
    final projects = List<OfficeProject>.unmodifiable(event.workspace.projects);
    final firstProject = projects.isEmpty ? null : projects.first;
    final firstMission = firstProject?.missions.isEmpty ?? true
        ? null
        : firstProject!.missions.first;
    final lastMissionByProject = <String, String?>{
      if (firstProject != null) firstProject.id: firstMission?.id,
    };
    emit(
      ProjectsState(
        status: ProjectsStatus.ready,
        projects: projects,
        selectedProjectId: firstProject?.id,
        selectedMissionId: firstMission?.id,
        expandedProjectIds: firstProject == null ? const {} : {firstProject.id},
        lastMissionByProject: Map.unmodifiable(lastMissionByProject),
        mutationStatus: ProjectsMutationStatus.idle,
        mutationError: null,
        activeOperation: null,
      ),
    );
  }

  Future<void> _refresh(
    ProjectsRefreshRequested event,
    Emitter<ProjectsState> emit,
  ) async {
    final gateway = _gateway;
    if (gateway is! WorkspaceGateway) return;
    try {
      final workspace = await (gateway as WorkspaceGateway).loadWorkspace();
      if (!isClosed) _initialize(ProjectsInitialized(workspace), emit);
    } on Object catch (error) {
      if (!isClosed) {
        emit(
          state.copyWith(
            mutationStatus: ProjectsMutationStatus.failure,
            mutationError: error.toString(),
          ),
        );
      }
    }
  }

  void _enterOfficeView(OfficeViewEntered event, Emitter<ProjectsState> emit) {
    final project =
        state.projectById(state.selectedProjectId) ??
        (state.projects.isEmpty ? null : state.projects.first);
    if (project == null) return;
    final mission = state.missionForProject(project);
    final expanded = {...state.expandedProjectIds, project.id};
    final lastMissions = {
      ...state.lastMissionByProject,
      project.id: mission?.id,
    };
    emit(
      state.copyWith(
        selectedProjectId: project.id,
        selectedMissionId: mission?.id,
        expandedProjectIds: Set.unmodifiable(expanded),
        lastMissionByProject: Map.unmodifiable(lastMissions),
      ),
    );
  }

  void _toggleProject(ProjectToggled event, Emitter<ProjectsState> emit) {
    final project = state.projectById(event.projectId);
    if (project == null) return;
    if (project.missions.isEmpty) {
      _emitProjectSelection(project, emit);
      return;
    }
    final expanded = {...state.expandedProjectIds};
    if (!expanded.add(project.id)) expanded.remove(project.id);
    emit(state.copyWith(expandedProjectIds: Set.unmodifiable(expanded)));
  }

  void _activateProject(ProjectActivated event, Emitter<ProjectsState> emit) {
    final project = state.projectById(event.projectId);
    if (project == null) return;
    _emitProjectSelection(project, emit);
  }

  void _selectMission(MissionSelected event, Emitter<ProjectsState> emit) {
    final project = state.projectById(event.projectId);
    final mission = state.missionById(project, event.missionId);
    if (project == null || mission == null) return;
    final lastMissions = {
      ...state.lastMissionByProject,
      project.id: mission.id,
    };
    emit(
      state.copyWith(
        selectedProjectId: project.id,
        selectedMissionId: mission.id,
        expandedProjectIds: Set.unmodifiable({
          ...state.expandedProjectIds,
          project.id,
        }),
        lastMissionByProject: Map.unmodifiable(lastMissions),
      ),
    );
  }

  void _emitProjectSelection(
    OfficeProject project,
    Emitter<ProjectsState> emit,
  ) {
    final mission = state.missionForProject(project);
    final lastMissions = {
      ...state.lastMissionByProject,
      project.id: mission?.id,
    };
    emit(
      state.copyWith(
        selectedProjectId: project.id,
        selectedMissionId: mission?.id,
        expandedProjectIds: Set.unmodifiable({
          ...state.expandedProjectIds,
          project.id,
        }),
        lastMissionByProject: Map.unmodifiable(lastMissions),
      ),
    );
  }

  void _pinProject(ProjectPinRequested event, Emitter<ProjectsState> emit) {
    final project = state.projectById(event.projectId);
    if (project != null) _notice('Pin simulated for ${project.name}.', emit);
  }

  void _pinMission(MissionPinRequested event, Emitter<ProjectsState> emit) {
    final mission = state.missionById(
      state.projectById(event.projectId),
      event.missionId,
    );
    if (mission != null) {
      _notice('Pin simulated for mission ${mission.title}.', emit);
    }
  }

  void _createMission(
    MissionCreationRequested event,
    Emitter<ProjectsState> emit,
  ) {
    final project = state.projectById(event.projectId);
    if (project != null && _gateway == null) {
      _notice(
        'Select New mission and enter an objective for ${project.name}.',
        emit,
      );
    }
  }

  Future<void> _registerProject(
    ProjectRegisterSubmitted event,
    Emitter<ProjectsState> emit,
  ) async {
    final gateway = _gateway;
    if (gateway == null) {
      _notice('Project registration requires a configured Frank server.', emit);
      return;
    }
    _lastRegisterDraft = event.draft;
    _lastMutationKind = _ProjectMutationKind.register;
    emit(
      state.copyWith(
        mutationStatus: ProjectsMutationStatus.registering,
        mutationError: null,
      ),
    );
    try {
      await gateway.registerProject(event.draft);
      await _refresh(ProjectsRefreshRequested(), emit);
      final project = state.projects
          .where(
            (candidate) =>
                candidate.name.trim().toLowerCase() ==
                event.draft.name.trim().toLowerCase(),
          )
          .firstOrNull;
      if (project != null) _emitProjectSelection(project, emit);
      emit(state.copyWith(mutationStatus: ProjectsMutationStatus.idle));
    } on Object catch (error) {
      emit(
        state.copyWith(
          mutationStatus: ProjectsMutationStatus.failure,
          mutationError: error.toString(),
        ),
      );
    }
  }

  Future<void> _cloneProject(
    ProjectCloneSubmitted event,
    Emitter<ProjectsState> emit,
  ) async {
    final gateway = _gateway;
    if (gateway == null) {
      _notice('Project cloning requires a configured Frank server.', emit);
      return;
    }
    _lastCloneDraft = event.draft;
    _lastMutationKind = _ProjectMutationKind.clone;
    emit(
      state.copyWith(
        mutationStatus: ProjectsMutationStatus.cloning,
        mutationError: null,
      ),
    );
    try {
      final receipt = await gateway.cloneProject(event.draft);
      // The command acknowledgement and the next snapshot are allowed to
      // arrive on different event-loop turns. Keep the operation visible as
      // queued and let the normal invalidation/poll loop pick it up instead
      // of turning that short reporting gap into a false clone failure.
      var operation = await _loadCloneOperation(gateway, receipt.operationId);
      emit(
        state.copyWith(
          mutationStatus: operation.finished
              ? ProjectsMutationStatus.idle
              : ProjectsMutationStatus.cloning,
          activeOperation: operation,
        ),
      );
      if (!operation.finished) {
        operation = await _watchClone(gateway, receipt, emit) ?? operation;
      }
      if (operation.status == ProjectOperationStatus.failed ||
          operation.status == ProjectOperationStatus.cancelled) {
        emit(
          state.copyWith(
            mutationStatus: ProjectsMutationStatus.failure,
            mutationError: operation.error ?? 'Project clone did not complete.',
          ),
        );
        return;
      }
      if (operation.status == ProjectOperationStatus.succeeded ||
          state.activeOperation?.status == ProjectOperationStatus.succeeded) {
        await _refresh(ProjectsRefreshRequested(), emit);
        final destinationName = _basename(event.draft.destination);
        final project =
            state.projects
                .where(
                  (candidate) =>
                      candidate.name.trim().toLowerCase() ==
                      destinationName.toLowerCase(),
                )
                .firstOrNull ??
            state.projects.lastOrNull;
        if (project != null) _emitProjectSelection(project, emit);
        emit(state.copyWith(mutationStatus: ProjectsMutationStatus.idle));
      }
    } on Object catch (error) {
      emit(
        state.copyWith(
          mutationStatus: ProjectsMutationStatus.failure,
          mutationError: error.toString(),
        ),
      );
    }
  }

  Future<ProjectOperation?> _watchClone(
    ProjectGateway gateway,
    ProjectCloneReceipt receipt,
    Emitter<ProjectsState> emit,
  ) async {
    var operation = state.activeOperation;
    var invalidated = false;
    final changes = gateway.watchWorkspaceChanges().listen((_) {
      invalidated = true;
    });
    try {
      while (operation != null && !operation.finished && !isClosed) {
        if (!invalidated) {
          await Future<void>.delayed(const Duration(seconds: 2));
        }
        invalidated = false;
        if (isClosed) return operation;
        final latest = await _loadCloneOperation(
          gateway,
          receipt.operationId,
          fallback: operation,
        );
        operation = latest;
        emit(
          state.copyWith(
            mutationStatus: operation.failed
                ? ProjectsMutationStatus.failure
                : operation.finished
                ? ProjectsMutationStatus.idle
                : ProjectsMutationStatus.cloning,
            activeOperation: operation,
            mutationError: operation.error,
          ),
        );
      }
      return operation;
    } finally {
      await changes.cancel();
    }
  }

  Future<ProjectOperation> _loadCloneOperation(
    ProjectGateway gateway,
    String operationId, {
    ProjectOperation? fallback,
  }) async {
    try {
      return await gateway.loadProjectOperation(operationId);
    } on Object {
      // A server may acknowledge a clone before it includes the operation in
      // its next snapshot. Preserve the operation id and keep polling; any
      // terminal server state will replace this placeholder on a later pass.
      return fallback ??
          ProjectOperation(
            id: operationId,
            status: ProjectOperationStatus.queued,
            phase: 'waiting for server report',
          );
    }
  }

  Future<void> _operationChanged(
    ProjectOperationChanged event,
    Emitter<ProjectsState> emit,
  ) async {
    emit(
      state.copyWith(
        activeOperation: event.operation,
        mutationStatus: event.operation.failed
            ? ProjectsMutationStatus.failure
            : event.operation.finished
            ? ProjectsMutationStatus.idle
            : ProjectsMutationStatus.cloning,
        mutationError: event.operation.error,
      ),
    );
  }

  Future<void> _createMissionPersisted(
    MissionCreationSubmitted event,
    Emitter<ProjectsState> emit,
  ) async {
    final gateway = _gateway;
    if (gateway == null) {
      _notice('Mission creation requires a configured Frank server.', emit);
      return;
    }
    final objective = event.objective.trim();
    if (objective.isEmpty) {
      emit(
        state.copyWith(
          mutationStatus: ProjectsMutationStatus.failure,
          mutationError: 'Objective is required.',
        ),
      );
      return;
    }
    _lastMission = event;
    _lastMutationKind = _ProjectMutationKind.mission;
    emit(
      state.copyWith(
        mutationStatus: ProjectsMutationStatus.creatingMission,
        mutationError: null,
      ),
    );
    try {
      await gateway.createMission(event.projectId, objective);
      await _refresh(ProjectsRefreshRequested(), emit);
      final project = state.projectById(event.projectId);
      final mission =
          project?.missions
              .where(
                (candidate) =>
                    candidate.title.trim().toLowerCase() ==
                    objective.toLowerCase(),
              )
              .lastOrNull ??
          project?.missions.lastOrNull;
      if (project != null) {
        emit(
          state.copyWith(
            selectedProjectId: project.id,
            selectedMissionId: mission?.id,
            expandedProjectIds: {...state.expandedProjectIds, project.id},
            lastMissionByProject: {
              ...state.lastMissionByProject,
              project.id: mission?.id,
            },
            mutationStatus: ProjectsMutationStatus.idle,
            mutationError: null,
          ),
        );
      }
    } on Object catch (error) {
      emit(
        state.copyWith(
          mutationStatus: ProjectsMutationStatus.failure,
          mutationError: error.toString(),
        ),
      );
    }
  }

  Future<void> _retryMutation(
    ProjectMutationRetryRequested event,
    Emitter<ProjectsState> emit,
  ) async {
    if (_lastMutationKind == _ProjectMutationKind.clone &&
        _lastCloneDraft != null) {
      await _cloneProject(ProjectCloneSubmitted(_lastCloneDraft!), emit);
    } else if (_lastMutationKind == _ProjectMutationKind.register &&
        _lastRegisterDraft != null) {
      await _registerProject(
        ProjectRegisterSubmitted(_lastRegisterDraft!),
        emit,
      );
    } else if (_lastMutationKind == _ProjectMutationKind.mission &&
        _lastMission != null) {
      await _createMissionPersisted(_lastMission!, emit);
    } else {
      await _refresh(const ProjectsRefreshRequested(), emit);
    }
  }

  void _renameProject(
    ProjectRenameConfirmed event,
    Emitter<ProjectsState> emit,
  ) {
    final project = state.projectById(event.projectId);
    final name = event.name.trim();
    if (project != null && name.isNotEmpty) {
      _notice('Rename simulated: ${project.name} → $name.', emit);
    }
  }

  void _renameMission(
    MissionRenameConfirmed event,
    Emitter<ProjectsState> emit,
  ) {
    final mission = state.missionById(
      state.projectById(event.projectId),
      event.missionId,
    );
    final name = event.name.trim();
    if (mission != null && name.isNotEmpty) {
      _notice('Rename mission simulated: ${mission.title} → $name.', emit);
    }
  }

  void _archiveProject(
    ProjectArchiveConfirmed event,
    Emitter<ProjectsState> emit,
  ) {
    final project = state.projectById(event.projectId);
    if (project != null) {
      _notice('Archive simulated for ${project.name}.', emit);
    }
  }

  void _archiveMission(
    MissionArchiveConfirmed event,
    Emitter<ProjectsState> emit,
  ) {
    final mission = state.missionById(
      state.projectById(event.projectId),
      event.missionId,
    );
    if (mission != null) {
      _notice('Archive mission simulated for ${mission.title}.', emit);
    }
  }

  void _removeProject(
    ProjectRemoveConfirmed event,
    Emitter<ProjectsState> emit,
  ) {
    final project = state.projectById(event.projectId);
    if (project != null) _notice('Remove simulated for ${project.name}.', emit);
  }

  void _consumeNotice(
    ProjectsNoticeConsumed event,
    Emitter<ProjectsState> emit,
  ) {
    if (state.notice?.id == event.noticeId) {
      emit(state.copyWith(notice: null));
    }
  }

  void _notice(String message, Emitter<ProjectsState> emit) {
    emit(
      state.copyWith(
        notice: ProjectsNotice(id: _nextNoticeId++, message: message),
      ),
    );
  }
}

String _basename(String path) {
  final trimmed = path.trim().replaceAll(RegExp(r'[/\\]+$'), '');
  if (trimmed.isEmpty) return '';
  return trimmed.split(RegExp(r'[/\\]')).last;
}
