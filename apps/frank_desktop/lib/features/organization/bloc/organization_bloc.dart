import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/organization_models.dart';
import '../organization_catalog.dart';
import '../organization_validator.dart';

enum OrganizationLoadStatus { initial, loading, ready, failure }

enum OrganizationPersistenceStatus {
  published,
  clean,
  dirty,
  saving,
  saveFailure,
  publishing,
  publishFailure,
}

class OrganizationState {
  const OrganizationState({
    this.loadStatus = OrganizationLoadStatus.initial,
    this.persistenceStatus = OrganizationPersistenceStatus.clean,
    this.graph,
    this.validation = const OrganizationValidation([]),
    this.selectedNodeId,
    this.selectedRelationId,
    this.selectedGroupId,
    this.selectedNodeIds = const [],
    this.selectedRelationIds = const [],
    this.selectedGroupIds = const [],
    this.canUndo = false,
    this.canRedo = false,
    this.error,
  });

  static const _unset = Object();

  final OrganizationLoadStatus loadStatus;
  final OrganizationPersistenceStatus persistenceStatus;
  final OrganizationGraph? graph;
  final OrganizationValidation validation;
  final String? selectedNodeId;
  final String? selectedRelationId;
  final String? selectedGroupId;
  final List<String> selectedNodeIds;
  final List<String> selectedRelationIds;
  final List<String> selectedGroupIds;
  final bool canUndo;
  final bool canRedo;
  final String? error;

  bool get canPublish =>
      loadStatus == OrganizationLoadStatus.ready &&
      graph != null &&
      !validation.hasErrors &&
      persistenceStatus != OrganizationPersistenceStatus.dirty &&
      persistenceStatus != OrganizationPersistenceStatus.saving &&
      persistenceStatus != OrganizationPersistenceStatus.saveFailure &&
      persistenceStatus != OrganizationPersistenceStatus.publishing;

  OrganizationState copyWith({
    OrganizationLoadStatus? loadStatus,
    OrganizationPersistenceStatus? persistenceStatus,
    OrganizationGraph? graph,
    OrganizationValidation? validation,
    Object? selectedNodeId = _unset,
    Object? selectedRelationId = _unset,
    Object? selectedGroupId = _unset,
    List<String>? selectedNodeIds,
    List<String>? selectedRelationIds,
    List<String>? selectedGroupIds,
    bool? canUndo,
    bool? canRedo,
    Object? error = _unset,
  }) => OrganizationState(
    loadStatus: loadStatus ?? this.loadStatus,
    persistenceStatus: persistenceStatus ?? this.persistenceStatus,
    graph: graph ?? this.graph,
    validation: validation ?? this.validation,
    selectedNodeId: identical(selectedNodeId, _unset)
        ? this.selectedNodeId
        : selectedNodeId as String?,
    selectedRelationId: identical(selectedRelationId, _unset)
        ? this.selectedRelationId
        : selectedRelationId as String?,
    selectedGroupId: identical(selectedGroupId, _unset)
        ? this.selectedGroupId
        : selectedGroupId as String?,
    selectedNodeIds: selectedNodeIds ?? this.selectedNodeIds,
    selectedRelationIds: selectedRelationIds ?? this.selectedRelationIds,
    selectedGroupIds: selectedGroupIds ?? this.selectedGroupIds,
    canUndo: canUndo ?? this.canUndo,
    canRedo: canRedo ?? this.canRedo,
    error: identical(error, _unset) ? this.error : error as String?,
  );
}

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

