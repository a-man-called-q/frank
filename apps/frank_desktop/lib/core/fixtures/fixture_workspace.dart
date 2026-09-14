import 'fixture_organization.dart';
import '../gateway/frank_gateway.dart';
import '../models/organization_models.dart';
import '../models/connection_models.dart';
import '../models/ledger_models.dart';
import '../models/openrouter_models.dart';
import '../models/project_models.dart';
import '../models/taskboard_models.dart';
import '../models/team_models.dart';
import '../models/workspace_models.dart';
import '../models/workflow_models.dart';
import 'fixture_ledger.dart';
import 'fixture_team.dart';

class FixtureFrankGateway implements FrankGateway {
  FixtureFrankGateway({
    this.latency = const Duration(milliseconds: 180),
    this.organizationLatency = Duration.zero,
    this.organizationLoadError,
    this.organizationSaveError,
    this.organizationPublishError,
    this.taskboardLatency = Duration.zero,
    this.taskboardLoadError,
    this.taskboardDecisionError,
    this.initialTaskboard,
  });

  final Duration latency;
  final Duration organizationLatency;
  final Object? organizationLoadError;
  final Object? organizationSaveError;
  final Object? organizationPublishError;
  final Duration taskboardLatency;
  final Object? taskboardLoadError;
  final Object? taskboardDecisionError;
  final TaskboardSnapshot? initialTaskboard;
  OrganizationGraph? _organizationDraft;
  OrganizationGraph? _organizationPublished;
  TaskboardSnapshot? _taskboard;
  OfficeWorkspace? _workspaceCache;
  List<TeamAgentProfile>? _teamProfilesCache;
  OpenRouterConnection _providerConnection = const OpenRouterConnection(
    configured: false,
    credentialSource: null,
    checkedAt: null,
    catalogRefreshedAt: null,
    diagnostic: null,
  );
  int _snapshotRevision = 1;
  String? _supervisorModel;
  final Map<String, ProjectOperation> _projectOperations = {};

  static const _providerCatalog = OpenRouterCatalog(
    refreshedAt: null,
    stale: false,
    models: [
      OpenRouterModel(
        id: 'qwen/qwen-2.5-72b-instruct:free',
        name: 'Qwen 2.5 72B Instruct (Free)',
        canonicalSlug: 'qwen/qwen-2.5-72b-instruct:free',
        contextLength: 32768,
        inputPricePerToken: '0',
        outputPricePerToken: '0',
        supportedParameters: ['tools'],
        deprecatedAt: null,
      ),
      OpenRouterModel(
        id: 'openai/gpt-4o-mini',
        name: 'GPT-4o mini',
        canonicalSlug: 'openai/gpt-4o-mini',
        contextLength: 128000,
        inputPricePerToken: '0.00000015',
        outputPricePerToken: '0.0000006',
        supportedParameters: ['tools'],
        deprecatedAt: null,
      ),
      OpenRouterModel(
        id: 'anthropic/claude-3.5-sonnet',
        name: 'Claude 3.5 Sonnet',
        canonicalSlug: 'anthropic/claude-3.5-sonnet',
        contextLength: 200000,
        inputPricePerToken: '0.000003',
        outputPricePerToken: '0.000015',
        supportedParameters: ['tools'],
        deprecatedAt: null,
      ),
      OpenRouterModel(
        id: 'google/gemini-2.5-flash',
        name: 'Gemini 2.5 Flash',
        canonicalSlug: 'google/gemini-2.5-flash',
        contextLength: 1000000,
        inputPricePerToken: '0.0000003',
        outputPricePerToken: '0.0000025',
        supportedParameters: ['tools'],
        deprecatedAt: null,
      ),
    ],
  );

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
  Future<List<ProjectDirectoryEntry>> browseProjectDirectories([
    String? path,
  ]) async => const <ProjectDirectoryEntry>[];

  @override
  Future<void> registerProject(ProjectRegistrationDraft draft) async {
    final workspace = _workspaceCache ?? await loadWorkspace();
    final name = draft.name.trim();
    final project = OfficeProject(
      id: 'fixture-project-${workspace.projects.length + 1}',
      name: name.isEmpty ? 'Registered project' : name,
      client: name.isEmpty ? 'Registered project' : name,
      status: ProjectStatus.planning,
      progress: 0,
      team: const [],
      summary: 'Registered on the Frank demo gateway.',
      messages: const [],
      missions: const [],
    );
    _workspaceCache = OfficeWorkspace(
      name: workspace.name,
      projects: [...workspace.projects, project],
      employees: workspace.employees,
      accountExecutive: workspace.accountExecutive,
    );
  }

