import 'dart:async';

import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/fixtures/fixture_ledger.dart';
import 'package:frank_desktop/core/fixtures/fixture_team.dart';
import 'package:frank_desktop/core/gateway/frank_gateway.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/core/models/connection_models.dart';
import 'package:frank_desktop/core/models/ledger_models.dart';
import 'package:frank_desktop/core/models/journal_models.dart';
import 'package:frank_desktop/core/models/openrouter_models.dart';
import 'package:frank_desktop/core/models/project_models.dart';
import 'package:frank_desktop/core/models/taskboard_models.dart';
import 'package:frank_desktop/core/models/team_models.dart';
import 'package:frank_desktop/core/models/toolchain_models.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/core/models/workflow_models.dart';

class FakeGateway implements FrankGateway {
  FakeGateway({
    this.workspace,
    this.loadError,
    this.organization,
    this.taskboard,
    this.taskboardLatency = Duration.zero,
    this.taskboardLoadError,
    this.taskboardDecisionError,
    this.teamProfiles,
    this.ledgerDashboard,
    this.teamProfilesError,
    this.ledgerDashboardError,
  });

  @override
  bool get isFixture => true;

  @override
  FrankConnectionStatus get connectionStatus =>
      const FrankConnectionStatus(
        phase: FrankConnectionPhase.connected,
        appProtocolVersion: 2,
        detail: 'Demo data',
      );

  @override
  Stream<FrankConnectionStatus> watchConnectionStatus() =>
      const Stream<FrankConnectionStatus>.empty();

  @override
  Future<FrankServerCapabilities?> preflightCapabilities({
    bool refresh = false,
  }) async => null;

  @override
  Stream<void> watchTaskboard() => const Stream<void>.empty();

  @override
  Future<JournalPage> loadJournal({
    int? beforeSequence,
    int limit = 50,
    String? projectId,
    String? missionId,
    String? taskId,
    String? agentId,
    JournalEntryKind? kind,
    JournalOutcome? outcome,
  }) async => const JournalPage(entries: []);

  @override
  Future<List<ToolchainRequirement>> loadToolchains({
    String? projectPath,
  }) async => const <ToolchainRequirement>[];

  @override
  Future<List<RunnerInfo>> loadRunners() async => const <RunnerInfo>[];

  @override
  Future<String> requestToolchainApproval({
    required String agentId,
    required String taskId,
    required String operation,
    required String cwd,
    required String project,
    required String reason,
  }) => Future<String>.error(
    StateError('Toolchain approvals are unavailable in fake mode.'),
  );

  @override
  Future<void> decideToolchainApproval({
    required String approvalId,
    required ToolchainApprovalDecision decision,
  }) => Future<void>.error(
    StateError('Toolchain approvals are unavailable in fake mode.'),
  );

  @override
  Future<ToolchainInstallResult> installToolchain({
    required String runnerId,
    required String projectId,
    required String taskId,
    required String manifestId,
    required String version,
    required String projectPath,
    required String approvalId,
  }) => Future<ToolchainInstallResult>.error(
    StateError('Toolchain installation is unavailable in fake mode.'),
  );

  @override
  Future<List<ProjectDirectoryEntry>> browseProjectDirectories([
    String? path,
  ]) async => const <ProjectDirectoryEntry>[];

  @override
  Future<WorkflowProjection> loadWorkflowProjection() async =>
      const WorkflowProjection(
        boards: [
          WorkflowTaskboard(
            id: 'fake-inbox',
            name: 'Inbox',
            dispatchMode: WorkflowDispatchMode.pull,
          ),
        ],
      );

  @override
  Future<WorkflowProjection> executeWorkflowCommand({
    required String type,
    Map<String, Object?> data = const <String, Object?>{},
  }) => loadWorkflowProjection();

  OfficeWorkspace? workspace;
  Object? loadError;
  OrganizationGraph? organization;
  TaskboardSnapshot? taskboard;
  Duration taskboardLatency;
  OrganizationGraph? publishedOrganization;
  Object? organizationLoadError;
  Object? organizationSaveError;
  Object? organizationPublishError;
  Object? taskboardLoadError;
  Object? taskboardDecisionError;
  List<TeamAgentProfile>? teamProfiles;
  LedgerDashboardData? ledgerDashboard;
  Object? teamProfilesError;
  Object? ledgerDashboardError;
  int loadCalls = 0;
  int organizationLoadCalls = 0;
  int taskboardLoadCalls = 0;
  final List<OrganizationGraph> organizationSaves = [];
  final List<OrganizationGraph> organizationPublishes = [];
  final List<TaskboardDecisionRequest> taskboardDecisions = [];
  final List<ReplyRequest> replies = [];
  Stream<String> Function(ReplyRequest request)? onReply;
  int _snapshotRevision = 1;
  String? _supervisorModel;
  final Map<String, ProjectOperation> projectOperations = {};

