import '../../core/models/organization_models.dart';
import 'organization_catalog.dart';

OrganizationValidation validateOrganization(OrganizationGraph graph) {
  final issues = <OrganizationValidationIssue>[];
  final nodesById = <String, OrganizationNode>{};
  final groupsById = <String, OrganizationGroup>{};

  for (final group in graph.groups) {
    if (group.id.trim().isEmpty) {
      issues.add(
        const OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'A group must have an ID.',
        ),
      );
      continue;
    }
    if (groupsById.containsKey(group.id)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Duplicate group ID “${group.id}”.',
          groupId: group.id,
        ),
      );
    }
    if (group.label.trim().isEmpty) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Group “${group.id}” is missing its name.',
          groupId: group.id,
        ),
      );
    }
    groupsById[group.id] = group;
  }

  for (final node in graph.nodes) {
    if (nodesById.containsKey(node.id)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Duplicate node ID “${node.id}”.',
          nodeId: node.id,
        ),
      );
    }
    nodesById[node.id] = node;
  }

  final staffByEmployeeId = <String, OrganizationNode>{};
  for (final node in graph.nodes.where(
    (node) => node.kind == OrganizationNodeKind.staff,
  )) {
    final employeeId = node.employeeId;
    if (employeeId == null || employeeId.isEmpty) continue;
    final previous = staffByEmployeeId[employeeId];
    if (previous != null) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message:
              'Employee “$employeeId” appears more than once in the agency.',
          nodeId: node.id,
        ),
      );
    } else {
      staffByEmployeeId[employeeId] = node;
    }
  }

  if (!graph.nodes.any((node) => node.kind == OrganizationNodeKind.staff)) {
    issues.add(
      const OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.error,
        message: 'Add at least one staff member before publishing.',
      ),
    );
  }

  final relationIds = <String>{};
  final connectedNodeIds = <String>{};
  for (final relation in graph.relations) {
    if (!relationIds.add(relation.id)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Duplicate relation ID “${relation.id}”.',
          relationId: relation.id,
        ),
      );
    }

    final source = nodesById[relation.sourceNodeId];
    final target = nodesById[relation.targetNodeId];
    if (source == null || target == null) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Relation “${relation.id}” points to a missing node.',
          relationId: relation.id,
        ),
      );
      continue;
    }
    if (source.id == target.id) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'A node cannot connect to itself.',
          relationId: relation.id,
        ),
      );
      continue;
    }
    if (!_isValidPair(relation.kind, source.kind, target.kind)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message:
              '${relation.kind.label} cannot connect ${source.kind.label} to ${target.kind.label}.',
          relationId: relation.id,
        ),
      );
      continue;
    }
    if (relation.kind == OrganizationRelationKind.toolAccess &&
        target.capability != null) {
      final allowed = target.capability!.permissions.toSet();
      if (relation.permissions.isEmpty) {
        issues.add(
          OrganizationValidationIssue(
            severity: OrganizationIssueSeverity.error,
            message: 'Tool access for “${target.label}” needs a permission.',
            relationId: relation.id,
          ),
        );
      }
      final invalid = relation.permissions
          .where((permission) => !allowed.contains(permission))
          .toList();
      if (invalid.isNotEmpty) {
        issues.add(
          OrganizationValidationIssue(
            severity: OrganizationIssueSeverity.error,
            message:
                'Permission “${invalid.join(', ')}” is not available on “${target.label}”.',
            relationId: relation.id,
          ),
        );
      }
    }
    connectedNodeIds
      ..add(source.id)
      ..add(target.id);
  }

  for (final node in graph.nodes) {
    if (node.kind == OrganizationNodeKind.staff &&
        (node.employeeId == null || node.employeeId!.isEmpty)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Staff “${node.label}” is missing its employee reference.',
          nodeId: node.id,
        ),
      );
    }
    if (node.kind == OrganizationNodeKind.capability &&
        node.capability == null) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: 'Capability “${node.label}” is missing its capability kind.',
          nodeId: node.id,
        ),
      );
    }
    if (!connectedNodeIds.contains(node.id)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.warning,
          message: '“${node.label}” is not connected to the agency flow.',
          nodeId: node.id,
        ),
      );
    }
    if (node.kind == OrganizationNodeKind.capability && !node.configured) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.warning,
          message: '“${node.label}” still needs a connection profile.',
          nodeId: node.id,
        ),
      );
    }
    if (node.groupId != null && !groupsById.containsKey(node.groupId)) {
      issues.add(
        OrganizationValidationIssue(
          severity: OrganizationIssueSeverity.error,
          message: '“${node.label}” points to a missing group.',
          nodeId: node.id,
          groupId: node.groupId,
        ),
      );
    }
  }

  final handoffs = graph.relations
      .where((relation) => relation.kind == OrganizationRelationKind.handoff)
      .toList();
  final adjacency = <String, List<OrganizationRelation>>{};
  for (final relation in handoffs) {
    adjacency.putIfAbsent(relation.sourceNodeId, () => []).add(relation);
  }
  final visiting = <String>{};
  final visited = <String>{};
  OrganizationRelation? loopRelation;

  bool visit(String nodeId) {
    if (visiting.contains(nodeId)) return true;
    if (!visited.add(nodeId)) return false;
    visiting.add(nodeId);
    for (final relation in adjacency[nodeId] ?? const []) {
      if (visit(relation.targetNodeId)) {
        loopRelation ??= relation;
        return true;
      }
    }
    visiting.remove(nodeId);
    return false;
  }

  for (final node in graph.nodes) {
    if (loopRelation != null) break;
    visit(node.id);
  }
  if (loopRelation != null) {
    issues.add(
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.warning,
        message:
            'Handoff loop detected. It is allowed here, but runtime task DAGs remain acyclic.',
        relationId: loopRelation!.id,
      ),
    );
  }

  return OrganizationValidation(List.unmodifiable(issues));
}