  @override
  Future<ProjectCloneReceipt> cloneProject(ProjectCloneDraft draft) async {
    final operationId = 'fixture-operation-${_projectOperations.length + 1}';
    _projectOperations[operationId] = ProjectOperation(
      id: operationId,
      status: ProjectOperationStatus.succeeded,
      phase: 'complete',
    );
    await registerProject(
      ProjectRegistrationDraft(
        name: draft.destination.split(RegExp(r'[/\\]')).last,
        path: draft.destination,
      ),
    );
    return ProjectCloneReceipt(
      operationId: operationId,
      destination: draft.destination,
    );
  }

  @override
  Future<ProjectOperation> loadProjectOperation(String operationId) async {
    return _projectOperations[operationId] ??
        ProjectOperation(
          id: operationId,
          status: ProjectOperationStatus.succeeded,
          phase: 'complete',
        );
  }

  @override
  Future<void> createMission(String projectId, String objective) async {
    final workspace = _workspaceCache ?? await loadWorkspace();
    final project = workspace.projects.where((value) => value.id == projectId).firstOrNull;
    if (project == null) throw StateError('Project not found.');
    final mission = OfficeMission(
      id: 'fixture-mission-${project.missions.length + 1}',
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
    _workspaceCache = OfficeWorkspace(
      name: workspace.name,
      projects: [
        for (final value in workspace.projects)
          value.id == project.id ? updated : value,
      ],
      employees: workspace.employees,
      accountExecutive: workspace.accountExecutive,
    );
  }

  @override
  Stream<void> watchWorkspaceChanges() => const Stream<void>.empty();

  static const _ae = OfficeEmployee(
    id: 'ae-maya',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
  );

  static const _employees = <OfficeEmployee>[
    _ae,
    OfficeEmployee(
      id: 'analyst-budi',
      name: 'Budi Santoso',
      role: 'System Analyst',
      status: 'Working',
      initials: 'BS',
      color: 0xFF82B7E8,
    ),
    OfficeEmployee(
      id: 'programmer-nia',
      name: 'Nia Alvarez',
      role: 'Junior Programmer',
      status: 'Idle',
      initials: 'NA',
      color: 0xFF77C69B,
    ),
    OfficeEmployee(
      id: 'accountant-dimas',
      name: 'Dimas Pratama',
      role: 'Accountant',
      status: 'Reviewing',
      initials: 'DP',
      color: 0xFFBE9DEB,
    ),
  ];

  static const _northstarMessages = <OfficeMessage>[
    OfficeMessage(
      id: 'northstar-welcome',
      role: ChatRole.assistant,
      text:
          'Hi, I’m Maya, your Account Executive. Tell me what your team needs and I’ll turn it into a clear engagement for the office.',
    ),
    OfficeMessage(
      id: 'northstar-brief',
      role: ChatRole.user,
      text: 'I need a usable inventory workflow for a small logistics team.',
    ),
    OfficeMessage(
      id: 'northstar-brief-reply',
      role: ChatRole.assistant,
      text:
          'I’ve opened Northstar Inventory as a working brief. I can bring in a system analyst first, then staff the build once we agree on the scope.',
    ),
  ];

  static final _projects = <OfficeProject>[
    OfficeProject(
      id: 'northstar-inventory',
      name: 'Northstar Inventory',
      client: 'Northstar Logistics',
      status: ProjectStatus.active,
      progress: 0.62,
      team: ['Budi Santoso', 'Nia Alvarez'],
      summary: 'Warehouse inventory and replenishment dashboard.',
      messages: _northstarMessages,
      missions: [
        OfficeMission(
          id: 'northstar-discovery',
          title: 'Map warehouse intake',
          status: MissionStatus.active,
          updatedAt: DateTime.utc(2026, 8, 31, 15, 20),
          assignedAgentIds: const ['analyst-budi'],
          pendingApprovalCount: 1,
          messages: [
            OfficeMessage(
              id: 'northstar-discovery-welcome',
              role: ChatRole.assistant,
              text:
                  'I’ll map the warehouse intake flow first so the team can agree on the smallest useful workflow.',
            ),
          ],
        ),
        OfficeMission(
          id: 'northstar-dashboard',
          title: 'Design replenishment dashboard',
          status: MissionStatus.planned,
          updatedAt: DateTime.utc(2026, 8, 29, 10, 30),
          messages: [],
        ),
      ],
    ),
    OfficeProject(
      id: 'meridian-finance',
      name: 'Meridian Finance',
      client: 'Meridian & Co.',
      status: ProjectStatus.planning,
      progress: 0.18,
      team: ['Maya Chen', 'Dimas Pratama'],
      summary: 'A lightweight finance operations workspace.',
      messages: [
        OfficeMessage(
          id: 'meridian-welcome',
          role: ChatRole.assistant,
          text:
              'I can help turn Meridian’s finance needs into a focused project brief.',
        ),
      ],
      missions: [
        OfficeMission(
          id: 'meridian-intake',
          title: 'Define finance workflow',
          status: MissionStatus.planned,
          updatedAt: DateTime.utc(2026, 8, 27, 9, 10),
          messages: [],
        ),
        OfficeMission(
          id: 'meridian-controls',
          title: 'Review approval controls',
          status: MissionStatus.blocked,
          updatedAt: DateTime.utc(2026, 8, 30, 13, 45),
          messages: [],
        ),
      ],
    ),
    OfficeProject(
      id: 'atlas-handoff',
      name: 'Atlas Handoff',
      client: 'Atlas Studio',
      status: ProjectStatus.review,
      progress: 0.84,
      team: ['Budi Santoso', 'Dimas Pratama'],
      summary: 'Documentation and delivery readiness review.',
      messages: [
        OfficeMessage(
          id: 'atlas-welcome',
          role: ChatRole.assistant,
          text:
              'Atlas is in review. I can help close the remaining delivery and documentation gaps.',
        ),
      ],
      missions: [
        OfficeMission(
          id: 'atlas-readiness',
          title: 'Review delivery readiness',
          status: MissionStatus.complete,
          updatedAt: DateTime.utc(2026, 8, 26, 16, 5),
          assignedAgentIds: const ['accountant-dimas'],
          messages: [
            OfficeMessage(
              id: 'atlas-readiness-welcome',
              role: ChatRole.assistant,
              text:
                  'Let’s review the final handoff checklist and surface anything that still needs an owner.',
            ),
          ],
        ),
      ],
    ),
  ];

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    final workspace = OfficeWorkspace(
      name: 'Frank Agency',
      projects: _projects,
      employees: _employees,
      accountExecutive: _ae,
    );
    // Publish the immutable fixture identity before the simulated transport
    // delay completes. Surfaces opened while the shell is still settling can
    // project their shared profiles without starting a second workspace load.
    _workspaceCache = workspace;
    await Future<void>.delayed(latency);
    return workspace;
  }