class OrganizationBloc extends Bloc<OrganizationEvent, OrganizationState> {
  OrganizationBloc({
    required FrankGateway gateway,
    this.autosaveDelay = const Duration(milliseconds: 500),
  }) : _gateway = gateway,
       super(const OrganizationState()) {
    on<OrganizationStarted>(_load);
    on<OrganizationRetryRequested>(_retry);
    on<OrganizationNodeAdded>(_addNode);
    on<OrganizationNodeUpdated>(_updateNode);
    on<OrganizationNodeMoved>(_moveNode);
    on<OrganizationNodesMoved>(_moveNodes);
    on<OrganizationGroupAdded>(_addGroup);
    on<OrganizationGroupUpdated>(_updateGroup);
    on<OrganizationGroupMoved>(_moveGroup);
    on<OrganizationGroupResized>(_resizeGroup);
    on<OrganizationGroupsDeleted>(_deleteGroups);
    on<OrganizationGroupDeleted>(
      (event, emit) =>
          _deleteGroups(OrganizationGroupsDeleted([event.groupId]), emit),
    );
    on<OrganizationNodeDuplicated>(_duplicateNode);
    on<OrganizationElementsDeleted>(_deleteElements);
    on<OrganizationRelationAdded>(_addRelation);
    on<OrganizationRelationUpdated>(_updateRelation);
    on<OrganizationSelectionChanged>(_select);
    on<OrganizationNodesAligned>(_alignNodes);
    on<OrganizationNodesDistributed>(_distributeNodes);
    on<OrganizationViewportChanged>(_changeViewport);
    on<OrganizationUndoRequested>(_undo);
    on<OrganizationRedoRequested>(_redo);
    on<OrganizationValidateRequested>(_validate);
    on<OrganizationPublishRequested>(_publish);
    on<_OrganizationSaveRequested>(_save);
  }

  final FrankGateway _gateway;
  final Duration autosaveDelay;
  final List<OrganizationGraph> _undoStack = [];
  final List<OrganizationGraph> _redoStack = [];
  Timer? _autosaveTimer;
  int _editGeneration = 0;
  int _idSequence = 0;

  Future<void> _load(
    OrganizationStarted event,
    Emitter<OrganizationState> emit,
  ) async {
    if (state.loadStatus != OrganizationLoadStatus.initial) return;
    await _performLoad(emit);
  }

  Future<void> _retry(
    OrganizationRetryRequested event,
    Emitter<OrganizationState> emit,
  ) async {
    if (state.loadStatus == OrganizationLoadStatus.failure) {
      await _performLoad(emit);
      return;
    }
    if (state.persistenceStatus == OrganizationPersistenceStatus.saveFailure) {
      await _save(const _OrganizationSaveRequested(), emit);
      return;
    }
    if (state.persistenceStatus ==
        OrganizationPersistenceStatus.publishFailure) {
      await _publish(const OrganizationPublishRequested(), emit);
    }
  }

  Future<void> _performLoad(Emitter<OrganizationState> emit) async {
    emit(
      state.copyWith(loadStatus: OrganizationLoadStatus.loading, error: null),
    );
    try {
      final graph = await _gateway.loadOrganization();
      if (isClosed) return;
      _undoStack.clear();
      _redoStack.clear();
      emit(
        state.copyWith(
          loadStatus: OrganizationLoadStatus.ready,
          persistenceStatus: OrganizationPersistenceStatus.clean,
          graph: graph,
          validation: validateOrganization(graph),
          selectedNodeId: null,
          selectedRelationId: null,
          selectedGroupId: null,
          selectedNodeIds: const [],
          selectedRelationIds: const [],
          selectedGroupIds: const [],
          canUndo: false,
          canRedo: false,
          error: null,
        ),
      );
    } on Object catch (error) {
      if (isClosed) return;
      emit(
        state.copyWith(
          loadStatus: OrganizationLoadStatus.failure,
          error: error.toString(),
        ),
      );
    }
  }

  void _addNode(OrganizationNodeAdded event, Emitter<OrganizationState> emit) {
    final graph = state.graph;
    if (graph == null) return;
    if (graph.nodes.any((node) => node.id == event.node.id)) {
      emit(state.copyWith(error: 'That office element already exists.'));
      return;
    }
    if (event.node.kind == OrganizationNodeKind.staff &&
        (event.node.employeeId == null || event.node.employeeId!.isEmpty)) {
      emit(state.copyWith(error: 'Staff must reference an employee.'));
      return;
    }
    if (event.node.kind == OrganizationNodeKind.staff &&
        graph.nodes.any((node) => node.employeeId == event.node.employeeId)) {
      emit(
        state.copyWith(error: '${event.node.label} is already on the team.'),
      );
      return;
    }
    _commit(
      graph.copyWith(nodes: [...graph.nodes, event.node]),
      emit,
      selectedNodeId: event.node.id,
      selectedRelationId: null,
      selectedNodeIds: [event.node.id],
      selectedRelationIds: const [],
    );
  }

