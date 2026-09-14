import 'team_models.dart';
import 'workspace_models.dart';

enum OrganizationViewMode { canvas, outline }

enum OrganizationNodeKind {
  staff,
  capability,
  approval,

  /// Executable v2 worker node. It targets a Team role, never a person.
  role,

  /// Durable shared hand-off surface used by the taskboard broker.
  taskboard,

  /// One-level workflow composition/navigation node.
  childWorkflow,
}

extension OrganizationNodeKindWire on OrganizationNodeKind {
  String get wireName => switch (this) {
    OrganizationNodeKind.staff => 'staff',
    OrganizationNodeKind.capability => 'capability',
    OrganizationNodeKind.approval => 'approval',
    OrganizationNodeKind.role => 'role',
    OrganizationNodeKind.taskboard => 'taskboard',
    OrganizationNodeKind.childWorkflow => 'child_workflow',
  };

  static OrganizationNodeKind fromWire(Object? value) => switch (value) {
    'staff' => OrganizationNodeKind.staff,
    'capability' => OrganizationNodeKind.capability,
    'approval' => OrganizationNodeKind.approval,
    'role' => OrganizationNodeKind.role,
    'taskboard' => OrganizationNodeKind.taskboard,
    'child_workflow' || 'childWorkflow' => OrganizationNodeKind.childWorkflow,
    // Legacy snapshots should never fail the whole Organization surface when
    // a future node kind is introduced. Keep the node visible as a neutral
    // role-like card until the client learns that kind.
    _ => OrganizationNodeKind.role,
  };
}

enum OrganizationCapabilityKind {
  email,
  calendar,
  drive,
  browser,
  terminal,
  database,
}

extension OrganizationCapabilityKindJson on OrganizationCapabilityKind {
  String get wireName => name;

  static OrganizationCapabilityKind? fromWire(Object? value) => switch (value) {
    'email' => OrganizationCapabilityKind.email,
    'calendar' => OrganizationCapabilityKind.calendar,
    'drive' => OrganizationCapabilityKind.drive,
    'browser' => OrganizationCapabilityKind.browser,
    'terminal' => OrganizationCapabilityKind.terminal,
    'database' => OrganizationCapabilityKind.database,
    // The legacy taskboard capability is intentionally not represented in
    // the production capability union.
    'taskboard' => null,
    _ => null,
  };
}

enum OrganizationRelationKind {
  handoff,
  toolAccess,
  review,
  pickup,
  drop,
  rework,
}

extension OrganizationRelationKindWire on OrganizationRelationKind {
  String get wireName => switch (this) {
    OrganizationRelationKind.handoff => 'handoff',
    OrganizationRelationKind.toolAccess => 'tool_access',
    OrganizationRelationKind.review => 'review',
    OrganizationRelationKind.pickup => 'pickup',
    OrganizationRelationKind.drop => 'drop',
    OrganizationRelationKind.rework => 'rework',
  };

  static OrganizationRelationKind fromWire(Object? value) => switch (value) {
    'handoff' => OrganizationRelationKind.handoff,
    'tool_access' || 'toolAccess' => OrganizationRelationKind.toolAccess,
    'review' => OrganizationRelationKind.review,
    'pickup' => OrganizationRelationKind.pickup,
    'drop' => OrganizationRelationKind.drop,
    'rework' => OrganizationRelationKind.rework,
    _ => OrganizationRelationKind.handoff,
  };
}

enum OrganizationContextPolicy {
  minimumRequired,
  summaryAndArtifacts,
  fullContext,
}

/// The visual treatment used by a group header and its border.
enum OrganizationGroupTone { aubergine, blue, amber, neutral }

enum OrganizationIssueSeverity { error, warning }

enum ConnectorKind {
  googleWorkspace,
  unknown,
  browser,
  terminal,
  postgres,
  sqlite,
}

enum ConnectorHealth { unknown, healthy, degraded, unhealthy }

extension ConnectorKindJson on ConnectorKind {
  String get wireName => switch (this) {
    ConnectorKind.googleWorkspace => 'google_workspace',
    ConnectorKind.unknown => 'unknown',
    ConnectorKind.browser => 'browser',
    ConnectorKind.terminal => 'terminal',
    ConnectorKind.postgres => 'postgres',
    ConnectorKind.sqlite => 'sqlite',
  };

