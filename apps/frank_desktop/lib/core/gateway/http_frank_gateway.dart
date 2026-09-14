import 'dart:async';
import 'dart:math';

import '../models/ledger_models.dart';
import '../models/organization_models.dart';
import '../models/openrouter_models.dart';
import '../models/project_models.dart';
import '../models/taskboard_models.dart';
import '../models/team_models.dart';
import '../models/workspace_models.dart';
import '../models/workflow_models.dart';
import '../auth/auth_models.dart';
import 'frank_gateway.dart';
import 'snapshot_store.dart';
import '../transport/frank_transport.dart';
import '../models/connection_models.dart';

/// Remote gateway used by the production desktop entrypoint.
///
/// It depends on the shared [FrankTransport] boundary. Snapshot state is
/// owned by [SnapshotStore]; this class only turns that raw DTO into feature
/// projections.
class HttpFrankGateway implements FrankGateway {
  HttpFrankGateway(this._transport, {SnapshotStore? snapshotStore}) {
    _snapshots =
        snapshotStore ??
        SnapshotStore(
          loader: () => _request('GET', '/v2/snapshot'),
          commandSender:
              ({
                required String type,
                required Map<String, dynamic> data,
                int? expectedRevision,
              }) => _request(
                'POST',
                '/v2/commands',
                body: {
                  'protocol_version': FrankApiVersion.current,
                  'command_id': _uuid(),
                  'expected_revision': expectedRevision,
                  'command': {'type': type, 'data': data},
                },
              ),
          eventStreamFactory: _watchEventCursors,
        );
  }

  @override
  bool get isFixture => false;

  @override
  FrankConnectionStatus get connectionStatus => _transport.connectionStatus;

  @override
  Stream<FrankConnectionStatus> watchConnectionStatus() =>
      _transport.connectionStatusStream;

  @override
  int get snapshotRevision => _snapshots.revision;

  @override
  String? get cachedSupervisorModel => _nullableString(
    (_snapshots.snapshot?['server'] as Map?)?['supervisor_model'],
  );

  final FrankTransport _transport;
  late final SnapshotStore _snapshots;
  FrankServerCapabilities? _capabilities;
  Future<FrankServerCapabilities>? _capabilitiesInFlight;

  FrankServerCapabilities? get capabilities => _capabilities;

  /// Bootstrap metadata is intentionally readable before authentication.
  @override
  Future<FrankServerCapabilities> preflightCapabilities({
    bool refresh = false,
  }) => _loadCapabilities(refresh: refresh, preflight: true);

  /// Loads a single shared capability document for feature guards.
  Future<FrankServerCapabilities> loadCapabilities({bool refresh = false}) =>
      _loadCapabilities(refresh: refresh, preflight: false);

  Future<Map<String, dynamic>> _request(
    String method,
    String path, {
    Map<String, dynamic>? body,
  }) async {
    // Keep every mutating HTTP endpoint behind the same compatibility and
    // connection gate as command envelopes. Credential writes, provider
    // probes, catalog refreshes, and connector mutations must not slip past
    // the shell's disabled-CTA state just because they are not commands.
    final verb = method.toUpperCase();
    if (verb != 'GET' && verb != 'HEAD' && verb != 'OPTIONS') {
      await _ensureMutationAllowed();
    }
    return _transport.authorizedJson(method, path, body: body);
  }

  Stream<int> _watchEventCursors({int after = 0}) {
    return _transport.watchEvents(after: after);
  }

  Future<FrankServerCapabilities> _loadCapabilities({
    required bool refresh,
    required bool preflight,
  }) {
    final pending = _capabilitiesInFlight;
    // Refresh requests still share an already-running negotiation. This is
    // the single-flight boundary; `refresh` only bypasses the completed cache.
    if (pending != null) return pending;
    if (!refresh && _capabilities != null) {
      return Future<FrankServerCapabilities>.value(_capabilities!);
    }
    _transport.markChecking();
    final request =
        (preflight
                ? _transport.request('GET', '/v2/capabilities')
                : _request('GET', '/v2/capabilities'))
            .then((json) {
              final capabilities = FrankServerCapabilities.fromJson(json);
              _capabilities = capabilities;
              _transport.markCapabilities(capabilities);
              return capabilities;
            });
    _capabilitiesInFlight = request;
    request.then<void>(
      (_) {
        if (identical(_capabilitiesInFlight, request)) {
          _capabilitiesInFlight = null;
        }
      },
      onError: (_, _) {
        if (identical(_capabilitiesInFlight, request)) {
          _capabilitiesInFlight = null;
        }
      },
    );
    return request;
  }

  @override
  Future<OpenRouterConnection> loadOpenRouterConnection() async {
    final json = await _request('GET', '/v2/providers/openrouter');
    return OpenRouterConnection.fromJson(json);
  }

  @override
  Future<OpenRouterConnection> testOpenRouterConnection() async {
    final json = await _request('POST', '/v2/providers/openrouter/test');
    return OpenRouterConnection.fromJson(json);
  }

  @override
  Future<OpenRouterConnection> saveOpenRouterCredential(String apiKey) async {
    if (apiKey.trim().isEmpty) throw ArgumentError.value(apiKey, 'apiKey');
    await _request(
      'PUT',
      '/v2/providers/openrouter/credential',
      body: {'api_key': apiKey},
    );
    return OpenRouterConnection.fromJson(
      await _request('GET', '/v2/providers/openrouter'),
    );
  }

  @override
  Future<OpenRouterConnection> removeOpenRouterCredential() async {
    await _request('DELETE', '/v2/providers/openrouter/credential');
    return OpenRouterConnection.fromJson(
      await _request('GET', '/v2/providers/openrouter'),
    );
  }

  @override
  Future<OpenRouterCatalog> loadOpenRouterModels({bool refresh = false}) async {
    final json = refresh
        ? await _request('POST', '/v2/providers/openrouter/models/refresh')
        : await _request(
            'GET',
            '/v2/providers/openrouter/models?refresh=false',
          );
    return OpenRouterCatalog.fromJson(json);
  }