  void _updateNode(
    OrganizationNodeUpdated event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    if (!graph.nodes.any((node) => node.id == event.node.id)) return;
    _commit(
      graph.copyWith(
        nodes: [
          for (final node in graph.nodes)
            if (node.id == event.node.id) event.node else node,
        ],
      ),
      emit,
    );
  }

  void _addGroup(
    OrganizationGroupAdded event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final id = event.group.id.trim();
    final label = event.group.label.trim();
    if (id.isEmpty || label.isEmpty) {
      emit(state.copyWith(error: 'A group needs a name.'));
      return;
    }
    if (graph.groups.any((group) => group.id == id) ||
        OrganizationGroup.builtInById(id) != null) {
      emit(state.copyWith(error: 'That group already exists.'));
      return;
    }
    final group = event.group.copyWith(id: id, label: label, locked: false);
    _commit(
      graph.copyWith(groups: [...graph.groups, group]),
      emit,
      selectedGroupId: group.id,
      selectedGroupIds: [group.id],
      selectedNodeId: null,
      selectedRelationId: null,
      selectedNodeIds: const [],
      selectedRelationIds: const [],
    );
  }

  void _updateGroup(
    OrganizationGroupUpdated event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final existing = graph.groupById(event.group.id);
    if (existing == null) return;
    if (existing.isBuiltIn || existing.locked) {
      emit(state.copyWith(error: 'Built-in groups cannot be edited.'));
      return;
    }
    final label = event.group.label.trim();
    if (label.isEmpty) {
      emit(state.copyWith(error: 'A group needs a name.'));
      return;
    }
    final nextGroup = event.group.copyWith(
      id: existing.id,
      label: label,
      locked: false,
    );
    _commit(
      graph.copyWith(
        groups: [
          for (final group in graph.groups)
            if (group.id == existing.id) nextGroup else group,
        ],
      ),
      emit,
      selectedGroupId: existing.id,
      selectedGroupIds: [existing.id],
      selectedNodeId: null,
      selectedRelationId: null,
      selectedNodeIds: const [],
      selectedRelationIds: const [],
    );
  }

  void _moveGroup(
    OrganizationGroupMoved event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final existing = graph.groupById(event.groupId);
    if (existing == null) return;
    if (existing.isBuiltIn || existing.locked) {
      emit(state.copyWith(error: 'Built-in groups cannot be moved.'));
      return;
    }
    final snapped = OrganizationPoint(
      _snapToGrid(event.position.x),
      _snapToGrid(event.position.y),
    );
    final dx = snapped.x - existing.position.x;
    final dy = snapped.y - existing.position.y;
    final nextNodes = [
      for (final node in graph.nodes)
        if (node.groupId == existing.id)
          node.copyWith(
            position: _snapPoint(
              event.nodePositions[node.id] ??
                  OrganizationPoint(node.position.x + dx, node.position.y + dy),
            ),
          )
        else
          node,
    ];
    final moved =
        existing.position != snapped ||
        nextNodes.asMap().entries.any((entry) {
          final before = graph.nodes[entry.key];
          final after = entry.value;
          return before.position != after.position;
        });
    if (!moved) return;
    _commit(
      graph.copyWith(
        groups: _replaceGroup(
          graph.groups,
          existing.copyWith(position: snapped),
        ),
        nodes: nextNodes,
      ),
      emit,
      selectedGroupId: existing.id,
      selectedGroupIds: [existing.id],
      selectedNodeId: null,
      selectedRelationId: null,
      selectedNodeIds: const [],
      selectedRelationIds: const [],
    );
  }

  void _resizeGroup(
    OrganizationGroupResized event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final existing = graph.groupById(event.groupId);
    if (existing == null) return;
    if (existing.isBuiltIn || existing.locked) {
      emit(state.copyWith(error: 'Built-in groups cannot be resized.'));
      return;
    }
    final position = _snapPoint(event.position);
    final size = OrganizationSize(
      _snapToGrid(event.size.width.clamp(100, 2400).toDouble()),
      _snapToGrid(event.size.height.clamp(60, 1600).toDouble()),
    );
    if (existing.position == position && existing.size == size) return;
    final resized = existing.copyWith(position: position, size: size);
    final resizedGraph = graph.copyWith(
      groups: _replaceGroup(graph.groups, resized),
    );
    final nextNodes = [
      for (final node in graph.nodes)
        node.groupId == existing.id
            ? node.copyWith(
                groupId: _containsGroup(resized, node)
                    ? existing.id
                    : _smallestContainingGroup(
                        resizedGraph,
                        node,
                        excluding: existing.id,
                      )?.id,
              )
            : node,
    ];
    _commit(
      resizedGraph.copyWith(nodes: nextNodes),
      emit,
      selectedGroupId: existing.id,
      selectedGroupIds: [existing.id],
      selectedNodeId: null,
      selectedRelationId: null,
      selectedNodeIds: const [],
      selectedRelationIds: const [],
    );
  }