  static ConnectorKind fromWire(Object? value) => switch (value) {
    'google_workspace' || 'googleWorkspace' => ConnectorKind.googleWorkspace,
    'unknown' => ConnectorKind.unknown,
    'browser' => ConnectorKind.browser,
    'terminal' => ConnectorKind.terminal,
    'postgres' => ConnectorKind.postgres,
    'sqlite' => ConnectorKind.sqlite,
    _ => ConnectorKind.unknown,
  };
}

extension ConnectorHealthJson on ConnectorHealth {
  String get wireName => name;

  static ConnectorHealth fromWire(Object? value) => switch (value) {
    'healthy' => ConnectorHealth.healthy,
    'degraded' => ConnectorHealth.degraded,
    'unhealthy' => ConnectorHealth.unhealthy,
    _ => ConnectorHealth.unknown,
  };
}

/// Non-secret connector metadata returned in the authenticated snapshot.
/// Tokens, cookies, passwords and DSNs are intentionally never represented by
/// this value object; the daemon keeps them in its credential store.
class ConnectorProfile {
  const ConnectorProfile({
    required this.id,
    required this.name,
    required this.kind,
    this.config = const <String, Object?>{},
    this.health = ConnectorHealth.unknown,
    this.configured = false,
    this.diagnostic,
    this.checkedAt,
    this.archived = false,
  });

  final String id;
  final String name;
  final ConnectorKind kind;
  final Map<String, Object?> config;
  final ConnectorHealth health;
  final bool configured;
  final String? diagnostic;
  final DateTime? checkedAt;
  final bool archived;

  Map<String, Object?> toJson() {
    final json = <String, Object?>{
      'id': id,
      'name': name,
      'kind': kind.wireName,
      'config': config,
      'health': health.wireName,
      'configured': configured,
      'diagnostic': diagnostic,
      'checked_at': checkedAt?.toIso8601String(),
      'archived': archived,
    };
    return json;
  }

  factory ConnectorProfile.fromJson(Map<String, Object?> json) =>
      ConnectorProfile(
        id: json['id'] as String? ?? '',
        name: json['name'] as String? ?? 'Connector',
        kind: ConnectorKindJson.fromWire(json['kind']),
        config: json['config'] is Map
            ? Map<String, Object?>.from(json['config']! as Map)
            : const <String, Object?>{},
        health: ConnectorHealthJson.fromWire(json['health']),
        configured: json['configured'] as bool? ?? false,
        diagnostic: json['diagnostic'] as String?,
        checkedAt: DateTime.tryParse(json['checked_at'] as String? ?? ''),
        archived: json['archived'] as bool? ?? false,
      );
}

class OrganizationPoint {
  const OrganizationPoint(this.x, this.y);

  final double x;
  final double y;

  @override
  bool operator ==(Object other) =>
      other is OrganizationPoint && other.x == x && other.y == y;

  @override
  int get hashCode => Object.hash(x, y);

  Map<String, Object?> toJson() => {'x': x, 'y': y};

  factory OrganizationPoint.fromJson(Map<String, Object?> json) =>
      OrganizationPoint(
        (json['x'] as num?)?.toDouble() ?? 0,
        (json['y'] as num?)?.toDouble() ?? 0,
      );
}

class OrganizationSize {
  const OrganizationSize(this.width, this.height);

  final double width;
  final double height;

  OrganizationSize copyWith({double? width, double? height}) =>
      OrganizationSize(width ?? this.width, height ?? this.height);

  @override
  bool operator ==(Object other) =>
      other is OrganizationSize &&
      other.width == width &&
      other.height == height;

  @override
  int get hashCode => Object.hash(width, height);

  Map<String, Object?> toJson() => {'width': width, 'height': height};

  factory OrganizationSize.fromJson(Map<String, Object?> json) =>
      OrganizationSize(
        (json['width'] as num?)?.toDouble() ?? 420,
        (json['height'] as num?)?.toDouble() ?? 300,
      );
}