  @override
  Future<List<TeamAgentProfile>> updateAgentModelOverride({
    required String agentId,
    required String? model,
    required int expectedRevision,
  }) async {
    return updateAgentPatch(
      agentId: agentId,
      patch: TeamAgentPatch(
        modelOverride: model == null
            ? const TeamPatchField<String>.clear()
            : TeamPatchField<String>.set(model),
      ),
      expectedRevision: expectedRevision,
    );
  }

  @override
  Future<List<TeamAgentProfile>> updateRoleDefaultModel({
    required String roleId,
    required String? model,
    required int expectedRevision,
  }) async {
    return updateRolePatch(
      roleId: roleId,
      patch: TeamRolePatch(
        defaultModel: model == null
            ? const TeamPatchField<String>.clear()
            : TeamPatchField<String>.set(model),
      ),
      expectedRevision: expectedRevision,
    );
  }

  @override
  Future<List<TeamAgentProfile>> createRole(TeamRoleDraft draft) async {
    await _requireFeature('team');
    await _sendCommand(
      'create_role',
      Map<String, dynamic>.from(draft.toJson()),
      expectedRevision: snapshotRevision,
    );
    return _profilesFrom(await _snapshot());
  }

  @override
  Future<List<TeamAgentProfile>> createAgent(TeamAgentDraft draft) async {
    await _requireFeature('team');
    await _sendCommand(
      'create_agent',
      Map<String, dynamic>.from(draft.toJson()),
      expectedRevision: snapshotRevision,
    );
    return _profilesFrom(await _snapshot());
  }

  @override
  Future<List<TeamAgentProfile>> updateAgentPatch({
    required String agentId,
    required TeamAgentPatch patch,
    required int expectedRevision,
  }) async {
    await _requireFeature('team');
    await _sendCommand('update_agent', <String, dynamic>{
      'agent_id': agentId,
      'patch': Map<String, dynamic>.from(patch.toJson()),
    }, expectedRevision: expectedRevision);
    return _profilesFrom(await _snapshot());
  }

  @override
  Future<List<TeamAgentProfile>> archiveAgent({
    required String agentId,
    required int expectedRevision,
  }) async {
    await _requireFeature('team');
    await _sendCommand('archive_agent', <String, dynamic>{
      'agent_id': agentId,
    }, expectedRevision: expectedRevision);
    return _profilesFrom(await _snapshot());
  }

  @override
  Future<List<TeamAgentProfile>> updateRolePatch({
    required String roleId,
    required TeamRolePatch patch,
    required int expectedRevision,
  }) async {
    await _requireFeature('team');
    await _sendCommand('update_role', <String, dynamic>{
      'role_id': roleId,
      'patch': Map<String, dynamic>.from(patch.toJson()),
    }, expectedRevision: expectedRevision);
    return _profilesFrom(await _snapshot());
  }

  @override
  Future<List<TeamAgentProfile>> archiveRole({
    required String roleId,
    required int expectedRevision,
  }) async {
    await _requireFeature('team');
    await _sendCommand('archive_role', <String, dynamic>{
      'role_id': roleId,
    }, expectedRevision: expectedRevision);
    return _profilesFrom(await _snapshot());
  }

  @override
  Future<void> updateSupervisorModel({
    required String? model,
    required int expectedRevision,
  }) async {
    await _sendCommand('update_settings', <String, dynamic>{
      'patch': <String, dynamic>{
        'supervisor_model': ?model,
        'clear_supervisor_model': model == null,
      },
    }, expectedRevision: expectedRevision);
  }

