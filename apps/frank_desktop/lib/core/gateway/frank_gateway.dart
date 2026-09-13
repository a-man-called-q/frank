import '../models/organization_models.dart';
import '../models/ledger_models.dart';
import '../models/openrouter_models.dart';
import '../models/taskboard_models.dart';
import '../models/team_models.dart';
import '../models/workspace_models.dart';
import '../models/workflow_models.dart';

abstract interface class GatewaySnapshotMetadata {
  int get snapshotRevision;
  String? get cachedSupervisorModel;
}

abstract interface class WorkspaceGateway {
  Future<OfficeWorkspace> loadWorkspace();
}

abstract interface class TeamGateway {
  Future<List<TeamAgentProfile>> loadTeamProfiles();
  Future<List<TeamRoleSummary>> loadTeamRoles();
  Future<List<TeamAgentProfile>> updateAgentPatch({
    required String agentId,
    required TeamAgentPatch patch,
    required int expectedRevision,
  });
  Future<List<TeamAgentProfile>> updateRolePatch({
    required String roleId,
    required TeamRolePatch patch,
    required int expectedRevision,
  });
  Future<List<TeamAgentProfile>> updateAgentModelOverride({
    required String agentId,
    required String? model,
    required int expectedRevision,
  });
  Future<List<TeamAgentProfile>> updateRoleDefaultModel({
    required String roleId,
    required String? model,
    required int expectedRevision,
  });
  Future<List<TeamAgentProfile>> createRole(TeamRoleDraft draft);
  Future<List<TeamAgentProfile>> createAgent(TeamAgentDraft draft);
  Future<List<TeamAgentProfile>> archiveAgent({
    required String agentId,
    required int expectedRevision,
  });
  Future<List<TeamAgentProfile>> archiveRole({
    required String roleId,
    required int expectedRevision,
  });
  List<TeamAgentProfile>? get cachedTeamProfiles;
}

abstract interface class OpenRouterGateway implements GatewaySnapshotMetadata {
  Future<OpenRouterConnection> loadOpenRouterConnection();
  Future<OpenRouterConnection> testOpenRouterConnection();
  Future<OpenRouterConnection> saveOpenRouterCredential(String apiKey);
  Future<OpenRouterConnection> removeOpenRouterCredential();
  Future<OpenRouterCatalog> loadOpenRouterModels({bool refresh = false});
  Future<void> updateSupervisorModel({
    required String? model,
    required int expectedRevision,
  });
}

abstract interface class LedgerGateway {
  Future<LedgerDashboardData> loadLedgerDashboard();
  LedgerDashboardData? get cachedLedgerDashboard;
}

abstract interface class OrganizationGateway {
  Future<OrganizationGraph> loadOrganization();
  Future<OrganizationGraph> saveOrganizationDraft(OrganizationGraph graph);
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  });
}

/// Connector registry and credential operations are independent from the
/// organization graph. Keeping this surface separate lets the organization
/// editor load a graph without gaining access to connector secrets.
abstract interface class ConnectorGateway {
  Future<List<ConnectorProfile>> loadConnectorProfiles();
  Future<ConnectorProfile> testConnectorProfile({required String profileId});
  Future<List<ConnectorProfile>> createConnectorProfile({
    required String name,
    required ConnectorKind kind,
    Map<String, Object?> config,
  });
  Future<List<ConnectorProfile>> updateConnectorProfile({
    required String profileId,
    String? name,
    ConnectorKind? kind,
    Map<String, Object?>? config,
  });
  Future<List<ConnectorProfile>> archiveConnectorProfile({
    required String profileId,
  });
  Future<void> saveConnectorCredential({
    required String profileId,
    required String secret,
  });
  Future<void> removeConnectorCredential({required String profileId});
}

/// Additive organization-workflow projection and command surface.
abstract interface class WorkflowGateway {
  Future<WorkflowProjection> loadWorkflowProjection();
  Future<WorkflowProjection> executeWorkflowCommand({
    required String type,
    Map<String, Object?> data,
  });
}

abstract interface class TaskboardGateway {
  Future<TaskboardSnapshot> loadTaskboard();
  Future<TaskboardSnapshot> submitTaskboardDecision({
    required String taskId,
    required TaskboardDecisionInput decision,
  });
  Future<TaskboardSnapshot> claimTask({
    required String taskId,
    required String agentId,
  });
  Future<TaskboardSnapshot> releaseTask({required String taskId});
  Future<TaskboardSnapshot> addTaskComment({
    required String taskId,
    required String body,
  });
  Stream<void> watchTaskboard();
}