/// A persisted group on the organization board.
///
/// The three built-ins retain their historical ids so existing organization
/// documents can be read without a migration. Custom groups are represented
/// by the same value object, but are never locked by model normalization.
class OrganizationGroup {
  const OrganizationGroup({
    required this.id,
    required this.label,
    required this.position,
    required this.size,
    this.tone = OrganizationGroupTone.neutral,
    this.locked = false,
  });

  static const clientServices = OrganizationGroup(
    id: 'clientServices',
    label: 'CLIENT SERVICES',
    position: OrganizationPoint(0, 20),
    size: OrganizationSize(450, 390),
    tone: OrganizationGroupTone.aubergine,
    locked: true,
  );

  static const delivery = OrganizationGroup(
    id: 'delivery',
    label: 'DELIVERY',
    position: OrganizationPoint(470, 20),
    size: OrganizationSize(850, 450),
    tone: OrganizationGroupTone.blue,
    locked: true,
  );

  static const operationsReview = OrganizationGroup(
    id: 'operationsReview',
    label: 'OPERATIONS & REVIEW',
    position: OrganizationPoint(1340, 20),
    size: OrganizationSize(450, 450),
    tone: OrganizationGroupTone.amber,
    locked: true,
  );

  static const builtIns = <OrganizationGroup>[
    clientServices,
    delivery,
    operationsReview,
  ];

  final String id;
  final String label;
  final OrganizationPoint position;
  final OrganizationSize size;
  final OrganizationGroupTone tone;
  final bool locked;

  String get name => id;

  bool get isBuiltIn => builtInById(id) != null;

  static OrganizationGroup? builtInById(String? id) {
    for (final group in builtIns) {
      if (group.id == id) return group;
    }
    return null;
  }

  OrganizationGroup copyWith({
    String? id,
    String? label,
    OrganizationPoint? position,
    OrganizationSize? size,
    OrganizationGroupTone? tone,
    bool? locked,
  }) => OrganizationGroup(
    id: id ?? this.id,
    label: label ?? this.label,
    position: position ?? this.position,
    size: size ?? this.size,
    tone: tone ?? this.tone,
    locked: locked ?? this.locked,
  );

  @override
  bool operator ==(Object other) =>
      other is OrganizationGroup &&
      other.id == id &&
      other.label == label &&
      other.position == position &&
      other.size == size &&
      other.tone == tone &&
      other.locked == locked;

  @override
  int get hashCode => Object.hash(id, label, position, size, tone, locked);

  Map<String, Object?> toJson() {
    final json = <String, Object?>{
      'id': id,
      'label': label,
      'position': position.toJson(),
      'size': size.toJson(),
      'tone': tone.name,
      'locked': locked,
    };
    return json;
  }

  factory OrganizationGroup.fromJson(Map<String, Object?> json) {
    final id = json['id'] as String? ?? '';
    final builtIn = builtInById(id);
    if (builtIn != null) return builtIn;

    var tone = OrganizationGroupTone.neutral;
    final rawTone = json['tone'] as String?;
    for (final value in OrganizationGroupTone.values) {
      if (value.name == rawTone) {
        tone = value;
        break;
      }
    }

    return OrganizationGroup(
      id: id,
      label: json['label'] as String? ?? id,
      position: json['position'] is Map
          ? OrganizationPoint.fromJson(
              Map<String, Object?>.from(json['position']! as Map),
            )
          : const OrganizationPoint(0, 0),
      size: json['size'] is Map
          ? OrganizationSize.fromJson(
              Map<String, Object?>.from(json['size']! as Map),
            )
          : const OrganizationSize(420, 300),
      tone: tone,
      // Built-ins are canonical and custom groups cannot be locked by a
      // persisted document. This prevents malformed data from making a
      // custom group permanently uneditable.
      locked: false,
    );
  }
}

class OrganizationViewport {
  const OrganizationViewport({this.x = 0, this.y = 0, this.zoom = 1});

  final double x;
  final double y;
  final double zoom;

  Map<String, Object?> toJson() => {'x': x, 'y': y, 'zoom': zoom};

  factory OrganizationViewport.fromJson(Map<String, Object?> json) =>
      OrganizationViewport(
        x: (json['x'] as num?)?.toDouble() ?? 0,
        y: (json['y'] as num?)?.toDouble() ?? 0,
        zoom: (json['zoom'] as num?)?.toDouble() ?? 1,
      );
}