  @override
  int get snapshotRevision => _snapshotRevision;

  @override
  String? get cachedSupervisorModel => _supervisorModel;

  @override
  Future<OpenRouterConnection> loadOpenRouterConnection() async =>
      const OpenRouterConnection(
        configured: true,
        credentialSource: 'environment',
        checkedAt: null,
        catalogRefreshedAt: null,
        diagnostic: null,
      );

  @override
  Future<OpenRouterConnection> testOpenRouterConnection() =>
      loadOpenRouterConnection();

  @override
  Future<OpenRouterConnection> saveOpenRouterCredential(String apiKey) =>
      loadOpenRouterConnection();

  @override
  Future<OpenRouterConnection> removeOpenRouterCredential() =>
      loadOpenRouterConnection();

  @override
  Future<OpenRouterCatalog> loadOpenRouterModels({bool refresh = false}) async {
    return FixtureFrankGateway(latency: Duration.zero).loadOpenRouterModels();
  }

  @override
  Future<List<TeamAgentProfile>> updateAgentModelOverride({
    required String agentId,
    required String? model,
    required int expectedRevision,
  }) async {
    final profiles = await loadTeamProfiles();
    final index = profiles.indexWhere(
      (profile) => profile.employeeId == agentId,
    );
    if (index < 0) throw StateError('agent not found');
    final current = profiles[index];
    teamProfiles = [...profiles]
      ..[index] = model == null
          ? current.copyWith(
              model: current.roleDefaultModel ?? current.model,
              clearModelOverride: true,
              modelSource: 'role',
            )
          : current.copyWith(
              model: model,
              modelOverride: model,
              modelSource: 'agent',
            );
    _snapshotRevision++;
    return teamProfiles!;
  }

  @override
  Future<List<TeamAgentProfile>> updateRoleDefaultModel({
    required String roleId,
    required String? model,
    required int expectedRevision,
  }) async {
    final profiles = await loadTeamProfiles();
    teamProfiles = [
      for (final profile in profiles)
        profile.roleId == roleId
            ? profile.copyWith(
                roleDefaultModel: model,
                model: profile.modelOverride == null
                    ? (model ?? profile.model)
                    : profile.model,
              )
            : profile,
    ];
    _snapshotRevision++;
    return teamProfiles!;
  }

  @override
  Future<void> updateSupervisorModel({
    required String? model,
    required int expectedRevision,
  }) async {
    _supervisorModel = model;
    _snapshotRevision++;
  }

  @override
  List<TeamAgentProfile>? get cachedTeamProfiles => teamProfiles;

  @override
  LedgerDashboardData? get cachedLedgerDashboard => ledgerDashboard;

  @override
  Future<List<TeamAgentProfile>> loadTeamProfiles() async {
    if (teamProfilesError != null) throw teamProfilesError!;
    if (teamProfiles != null) return teamProfiles!;
    final loadedWorkspace = workspace ??= await FixtureFrankGateway()
        .loadWorkspace();
    return teamProfiles = fixtureTeamProfiles(loadedWorkspace);
  }

  @override
  Future<List<TeamRoleSummary>> loadTeamRoles() async {
    final profiles = await loadTeamProfiles();
    final seen = <String>{};
    return [
      for (final profile in profiles)
        if (profile.roleId != null && seen.add(profile.roleId!))
          TeamRoleSummary(
            id: profile.roleId!,
            name: profile.role,
            defaultModel: profile.roleDefaultModel ?? profile.model,
          ),
    ];
  }

  @override
  Future<List<TeamAgentProfile>> createRole(TeamRoleDraft draft) async {
    if (draft.name.trim().isEmpty) {
      throw ArgumentError.value(draft.name, 'name');
    }
    _snapshotRevision++;
    return loadTeamProfiles();
  }

  @override
  Future<List<TeamAgentProfile>> createAgent(TeamAgentDraft draft) async {
    if (draft.displayName.trim().isEmpty) {
      throw ArgumentError.value(draft.displayName, 'displayName');
    }
    if (draft.roleId.trim().isEmpty) {
      throw ArgumentError.value(draft.roleId, 'roleId');
    }
    _snapshotRevision++;
    return loadTeamProfiles();
  }

  @override
  @override
  Future<List<TeamAgentProfile>> updateAgentPatch({
    required String agentId,
    required TeamAgentPatch patch,
    required int expectedRevision,
  }) async {
    final profiles = await loadTeamProfiles();
    if (!profiles.any((profile) => profile.employeeId == agentId)) {
      throw StateError('agent not found');
    }
    _snapshotRevision++;
    return profiles;
  }