  void _deleteGroups(
    OrganizationGroupsDeleted event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final requested = event.groupIds.toSet();
    final removable = graph.groups
        .where((group) => requested.contains(group.id) && !group.isBuiltIn)
        .map((group) => group.id)
        .toSet();
    if (removable.isEmpty) return;
    _commit(
      graph.copyWith(
        groups: graph.groups
            .where((group) => !removable.contains(group.id))
            .toList(),
        nodes: [
          for (final node in graph.nodes)
            removable.contains(node.groupId)
                ? node.copyWith(groupId: null)
                : node,
        ],
      ),
      emit,
      selectedGroupId: null,
      selectedGroupIds: const [],
      selectedNodeId: null,
      selectedRelationId: null,
      selectedNodeIds: const [],
      selectedRelationIds: const [],
    );
  }

  void _moveNode(OrganizationNodeMoved event, Emitter<OrganizationState> emit) {
    _moveNodes(OrganizationNodesMoved({event.nodeId: event.position}), emit);
  }

  void _moveNodes(
    OrganizationNodesMoved event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final positions = event.positions;
    if (positions.isEmpty) {
      return;
    }
    var changed = false;
    final nextNodes = <OrganizationNode>[];
    for (final node in graph.nodes) {
      final position = positions[node.id];
      if (position == null) {
        nextNodes.add(node);
        continue;
      }
      final snapped = OrganizationPoint(
        _snapToGrid(position.x),
        _snapToGrid(position.y),
      );
      if (node.position.x != snapped.x || node.position.y != snapped.y) {
        changed = true;
        final movedNode = node.copyWith(position: snapped);
        nextNodes.add(
          movedNode.copyWith(
            groupId: _smallestContainingGroup(graph, movedNode)?.id,
          ),
        );
      } else {
        nextNodes.add(node);
      }
    }
    if (!changed) return;
    _commit(graph.copyWith(nodes: nextNodes), emit);
  }

  void _duplicateNode(
    OrganizationNodeDuplicated event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final source = graph.nodes
        .where((node) => node.id == event.nodeId)
        .firstOrNull;
    if (source == null || source.kind == OrganizationNodeKind.staff) return;
    final duplicate = OrganizationNode(
      id: '${source.id}-copy-${++_idSequence}',
      kind: source.kind,
      label: '${source.label} copy',
      position: OrganizationPoint(
        source.position.x + 20,
        source.position.y + 20,
      ),
      groupId: source.groupId,
      capability: source.capability,
      providerLabel: source.providerLabel,
      integrationRef: source.integrationRef,
      profileRef: source.profileRef,
      configured: source.configured,
      approvalRequired: source.approvalRequired,
    );
    _commit(
      graph.copyWith(nodes: [...graph.nodes, duplicate]),
      emit,
      selectedNodeId: duplicate.id,
      selectedRelationId: null,
      selectedNodeIds: [duplicate.id],
      selectedRelationIds: const [],
    );
  }