class OrganizationNode {
  static const _unset = Object();

  const OrganizationNode({
    required this.id,
    required this.kind,
    required this.label,
    required this.position,
    String? groupId,
    OrganizationGroup? group,
    this.employeeId,
    this.connectorProfileId,
    this.capability,
    this.connectorProfileLabel,
    this.integrationRef,
    this.profileRef,
    this.configured = false,
    this.approvalRequired = false,
    this.roleId,
    this.taskboardId,
    this.childWorkflowId,
    this.inputPort,
    this.outputPort,
    this.reworkLimit,
  }) : _storedGroupId = groupId,
       _legacyGroup = group;

  final String id;
  final OrganizationNodeKind kind;
  final String label;
  final OrganizationPoint position;

  /// Stable persisted group id. Null means the node is not in a group.
  final String? _storedGroupId;

  String? get groupId => _storedGroupId ?? _legacyGroup?.id;

  // Kept only for source compatibility with the original fixture API. New
  // values should use groupId; the getter still lets old UI code compile.
  final OrganizationGroup? _legacyGroup;

  @Deprecated('Use groupId for persisted organization membership.')
  OrganizationGroup? get group =>
      _legacyGroup ?? OrganizationGroup.builtInById(groupId);

  final String? employeeId;

  /// Stable server-side connector profile selected for a capability node.
  final String? connectorProfileId;
  final OrganizationCapabilityKind? capability;

  /// Stable, non-secret connector identity (for example `gmail`).
  final String? integrationRef;

  /// Stable, non-secret account/profile identity (for example `agency-inbox`).
  final String? profileRef;

  /// A display-only provider/profile label. It is never a credential and is
  /// kept alongside the stable references solely for the editor badge.
  final String? connectorProfileLabel;

  final bool configured;
  final bool approvalRequired;

  /// v2 executable worker reference. Roles are shared across multiple
  /// contextual nodes and are resolved from the Team role catalog.
  final String? roleId;

  /// v2 durable board reference. Work cards retain their id while moving
  /// between these board surfaces.
  final String? taskboardId;

  /// v2 one-level child workflow reference. This is composition/navigation,
  /// not a second runtime or provider mailbox.
  final String? childWorkflowId;
  final String? inputPort;
  final String? outputPort;
  final int? reworkLimit;

  OrganizationNode copyWith({
    String? label,
    OrganizationPoint? position,
    Object? groupId = _unset,
    Object? group = _unset,
    Object? connectorProfileId = _unset,
    Object? connectorProfileLabel = _unset,
    Object? integrationRef = _unset,
    Object? profileRef = _unset,
    bool? configured,
    bool? approvalRequired,
    Object? roleId = _unset,
    Object? taskboardId = _unset,
    Object? childWorkflowId = _unset,
    Object? inputPort = _unset,
    Object? outputPort = _unset,
    Object? reworkLimit = _unset,
  }) {
    final nextGroupId = _nextGroupId(groupId, group);
    final nextLegacyGroup = _nextLegacyGroup(groupId, group);
    return OrganizationNode(
      id: id,
      kind: kind,
      label: label ?? this.label,
      position: position ?? this.position,
      groupId: nextGroupId,
      group: nextLegacyGroup,
      employeeId: employeeId,
      connectorProfileId: identical(connectorProfileId, _unset)
          ? this.connectorProfileId
          : connectorProfileId as String?,
      capability: capability,
      connectorProfileLabel: identical(connectorProfileLabel, _unset)
          ? this.connectorProfileLabel
          : connectorProfileLabel as String?,
      integrationRef: identical(integrationRef, _unset)
          ? this.integrationRef
          : integrationRef as String?,
      profileRef: identical(profileRef, _unset)
          ? this.profileRef
          : profileRef as String?,
      configured: configured ?? this.configured,
      approvalRequired: approvalRequired ?? this.approvalRequired,
      roleId: identical(roleId, _unset) ? this.roleId : roleId as String?,
      taskboardId: identical(taskboardId, _unset)
          ? this.taskboardId
          : taskboardId as String?,
      childWorkflowId: identical(childWorkflowId, _unset)
          ? this.childWorkflowId
          : childWorkflowId as String?,
      inputPort: identical(inputPort, _unset)
          ? this.inputPort
          : inputPort as String?,
      outputPort: identical(outputPort, _unset)
          ? this.outputPort
          : outputPort as String?,
      reworkLimit: identical(reworkLimit, _unset)
          ? this.reworkLimit
          : reworkLimit as int?,
    );
  }