  Future<Map<String, dynamic>> _snapshot({bool force = false}) =>
      _snapshots.load(force: force);

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    final json = await _snapshot();
    final workspace = _workspaceFrom(json);
    return workspace;
  }

  @override
  Future<List<ProjectDirectoryEntry>> browseProjectDirectories([
    String? path,
  ]) async {
    final query = path == null || path.trim().isEmpty
        ? ''
        : '?path=${Uri.encodeQueryComponent(path.trim())}';
    final json = await _request('GET', '/v2/projects/browse$query');
    final entries = json['entries'];
    if (entries is! List) return const <ProjectDirectoryEntry>[];
    return [
      for (final value in entries)
        if (value is Map)
          ProjectDirectoryEntry(
            name: _string(value['name']),
            directory: value['directory'] == true,
          ),
    ];
  }

  @override
  Future<void> registerProject(ProjectRegistrationDraft draft) async {
    final name = draft.name.trim();
    final path = draft.path.trim();
    if (name.isEmpty || path.isEmpty) {
      throw ArgumentError('Project name and path are required.');
    }
    await _requireFeature('projects');
    await _sendCommand('create_project', <String, dynamic>{
      'name': name,
      'path': path,
      'clone_url': null,
      'base_branch': draft.baseBranch.trim().isEmpty
          ? 'main'
          : draft.baseBranch.trim(),
      'remote': 'origin',
      'check_commands': const <String>[],
      'worktree_root': null,
      'push_policy': 'mission-branch',
      'pr_policy': 'draft',
    }, expectedRevision: snapshotRevision);
  }

  @override
  Future<ProjectCloneReceipt> cloneProject(ProjectCloneDraft draft) async {
    final url = draft.url.trim();
    final destination = draft.destination.trim();
    if (url.isEmpty || destination.isEmpty) {
      throw ArgumentError('Repository URL and destination are required.');
    }
    await _requireFeature('projects');
    final response = await _sendCommand('clone_project', <String, dynamic>{
      'url': url,
      'destination': destination,
    }, expectedRevision: snapshotRevision);
    final result = response['result'];
    final data = result is Map && result['data'] is Map
        ? Map<String, dynamic>.from(result['data'] as Map)
        : const <String, dynamic>{};
    final operationId = _string(data['id']);
    if (operationId.isEmpty) {
      throw StateError('The server did not return a clone operation.');
    }
    return ProjectCloneReceipt(
      operationId: operationId,
      destination: destination,
    );
  }

  @override
  Future<ProjectOperation> loadProjectOperation(String operationId) async {
    final requested = operationId.trim();
    if (requested.isEmpty) {
      throw ArgumentError.value(operationId, 'operationId');
    }
    final snapshot = await _snapshot(force: true);
    final operations = snapshot['operations'];
    if (operations is List) {
      for (final value in operations) {
        if (value is! Map || _string(value['id']) != requested) continue;
        return _projectOperationFrom(Map<String, dynamic>.from(value));
      }
    }
    throw StateError(
      'Project operation $requested was not reported by the server.',
    );
  }

  @override
  Future<void> createMission(String projectId, String objective) async {
    final trimmedProject = projectId.trim();
    final trimmedObjective = objective.trim();
    if (trimmedProject.isEmpty || trimmedObjective.isEmpty) {
      throw ArgumentError('Project and objective are required.');
    }
    await _requireFeature('missions');
    await _sendCommand('create_mission', <String, dynamic>{
      'project_id': trimmedProject,
      'objective': trimmedObjective,
    }, expectedRevision: snapshotRevision);
  }

  @override
  Stream<void> watchWorkspaceChanges() => _snapshots.watchInvalidations();

  @override
  Future<List<TeamAgentProfile>> loadTeamProfiles() async {
    final json = await _snapshot();
    final profiles = _profilesFrom(json);
    return profiles;
  }

  @override
  Future<List<TeamRoleSummary>> loadTeamRoles() async {
    await _requireFeature('team');
    final json = await _snapshot();
    return [
      for (final role in _roleMaps(json))
        TeamRoleSummary(
          id: _string(role['id']),
          name: _string(role['name'], fallback: 'Not reported'),
          description: _string(role['description']),
          template: _string(role['template']),
          defaultModel: _nullableString(role['default_model'] ?? role['model']),
          packId: _nullableString(role['pack_id']),
          packLevel: _nullableString(role['pack_level']),
          instructions: _string(role['instructions']),
          policy: _mapObject(role['policy']),
          budget: _mapObject(role['budget']),
          avatarPalette: _string(
            (role['avatar'] as Map?)?['palette'],
            fallback: '',
          ),
          avatarSeed: _intOrNull((role['avatar'] as Map?)?['seed']) ?? 0,
          revision: _intOrNull(role['revision']) ?? 0,
          archived: role['archived'] == true,
        ),
    ];
  }

  @override
  List<TeamAgentProfile>? get cachedTeamProfiles {
    final json = _snapshots.snapshot;
    return json == null ? null : _profilesFrom(json);
  }

  @override
  Future<LedgerDashboardData> loadLedgerDashboard() async {
    return _ledgerFrom(await _snapshot());
  }

  @override
  LedgerDashboardData? get cachedLedgerDashboard {
    final json = _snapshots.snapshot;
    return json == null ? null : _ledgerFrom(json);
  }

  @override
  Future<OrganizationGraph> loadOrganization() async {
    await _requireFeature('organization');
    final json = await _snapshot();
    final organization = json['organization'];
    if (organization is! Map) {
      throw StateError('Organization data is unavailable on this server.');
    }
    final draft = organization['draft'];
    if (draft is! Map) {
      throw StateError('Organization draft is unavailable on this server.');
    }
    final draftGraph = OrganizationGraph.fromJson(
      Map<String, Object?>.from(draft),
    );
    final published = organization['published'];
    if (published is Map && published['published_revision'] != null) {
      return draftGraph.copyWith(
        publishedRevision:
            int.tryParse(published['published_revision'].toString()) ??
            draftGraph.publishedRevision,
      );
    }
    return draftGraph;
  }

  @override
  Future<OrganizationGraph> saveOrganizationDraft(
    OrganizationGraph graph,
  ) async {
    await _requireFeature('organization');
    await _sendCommand('save_organization_draft', <String, dynamic>{
      'graph': graph.toJson(),
      'expected_draft_revision': graph.draftRevision,
    }, organizationExpectedRevision: graph.draftRevision);
    return loadOrganization();
  }

  @override
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  }) async {
    await _requireFeature('organization');
    await _sendCommand('publish_organization', <String, dynamic>{
      'expected_published_revision': expectedPublishedRevision,
    }, organizationExpectedRevision: expectedPublishedRevision);
    return loadOrganization();
  }

  @override
  Future<List<ConnectorProfile>> loadConnectorProfiles() async {
    await _requireFeature('connector-registry');
    final json = await _snapshot();
    return _connectorProfilesFrom(json);
  }

  @override
  Future<ConnectorProfile> testConnectorProfile({
    required String profileId,
  }) async {
    await _requireFeature('connector-registry');
    final result = await _request('POST', '/v2/connectors/$profileId/test');
    final snapshot = await _snapshot();
    final profile = _connectorProfilesFrom(snapshot).firstWhere(
      (candidate) => candidate.id == profileId,
      orElse: () => throw StateError('Connector profile no longer exists.'),
    );
    return ConnectorProfile(
      id: profile.id,
      name: profile.name,
      kind: profile.kind,
      config: profile.config,
      health: ConnectorHealthJson.fromWire(result['health']),
      configured: result['configured'] as bool? ?? profile.configured,
      diagnostic: result['diagnostic'] as String?,
      checkedAt: DateTime.now(),
      archived: profile.archived,
    );
  }

  @override
  Future<List<ConnectorProfile>> createConnectorProfile({
    required String name,
    required ConnectorKind kind,
    Map<String, Object?> config = const <String, Object?>{},
  }) async {
    await _requireFeature('connector-registry');
    await _sendCommand('create_connector_profile', <String, dynamic>{
      'name': name,
      'kind': kind.wireName,
      'config': config,
    }, expectedRevision: snapshotRevision);
    return _connectorProfilesFrom(await _snapshot());
  }

  @override
  Future<List<ConnectorProfile>> updateConnectorProfile({
    required String profileId,
    String? name,
    ConnectorKind? kind,
    Map<String, Object?>? config,
  }) async {
    await _requireFeature('connector-registry');
    await _sendCommand('update_connector_profile', <String, dynamic>{
      'profile_id': profileId,
      'patch': <String, dynamic>{
        'name': ?name,
        if (kind case final value?) 'kind': value.wireName,
        'config': ?config,
      },
    }, expectedRevision: snapshotRevision);
    return _connectorProfilesFrom(await _snapshot());
  }

  @override
  Future<List<ConnectorProfile>> archiveConnectorProfile({
    required String profileId,
  }) async {
    await _requireFeature('connector-registry');
    await _sendCommand('archive_connector_profile', <String, dynamic>{
      'profile_id': profileId,
    }, expectedRevision: snapshotRevision);
    return _connectorProfilesFrom(await _snapshot());
  }

  @override
  Future<void> saveConnectorCredential({
    required String profileId,
    required String secret,
  }) async {
    await _requireFeature('connector-registry');
    if (secret.trim().isEmpty) throw ArgumentError.value(secret, 'secret');
    await _request(
      'PUT',
      '/v2/connectors/$profileId/credential',
      body: <String, dynamic>{'secret': secret},
    );
  }

  @override
  Future<void> removeConnectorCredential({required String profileId}) async {
    await _requireFeature('connector-registry');
    await _request('DELETE', '/v2/connectors/$profileId/credential');
  }

  @override
  Future<TaskboardSnapshot> loadTaskboard() async {
    // Taskboard refreshes are driven by the event cursor and a bounded poll;
    // they must bypass the composition-root cache or another client’s claim
    // and feed activity would never become visible in this process.
    final json = await _snapshot(force: true);
    return _taskboardFrom(json);
  }

  @override
  Future<TaskboardSnapshot> submitTaskboardDecision({
    required String taskId,
    required TaskboardDecisionInput decision,
  }) async {
    // Approval is represented by the durable TaskAccept command. Numeric
    // decisions are recorded as a task comment until a typed decision DTO is
    // added to the protocol; the board still remains the system of record.
    if (decision.value != null) {
      await _sendCommand('add_task_comment', <String, dynamic>{
        'task_id': taskId,
        'body': 'Decision recorded: ${decision.value}',
        'artifact_ids': const <String>[],
      });
    } else {
      await _sendCommand('task_accept', <String, dynamic>{'task_id': taskId});
    }
    return _taskboardFrom(await _snapshot());
  }

  @override
  Future<TaskboardSnapshot> claimTask({
    required String taskId,
    required String agentId,
  }) async {
    await _sendCommand('claim_task', <String, dynamic>{
      'task_id': taskId,
      'agent_id': agentId,
      'source': 'manual',
    });
    return _taskboardFrom(await _snapshot());
  }

  @override
  Future<TaskboardSnapshot> releaseTask({required String taskId}) async {
    await _sendCommand('release_task', <String, dynamic>{'task_id': taskId});
    return _taskboardFrom(await _snapshot());
  }

  @override
  Future<TaskboardSnapshot> addTaskComment({
    required String taskId,
    required String body,
  }) async {
    final trimmed = body.trim();
    if (trimmed.isEmpty) throw ArgumentError.value(body, 'body');
    await _sendCommand('add_task_comment', <String, dynamic>{
      'task_id': taskId,
      'body': trimmed,
      'artifact_ids': const <String>[],
    });
    return _taskboardFrom(await _snapshot());
  }

  @override
  Future<WorkflowProjection> loadWorkflowProjection() async {
    await _requireFeature('taskboard-routing');
    return WorkflowProjection.fromSnapshot(
      Map<String, Object?>.from(await _snapshot()),
    );
  }

  @override
  Future<WorkflowProjection> executeWorkflowCommand({
    required String type,
    Map<String, Object?> data = const <String, Object?>{},
  }) async {
    await _requireFeature('taskboard-routing');
    if (type.trim().isEmpty) {
      throw ArgumentError.value(type, 'type');
    }
    await _sendCommand(
      type,
      Map<String, dynamic>.from(data),
      expectedRevision: snapshotRevision,
    );
    return loadWorkflowProjection();
  }

  @override
  Stream<void> watchTaskboard() => _snapshots.watchInvalidations();

  @override
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  }) async* {
    // Chat streaming is intentionally not faked by the remote adapter. The
    // taskboard and Team surfaces use the authenticated snapshot contract;
    // Office chat can report the unsupported command through its existing
    // error state until the conversation stream is promoted to protocol v2.
    yield* const Stream<String>.empty();
  }

  Future<Map<String, dynamic>> _sendCommand(
    String type,
    Map<String, dynamic> data, {
    int? expectedRevision,
    int? organizationExpectedRevision,
  }) async {
    try {
      await _ensureMutationAllowed();
      return await _snapshots.command(
        type: type,
        data: data,
        expectedRevision: expectedRevision,
      );
    } on SnapshotCommandConflict catch (conflict) {
      final error = conflict.response['error'];
      if (error is Map &&
          conflict.code == 'organization-revision-conflict' &&
          organizationExpectedRevision != null) {
        final latest = conflict.latestSnapshot;
        var actual = organizationExpectedRevision;
        if (latest != null) {
          final organization = latest['organization'];
          if (organization is Map) {
            if (type == 'publish_organization') {
              final published = organization['published'];
              if (published is Map && published['published_revision'] != null) {
                actual =
                    int.tryParse(published['published_revision'].toString()) ??
                    actual;
              }
            } else {
              final draft = organization['draft'];
              if (draft is Map && draft['draft_revision'] != null) {
                actual =
                    int.tryParse(draft['draft_revision'].toString()) ?? actual;
              }
            }
          }
        }
        throw OrganizationRevisionConflict(
          organizationExpectedRevision,
          actual,
        );
      }
      throw StateError(conflict.message);
    }
  }

  OfficeWorkspace _workspaceFrom(Map<String, dynamic> json) {
    final agents = _agentMaps(json);
    final projects = _projectMaps(json);
    final missions = _missionMaps(json);
    final roles = _roleMaps(json);
    final employees = [for (final agent in agents) _employeeFrom(agent, roles)];
    final accountExecutive =
        employees.firstOrNull ??
        const OfficeEmployee(
          id: 'unavailable',
          name: 'Not reported',
          role: 'Not reported',
          status: 'Unavailable',
          initials: '—',
          color: 0xFF65686C,
        );
    return OfficeWorkspace(
      name: _mapString(json['server'], 'name', fallback: 'Workspace'),
      employees: employees,
      accountExecutive: accountExecutive,
      projects: [
        for (final project in projects)
          OfficeProject(
            id: _string(project['id']),
            name: _string(project['name'], fallback: 'Project'),
            client: _string(project['name'], fallback: 'Workspace'),
            status: _projectStatus(project['archived'] == true),
            progress: _projectProgress(_string(project['id']), json),
            team: <String>{
              for (final task in _taskMaps(json))
                if (_missionProject(task['mission_id'], missions) ==
                    project['id'])
                  _agentName(task['assigned_agent'], agents),
            }.toList(),
            summary: '',
            messages: const [],
            missions: [
              for (final mission in missions)
                if (_string(mission['project_id']) == _string(project['id']))
                  OfficeMission(
                    id: _string(mission['id']),
                    title: _string(mission['objective'], fallback: 'Mission'),
                    status: _missionStatus(_string(mission['status'])),
                    messages: const [],
                  ),
            ],
          ),
      ],
    );
  }

  List<TeamAgentProfile> _profilesFrom(Map<String, dynamic> json) {
    final agents = _agentMaps(json);
    final roles = _roleMaps(json);
    return [for (final agent in agents) _profileFrom(agent, roles, json)];
  }

  List<ConnectorProfile> _connectorProfilesFrom(Map<String, dynamic> json) {
    final organization = json['organization'];
    if (organization is! Map) return const <ConnectorProfile>[];
    final profiles =
        organization['connector_profiles'] ?? organization['connectorProfiles'];
    if (profiles is! List) return const <ConnectorProfile>[];
    return profiles
        .whereType<Map>()
        .map(
          (value) =>
              ConnectorProfile.fromJson(Map<String, Object?>.from(value)),
        )
        .toList(growable: false);
  }

  Future<void> _requireFeature(String feature) async {
    final capabilities = await loadCapabilities();
    if (!capabilities.supportsFeature(feature)) {
      throw StateError('This Frank daemon does not support $feature.');
    }
  }

  ProjectOperation _projectOperationFrom(Map<String, dynamic> value) {
    final rawStatus = _string(value['status']).toLowerCase();
    final status = switch (rawStatus) {
      'queued' => ProjectOperationStatus.queued,
      'running' => ProjectOperationStatus.running,
      'waiting' => ProjectOperationStatus.waiting,
      'succeeded' => ProjectOperationStatus.succeeded,
      'failed' => ProjectOperationStatus.failed,
      'cancelled' => ProjectOperationStatus.cancelled,
      'recovering' => ProjectOperationStatus.recovering,
      _ => ProjectOperationStatus.unknown,
    };
    return ProjectOperation(
      id: _string(value['id']),
      status: status,
      phase: _string(value['phase'], fallback: 'working'),
      error: _nullableString(value['error']),
    );
  }

  Future<void> _ensureMutationAllowed() async {
    // The first mutation can race shell bootstrap. Resolve the same
    // single-flight capability request before checking the gate so a command
    // can never slip through while compatibility is still unknown.
    final capabilities = _capabilities ?? await loadCapabilities();
    if (!capabilities.isCompatibleWith(FrankApiVersion.current)) {
      throw AuthFailure(
        kind: AuthFailureKind.protocolMismatch,
        message:
            'This Frank app is not compatible with the server protocol. Update required.',
        statusCode: 426,
      );
    }
    final status = _transport.connectionStatus;
    if (status.phase == FrankConnectionPhase.incompatible) {
      throw AuthFailure(
        kind: AuthFailureKind.protocolMismatch,
        message:
            status.detail ??
            'This Frank app is not compatible with the server.',
        statusCode: 426,
      );
    }
    // Existing fixture transport test doubles intentionally bypass the base
    // request implementation and therefore remain in the initial checking
    // phase. Once a real capability document exists, an offline/reconnecting
    // transport must not issue a mutation.
    if (status.phase != FrankConnectionPhase.checking && !status.canMutate) {
      throw AuthFailure(
        kind: AuthFailureKind.network,
        message: status.detail ?? 'The Frank server is not ready for changes.',
      );
    }
  }

  TeamAgentProfile _profileFrom(
    Map<String, dynamic> agent,
    List<Map<String, dynamic>> roles,
    Map<String, dynamic> json,
  ) {
    final role = roles.firstOrNullWhere(
      (candidate) => _string(candidate['id']) == _string(agent['role_id']),
    );
    final roleName = _string(role?['name'], fallback: 'Unassigned role');
    final tasks = _taskMaps(
      json,
    ).where((task) => _string(task['assigned_agent']) == _string(agent['id']));
    final current = tasks.firstOrNull;
    final template = _string(role?['template'] ?? agent['template']);
    final modelOverride = _nullableString(agent['model_override']);
    final roleDefaultModel = _nullableString(
      role?['default_model'] ?? role?['model'],
    );
    // Resolve the worker model at the gateway boundary as well as in the
    // daemon. Some older snapshots omit effective_model, but the precedence
    // contract remains agent override > role default; supervisor settings
    // are never a worker fallback.
    final effectiveModel =
        modelOverride ??
        _nullableString(agent['effective_model']) ??
        _nullableString(agent['model']) ??
        roleDefaultModel;
    final avatar = agent['avatar'] is Map
        ? Map<String, dynamic>.from(agent['avatar'] as Map)
        : role?['avatar'] is Map
        ? Map<String, dynamic>.from(role?['avatar'] as Map)
        : null;
    return TeamAgentProfile(
      employeeId: _string(agent['id']),
      roleId: role == null ? null : _string(role['id']),
      roleRevision: _intOrNull(agent['role_revision']) ?? 0,
      name: _string(agent['display_name'], fallback: 'Agent'),
      role: roleName,
      specialization: template.isEmpty ? null : template,
      initials: _initials(_string(agent['display_name'], fallback: 'Agent')),
      status: _agentStatus(_string(agent['status'])),
      accentColor: _accentFor(_string(agent['id'])),
      imageAsset: '',
      // These fields are deliberately descriptive only when the daemon has
      // supplied the underlying fact. Do not invent a persona, project, or
      // assignment for a remote member.
      tagline: _string(role?['description']),
      currentProject: current == null
          ? 'Not reported'
          : _string(current['project_id'], fallback: 'Not reported'),
      assignment: current == null
          ? 'No active task'
          : _string(current['title'], fallback: 'Untitled task'),
      model: effectiveModel ?? 'Unconfigured',
      modelOverride: modelOverride,
      modelSource: _string(
        agent['model_source'],
        fallback: modelOverride != null
            ? 'agent'
            : effectiveModel == null
            ? 'unavailable'
            : role == null
            ? 'system'
            : 'role',
      ),
      roleDefaultModel: roleDefaultModel,
      pendingModelOverride: _nullableString(agent['pending_model_override']),
      pendingModelChange: agent['pending_model_change'] == true,
      revision: _intOrNull(json['revision']) ?? 0,
      promptPack: _string(role?['pack_id'] ?? agent['pack_id']),
      level: _string(role?['pack_level'] ?? agent['pack_level']),
      traits: template.isEmpty ? const [] : <String>[template],
      capabilities: const [],
      activity: const [],
      avatarPalette: _nullableString(avatar?['palette']),
      avatarSeed: (avatar?['seed'] as num?)?.toInt(),
    );
  }

  TaskboardSnapshot _taskboardFrom(Map<String, dynamic> json) {
    final projects = _projectMaps(json);
    final missions = _missionMaps(json);
    final agents = _agentMaps(json);
    final roles = _roleMaps(json);
    final tasks = _taskMaps(json);
    final feed = _feedMaps(json);
    final byId = <String, Map<String, dynamic>>{
      for (final task in tasks) _string(task['id']): task,
    };
    return TaskboardSnapshot(
      isFixture: false,
      tasks: [
        for (final task in tasks)
          _taskboardTask(task, projects, missions, agents, roles, byId, feed),
      ],
    );
  }

  TaskboardTask _taskboardTask(
    Map<String, dynamic> task,
    List<Map<String, dynamic>> projects,
    List<Map<String, dynamic>> missions,
    List<Map<String, dynamic>> agents,
    List<Map<String, dynamic>> roles,
    Map<String, Map<String, dynamic>> byId,
    List<Map<String, dynamic>> feed,
  ) {
    final mission = missions.firstOrNullWhere(
      (candidate) => _string(candidate['id']) == _string(task['mission_id']),
    );
    final project = projects.firstOrNullWhere(
      (candidate) =>
          _string(candidate['id']) == _string(mission?['project_id']),
    );
    final agent = agents.firstOrNullWhere(
      (candidate) =>
          _string(candidate['id']) == _string(task['assigned_agent']),
    );
    final reviewer = agents.firstOrNullWhere(
      (candidate) =>
          _string(candidate['id']) == _string(task['reviewer_agent']),
    );
    final requiredRole = roles.firstOrNullWhere(
      (candidate) =>
          _string(candidate['id']) == _string(task['required_role_id']),
    );
    final agentId = _string(agent?['id'], fallback: 'unassigned');
    final dependencyIds = _stringList(task['dependencies']);
    final activities = [
      for (final entry in feed)
        if (_string(entry['task_id']) == _string(task['id']))
          TaskboardActivity(
            message: _string(entry['body'], fallback: 'Task activity'),
            actor: _activityActor(entry['actor'], agents),
            timeLabel: _timeLabel(_string(entry['created_at'])),
            kind: _string(entry['kind'], fallback: 'activity'),
            artifactIds: _stringList(entry['artifact_ids']),
          ),
    ];
    return TaskboardTask(
      id: _string(task['id']),
      projectId: _string(project?['id'], fallback: 'not-reported'),
      projectName: _string(project?['name'], fallback: 'Not reported'),
      missionId: _string(mission?['id'], fallback: 'not-reported'),
      missionName: _string(mission?['objective'], fallback: 'Not reported'),
      agentId: agentId,
      agentName: _string(agent?['display_name'], fallback: 'Unassigned'),
      agentInitials: _initials(
        _string(agent?['display_name'], fallback: 'Unassigned'),
      ),
      supervisorName: 'Not reported',
      title: _string(task['title'], fallback: 'Task'),
      objective: _string(task['objective']),
      requiredRoleId: _nullableString(task['required_role_id']),
      requiredRoleName: _nullableString(requiredRole?['name']),
      claimedAt: _timestampDate(task['claimed_at']),
      claimSource: _nullableString(task['claim_source']),
      reviewerAgentId: _nullableString(task['reviewer_agent']),
      reviewerName: _nullableString(reviewer?['display_name']),
      lane: _laneFor(_string(task['status']), dependencyIds, byId),
      dependencies: [
        for (final dependencyId in dependencyIds)
          TaskboardDependency(
            taskId: dependencyId,
            label:
                '$dependencyId · ${_string(byId[dependencyId]?['title'], fallback: 'Dependency')}',
          ),
      ],
      activities: activities,
      decision: _string(task['status']) == 'review'
          ? const TaskboardDecision(
              kind: TaskboardDecisionKind.approval,
              prompt: 'Accept this task handoff?',
              actionLabel: 'Accept task',
            )
          : null,
    );
  }

  String _activityActor(Object? actorValue, List<Map<String, dynamic>> agents) {
    final actor = actorValue is Map
        ? Map<String, dynamic>.from(actorValue)
        : const <String, dynamic>{};
    final displayName = _nullableString(actor['display_name']);
    if (displayName != null) return displayName;
    final agentName = _nullableString(
      agents.firstOrNullWhere(
        (candidate) => _string(candidate['id']) == _string(actor['id']),
      )?['display_name'],
    );
    return agentName ?? 'Not reported';
  }

  LedgerDashboardData _ledgerFrom(Map<String, dynamic> json) {
    final projects = _projectMaps(json);
    final missions = _missionMaps(json);
    final tasks = _taskMaps(json);
    final agents = _agentMaps(json);
    final entries = [
      for (final usage in _listMaps(json['usage']))
        _ledgerEntry(usage, projects, missions, tasks, agents),
    ];
    final lifetimeTotals = entries.fold(
      const LedgerTotals(),
      (sum, entry) => sum + entry.measuredTotals,
    );
    final model = _ledgerModel(entries);
    final incomplete = LedgerPeriodData(
      period: LedgerPeriod.session,
      sessionCount: 0,
      turnCount: 0,
      totals: const LedgerTotals(),
      trend: const [],
      attribution: const [],
      model: 'Not reported',
      evidenceCountsAvailable: false,
      injectedBytesAvailable: false,
      isIncomplete: true,
    );
    final lifetime = LedgerPeriodData(
      period: LedgerPeriod.lifetime,
      sessionCount: 0,
      // UsageView is recorded per attempt. It provides a record count, but no
      // session boundary, so sessionCount remains unavailable above.
      turnCount: entries.length,
      totals: lifetimeTotals,
      trend: const [],
      attribution: const [],
      model: model,
      evidenceCountsAvailable: false,
      injectedBytesAvailable: false,
      isIncomplete: true,
    );
    return LedgerDashboardData(
      session: incomplete,
      lifetime: lifetime,
      operational: LedgerOperationalData(
        entries: List.unmodifiable(entries),
        asOf: DateTime.now().toUtc(),
      ),
      isFixture: false,
    );
  }

  LedgerUsageEntry _ledgerEntry(
    Map<String, dynamic> usage,
    List<Map<String, dynamic>> projects,
    List<Map<String, dynamic>> missions,
    List<Map<String, dynamic>> tasks,
    List<Map<String, dynamic>> agents,
  ) {
    final scope = _string(usage['scope']).toLowerCase();
    final scopeId = _nullableString(usage['scope_id']);
    final task = scope == 'task'
        ? tasks.firstOrNullWhere(
            (candidate) => _string(candidate['id']) == scopeId,
          )
        : null;
    final missionId = switch (scope) {
      'mission' => scopeId,
      'task' => _nullableString(task?['mission_id']),
      _ => null,
    };
    final mission = missionId == null
        ? null
        : missions.firstOrNullWhere(
            (candidate) => _string(candidate['id']) == missionId,
          );
    final projectId = _nullableString(mission?['project_id']);
    final project = projectId == null
        ? null
        : projects.firstOrNullWhere(
            (candidate) => _string(candidate['id']) == projectId,
          );
    final agentId = switch (scope) {
      'agent' => scopeId,
      'task' => _nullableString(task?['assigned_agent']),
      _ => null,
    };
    final agent = agentId == null
        ? null
        : agents.firstOrNullWhere(
            (candidate) => _string(candidate['id']) == agentId,
          );

    return LedgerUsageEntry(
      id: _string(usage['id']),
      recordedAt: _parseDate(_string(usage['recorded_at'])),
      provider: _string(usage['provider'], fallback: 'unknown'),
      projectId: projectId,
      projectName: _nullableString(project?['name']),
      missionId: missionId,
      missionName: _nullableString(mission?['objective']),
      taskId: scope == 'task' ? scopeId : null,
      taskName: _nullableString(task?['title']),
      agentId: agentId,
      agentName: _nullableString(agent?['display_name']),
      model: _nullableString(usage['model']),
      measuredInputTokens: _intOrNull(usage['measured_input_tokens']),
      cacheReadInputTokens: _intOrNull(usage['cached_input_tokens']),
      reasoningTokens: _intOrNull(usage['reasoning_tokens']),
      measuredOutputTokens: _intOrNull(usage['measured_output_tokens']),
      estimatedInputTokens: _intOrNull(usage['estimated_input_tokens']),
      estimatedOutputTokens: _intOrNull(usage['estimated_output_tokens']),
      costMicros: _intOrNull(usage['cost_micros']),
      // UsageView carries no attribution basis. Do not treat a remote row as
      // transition-log evidence just because the presentation model has that
      // default for fixture rows.
      basis: LedgerAttributionBasis.unattributed,
    );
  }

  String _ledgerModel(List<LedgerUsageEntry> entries) {
    final models = entries
        .map((entry) => entry.model)
        .whereType<String>()
        .toSet();
    if (models.isEmpty) return 'Not reported';
    if (models.length == 1) return models.single;
    return 'Mixed models';
  }
}