  void _deleteElements(
    OrganizationElementsDeleted event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final nodeIds = event.nodeIds.toSet();
    final relationIds = event.relationIds.toSet();
    final requestedGroupIds = event.groupIds.toSet();
    final groupIds = graph.groups
        .where(
          (group) => requestedGroupIds.contains(group.id) && !group.isBuiltIn,
        )
        .map((group) => group.id)
        .toSet();
    if (nodeIds.isEmpty && relationIds.isEmpty && groupIds.isEmpty) return;
    _commit(
      graph.copyWith(
        relations: graph.relations
            .where(
              (relation) =>
                  !relationIds.contains(relation.id) &&
                  !nodeIds.contains(relation.sourceNodeId) &&
                  !nodeIds.contains(relation.targetNodeId),
            )
            .toList(),
        groups: graph.groups
            .where((group) => !groupIds.contains(group.id))
            .toList(),
        // Deleting a container leaves its nodes in place, but clears the
        // membership reference so no node points at a missing group.
        nodes: [
          for (final node in graph.nodes)
            if (!nodeIds.contains(node.id))
              groupIds.contains(node.groupId)
                  ? node.copyWith(groupId: null)
                  : node,
        ],
      ),
      emit,
      selectedNodeId: null,
      selectedRelationId: null,
      selectedGroupId: null,
      selectedNodeIds: const [],
      selectedRelationIds: const [],
      selectedGroupIds: const [],
    );
  }

  void _addRelation(
    OrganizationRelationAdded event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    final source = graph.nodes
        .where((node) => node.id == event.relation.sourceNodeId)
        .firstOrNull;
    final target = graph.nodes
        .where((node) => node.id == event.relation.targetNodeId)
        .firstOrNull;
    if (source == null || target == null) {
      emit(state.copyWith(error: 'Both relation endpoints must exist.'));
      return;
    }
    if (graph.relations.any((relation) => relation.id == event.relation.id)) {
      emit(state.copyWith(error: 'That relation already exists.'));
      return;
    }
    final inferred = inferOrganizationRelationKind(
      source: source,
      target: target,
    );
    if (inferred == null || inferred != event.relation.kind) {
      emit(state.copyWith(error: 'Those office elements cannot be connected.'));
      return;
    }
    if (!_hasValidPermissions(event.relation, target)) {
      emit(
        state.copyWith(
          error: 'Choose at least one permission available on the capability.',
        ),
      );
      return;
    }
    _commit(
      graph.copyWith(relations: [...graph.relations, event.relation]),
      emit,
      selectedNodeId: null,
      selectedRelationId: event.relation.id,
      selectedNodeIds: const [],
      selectedRelationIds: [event.relation.id],
    );
  }

  void _updateRelation(
    OrganizationRelationUpdated event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    if (!graph.relations.any((relation) => relation.id == event.relation.id)) {
      return;
    }
    final source = graph.nodes
        .where((node) => node.id == event.relation.sourceNodeId)
        .firstOrNull;
    final target = graph.nodes
        .where((node) => node.id == event.relation.targetNodeId)
        .firstOrNull;
    if (source == null ||
        target == null ||
        inferOrganizationRelationKind(source: source, target: target) !=
            event.relation.kind ||
        !_hasValidPermissions(event.relation, target)) {
      emit(state.copyWith(error: 'Those relation settings are not valid.'));
      return;
    }
    _commit(
      graph.copyWith(
        relations: [
          for (final relation in graph.relations)
            if (relation.id == event.relation.id) event.relation else relation,
        ],
      ),
      emit,
    );
  }

  void _select(
    OrganizationSelectionChanged event,
    Emitter<OrganizationState> emit,
  ) {
    emit(
      state.copyWith(
        selectedNodeId: event.nodeId,
        selectedRelationId: event.relationId,
        selectedGroupId: event.groupId,
        selectedNodeIds: List.unmodifiable(
          event.nodeIds ??
              (event.nodeId == null ? const <String>[] : [event.nodeId!]),
        ),
        selectedRelationIds: List.unmodifiable(
          event.relationIds ??
              (event.relationId == null
                  ? const <String>[]
                  : [event.relationId!]),
        ),
        selectedGroupIds: List.unmodifiable(
          event.groupIds ??
              (event.groupId == null ? const <String>[] : [event.groupId!]),
        ),
        error: null,
      ),
    );
  }

  void _changeViewport(
    OrganizationViewportChanged event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    if (graph.viewport.x == event.viewport.x &&
        graph.viewport.y == event.viewport.y &&
        graph.viewport.zoom == event.viewport.zoom) {
      return;
    }
    _commit(
      graph.copyWith(viewport: event.viewport),
      emit,
      recordHistory: false,
    );
  }