  String? _nextGroupId(Object? groupId, Object? group) {
    if (!identical(groupId, _unset)) return groupId as String?;
    if (!identical(group, _unset)) {
      return (group as OrganizationGroup?)?.id;
    }
    return this.groupId;
  }

  OrganizationGroup? _nextLegacyGroup(Object? groupId, Object? group) {
    if (!identical(group, _unset)) return group as OrganizationGroup?;
    return identical(groupId, _unset) ? _legacyGroup : null;
  }

  Map<String, Object?> toJson() {
    final json = <String, Object?>{
      'id': id,
      'kind': kind.wireName,
      'label': label,
      'position': position.toJson(),
      if (groupId != null) 'group_id': groupId,
    };
    switch (kind) {
      case OrganizationNodeKind.staff:
        if (employeeId != null) json['agent_id'] = employeeId;
      case OrganizationNodeKind.capability:
        if (capability != null) json['capability'] = capability!.wireName;
        if (connectorProfileId != null) {
          json['connector_profile_id'] = connectorProfileId;
        }
        if (connectorProfileLabel != null) {
          json['connector_profile_label'] = connectorProfileLabel;
        }
        if (integrationRef != null) json['integration_ref'] = integrationRef;
        if (profileRef != null) json['profile_ref'] = profileRef;
        json['configured'] = configured;
        json['approval_required'] = approvalRequired;
      case OrganizationNodeKind.role:
        if (roleId != null) json['role_id'] = roleId;
      case OrganizationNodeKind.taskboard:
        if (taskboardId != null) json['taskboard_id'] = taskboardId;
      case OrganizationNodeKind.childWorkflow:
        if (childWorkflowId != null) {
          json['child_workflow_id'] = childWorkflowId;
        }
        if (inputPort != null) json['input_port'] = inputPort;
        if (outputPort != null) json['output_port'] = outputPort;
        if (reworkLimit != null) json['rework_limit'] = reworkLimit;
      case OrganizationNodeKind.approval:
        // Approval nodes are read-only legacy data. New graphs use taskboard
        // state/feed events instead of authoring this node kind.
        json['approval_required'] = approvalRequired;
    }
    return json;
  }

  factory OrganizationNode.fromJson(Map<String, Object?> json) {
    final rawGroupId = json.containsKey('groupId')
        ? json['groupId'] as String?
        : json.containsKey('group')
        ? json['group'] as String?
        : json['group_id'] as String?;
    final kind = OrganizationNodeKindWire.fromWire(json['kind']);
    final isCapability = kind == OrganizationNodeKind.capability;
    final isTaskboard = kind == OrganizationNodeKind.taskboard;
    return OrganizationNode(
      id: json['id']! as String,
      kind: kind,
      label: json['label']! as String,
      position: OrganizationPoint.fromJson(
        json['position'] is Map
            ? Map<String, Object?>.from(json['position']! as Map)
            : const <String, Object?>{},
      ),
      groupId: rawGroupId,
      employeeId: kind == OrganizationNodeKind.staff
          ? json['employeeId'] as String? ?? json['agent_id'] as String?
          : null,
      connectorProfileId: isCapability
          ? json['connectorProfileId'] as String? ??
                json['connector_profile_id'] as String?
          : null,
      capability: isCapability
          ? OrganizationCapabilityKindJson.fromWire(json['capability'])
          : null,
      connectorProfileLabel: isCapability
          ? json['connectorProfileLabel'] as String? ??
                json['connector_profile_label'] as String? ??
                json['providerLabel'] as String?
          : null,
      integrationRef: isCapability
          ? json['integrationRef'] as String? ??
                json['integration_ref'] as String?
          : null,
      profileRef: isCapability
          ? json['profileRef'] as String? ?? json['profile_ref'] as String?
          : null,
      configured: isCapability
          ? json['configured'] as bool? ??
                json['is_configured'] as bool? ??
                false
          : false,
      approvalRequired: isCapability
          ? json['approvalRequired'] as bool? ??
                json['approval_required'] as bool? ??
                false
          : false,
      roleId: kind == OrganizationNodeKind.role
          ? json['roleId'] as String? ?? json['role_id'] as String?
          : null,
      taskboardId: isTaskboard
          ? json['taskboardId'] as String? ?? json['taskboard_id'] as String?
          : null,
      childWorkflowId: kind == OrganizationNodeKind.childWorkflow
          ? json['childWorkflowId'] as String? ??
                json['child_workflow_id'] as String?
          : null,
      inputPort: kind == OrganizationNodeKind.childWorkflow
          ? json['inputPort'] as String? ?? json['input_port'] as String?
          : null,
      outputPort: kind == OrganizationNodeKind.childWorkflow
          ? json['outputPort'] as String? ?? json['output_port'] as String?
          : null,
      reworkLimit: kind == OrganizationNodeKind.childWorkflow
          ? (json['reworkLimit'] as num?)?.toInt() ??
                (json['rework_limit'] as num?)?.toInt()
          : null,
    );
  }
}