List<Map<String, dynamic>> _listMaps(Object? value) => [
  for (final item in value is List ? value : const <dynamic>[])
    if (item is Map) Map<String, dynamic>.from(item),
];

List<Map<String, dynamic>> _agentMaps(Map<String, dynamic> json) =>
    _listMaps(json['agents']);
List<Map<String, dynamic>> _roleMaps(Map<String, dynamic> json) =>
    _listMaps(json['roles']);
List<Map<String, dynamic>> _projectMaps(Map<String, dynamic> json) =>
    _listMaps(json['projects']);
List<Map<String, dynamic>> _missionMaps(Map<String, dynamic> json) =>
    _listMaps(json['missions']);
List<Map<String, dynamic>> _taskMaps(Map<String, dynamic> json) =>
    _listMaps(json['tasks']);
List<Map<String, dynamic>> _feedMaps(Map<String, dynamic> json) =>
    _listMaps(json['task_feed']);

String _string(Object? value, {String fallback = ''}) {
  final result = value?.toString() ?? '';
  return result.isEmpty ? fallback : result;
}

String? _nullableString(Object? value) {
  final result = value?.toString() ?? '';
  return result.isEmpty ? null : result;
}

Map<String, Object?> _mapObject(Object? value) =>
    value is Map ? Map<String, Object?>.from(value) : const <String, Object?>{};