abstract interface class ChatGateway {
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  });
}

abstract interface class FrankGateway
    implements
        GatewaySnapshotMetadata,
        WorkspaceGateway,
        TeamGateway,
        OpenRouterGateway,
        LedgerGateway,
        OrganizationGateway,
        ConnectorGateway,
        WorkflowGateway,
        TaskboardGateway,
        ChatGateway {
  /// True only for the in-process demo adapter. Production surfaces use this
  /// to avoid labelling authenticated projections as sample data.
  bool get isFixture => false;

  /// Last revision observed in the authenticated snapshot. Mutating Team
  /// settings sends this value so frankd can reject stale edits instead of
  /// silently overwriting another owner's change.
  @override
  int get snapshotRevision => 0;

  /// The supervisor model is metadata from the server snapshot. A null value
  /// is meaningful: the supervisor has not been configured yet.
  @override
  String? get cachedSupervisorModel => null;

  @override
  Future<OfficeWorkspace> loadWorkspace();

  /// Loads the presentation metadata for the workspace's agents.
  ///
  /// Identity is keyed by the stable workspace employee id so every surface
  /// can render the same person without copying role or provider labels.
  @override
  Future<List<TeamAgentProfile>> loadTeamProfiles();

  @override
  Future<List<TeamRoleSummary>> loadTeamRoles() =>
      Future<List<TeamRoleSummary>>.error(
        StateError('Team roles are unavailable on this gateway.'),
      );

  /// Owner-only provider state and model catalog. The API key itself is never
  /// returned by these methods.
  @override
  Future<OpenRouterConnection> loadOpenRouterConnection() =>
      Future<OpenRouterConnection>.error(
        StateError('OpenRouter settings are unavailable on this gateway.'),
      );

  @override
  Future<OpenRouterConnection> testOpenRouterConnection() =>
      Future<OpenRouterConnection>.error(
        StateError('OpenRouter settings are unavailable on this gateway.'),
      );

  @override
  Future<OpenRouterConnection> saveOpenRouterCredential(String apiKey) =>
      Future<OpenRouterConnection>.error(
        StateError('OpenRouter settings are unavailable on this gateway.'),
      );

  @override
  Future<OpenRouterConnection> removeOpenRouterCredential() =>
      Future<OpenRouterConnection>.error(
        StateError('OpenRouter settings are unavailable on this gateway.'),
      );

  @override
  Future<OpenRouterCatalog> loadOpenRouterModels({bool refresh = false}) =>
      Future<OpenRouterCatalog>.error(
        StateError('OpenRouter settings are unavailable on this gateway.'),
      );

  @override
  Future<List<TeamAgentProfile>> updateAgentModelOverride({
    required String agentId,
    required String? model,
    required int expectedRevision,
  }) => Future<List<TeamAgentProfile>>.error(
    StateError('Team model editing is unavailable on this gateway.'),
  );

  @override
  Future<List<TeamAgentProfile>> updateRoleDefaultModel({
    required String roleId,
    required String? model,
    required int expectedRevision,
  }) => Future<List<TeamAgentProfile>>.error(
    StateError('Role model editing is unavailable on this gateway.'),
  );

  @override
  Future<List<TeamAgentProfile>> createRole(TeamRoleDraft draft) =>
      Future<List<TeamAgentProfile>>.error(
        StateError('Role creation is unavailable on this gateway.'),
      );

  @override
  Future<List<TeamAgentProfile>> createAgent(TeamAgentDraft draft) =>
      Future<List<TeamAgentProfile>>.error(
        StateError('Agent creation is unavailable on this gateway.'),
      );

  @override
  Future<List<TeamAgentProfile>> archiveAgent({
    required String agentId,
    required int expectedRevision,
  }) => Future<List<TeamAgentProfile>>.error(
    StateError('Agent archiving is unavailable on this gateway.'),
  );

  @override
  Future<List<TeamAgentProfile>> archiveRole({
    required String roleId,
    required int expectedRevision,
  }) => Future<List<TeamAgentProfile>>.error(
    StateError('Role archiving is unavailable on this gateway.'),
  );

  @override
  Future<void> updateSupervisorModel({
    required String? model,
    required int expectedRevision,
  }) => Future<void>.error(
    StateError('Supervisor model editing is unavailable on this gateway.'),
  );

  /// A composition-root cache may expose an already loaded projection so a
  /// surface can render it during the FutureBuilder's first frame. Remote
  /// gateways can return null and use the loading state instead.
  @override
  List<TeamAgentProfile>? get cachedTeamProfiles;

  /// Loads measured and estimated ledger data for the current workspace.
  @override
  Future<LedgerDashboardData> loadLedgerDashboard();

  /// Same-frame ledger data when the gateway has already projected it.
  @override
  LedgerDashboardData? get cachedLedgerDashboard;

  @override
  Future<OrganizationGraph> loadOrganization();

  @override
  Future<OrganizationGraph> saveOrganizationDraft(OrganizationGraph graph);

  @override
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  });

  @override
  Future<List<ConnectorProfile>> loadConnectorProfiles() =>
      Future<List<ConnectorProfile>>.error(
        StateError('Connector profiles are unavailable on this gateway.'),
      );

  @override
  Future<ConnectorProfile> testConnectorProfile({required String profileId}) =>
      Future<ConnectorProfile>.error(
        StateError('Connector profile tests are unavailable on this gateway.'),
      );

  @override
  Future<List<ConnectorProfile>> createConnectorProfile({
    required String name,
    required ConnectorKind kind,
    Map<String, Object?> config = const <String, Object?>{},
  }) => Future<List<ConnectorProfile>>.error(
    StateError('Connector profiles are unavailable on this gateway.'),
  );

  @override
  Future<List<ConnectorProfile>> updateConnectorProfile({
    required String profileId,
    String? name,
    ConnectorKind? kind,
    Map<String, Object?>? config,
  }) => Future<List<ConnectorProfile>>.error(
    StateError('Connector profiles are unavailable on this gateway.'),
  );

  @override
  Future<List<ConnectorProfile>> archiveConnectorProfile({
    required String profileId,
  }) => Future<List<ConnectorProfile>>.error(
    StateError('Connector profiles are unavailable on this gateway.'),
  );

  @override
  Future<void> saveConnectorCredential({
    required String profileId,
    required String secret,
  }) => Future<void>.error(
    StateError('Connector credentials are unavailable on this gateway.'),
  );

  @override
  Future<void> removeConnectorCredential({required String profileId}) =>
      Future<void>.error(
        StateError('Connector credentials are unavailable on this gateway.'),
      );

  @override
  Future<TaskboardSnapshot> loadTaskboard();

  @override
  Future<TaskboardSnapshot> submitTaskboardDecision({
    required String taskId,
    required TaskboardDecisionInput decision,
  });

  /// Claim an available task for an eligible role member. The daemon records
  /// the claim as a feed event and applies the member's latest role revision
  /// at this boundary.
  @override
  Future<TaskboardSnapshot> claimTask({
    required String taskId,
    required String agentId,
  });

  /// Release a non-running task back to the role queue. The release is also
  /// written to the task feed so the next worker can reconstruct the handoff.
  @override
  Future<TaskboardSnapshot> releaseTask({required String taskId});

  /// Append a human/agent handoff note to the task's flat activity feed.
  @override
  Future<TaskboardSnapshot> addTaskComment({
    required String taskId,
    required String body,
  });

  /// Loads the additive Organization-workflow projection. The legacy
  /// TaskboardSnapshot remains available for older clients; this projection
  /// carries board definitions, pull offers, human-input waits, and drain
  /// state without changing the stable TaskId card contract.
  @override
  Future<WorkflowProjection> loadWorkflowProjection() =>
      Future<WorkflowProjection>.error(
        StateError('Organization workflow projection is unavailable.'),
      );

  /// Sends one typed workflow command through the authenticated command
  /// envelope and returns the latest projection. Keeping the command name/data
  /// pair here lets the desktop roll out new board actions independently of
  /// the legacy Taskboard UI while the wire protocol remains additive.
  @override
  Future<WorkflowProjection> executeWorkflowCommand({
    required String type,
    Map<String, Object?> data = const <String, Object?>{},
  }) => Future<WorkflowProjection>.error(
    StateError('Organization workflow commands are unavailable.'),
  );

  /// Emits when the remote daemon has a new event. Fixture gateways return an
  /// empty stream; the taskboard keeps its bounded refresh fallback as well.
  @override
  Stream<void> watchTaskboard() => const Stream<void>.empty();

  @override
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  });
}