class OrganizationHandoffContract {
  const OrganizationHandoffContract({
    this.inputSummary = 'Task brief and relevant artifacts',
    this.expectedOutput = 'A concise, reviewable deliverable',
    this.contextPolicy = OrganizationContextPolicy.minimumRequired,
  });

  final String inputSummary;
  final String expectedOutput;
  final OrganizationContextPolicy contextPolicy;

  OrganizationHandoffContract copyWith({
    String? inputSummary,
    String? expectedOutput,
    OrganizationContextPolicy? contextPolicy,
  }) => OrganizationHandoffContract(
    inputSummary: inputSummary ?? this.inputSummary,
    expectedOutput: expectedOutput ?? this.expectedOutput,
    contextPolicy: contextPolicy ?? this.contextPolicy,
  );

  Map<String, Object?> toJson() => {
    'inputSummary': inputSummary,
    'input_summary': inputSummary,
    'expectedOutput': expectedOutput,
    'expected_output': expectedOutput,
    'contextPolicy': contextPolicy.name,
    'context_policy': contextPolicy.name,
  };

  factory OrganizationHandoffContract.fromJson(Map<String, Object?> json) =>
      OrganizationHandoffContract(
        inputSummary:
            json['inputSummary'] as String? ??
            json['input_summary'] as String? ??
            '',
        expectedOutput:
            json['expectedOutput'] as String? ??
            json['expected_output'] as String? ??
            '',
        contextPolicy: OrganizationContextPolicy.values.byName(
          json['contextPolicy'] as String? ??
              json['context_policy'] as String? ??
              OrganizationContextPolicy.minimumRequired.name,
        ),
      );
}

class OrganizationRelation {
  const OrganizationRelation({
    required this.id,
    required this.kind,
    required this.sourceNodeId,
    required this.targetNodeId,
    this.contract = const OrganizationHandoffContract(),
    this.permissions = const [],
  });

  final String id;
  final OrganizationRelationKind kind;
  final String sourceNodeId;
  final String targetNodeId;
  final OrganizationHandoffContract contract;
  final List<String> permissions;

  OrganizationRelation copyWith({
    OrganizationHandoffContract? contract,
    List<String>? permissions,
  }) => OrganizationRelation(
    id: id,
    kind: kind,
    sourceNodeId: sourceNodeId,
    targetNodeId: targetNodeId,
    contract: contract ?? this.contract,
    permissions: List.unmodifiable(permissions ?? this.permissions),
  );

  Map<String, Object?> toJson() => {
    'id': id,
    'kind': kind.wireName,
    'sourceNodeId': sourceNodeId,
    'source_node_id': sourceNodeId,
    'targetNodeId': targetNodeId,
    'target_node_id': targetNodeId,
    'contract': contract.toJson(),
    'permissions': permissions,
  };

