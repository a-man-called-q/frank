enum OrganizationNodeKind { staff, capability, approval }

enum OrganizationCapabilityKind {
  email,
  calendar,
  taskboard,
  drive,
  browser,
  terminal,
  database,
}

enum OrganizationRelationKind { handoff, toolAccess, review }

enum OrganizationContextPolicy {
  minimumRequired,
  summaryAndArtifacts,
  fullContext,
}

/// The visual treatment used by a group header and its border.
enum OrganizationGroupTone { aubergine, blue, amber, neutral }

enum OrganizationIssueSeverity { error, warning }

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

  Map<String, Object?> toJson() => {
    'id': id,
    'label': label,
    'position': position.toJson(),
    'size': size.toJson(),
    'tone': tone.name,
    'locked': locked,
  };

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
    this.capability,
    this.providerLabel,
    this.integrationRef,
    this.profileRef,
    this.configured = false,
    this.approvalRequired = false,
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
  final OrganizationCapabilityKind? capability;

  /// Stable, non-secret connector identity (for example `gmail`).
  final String? integrationRef;

  /// Stable, non-secret account/profile identity (for example `agency-inbox`).
  final String? profileRef;

  /// A display-only provider/profile label. It is never a credential and is
  /// kept alongside the stable references solely for the editor badge.
  final String? providerLabel;
  final bool configured;
  final bool approvalRequired;

  OrganizationNode copyWith({
    String? label,
    OrganizationPoint? position,
    Object? groupId = _unset,
    Object? group = _unset,
    Object? providerLabel = _unset,
    Object? integrationRef = _unset,
    Object? profileRef = _unset,
    bool? configured,
    bool? approvalRequired,
  }) {
    final nextGroupId = identical(groupId, _unset)
        ? identical(group, _unset)
              ? this.groupId
              : (group as OrganizationGroup?)?.id
        : groupId as String?;
    final nextLegacyGroup = identical(group, _unset)
        ? (identical(groupId, _unset) ? _legacyGroup : null)
        : group as OrganizationGroup?;
    return OrganizationNode(
      id: id,
      kind: kind,
      label: label ?? this.label,
      position: position ?? this.position,
      groupId: nextGroupId,
      group: nextLegacyGroup,
      employeeId: employeeId,
      capability: capability,
      providerLabel: identical(providerLabel, _unset)
          ? this.providerLabel
          : providerLabel as String?,
      integrationRef: identical(integrationRef, _unset)
          ? this.integrationRef
          : integrationRef as String?,
      profileRef: identical(profileRef, _unset)
          ? this.profileRef
          : profileRef as String?,
      configured: configured ?? this.configured,
      approvalRequired: approvalRequired ?? this.approvalRequired,
    );
  }

  Map<String, Object?> toJson() => {
    'id': id,
    'kind': kind.name,
    'label': label,
    'position': position.toJson(),
    'groupId': groupId,
    'employeeId': employeeId,
    'capability': capability?.name,
    'providerLabel': providerLabel,
    'integrationRef': integrationRef,
    'profileRef': profileRef,
    'configured': configured,
    'approvalRequired': approvalRequired,
  };

  factory OrganizationNode.fromJson(Map<String, Object?> json) {
    final rawGroupId = json['groupId'] as String? ?? json['group'] as String?;
    return OrganizationNode(
      id: json['id']! as String,
      kind: OrganizationNodeKind.values.byName(json['kind']! as String),
      label: json['label']! as String,
      position: OrganizationPoint.fromJson(
        json['position'] is Map
            ? Map<String, Object?>.from(json['position']! as Map)
            : const <String, Object?>{},
      ),
      groupId: rawGroupId,
      employeeId: json['employeeId'] as String?,
      capability: json['capability'] == null
          ? null
          : OrganizationCapabilityKind.values.byName(
              json['capability']! as String,
            ),
      providerLabel: json['providerLabel'] as String?,
      integrationRef: json['integrationRef'] as String?,
      profileRef: json['profileRef'] as String?,
      configured: json['configured'] as bool? ?? false,
      approvalRequired: json['approvalRequired'] as bool? ?? false,
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
    'expectedOutput': expectedOutput,
    'contextPolicy': contextPolicy.name,
  };

  factory OrganizationHandoffContract.fromJson(Map<String, Object?> json) =>
      OrganizationHandoffContract(
        inputSummary: json['inputSummary'] as String? ?? '',
        expectedOutput: json['expectedOutput'] as String? ?? '',
        contextPolicy: OrganizationContextPolicy.values.byName(
          json['contextPolicy'] as String? ??
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
    'kind': kind.name,
    'sourceNodeId': sourceNodeId,
    'targetNodeId': targetNodeId,
    'contract': contract.toJson(),
    'permissions': permissions,
  };

  factory OrganizationRelation.fromJson(Map<String, Object?> json) =>
      OrganizationRelation(
        id: json['id']! as String,
        kind: OrganizationRelationKind.values.byName(json['kind']! as String),
        sourceNodeId: json['sourceNodeId']! as String,
        targetNodeId: json['targetNodeId']! as String,
        contract: json['contract'] is Map
            ? OrganizationHandoffContract.fromJson(
                Map<String, Object?>.from(json['contract']! as Map),
              )
            : const OrganizationHandoffContract(),
        permissions: List<String>.from(
          json['permissions'] as List? ?? const [],
        ),
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
    'publishedRevision': publishedRevision,
    'nodes': nodes.map((node) => node.toJson()).toList(),
    'relations': relations.map((relation) => relation.toJson()).toList(),
    'groups': groups.map((group) => group.toJson()).toList(),
    'viewport': viewport.toJson(),
  };

  factory OrganizationGraph.fromJson(Map<String, Object?> json) {
    final rawGroups = json['groups'];
    final groups = rawGroups is List
        ? _normalizeGroups(
            rawGroups
                .whereType<Map>()
                .map(
                  (group) => OrganizationGroup.fromJson(
                    Map<String, Object?>.from(group),
                  ),
                )
                .toList(),
          )
        : OrganizationGroup.builtIns;
    return OrganizationGraph(
      id: json['id']! as String,
      draftRevision: (json['draftRevision'] as num?)?.toInt() ?? 0,
      publishedRevision: (json['publishedRevision'] as num?)?.toInt() ?? 0,
      nodes: (json['nodes'] as List? ?? const [])
          .whereType<Map>()
          .map(
            (node) =>
                OrganizationNode.fromJson(Map<String, Object?>.from(node)),
          )
          .toList(),
      relations: (json['relations'] as List? ?? const [])
          .whereType<Map>()
          .map(
            (relation) => OrganizationRelation.fromJson(
              Map<String, Object?>.from(relation),
            ),
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

  static List<OrganizationGroup> _normalizeGroups(
    List<OrganizationGroup>? source,
  ) {
    final result = <OrganizationGroup>[...OrganizationGroup.builtIns];
    final seen = result.map((group) => group.id).toSet();
    for (final group in source ?? const <OrganizationGroup>[]) {
      final id = group.id.trim();
      if (id.isEmpty || seen.contains(id)) continue;
      result.add(
        group.copyWith(
          id: id,
          label: group.label.trim().isEmpty ? id : group.label.trim(),
          locked: false,
        ),
      );
      seen.add(id);
    }
    return result;
  }
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