  @override
  Future<List<TeamAgentProfile>> loadTeamProfiles() async {
    final workspace = _workspaceCache ?? await loadWorkspace();
    return _teamProfilesCache ??= fixtureTeamProfiles(workspace);
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
            description: profile.tagline,
            template: profile.specialization?.toLowerCase() ?? 'generalist',
            defaultModel: profile.roleDefaultModel ?? profile.model,
          ),
    ];
  }

  @override
  List<TeamAgentProfile>? get cachedTeamProfiles {
    if (_teamProfilesCache != null) return _teamProfilesCache;
    final workspace = _workspaceCache;
    return workspace == null ? null : fixtureTeamProfiles(workspace);
  }

  @override
  int get snapshotRevision => _snapshotRevision;

  @override
  String? get cachedSupervisorModel => _supervisorModel;

  @override
  Future<OpenRouterConnection> loadOpenRouterConnection() async =>
      _providerConnection;

  @override
  Future<OpenRouterConnection> testOpenRouterConnection() async {
    final now = DateTime.now().toUtc();
    _providerConnection = OpenRouterConnection(
      configured: _providerConnection.configured,
      credentialSource: _providerConnection.credentialSource,
      checkedAt: now,
      catalogRefreshedAt: _providerConnection.catalogRefreshedAt,
      diagnostic: _providerConnection.diagnostic,
    );
    return _providerConnection;
  }

  @override
  Future<OpenRouterConnection> saveOpenRouterCredential(String apiKey) async {
    if (apiKey.trim().isEmpty) throw ArgumentError.value(apiKey, 'apiKey');
    _providerConnection = OpenRouterConnection(
      configured: true,
      credentialSource: 'file',
      checkedAt: DateTime.now().toUtc(),
      catalogRefreshedAt: _providerConnection.catalogRefreshedAt,
      diagnostic: null,
    );
    return _providerConnection;
  }

  @override
  Future<OpenRouterConnection> removeOpenRouterCredential() async {
    _providerConnection = OpenRouterConnection(
      configured: false,
      credentialSource: null,
      checkedAt: DateTime.now().toUtc(),
      catalogRefreshedAt: _providerConnection.catalogRefreshedAt,
      diagnostic: null,
    );
    return _providerConnection;
  }

  @override
  Future<OpenRouterCatalog> loadOpenRouterModels({
    bool refresh = false,
  }) async => _providerCatalog;

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
    final next =
        current.status == TeamAgentStatus.working ||
            current.status == TeamAgentStatus.reviewing
        ? current.copyWith(
            pendingModelChange: true,
            pendingModelOverride: model,
          )
        : model == null
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
    _teamProfilesCache = [...profiles]..[index] = next;
    _snapshotRevision++;
    return _teamProfilesCache!;
  }

  @override
  Future<List<TeamAgentProfile>> updateRoleDefaultModel({
    required String roleId,
    required String? model,
    required int expectedRevision,
  }) async {
    final profiles = await loadTeamProfiles();
    _teamProfilesCache = [
      for (final profile in profiles)
        if (profile.roleId == roleId)
          profile.copyWith(
            model: profile.modelOverride == null
                ? (model ?? profile.model)
                : profile.model,
            roleDefaultModel: model,
          )
        else
          profile,
    ];
    _snapshotRevision++;
    return _teamProfilesCache!;
  }

  @override
  Future<List<TeamAgentProfile>> createRole(TeamRoleDraft draft) async {
    if (draft.name.trim().isEmpty) {
      throw ArgumentError.value(draft.name, 'name');
    }
    // The fixture has no server-side role projection; keep the operation
    // observable and return the same profile list as the production gateway.
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
  Future<void> updateSupervisorModel({
    required String? model,
    required int expectedRevision,
  }) async {
    _supervisorModel = model;
    _snapshotRevision++;
  }

  @override
  Future<LedgerDashboardData> loadLedgerDashboard() async {
    final workspace = _workspaceCache ?? await loadWorkspace();
    return fixtureLedgerDashboard(workspace);
  }

  @override
  LedgerDashboardData? get cachedLedgerDashboard {
    final workspace = _workspaceCache;
    return workspace == null ? null : fixtureLedgerDashboard(workspace);
  }

  @override
  Future<OrganizationGraph> loadOrganization() async {
    if (organizationLoadError != null) throw organizationLoadError!;
    if (organizationLatency > Duration.zero) {
      await Future<void>.delayed(organizationLatency);
    }
    _organizationDraft ??= _copyOrganization(fixtureOrganizationGraph());
    _organizationPublished ??= _copyOrganization(_organizationDraft!);
    return _copyOrganization(_organizationDraft!);
  }

  @override
  Future<OrganizationGraph> saveOrganizationDraft(
    OrganizationGraph graph,
  ) async {
    if (organizationSaveError != null) throw organizationSaveError!;
    if (latency > Duration.zero) await Future<void>.delayed(latency);
    final saved = graph.copyWith(draftRevision: graph.draftRevision + 1);
    _organizationDraft = _copyOrganization(saved);
    return _copyOrganization(saved);
  }

  @override
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  }) async {
    if (organizationPublishError != null) throw organizationPublishError!;
    if (latency > Duration.zero) await Future<void>.delayed(latency);
    final actual =
        _organizationPublished?.publishedRevision ??
        fixtureOrganizationGraph().publishedRevision;
    if (actual != expectedPublishedRevision) {
      throw OrganizationRevisionConflict(expectedPublishedRevision, actual);
    }
    final published = graph.copyWith(publishedRevision: actual + 1);
    _organizationDraft = _copyOrganization(published);
    _organizationPublished = _copyOrganization(published);
    return _copyOrganization(published);
  }

  @override
  Future<List<ConnectorProfile>> loadConnectorProfiles() async => const [];

  @override
  Future<ConnectorProfile> testConnectorProfile({required String profileId}) =>
      Future<ConnectorProfile>.error(
        StateError('Connector profile tests are unavailable in fixture mode.'),
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
  Future<TaskboardSnapshot> loadTaskboard() async {
    if (taskboardLoadError != null) throw taskboardLoadError!;
    if (taskboardLatency > Duration.zero) {
      await Future<void>.delayed(taskboardLatency);
    }
    _taskboard ??= initialTaskboard ?? _fixtureTaskboard();
    return _taskboard!;
  }

  @override
  Future<WorkflowProjection> loadWorkflowProjection() async {
    if (taskboardLatency > Duration.zero) {
      await Future<void>.delayed(taskboardLatency);
    }
    // Fixtures intentionally expose the same shared-board shape as the
    // daemon, while the legacy fixture cards remain owned by TaskboardSnapshot.
    return const WorkflowProjection(
      boards: [
        WorkflowTaskboard(
          id: 'fixture-inbox',
          name: 'Inbox',
          dispatchMode: WorkflowDispatchMode.pull,
        ),
      ],
    );
  }

  @override
  Future<WorkflowProjection> executeWorkflowCommand({
    required String type,
    Map<String, Object?> data = const <String, Object?>{},
  }) => loadWorkflowProjection();

  @override
  Future<TaskboardSnapshot> submitTaskboardDecision({
    required String taskId,
    required TaskboardDecisionInput decision,
  }) async {
    if (taskboardDecisionError != null) throw taskboardDecisionError!;
    if (taskboardLatency > Duration.zero) {
      await Future<void>.delayed(taskboardLatency);
    }
    final current = _taskboard ??= initialTaskboard ?? _fixtureTaskboard();
    final task = current.taskById(taskId);
    if (task == null) throw StateError('Task $taskId was not found.');
    if (task.decision == null) {
      throw StateError('Task $taskId has no pending decision.');
    }
    if (task.decision!.requiresInput &&
        (decision.value == null || decision.value! <= 0)) {
      throw ArgumentError.value(
        decision.value,
        'decision.value',
        'must be a positive integer',
      );
    }
    final activity = TaskboardActivity(
      message: task.decision!.requiresInput
          ? 'Threshold saved · ${decision.value} USD'
          : 'Scope approved · Preparing specification',
      actor: 'Tsany',
      timeLabel: 'Just now',
    );
    final updated = task.copyWith(
      lane: TaskboardLane.working,
      activities: [activity, ...task.activities],
      clearDecision: true,
    );
    _taskboard = current.replaceTask(updated);
    return _taskboard!;
  }

  @override
  Future<TaskboardSnapshot> claimTask({
    required String taskId,
    required String agentId,
  }) async {
    if (taskboardLatency > Duration.zero) {
      await Future<void>.delayed(taskboardLatency);
    }
    final current = _taskboard ??= initialTaskboard ?? _fixtureTaskboard();
    final task = current.taskById(taskId);
    if (task == null) throw StateError('Task $taskId was not found.');
    if (task.lane == TaskboardLane.done) {
      throw StateError('Completed tasks cannot be claimed.');
    }
    if (task.isClaimed) {
      throw StateError('Task $taskId is already claimed.');
    }
    if (task.dependencies.isNotEmpty && task.lane == TaskboardLane.queued) {
      throw StateError('Task dependencies must be completed first.');
    }
    final profile = fixtureTeamProfiles(
      _workspaceCache ?? await loadWorkspace(),
    ).where((candidate) => candidate.employeeId == agentId).firstOrNull;
    if (profile == null) throw StateError('Agent $agentId was not found.');
    if (profile.status != TeamAgentStatus.available &&
        profile.status != TeamAgentStatus.idle &&
        profile.status != TeamAgentStatus.offline) {
      throw StateError('Only an idle or offline member can claim a task.');
    }
    if (task.requiredRoleId != null && profile.roleId != task.requiredRoleId) {
      throw StateError('The member does not belong to the required role.');
    }
    if (current.tasks.any(
      (candidate) =>
          candidate.id != taskId &&
          candidate.agentId == agentId &&
          candidate.lane != TaskboardLane.done,
    )) {
      throw StateError('The member already owns an unfinished task.');
    }
    final updated = task.copyWith(
      agentId: profile.employeeId,
      agentName: profile.name,
      agentInitials: profile.initials,
      claimedAt: DateTime.now().toUtc(),
      claimSource: 'manual',
      activities: [
        TaskboardActivity(
          message: 'Task claimed by ${profile.name}',
          actor: 'Tsany',
          timeLabel: 'Just now',
          kind: 'claimed',
        ),
        ...task.activities,
      ],
    );
    _taskboard = current.replaceTask(updated);
    return _taskboard!;
  }

  @override
  Future<TaskboardSnapshot> releaseTask({required String taskId}) async {
    if (taskboardLatency > Duration.zero) {
      await Future<void>.delayed(taskboardLatency);
    }
    final current = _taskboard ??= initialTaskboard ?? _fixtureTaskboard();
    final task = current.taskById(taskId);
    if (task == null) throw StateError('Task $taskId was not found.');
    final updated = task.copyWith(
      agentId: 'unassigned',
      agentName: 'Unassigned',
      agentInitials: 'UN',
      clearClaim: true,
      activities: [
        const TaskboardActivity(
          message: 'Task claim released',
          actor: 'Tsany',
          timeLabel: 'Just now',
          kind: 'released',
        ),
        ...task.activities,
      ],
    );
    _taskboard = current.replaceTask(updated);
    return _taskboard!;
  }

  @override
  Future<TaskboardSnapshot> addTaskComment({
    required String taskId,
    required String body,
  }) async {
    if (taskboardLatency > Duration.zero) {
      await Future<void>.delayed(taskboardLatency);
    }
    final trimmed = body.trim();
    if (trimmed.isEmpty) throw ArgumentError.value(body, 'body');
    final current = _taskboard ??= initialTaskboard ?? _fixtureTaskboard();
    final task = current.taskById(taskId);
    if (task == null) throw StateError('Task $taskId was not found.');
    final updated = task.copyWith(
      activities: [
        TaskboardActivity(
          message: trimmed,
          actor: 'Tsany',
          timeLabel: 'Just now',
          kind: 'comment',
        ),
        ...task.activities,
      ],
    );
    _taskboard = current.replaceTask(updated);
    return _taskboard!;
  }

  OrganizationGraph _copyOrganization(OrganizationGraph graph) =>
      OrganizationGraph.fromJson(graph.toJson());

  TaskboardSnapshot _fixtureTaskboard() {
    const northstar = 'northstar-inventory';
    const meridian = 'meridian-finance';
    const discovery = 'northstar-discovery';
    const controls = 'meridian-controls';
    return const TaskboardSnapshot(
      tasks: [
        TaskboardTask(
          id: 'NS-04',
          projectId: northstar,
          projectName: 'Northstar Inventory',
          missionId: discovery,
          missionName: 'Map warehouse intake',
          agentId: 'ae-maya',
          agentName: 'Maya Chen',
          agentInitials: 'MC',
          supervisorName: 'Budi Santoso',
          title: 'Draft intake handoff',
          objective:
              'Turn the agreed warehouse intake process into a clear handoff for the implementation team.',
          lane: TaskboardLane.queued,
          dependencies: [
            TaskboardDependency(
              taskId: 'NS-02',
              label: 'NS-02 · Map receiving exceptions',
            ),
          ],
          activities: [
            TaskboardActivity(
              message: 'Waiting on NS-02',
              actor: 'Maya Chen',
              timeLabel: 'Queued',
            ),
          ],
        ),
        TaskboardTask(
          id: 'NS-02',
          projectId: northstar,
          projectName: 'Northstar Inventory',
          missionId: discovery,
          missionName: 'Map warehouse intake',
          agentId: 'analyst-budi',
          agentName: 'Budi Santoso',
          agentInitials: 'BS',
          supervisorName: 'Budi Santoso',
          title: 'Map receiving exceptions',
          objective:
              'Document damaged goods, partial shipments, and duplicate receipts before defining the intake workflow.',
          lane: TaskboardLane.working,
          dependencies: [
            TaskboardDependency(
              taskId: 'NS-01',
              label: 'NS-01 · Collect warehouse notes',
            ),
          ],
          activities: [
            TaskboardActivity(
              message: 'Comparing warehouse notes',
              actor: 'Budi Santoso',
              timeLabel: '6 min ago',
            ),
          ],
        ),
        TaskboardTask(
          id: 'NS-03',
          projectId: northstar,
          projectName: 'Northstar Inventory',
          missionId: discovery,
          missionName: 'Map warehouse intake',
          agentId: 'programmer-nia',
          agentName: 'Nia Alvarez',
          agentInitials: 'NA',
          supervisorName: 'Budi Santoso',
          title: 'Confirm intake scope',
          objective:
              'Confirm the smallest useful first release for the warehouse intake workflow.',
          lane: TaskboardLane.attention,
          dependencies: [
            TaskboardDependency(
              taskId: 'NS-01',
              label: 'NS-01 · Collect warehouse notes',
            ),
          ],
          activities: [
            TaskboardActivity(
              message: 'Approval needed · Scope',
              actor: 'Nia Alvarez',
              timeLabel: '12 min ago',
            ),
          ],
          decision: TaskboardDecision(
            kind: TaskboardDecisionKind.approval,
            prompt:
                'Approve this scope so Nia can finish the intake specification.',
            actionLabel: 'Approve scope',
          ),
        ),
        TaskboardTask(
          id: 'NS-01',
          projectId: northstar,
          projectName: 'Northstar Inventory',
          missionId: discovery,
          missionName: 'Map warehouse intake',
          agentId: 'analyst-budi',
          agentName: 'Budi Santoso',
          agentInitials: 'BS',
          supervisorName: 'Budi Santoso',
          title: 'Collect warehouse notes',
          objective:
              'Gather warehouse interviews and intake notes for analysis.',
          lane: TaskboardLane.done,
          dependencies: [],
          activities: [
            TaskboardActivity(
              message: '4 source documents attached',
              actor: 'Budi Santoso',
              timeLabel: '34 min ago',
            ),
          ],
        ),
        TaskboardTask(
          id: 'MF-03',
          projectId: meridian,
          projectName: 'Meridian Finance',
          missionId: controls,
          missionName: 'Review approval controls',
          agentId: 'ae-maya',
          agentName: 'Maya Chen',
          agentInitials: 'MC',
          supervisorName: 'Dimas Pratama',
          title: 'Write approval policy',
          objective:
              'Write the approval policy once the finance owner confirms the spending thresholds.',
          lane: TaskboardLane.queued,
          dependencies: [
            TaskboardDependency(
              taskId: 'MF-02',
              label: 'MF-02 · Confirm spend thresholds',
            ),
          ],
          activities: [
            TaskboardActivity(
              message: 'Waiting on MF-02',
              actor: 'Maya Chen',
              timeLabel: 'Queued',
            ),
          ],
        ),
        TaskboardTask(
          id: 'MF-02',
          projectId: meridian,
          projectName: 'Meridian Finance',
          missionId: controls,
          missionName: 'Review approval controls',
          agentId: 'accountant-dimas',
          agentName: 'Dimas Pratama',
          agentInitials: 'DP',
          supervisorName: 'Dimas Pratama',
          title: 'Confirm spend thresholds',
          objective:
              'Set the spending limit that triggers a second approver in the finance workflow.',
          lane: TaskboardLane.attention,
          dependencies: [],
          activities: [
            TaskboardActivity(
              message: 'Blocked · Missing limits',
              actor: 'Dimas Pratama',
              timeLabel: '28 min ago',
            ),
          ],
          decision: TaskboardDecision(
            kind: TaskboardDecisionKind.positiveInteger,
            prompt: 'What amount should require a second approver?',
            inputLabel: 'Second approval threshold (USD)',
            actionLabel: 'Save threshold',
          ),
        ),
        TaskboardTask(
          id: 'MF-01',
          projectId: meridian,
          projectName: 'Meridian Finance',
          missionId: controls,
          missionName: 'Review approval controls',
          agentId: 'accountant-dimas',
          agentName: 'Dimas Pratama',
          agentInitials: 'DP',
          supervisorName: 'Dimas Pratama',
          title: 'Review existing controls',
          objective:
              'Review the current approval steps and identify where explicit spending limits are needed.',
          lane: TaskboardLane.done,
          dependencies: [],
          activities: [
            TaskboardActivity(
              message: 'Control review completed',
              actor: 'Dimas Pratama',
              timeLabel: '1 hr ago',
            ),
          ],
        ),
      ],
    );
  }

  @override
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  }) async* {
    final subject = missionId == null ? 'the project brief' : 'that mission';
    final response = [
      'I’ll turn that into a clear plan for $subject. ',
      'First I’ll clarify the outcome, then I’ll suggest the smallest team needed. ',
      'You can approve the plan before anyone starts delivery.',
    ];
    for (final chunk in response) {
      await Future<void>.delayed(const Duration(milliseconds: 220));
      yield chunk;
    }
  }
}