  factory OrganizationRelation.fromJson(
    Map<String, Object?> json,
  ) => OrganizationRelation(
    id: json['id']! as String,
    kind: OrganizationRelationKindWire.fromWire(json['kind']),
    sourceNodeId:
        json['sourceNodeId'] as String? ?? json['source_node_id']! as String,
    targetNodeId:
        json['targetNodeId'] as String? ?? json['target_node_id']! as String,
    contract: json['contract'] is Map
        ? OrganizationHandoffContract.fromJson(
            Map<String, Object?>.from(json['contract']! as Map),
          )
        : const OrganizationHandoffContract(),
    permissions: List<String>.from(json['permissions'] as List? ?? const []),
  );
}

class OrganizationGraph {
  const OrganizationGraph({
    required this.id,
    required this.draftRevision,
    required this.publishedRevision,
    required this.nodes,
    required this.relations,
    this.groups = OrganizationGroup.builtIns,
    this.viewport = const OrganizationViewport(),
  });

  final String id;
  final int draftRevision;
  final int publishedRevision;
  final List<OrganizationNode> nodes;
  final List<OrganizationRelation> relations;
  final List<OrganizationGroup> groups;
  final OrganizationViewport viewport;

  OrganizationGroup? groupById(String? groupId) {
    if (groupId == null) return null;
    for (final group in groups) {
      if (group.id == groupId) return group;
    }
    return null;
  }

  OrganizationGraph copyWith({
    int? draftRevision,
    int? publishedRevision,
    List<OrganizationNode>? nodes,
    List<OrganizationRelation>? relations,
    List<OrganizationGroup>? groups,
    OrganizationViewport? viewport,
  }) => OrganizationGraph(
    id: id,
    draftRevision: draftRevision ?? this.draftRevision,
    publishedRevision: publishedRevision ?? this.publishedRevision,
    nodes: nodes ?? this.nodes,
    relations: relations ?? this.relations,
    groups: groups ?? this.groups,
    viewport: viewport ?? this.viewport,
  );

  Map<String, Object?> toJson() => {
    'id': id,
    'draftRevision': draftRevision,
    'draft_revision': draftRevision,
    'publishedRevision': publishedRevision,
    'published_revision': publishedRevision,
    'nodes': nodes.map((node) => node.toJson()).toList(),
    'relations': relations.map((relation) => relation.toJson()).toList(),
    'groups': groups.map((group) => group.toJson()).toList(),
    'viewport': viewport.toJson(),
  };

  factory OrganizationGraph.fromJson(Map<String, Object?> json) {
    final rawGroups = json['groups'];
    final groups = rawGroups is List
        ? rawGroups
              .whereType<Map>()
              .map(
                (group) => OrganizationGroup.fromJson(
                  Map<String, Object?>.from(group),
                ),
              )
              .where((group) => group.id.trim().isNotEmpty)
              .toList(growable: false)
        : const <OrganizationGroup>[];
    final parsedNodes = (json['nodes'] as List? ?? const [])
        .whereType<Map>()
        .map(
          (node) => OrganizationNode.fromJson(Map<String, Object?>.from(node)),
        )
        // Human approval is a taskboard state/feed now, not an authorable
        // Organization node. Ignore retired nodes from stale remote payloads
        // so the editor cannot resurrect the removed approval surface.
        .where((node) => node.kind != OrganizationNodeKind.approval)
        .toList(growable: false);
    final nodeIds = parsedNodes.map((node) => node.id).toSet();
    return OrganizationGraph(
      id: json['id']! as String,
      draftRevision:
          (json['draftRevision'] as num?)?.toInt() ??
          (json['draft_revision'] as num?)?.toInt() ??
          0,
      publishedRevision:
          (json['publishedRevision'] as num?)?.toInt() ??
          (json['published_revision'] as num?)?.toInt() ??
          0,
      nodes: parsedNodes,
      relations: (json['relations'] as List? ?? const [])
          .whereType<Map>()
          .map(
            (relation) => OrganizationRelation.fromJson(
              Map<String, Object?>.from(relation),
            ),
          )
          .where(
            (relation) =>
                nodeIds.contains(relation.sourceNodeId) &&
                nodeIds.contains(relation.targetNodeId),
          )
          .toList(),
      groups: groups,
      viewport: json['viewport'] is Map
          ? OrganizationViewport.fromJson(
              Map<String, Object?>.from(json['viewport']! as Map),
            )
          : const OrganizationViewport(),
    );
  }
}