bool isValidOrganizationConnection({
  required OrganizationNode source,
  required OrganizationNode target,
}) => inferOrganizationRelationKind(source: source, target: target) != null;

OrganizationRelationKind? inferOrganizationRelationKind({
  required OrganizationNode source,
  required OrganizationNode target,
}) {
  if (source.id == target.id) return null;
  if (source.kind == OrganizationNodeKind.staff &&
      target.kind == OrganizationNodeKind.staff) {
    return OrganizationRelationKind.handoff;
  }
  if (source.kind == OrganizationNodeKind.staff &&
      target.kind == OrganizationNodeKind.capability) {
    return OrganizationRelationKind.toolAccess;
  }
  if ((source.kind == OrganizationNodeKind.staff &&
          target.kind == OrganizationNodeKind.approval) ||
      (source.kind == OrganizationNodeKind.approval &&
          target.kind == OrganizationNodeKind.staff)) {
    return OrganizationRelationKind.review;
  }
  return null;
}

bool _isValidPair(
  OrganizationRelationKind relation,
  OrganizationNodeKind source,
  OrganizationNodeKind target,
) => switch (relation) {
  OrganizationRelationKind.handoff =>
    source == OrganizationNodeKind.staff &&
        target == OrganizationNodeKind.staff,
  OrganizationRelationKind.toolAccess =>
    source == OrganizationNodeKind.staff &&
        target == OrganizationNodeKind.capability,
  OrganizationRelationKind.review =>
    (source == OrganizationNodeKind.staff &&
            target == OrganizationNodeKind.approval) ||
        (source == OrganizationNodeKind.approval &&
            target == OrganizationNodeKind.staff),
};

extension OrganizationRelationKindLabel on OrganizationRelationKind {
  String get label => switch (this) {
    OrganizationRelationKind.handoff => 'Handoff',
    OrganizationRelationKind.toolAccess => 'Tool access',
    OrganizationRelationKind.review => 'Review',
  };
}

extension OrganizationNodeKindLabel on OrganizationNodeKind {
  String get label => switch (this) {
    OrganizationNodeKind.staff => 'staff',
    OrganizationNodeKind.capability => 'capability',
    OrganizationNodeKind.approval => 'approval desk',
  };
}