String _mapString(Object? value, String key, {required String fallback}) =>
    value is Map ? _string(value[key], fallback: fallback) : fallback;

List<String> _stringList(Object? value) => [
  for (final item in value is List ? value : const <dynamic>[])
    if (item != null) item.toString(),
];

String _agentName(Object? id, List<Map<String, dynamic>> agents) => _string(
  agents.firstOrNullWhere(
    (agent) => _string(agent['id']) == _string(id),
  )?['display_name'],
  fallback: 'Unassigned',
);

String? _missionProject(
  Object? missionId,
  List<Map<String, dynamic>> missions,
) => missions
    .firstOrNullWhere(
      (mission) => _string(mission['id']) == _string(missionId),
    )?['project_id']
    ?.toString();

OfficeEmployee _employeeFrom(
  Map<String, dynamic> agent,
  List<Map<String, dynamic>> roles,
) {
  final name = _string(agent['display_name'], fallback: 'Not reported');
  final role = roles.firstOrNullWhere(
    (candidate) => _string(candidate['id']) == _string(agent['role_id']),
  );
  return OfficeEmployee(
    id: _string(agent['id']),
    name: name,
    role: _string(role?['name'], fallback: 'Not reported'),
    status: _string(agent['status'], fallback: 'Unavailable'),
    initials: _initials(name),
    color: _accentFor(_string(agent['id'])),
  );
}