/// Immutable O(1) lookup tables for Organization rendering and inspection.
/// Build once for a graph/revision and reuse across node builders; preserving
/// the source list order keeps render semantics identical to the old scans.
class OrganizationLookupIndex {
  OrganizationLookupIndex._({
    required this.nodeById,
    required this.employeeById,
    required this.profileByEmployeeId,
    required this.connectorProfileById,
    required this.outgoingRelationsByNodeId,
    required this.incomingRelationsByNodeId,
  });

  factory OrganizationLookupIndex.build({
    required OrganizationGraph graph,
    required List<OfficeEmployee> employees,
    required List<TeamAgentProfile> profiles,
    required List<ConnectorProfile> connectorProfiles,
  }) {
    final outgoing = <String, List<OrganizationRelation>>{};
    final incoming = <String, List<OrganizationRelation>>{};
    for (final relation in graph.relations) {
      outgoing.putIfAbsent(relation.sourceNodeId, () => []).add(relation);
      incoming.putIfAbsent(relation.targetNodeId, () => []).add(relation);
    }
    return OrganizationLookupIndex._(
      nodeById: {for (final node in graph.nodes) node.id: node},
      employeeById: {for (final employee in employees) employee.id: employee},
      profileByEmployeeId: {
        for (final profile in profiles) profile.employeeId: profile,
      },
      connectorProfileById: {
        for (final profile in connectorProfiles) profile.id: profile,
      },
      outgoingRelationsByNodeId: {
        for (final entry in outgoing.entries)
          entry.key: List.unmodifiable(entry.value),
      },
      incomingRelationsByNodeId: {
        for (final entry in incoming.entries)
          entry.key: List.unmodifiable(entry.value),
      },
    );
  }

  final Map<String, OrganizationNode> nodeById;
  final Map<String, OfficeEmployee> employeeById;
  final Map<String, TeamAgentProfile> profileByEmployeeId;
  final Map<String, ConnectorProfile> connectorProfileById;
  final Map<String, List<OrganizationRelation>> outgoingRelationsByNodeId;
  final Map<String, List<OrganizationRelation>> incomingRelationsByNodeId;
}

class OrganizationValidationIssue {
  const OrganizationValidationIssue({
    required this.severity,
    required this.message,
    this.nodeId,
    this.relationId,
    this.groupId,
  });

  final OrganizationIssueSeverity severity;
  final String message;
  final String? nodeId;
  final String? relationId;
  final String? groupId;

  Map<String, Object?> toJson() => {
    'severity': severity.name,
    'message': message,
    'nodeId': nodeId,
    'relationId': relationId,
    'groupId': groupId,
  };

  factory OrganizationValidationIssue.fromJson(Map<String, Object?> json) =>
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.values.byName(
          json['severity']! as String,
        ),
        message: json['message']! as String,
        nodeId: json['nodeId'] as String?,
        relationId: json['relationId'] as String?,
        groupId: json['groupId'] as String?,
      );
}

class OrganizationValidation {
  const OrganizationValidation(this.issues);

  final List<OrganizationValidationIssue> issues;

  List<OrganizationValidationIssue> get errors => issues
      .where((issue) => issue.severity == OrganizationIssueSeverity.error)
      .toList(growable: false);

  List<OrganizationValidationIssue> get warnings => issues
      .where((issue) => issue.severity == OrganizationIssueSeverity.warning)
      .toList(growable: false);

  int get errorCount => errors.length;

  bool get hasErrors =>
      issues.any((issue) => issue.severity == OrganizationIssueSeverity.error);

  int get warningCount => warnings.length;

  Map<String, Object?> toJson() => {
    'issues': issues.map((issue) => issue.toJson()).toList(),
  };

  factory OrganizationValidation.fromJson(Map<String, Object?> json) =>
      OrganizationValidation(
        (json['issues'] as List? ?? const [])
            .whereType<Map>()
            .map(
              (issue) => OrganizationValidationIssue.fromJson(
                Map<String, Object?>.from(issue),
              ),
            )
            .toList(),
      );
}

class OrganizationRevisionConflict implements Exception {
  const OrganizationRevisionConflict(this.expected, this.actual);

  final int expected;
  final int actual;

  @override
  String toString() =>
      'Organization changed elsewhere (expected revision $expected, actual $actual).';
}
