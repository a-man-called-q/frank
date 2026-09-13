part of 'organization_bloc.dart';

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
    this.connectorProfiles = const [],
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
  final List<ConnectorProfile> connectorProfiles;
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
    List<ConnectorProfile>? connectorProfiles,
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
    connectorProfiles: connectorProfiles ?? this.connectorProfiles,
    error: identical(error, _unset) ? this.error : error as String?,
  );
}