  @override
  Future<List<TeamAgentProfile>> archiveAgent({
    required String agentId,
    required int expectedRevision,
  }) async {
    final profiles = await loadTeamProfiles();
    if (!profiles.any((profile) => profile.employeeId == agentId)) {
      throw StateError('agent not found');
    }
    _snapshotRevision++;
    return profiles;
  }

  @override
  @override
  Future<List<TeamAgentProfile>> updateRolePatch({
    required String roleId,
    required TeamRolePatch patch,
    required int expectedRevision,
  }) async {
    final roles = await loadTeamRoles();
    if (!roles.any((role) => role.id == roleId)) {
      throw StateError('role not found');
    }
    _snapshotRevision++;
    return loadTeamProfiles();
  }

  @override
  Future<List<TeamAgentProfile>> archiveRole({
    required String roleId,
    required int expectedRevision,
  }) async {
    final roles = await loadTeamRoles();
    if (!roles.any((role) => role.id == roleId)) {
      throw StateError('role not found');
    }
    _snapshotRevision++;
    return loadTeamProfiles();
  }

  @override
  Future<LedgerDashboardData> loadLedgerDashboard() async {
    if (ledgerDashboardError != null) throw ledgerDashboardError!;
    if (ledgerDashboard != null) return ledgerDashboard!;
    final loadedWorkspace = workspace ??= await FixtureFrankGateway()
        .loadWorkspace();
    return ledgerDashboard = fixtureLedgerDashboard(loadedWorkspace);
  }

  @override
  Future<TaskboardSnapshot> loadTaskboard() async {
    taskboardLoadCalls++;
    if (taskboardLoadError != null) throw taskboardLoadError!;
    taskboard ??= await FixtureFrankGateway(
      taskboardLatency: taskboardLatency,
    ).loadTaskboard();
    return taskboard!;
  }

  @override
  Future<TaskboardSnapshot> submitTaskboardDecision({
    required String taskId,
    required TaskboardDecisionInput decision,
  }) async {
    taskboardDecisions.add(
      TaskboardDecisionRequest(taskId: taskId, decision: decision),
    );
    if (taskboardDecisionError != null) throw taskboardDecisionError!;
    final fixture = FixtureFrankGateway(
      initialTaskboard: taskboard,
      taskboardLatency: taskboardLatency,
    );
    // Keep the fake's snapshot mutable across multiple decision calls while
    // retaining the same production fixture semantics.
    taskboard = await fixture.submitTaskboardDecision(
      taskId: taskId,
      decision: decision,
    );
    return taskboard!;
  }

  @override
  Future<TaskboardSnapshot> claimTask({
    required String taskId,
    required String agentId,
  }) async {
    final fixture = FixtureFrankGateway(
      initialTaskboard: taskboard,
      taskboardLatency: taskboardLatency,
    );
    taskboard = await fixture.claimTask(taskId: taskId, agentId: agentId);
    return taskboard!;
  }

  @override
  Future<TaskboardSnapshot> releaseTask({required String taskId}) async {
    final fixture = FixtureFrankGateway(
      initialTaskboard: taskboard,
      taskboardLatency: taskboardLatency,
    );
    taskboard = await fixture.releaseTask(taskId: taskId);
    return taskboard!;
  }

