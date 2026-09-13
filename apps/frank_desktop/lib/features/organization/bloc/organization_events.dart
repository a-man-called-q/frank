part of 'organization_bloc.dart';

sealed class OrganizationEvent {
  const OrganizationEvent();
}

final class OrganizationStarted extends OrganizationEvent {
  const OrganizationStarted();
}

final class OrganizationRetryRequested extends OrganizationEvent {
  const OrganizationRetryRequested();
}

final class OrganizationNodeAdded extends OrganizationEvent {
  const OrganizationNodeAdded(this.node);

  final OrganizationNode node;
}

final class OrganizationNodeUpdated extends OrganizationEvent {
  const OrganizationNodeUpdated(this.node);

  final OrganizationNode node;
}

final class OrganizationNodeMoved extends OrganizationEvent {
  const OrganizationNodeMoved(this.nodeId, this.position);

  final String nodeId;
  final OrganizationPoint position;
}

final class OrganizationGroupAdded extends OrganizationEvent {
  const OrganizationGroupAdded(this.group);

  final OrganizationGroup group;
}

final class OrganizationGroupUpdated extends OrganizationEvent {
  const OrganizationGroupUpdated(this.group);

  final OrganizationGroup group;
}

final class OrganizationGroupMoved extends OrganizationEvent {
  const OrganizationGroupMoved(
    this.groupId,
    this.position, {
    this.nodePositions = const {},
  });

  final String groupId;
  final OrganizationPoint position;
  final Map<String, OrganizationPoint> nodePositions;
}

final class OrganizationGroupResized extends OrganizationEvent {
  const OrganizationGroupResized(this.groupId, this.position, this.size);

  final String groupId;
  final OrganizationPoint position;
  final OrganizationSize size;
}

final class OrganizationGroupsDeleted extends OrganizationEvent {
  const OrganizationGroupsDeleted(this.groupIds);

  final List<String> groupIds;
}

final class OrganizationGroupDeleted extends OrganizationEvent {
  const OrganizationGroupDeleted(this.groupId);

  final String groupId;
}

/// A completed drag can contain several selected nodes. Keeping the batch as
/// one event lets undo restore the whole drag with a single snapshot.
final class OrganizationNodesMoved extends OrganizationEvent {
  const OrganizationNodesMoved(this.positions);

  final Map<String, OrganizationPoint> positions;
}

final class OrganizationNodeDuplicated extends OrganizationEvent {
  const OrganizationNodeDuplicated(this.nodeId);

  final String nodeId;
}

final class OrganizationElementsDeleted extends OrganizationEvent {
  const OrganizationElementsDeleted({
    this.nodeIds = const [],
    this.relationIds = const [],
    this.groupIds = const [],
  });

  final List<String> nodeIds;
  final List<String> relationIds;
  final List<String> groupIds;
}

final class OrganizationRelationAdded extends OrganizationEvent {
  const OrganizationRelationAdded(this.relation);

  final OrganizationRelation relation;
}

final class OrganizationRelationUpdated extends OrganizationEvent {
  const OrganizationRelationUpdated(this.relation);

  final OrganizationRelation relation;
}

final class OrganizationSelectionChanged extends OrganizationEvent {
  const OrganizationSelectionChanged({
    this.nodeId,
    this.relationId,
    this.groupId,
    this.nodeIds,
    this.relationIds,
    this.groupIds,
  });

  final String? nodeId;
  final String? relationId;
  final String? groupId;
  final List<String>? nodeIds;
  final List<String>? relationIds;
  final List<String>? groupIds;
}

enum OrganizationAlignment { left, right, top, bottom, centerX, centerY }

final class OrganizationNodesAligned extends OrganizationEvent {
  const OrganizationNodesAligned(this.nodeIds, this.alignment);

  final List<String> nodeIds;
  final OrganizationAlignment alignment;
}

enum OrganizationDistributionAxis { horizontal, vertical }

final class OrganizationNodesDistributed extends OrganizationEvent {
  const OrganizationNodesDistributed(this.nodeIds, this.axis);

  final List<String> nodeIds;
  final OrganizationDistributionAxis axis;
}

final class OrganizationViewportChanged extends OrganizationEvent {
  const OrganizationViewportChanged(this.viewport);

  final OrganizationViewport viewport;
}

final class OrganizationUndoRequested extends OrganizationEvent {
  const OrganizationUndoRequested();
}

final class OrganizationRedoRequested extends OrganizationEvent {
  const OrganizationRedoRequested();
}

final class OrganizationValidateRequested extends OrganizationEvent {
  const OrganizationValidateRequested();
}

final class OrganizationPublishRequested extends OrganizationEvent {
  const OrganizationPublishRequested();
}

final class _OrganizationSaveRequested extends OrganizationEvent {
  const _OrganizationSaveRequested();
}