String _initials(String name) {
  final pieces = name
      .trim()
      .split(RegExp(r'\s+'))
      .where((part) => part.isNotEmpty)
      .toList();
  if (pieces.isEmpty) return 'FR';
  if (pieces.length == 1) {
    return pieces.first.substring(0, min(2, pieces.first.length)).toUpperCase();
  }
  return '${pieces.first[0]}${pieces.last[0]}'.toUpperCase();
}

int _accentFor(String id) =>
    [0xFF9A68A5, 0xFF82B7E8, 0xFF77C69B, 0xFFBE9DEB][id.hashCode.abs() % 4];

TeamAgentStatus _agentStatus(String value) => switch (value) {
  'working' || 'thinking' || 'starting' => TeamAgentStatus.working,
  'needs_approval' || 'waiting' => TeamAgentStatus.reviewing,
  'failed' || 'paused' => TeamAgentStatus.blocked,
  'offline' => TeamAgentStatus.offline,
  'idle' => TeamAgentStatus.idle,
  _ => TeamAgentStatus.available,
};

MissionStatus _missionStatus(String value) => switch (value) {
  'active' => MissionStatus.active,
  'paused' => MissionStatus.paused,
  'blocked' => MissionStatus.blocked,
  'completed' => MissionStatus.completed,
  'failed' => MissionStatus.failed,
  'cancelled' => MissionStatus.cancelled,
  _ => MissionStatus.draft,
};