  @override
  Future<TaskboardSnapshot> addTaskComment({
    required String taskId,
    required String body,
  }) async {
    final fixture = FixtureFrankGateway(
      initialTaskboard: taskboard,
      taskboardLatency: taskboardLatency,
    );
    taskboard = await fixture.addTaskComment(taskId: taskId, body: body);
    return taskboard!;
  }

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    loadCalls++;
    if (loadError != null) throw loadError!;
    return workspace ??= await FixtureFrankGateway().loadWorkspace();
  }

  @override
  Future<void> registerProject(ProjectRegistrationDraft draft) async {
    final current = await loadWorkspace();
    final name = draft.name.trim().isEmpty ? 'Registered project' : draft.name.trim();
    final project = OfficeProject(
      id: 'fake-project-${current.projects.length + 1}',
      name: name,
      client: name,
      status: ProjectStatus.planning,
      progress: 0,
      team: const [],
      summary: 'Registered project',
      messages: const [],
      missions: const [],
    );
    workspace = OfficeWorkspace(
      name: current.name,
      projects: [...current.projects, project],
      employees: current.employees,
      accountExecutive: current.accountExecutive,
    );
  }

  @override
  Future<ProjectCloneReceipt> cloneProject(ProjectCloneDraft draft) async {
    final id = 'fake-operation-${projectOperations.length + 1}';
    projectOperations[id] = ProjectOperation(
      id: id,
      status: ProjectOperationStatus.succeeded,
      phase: 'complete',
    );
    await registerProject(
      ProjectRegistrationDraft(
        name: draft.destination.split(RegExp(r'[/\\]')).last,
        path: draft.destination,
      ),
    );
    return ProjectCloneReceipt(operationId: id, destination: draft.destination);
  }

  @override
  Future<ProjectOperation> loadProjectOperation(String operationId) async =>
      projectOperations[operationId] ??
      ProjectOperation(
        id: operationId,
        status: ProjectOperationStatus.succeeded,
        phase: 'complete',
      );

  @override
  Future<void> createMission(String projectId, String objective) async {
    final current = await loadWorkspace();
    final project = current.projects.where((value) => value.id == projectId).firstOrNull;
    if (project == null) throw StateError('Project not found.');
    final mission = OfficeMission(
      id: 'fake-mission-${project.missions.length + 1}',
      title: objective.trim(),
      status: MissionStatus.planned,
      messages: const [],
    );
    final updated = OfficeProject(
      id: project.id,
      name: project.name,
      client: project.client,
      status: project.status,
      progress: project.progress,
      team: project.team,
      summary: project.summary,
      messages: project.messages,
      missions: [...project.missions, mission],
    );
    workspace = OfficeWorkspace(
      name: current.name,
      projects: [
        for (final value in current.projects)
          value.id == project.id ? updated : value,
      ],
      employees: current.employees,
      accountExecutive: current.accountExecutive,
    );
  }

  @override
  Future<void> setMissionStatus({
    required String missionId,
    required MissionStatus status,
  }) async {}

  @override
  Future<void> retryMissionPlan(String missionId) async {}

  @override
  Stream<void> watchWorkspaceChanges() => const Stream<void>.empty();

  @override
  Future<OrganizationGraph> loadOrganization() async {
    organizationLoadCalls++;
    if (organizationLoadError != null) throw organizationLoadError!;
    organization ??= await FixtureFrankGateway().loadOrganization();
    publishedOrganization ??= organization;
    return organization!;
  }

  @override
  Future<OrganizationGraph> saveOrganizationDraft(
    OrganizationGraph graph,
  ) async {
    organizationSaves.add(graph);
    if (organizationSaveError != null) throw organizationSaveError!;
    organization = graph.copyWith(draftRevision: graph.draftRevision + 1);
    return organization!;
  }

  @override
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  }) async {
    organizationPublishes.add(graph);
    if (organizationPublishError != null) throw organizationPublishError!;
    final actual =
        publishedOrganization?.publishedRevision ??
        organization?.publishedRevision ??
        0;
    if (actual != expectedPublishedRevision) {
      throw OrganizationRevisionConflict(expectedPublishedRevision, actual);
    }
    organization = graph.copyWith(publishedRevision: actual + 1);
    publishedOrganization = organization;
    return organization!;
  }

  @override
  Future<List<ConnectorProfile>> loadConnectorProfiles() async => const [];

  @override
  Future<ConnectorProfile> testConnectorProfile({required String profileId}) =>
      Future<ConnectorProfile>.error(
        StateError(
          'Connector profile tests are unavailable in the fake gateway.',
        ),
      );

  @override
  Future<List<ConnectorProfile>> createConnectorProfile({
    required String name,
    required ConnectorKind kind,
    Map<String, Object?> config = const <String, Object?>{},
  }) async => const [];

  @override
  Future<List<ConnectorProfile>> updateConnectorProfile({
    required String profileId,
    String? name,
    ConnectorKind? kind,
    Map<String, Object?>? config,
  }) async => const [];

  @override
  Future<List<ConnectorProfile>> archiveConnectorProfile({
    required String profileId,
  }) async => const [];

  @override
  Future<void> saveConnectorCredential({
    required String profileId,
    required String secret,
  }) async {}

  @override
  Future<void> removeConnectorCredential({required String profileId}) async {}

  @override
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  }) {
    final request = ReplyRequest(
      text: text,
      projectId: projectId,
      missionId: missionId,
    );
    replies.add(request);
    return onReply?.call(request) ?? const Stream<String>.empty();
  }
}

class ReplyRequest {
  const ReplyRequest({
    required this.text,
    required this.projectId,
    required this.missionId,
  });

  final String text;
  final String projectId;
  final String? missionId;
}

class TaskboardDecisionRequest {
  const TaskboardDecisionRequest({
    required this.taskId,
    required this.decision,
  });

  final String taskId;
  final TaskboardDecisionInput decision;
}

class ReplyStream {
  ReplyStream() : controller = StreamController<String>();

  final StreamController<String> controller;

  Stream<String> get stream => controller.stream;

  Future<void> close() => controller.close();
}