  void _alignNodes(
    OrganizationNodesAligned event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null || event.nodeIds.length < 2) return;
    final selected = graph.nodes
        .where((node) => event.nodeIds.contains(node.id))
        .toList();
    if (selected.length < 2) return;
    final xs = selected.map((node) => node.position.x).toList();
    final ys = selected.map((node) => node.position.y).toList();
    final right = selected
        .map((node) => node.position.x + _nodeWidth(node))
        .reduce((a, b) => a > b ? a : b);
    final bottom = selected
        .map((node) => node.position.y + _nodeHeight(node))
        .reduce((a, b) => a > b ? a : b);
    final centerX =
        selected
            .map((node) => node.position.x + _nodeWidth(node) / 2)
            .reduce((a, b) => a + b) /
        selected.length;
    final centerY =
        selected
            .map((node) => node.position.y + _nodeHeight(node) / 2)
            .reduce((a, b) => a + b) /
        selected.length;
    final left = xs.reduce((a, b) => a < b ? a : b);
    final top = ys.reduce((a, b) => a < b ? a : b);
    final nextNodes = [
      for (final node in graph.nodes)
        if (event.nodeIds.contains(node.id))
          _withMembership(
            graph,
            node,
            OrganizationPoint(
              switch (event.alignment) {
                OrganizationAlignment.left => left,
                OrganizationAlignment.right => right - _nodeWidth(node),
                OrganizationAlignment.centerX => centerX - _nodeWidth(node) / 2,
                _ => node.position.x,
              },
              switch (event.alignment) {
                OrganizationAlignment.top => top,
                OrganizationAlignment.bottom => bottom - _nodeHeight(node),
                OrganizationAlignment.centerY =>
                  centerY - _nodeHeight(node) / 2,
                _ => node.position.y,
              },
            ),
          )
        else
          node,
    ];
    _commit(graph.copyWith(nodes: nextNodes), emit);
  }

  void _distributeNodes(
    OrganizationNodesDistributed event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null || event.nodeIds.length < 3) return;
    final selected =
        graph.nodes.where((node) => event.nodeIds.contains(node.id)).toList()
          ..sort(
            (a, b) => event.axis == OrganizationDistributionAxis.horizontal
                ? a.position.x.compareTo(b.position.x)
                : a.position.y.compareTo(b.position.y),
          );
    if (selected.length < 3) return;
    final first = event.axis == OrganizationDistributionAxis.horizontal
        ? selected.first.position.x
        : selected.first.position.y;
    final last = event.axis == OrganizationDistributionAxis.horizontal
        ? selected.last.position.x
        : selected.last.position.y;
    final step = (last - first) / (selected.length - 1);
    final positions = {
      for (var index = 0; index < selected.length; index++)
        selected[index].id:
            event.axis == OrganizationDistributionAxis.horizontal
            ? OrganizationPoint(
                first + step * index,
                selected[index].position.y,
              )
            : OrganizationPoint(
                selected[index].position.x,
                first + step * index,
              ),
    };
    _commit(
      graph.copyWith(
        nodes: [
          for (final node in graph.nodes)
            positions[node.id] == null
                ? node
                : _withMembership(graph, node, positions[node.id]!),
        ],
      ),
      emit,
    );
  }

  void _undo(OrganizationUndoRequested event, Emitter<OrganizationState> emit) {
    final graph = state.graph;
    if (graph == null || _undoStack.isEmpty) return;
    _redoStack.add(graph);
    final previous = _undoStack.removeLast();
    _emitChanged(previous, emit);
  }

  void _redo(OrganizationRedoRequested event, Emitter<OrganizationState> emit) {
    final graph = state.graph;
    if (graph == null || _redoStack.isEmpty) return;
    _undoStack.add(graph);
    final next = _redoStack.removeLast();
    _emitChanged(next, emit);
  }

  void _validate(
    OrganizationValidateRequested event,
    Emitter<OrganizationState> emit,
  ) {
    final graph = state.graph;
    if (graph == null) return;
    emit(state.copyWith(validation: validateOrganization(graph), error: null));
  }

  Future<void> _save(
    _OrganizationSaveRequested event,
    Emitter<OrganizationState> emit,
  ) async {
    final graph = state.graph;
    if (graph == null ||
        state.persistenceStatus == OrganizationPersistenceStatus.saving ||
        state.persistenceStatus == OrganizationPersistenceStatus.publishing) {
      return;
    }
    final generation = _editGeneration;
    emit(
      state.copyWith(
        persistenceStatus: OrganizationPersistenceStatus.saving,
        error: null,
      ),
    );
    try {
      final saved = await _gateway.saveOrganizationDraft(graph);
      if (isClosed) return;
      if (generation == _editGeneration) {
        emit(
          state.copyWith(
            graph: saved,
            persistenceStatus: OrganizationPersistenceStatus.clean,
            error: null,
          ),
        );
      } else {
        emit(
          state.copyWith(
            persistenceStatus: OrganizationPersistenceStatus.dirty,
            error: null,
          ),
        );
        _scheduleAutosave();
      }
    } on Object catch (error) {
      if (isClosed) return;
      emit(
        state.copyWith(
          persistenceStatus: OrganizationPersistenceStatus.saveFailure,
          error: error.toString(),
        ),
      );
    }
  }

  Future<void> _publish(
    OrganizationPublishRequested event,
    Emitter<OrganizationState> emit,
  ) async {
    final graph = state.graph;
    if (graph == null || !state.canPublish) return;
    final generation = _editGeneration;
    emit(
      state.copyWith(
        persistenceStatus: OrganizationPersistenceStatus.publishing,
        error: null,
      ),
    );
    try {
      final published = await _gateway.publishOrganization(
        graph,
        expectedPublishedRevision: graph.publishedRevision,
      );
      if (isClosed) return;
      if (generation != _editGeneration) {
        // A user can keep editing while the publish request is in flight.
        // Preserve that newer local draft, but carry forward the revision
        // returned by the server so the next publish does not look stale.
        final current = state.graph;
        if (current == null) return;
        final rebased = current.copyWith(
          publishedRevision: published.publishedRevision,
        );
        emit(
          state.copyWith(
            graph: rebased,
            persistenceStatus: OrganizationPersistenceStatus.dirty,
            validation: validateOrganization(rebased),
            error: null,
          ),
        );
        _scheduleAutosave();
        return;
      }
      emit(
        state.copyWith(
          graph: published,
          persistenceStatus: OrganizationPersistenceStatus.published,
          validation: validateOrganization(published),
          error: null,
        ),
      );
    } on OrganizationRevisionConflict catch (error) {
      // Keep the local draft and history intact while rebasing only the
      // published revision token. A subsequent Retry can now publish against
      // the revision reported by the gateway instead of repeating the same
      // stale-write conflict forever.
      if (isClosed) return;
      final current = state.graph;
      if (generation != _editGeneration && current != null) {
        final rebased = current.copyWith(publishedRevision: error.actual);
        emit(
          state.copyWith(
            graph: rebased,
            persistenceStatus: OrganizationPersistenceStatus.dirty,
            validation: validateOrganization(rebased),
            error: error.toString(),
          ),
        );
        _scheduleAutosave();
        return;
      }
      emit(
        state.copyWith(
          graph: graph.copyWith(publishedRevision: error.actual),
          persistenceStatus: OrganizationPersistenceStatus.publishFailure,
          error: error.toString(),
        ),
      );
    } on Object catch (error) {
      if (isClosed) return;
      emit(
        state.copyWith(
          persistenceStatus: OrganizationPersistenceStatus.publishFailure,
          error: error.toString(),
        ),
      );
    }
  }

  void _commit(
    OrganizationGraph next,
    Emitter<OrganizationState> emit, {
    bool recordHistory = true,
    Object? selectedNodeId = OrganizationState._unset,
    Object? selectedRelationId = OrganizationState._unset,
    Object? selectedGroupId = OrganizationState._unset,
    Object? selectedNodeIds = OrganizationState._unset,
    Object? selectedRelationIds = OrganizationState._unset,
    Object? selectedGroupIds = OrganizationState._unset,
  }) {
    final current = state.graph;
    if (current == null) return;
    if (recordHistory) {
      _undoStack.add(current);
      if (_undoStack.length > 50) _undoStack.removeAt(0);
      _redoStack.clear();
    }
    _editGeneration++;
    emit(
      state.copyWith(
        graph: next,
        validation: validateOrganization(next),
        persistenceStatus: OrganizationPersistenceStatus.dirty,
        selectedNodeId: selectedNodeId,
        selectedRelationId: selectedRelationId,
        selectedGroupId: selectedGroupId,
        selectedNodeIds: _selectionOverride(selectedNodeIds),
        selectedRelationIds: _selectionOverride(selectedRelationIds),
        selectedGroupIds: _selectionOverride(selectedGroupIds),
        canUndo: _undoStack.isNotEmpty,
        canRedo: _redoStack.isNotEmpty,
        error: null,
      ),
    );
    _scheduleAutosave();
  }

  void _emitChanged(OrganizationGraph graph, Emitter<OrganizationState> emit) {
    _editGeneration++;
    emit(
      state.copyWith(
        graph: graph,
        validation: validateOrganization(graph),
        persistenceStatus: OrganizationPersistenceStatus.dirty,
        selectedNodeId: null,
        selectedRelationId: null,
        selectedGroupId: null,
        selectedNodeIds: const [],
        selectedRelationIds: const [],
        selectedGroupIds: const [],
        canUndo: _undoStack.isNotEmpty,
        canRedo: _redoStack.isNotEmpty,
        error: null,
      ),
    );
    _scheduleAutosave();
  }

  void _scheduleAutosave() {
    _autosaveTimer?.cancel();
    _autosaveTimer = Timer(
      autosaveDelay,
      () => add(const _OrganizationSaveRequested()),
    );
  }

  double _nodeWidth(OrganizationNode node) =>
      node.kind == OrganizationNodeKind.staff ? 220 : 190;

  double _nodeHeight(OrganizationNode node) =>
      node.kind == OrganizationNodeKind.staff ? 126 : 112;

  OrganizationPoint _snapPoint(OrganizationPoint point) =>
      OrganizationPoint(_snapToGrid(point.x), _snapToGrid(point.y));

  OrganizationNode _withMembership(
    OrganizationGraph graph,
    OrganizationNode node,
    OrganizationPoint position,
  ) {
    final moved = node.copyWith(position: _snapPoint(position));
    return moved.copyWith(groupId: _smallestContainingGroup(graph, moved)?.id);
  }

  List<OrganizationGroup> _replaceGroup(
    List<OrganizationGroup> groups,
    OrganizationGroup replacement,
  ) => [
    for (final group in groups)
      if (group.id == replacement.id) replacement else group,
  ];

  OrganizationGroup? _smallestContainingGroup(
    OrganizationGraph graph,
    OrganizationNode node, {
    String? excluding,
  }) {
    final left = node.position.x;
    final top = node.position.y;
    final right = left + _nodeWidth(node);
    final bottom = top + _nodeHeight(node);
    final containing = graph.groups.where((group) {
      if (group.id == excluding) return false;
      final groupRight = group.position.x + group.size.width;
      final groupBottom = group.position.y + group.size.height;
      return left >= group.position.x &&
          top >= group.position.y &&
          right <= groupRight &&
          bottom <= groupBottom;
    }).toList();
    if (containing.isEmpty) return null;
    containing.sort(
      (a, b) => (a.size.width * a.size.height).compareTo(
        b.size.width * b.size.height,
      ),
    );
    return containing.first;
  }

  bool _containsGroup(OrganizationGroup group, OrganizationNode node) {
    final left = node.position.x;
    final top = node.position.y;
    final right = left + _nodeWidth(node);
    final bottom = top + _nodeHeight(node);
    return left >= group.position.x &&
        top >= group.position.y &&
        right <= group.position.x + group.size.width &&
        bottom <= group.position.y + group.size.height;
  }

  double _snapToGrid(double value) =>
      value.isFinite ? (value / 20).round() * 20.0 : 0.0;

  List<String>? _selectionOverride(Object? selection) {
    if (selection == null || identical(selection, OrganizationState._unset)) {
      return null;
    }
    return List<String>.unmodifiable((selection as Iterable).cast<String>());
  }

  bool _hasValidPermissions(
    OrganizationRelation relation,
    OrganizationNode target,
  ) {
    if (relation.kind != OrganizationRelationKind.toolAccess) return true;
    final capability = target.capability;
    if (capability == null || relation.permissions.isEmpty) return false;
    final allowed = capability.permissions.toSet();
    return relation.permissions.every(allowed.contains);
  }

  @override
  Future<void> close() {
    _autosaveTimer?.cancel();
    return super.close();
  }
}
