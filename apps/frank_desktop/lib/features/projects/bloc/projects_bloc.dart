import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/models/workspace_models.dart';

enum ProjectsStatus { uninitialized, ready }

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
  });

  static const _unset = Object();

  final ProjectsStatus status;
  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final Set<String> expandedProjectIds;
  final Map<String, String?> lastMissionByProject;
  final ProjectsNotice? notice;

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
  ProjectsBloc() : super(const ProjectsState()) {
    on<ProjectsInitialized>(_initialize);
    on<OfficeViewEntered>(_enterOfficeView);
    on<ProjectToggled>(_toggleProject);
    on<ProjectActivated>(_activateProject);
    on<MissionSelected>(_selectMission);
    on<ProjectPinRequested>(_pinProject);
    on<MissionPinRequested>(_pinMission);
    on<MissionCreationRequested>(_createMission);
    on<ProjectRenameConfirmed>(_renameProject);
    on<MissionRenameConfirmed>(_renameMission);
    on<ProjectArchiveConfirmed>(_archiveProject);
    on<MissionArchiveConfirmed>(_archiveMission);
    on<ProjectRemoveConfirmed>(_removeProject);
    on<ProjectsNoticeConsumed>(_consumeNotice);
  }

  int _nextNoticeId = 0;

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
      ),
    );
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
      _notice('Pin simulated for task ${mission.title}.', emit);
    }
  }

  void _createMission(
    MissionCreationRequested event,
    Emitter<ProjectsState> emit,
  ) {
    final project = state.projectById(event.projectId);
    if (project != null) {
      _notice('Create task simulated for ${project.name}.', emit);
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
      _notice('Rename task simulated: ${mission.title} → $name.', emit);
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
      _notice('Archive task simulated for ${mission.title}.', emit);
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