ProjectStatus _projectStatus(bool archived) =>
    archived ? ProjectStatus.delivered : ProjectStatus.active;

double _projectProgress(String id, Map<String, dynamic> json) {
  final tasks = _taskMaps(
    json,
  ).where((task) => _string(task['mission_id']).isNotEmpty);
  if (tasks.isEmpty) return 0;
  return tasks.where((task) => _string(task['status']) == 'done').length /
      tasks.length;
}

TaskboardLane _laneFor(
  String status,
  List<String> dependencies,
  Map<String, Map<String, dynamic>> byId,
) {
  final locked = dependencies.any(
    (id) => _string(byId[id]?['status']) != 'done',
  );
  if (locked) return TaskboardLane.blocked;
  return switch (status) {
    'backlog' => TaskboardLane.backlog,
    'ready' => TaskboardLane.ready,
    'running' => TaskboardLane.running,
    'blocked' => TaskboardLane.blocked,
    'review' => TaskboardLane.review,
    'done' => TaskboardLane.done,
    'cancelled' => TaskboardLane.cancelled,
    // Older daemons used a smaller status vocabulary. Keep the mapping
    // lossless for those snapshots while the UI still renders fixed lanes.
    'working' => TaskboardLane.running,
    'attention' => TaskboardLane.blocked,
    _ => TaskboardLane.backlog,
  };
}

DateTime _parseDate(String value) =>
    DateTime.tryParse(value)?.toUtc() ?? DateTime.now().toUtc();
DateTime? _timestampDate(Object? value) {
  final raw = value?.toString();
  if (raw == null || raw.isEmpty) return null;
  final millis = int.tryParse(raw);
  if (millis != null) {
    return DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
  }
  return DateTime.tryParse(raw)?.toUtc();
}

int? _intOrNull(Object? value) =>
    value is num ? value.toInt() : int.tryParse(value?.toString() ?? '');
String _timeLabel(String value) => value.isEmpty ? 'Recently' : 'Updated';

String _uuid() {
  final random = Random.secure();
  final bytes = List<int>.generate(16, (_) => random.nextInt(256));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  final hex = bytes
      .map((value) => value.toRadixString(16).padLeft(2, '0'))
      .join();
  return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-${hex.substring(12, 16)}-${hex.substring(16, 20)}-${hex.substring(20)}';
}

extension _FirstOrNullWhere<T> on Iterable<T> {
  T? firstOrNullWhere(bool Function(T value) test) {
    for (final value in this) {
      if (test(value)) return value;
    }
    return null;
  }
}
