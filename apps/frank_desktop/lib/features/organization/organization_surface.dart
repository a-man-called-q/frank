import 'dart:async';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:vyuh_node_flow/vyuh_node_flow.dart';

import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../core/models/organization_models.dart';
import '../../core/models/workspace_models.dart';
import 'bloc/organization_bloc.dart';
import 'organization_catalog.dart';
import 'organization_flow_adapter.dart';
import 'organization_flow_theme.dart';
import 'organization_validator.dart';

class OrganizationSurface extends StatefulWidget {
  const OrganizationSurface({required this.workspace, super.key});

  final OfficeWorkspace workspace;

  @override
  State<OrganizationSurface> createState() => _OrganizationSurfaceState();
}

class _OrganizationSurfaceState extends State<OrganizationSurface> {
  @override
  void initState() {
    super.initState();
    final bloc = context.read<OrganizationBloc>();
    if (bloc.state.loadStatus == OrganizationLoadStatus.initial) {
      bloc.add(const OrganizationStarted());
    }
  }

  @override
  Widget build(BuildContext context) {
    return Material(
      type: MaterialType.transparency,
      child: Semantics(
        container: true,
        label: 'Organization flow editor',
        child: BlocBuilder<OrganizationBloc, OrganizationState>(
          builder: (context, state) {
            return switch (state.loadStatus) {
              OrganizationLoadStatus.initial ||
              OrganizationLoadStatus.loading => const _OrganizationLoading(),
              OrganizationLoadStatus.failure => _OrganizationFailure(
                message: state.error ?? 'The organization could not be loaded.',
              ),
              OrganizationLoadStatus.ready =>
                state.graph == null
                    ? const _OrganizationLoading()
                    : _OrganizationEditor(
                        graph: state.graph!,
                        state: state,
                        workspace: widget.workspace,
                      ),
            };
          },
        ),
      ),
    );
  }
}

class _OrganizationEditor extends StatefulWidget {
  const _OrganizationEditor({
    required this.graph,
    required this.state,
    required this.workspace,
  });

  final OrganizationGraph graph;
  final OrganizationState state;
  final OfficeWorkspace workspace;

  @override
  State<_OrganizationEditor> createState() => _OrganizationEditorState();
}

class _OrganizationEditorState extends State<_OrganizationEditor> {
  late OrganizationFlowProjection _projection;
  OrganizationGraph? _projectedGraph;
  bool _compactInspectorOpen = false;
  bool _reducedMotion = false;
  bool _wideLayout = true;
  int _newNodeSequence = 0;
  final Map<String, OrganizationPoint> _pendingDragPositions = {};
  Timer? _dragFlushTimer;

  @override
  void initState() {
    super.initState();
    _replaceProjection(reducedMotion: false);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reducedMotion =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    final mediaWidth = MediaQuery.maybeOf(context)?.size.width ?? 0;
    final view = View.maybeOf(context);
    final viewWidth = view == null || view.devicePixelRatio <= 0
        ? 1600.0
        : view.physicalSize.width / view.devicePixelRatio;
    final wideLayout = (mediaWidth > 0 ? mediaWidth : viewWidth) >= 900;
    if (reducedMotion == _reducedMotion &&
        wideLayout == _wideLayout &&
        _projectedGraph != null) {
      return;
    }
    _replaceProjection(reducedMotion: reducedMotion, wideLayout: wideLayout);
  }

  @override
  void didUpdateWidget(covariant _OrganizationEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(_projectedGraph, widget.graph)) {
      _replaceProjection(
        reducedMotion: _reducedMotion,
        wideLayout: _wideLayout,
      );
    }
    if (widget.state.selectedNodeId != oldWidget.state.selectedNodeId ||
        widget.state.selectedRelationId != oldWidget.state.selectedRelationId ||
        widget.state.selectedGroupId != oldWidget.state.selectedGroupId ||
        !listEquals(
          widget.state.selectedNodeIds,
          oldWidget.state.selectedNodeIds,
        ) ||
        !listEquals(
          widget.state.selectedRelationIds,
          oldWidget.state.selectedRelationIds,
        ) ||
        !listEquals(
          widget.state.selectedGroupIds,
          oldWidget.state.selectedGroupIds,
        )) {
      _syncControllerSelection();
    }
  }

  void _syncControllerSelection() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _projection.controller.clearSelection();
      final nodeIds = widget.state.selectedNodeIds.isNotEmpty
          ? widget.state.selectedNodeIds
          : widget.state.selectedNodeId == null
          ? const <String>[]
          : [widget.state.selectedNodeId!];
      final relationIds = widget.state.selectedRelationIds.isNotEmpty
          ? widget.state.selectedRelationIds
          : widget.state.selectedRelationId == null
          ? const <String>[]
          : [widget.state.selectedRelationId!];
      final groupIds = widget.state.selectedGroupIds.isNotEmpty
          ? widget.state.selectedGroupIds
          : widget.state.selectedGroupId == null
          ? const <String>[]
          : [widget.state.selectedGroupId!];
      final flowGroupIds = groupIds.map((id) => 'group-$id');
      if (nodeIds.isNotEmpty || groupIds.isNotEmpty) {
        _projection.controller.selectSpecificNodes([
          ...nodeIds,
          ...flowGroupIds,
        ]);
      }
      for (final relationId in relationIds) {
        _projection.controller.selectConnection(relationId);
      }
    });
  }

  void _replaceProjection({required bool reducedMotion, bool? wideLayout}) {
    final previous = _projectedGraph == null ? null : _projection.controller;
    final nextWideLayout = wideLayout ?? _wideLayout;
    _projection = OrganizationFlowAdapter.project(
      widget.graph,
      reducedMotion: reducedMotion,
      wideLayout: nextWideLayout,
      onGroupResized: _onGroupResized,
    );
    _reducedMotion = reducedMotion;
    _wideLayout = nextWideLayout;
    _projectedGraph = widget.graph;
    if (previous != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => previous.dispose());
    }
  }

  void _onGroupResized(String flowId, Offset position, Size size) {
    final groupId = flowId.startsWith('group-')
        ? flowId.substring('group-'.length)
        : flowId;
    context.read<OrganizationBloc>().add(
      OrganizationGroupResized(
        groupId,
        OrganizationPoint(position.dx, position.dy),
        OrganizationSize(size.width, size.height),
      ),
    );
  }

  @override
  void dispose() {
    _dragFlushTimer?.cancel();
    _pendingDragPositions.clear();
    _projection.controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 900;
        final inspectorVisible =
            widget.state.selectedNodeId != null ||
            widget.state.selectedRelationId != null ||
            widget.state.selectedGroupId != null;
        if (compact && inspectorVisible && !_compactInspectorOpen) {
          // Selection can be restored by the BLoC (for example after leaving
          // and returning to Organization), without a fresh canvas gesture.
          // Open the sheet after this frame so Navigator is not touched while
          // NodeFlow is mounting or rebuilding its interaction layers.
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) _showCompactInspector();
          });
        }
        return CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.keyZ, meta: true): _undo,
            const SingleActivator(LogicalKeyboardKey.keyZ, control: true):
                _undo,
            const SingleActivator(
              LogicalKeyboardKey.keyZ,
              meta: true,
              shift: true,
            ): _redo,
            const SingleActivator(
              LogicalKeyboardKey.keyZ,
              control: true,
              shift: true,
            ): _redo,
            const SingleActivator(LogicalKeyboardKey.delete): _deleteSelection,
            const SingleActivator(LogicalKeyboardKey.backspace):
                _deleteSelection,
          },
          child: Focus(
            autofocus: true,
            child: Stack(
              fit: StackFit.expand,
              children: [
                RepaintBoundary(
                  child:
                      NodeFlowEditor<
                        OrganizationFlowNodeData,
                        OrganizationFlowRelationData
                      >(
                        key: ObjectKey(_projection.controller),
                        controller: _projection.controller,
                        theme: frankOrganizationFlowTheme(),
                        behavior: NodeFlowBehavior.design,
                        nodeBuilder: _buildNode,
                        portBuilder: _buildPort,
                        labelBuilder: _buildConnectionLabel,
                        events: _events(compact),
                      ),
                ),
                Positioned(
                  left: FrankUiTokens.inset,
                  top: FrankUiTokens.inset,
                  right: compact
                      ? FrankUiTokens.inset
                      : (inspectorVisible
                            ? 320 + FrankUiTokens.inset
                            : FrankUiTokens.inset),
                  child: _OrganizationToolbar(
                    state: widget.state,
                    onAdd: _showAddPalette,
                    onUndo: _undo,
                    onRedo: _redo,
                    onValidate: () => context.read<OrganizationBloc>().add(
                      const OrganizationValidateRequested(),
                    ),
                    onFit: _fitView,
                    onMinimap: () => _projection.controller.minimap?.toggle(),
                    onPublish: () => context.read<OrganizationBloc>().add(
                      const OrganizationPublishRequested(),
                    ),
                    onRetry: () => context.read<OrganizationBloc>().add(
                      const OrganizationRetryRequested(),
                    ),
                  ),
                ),
                if (!compact && inspectorVisible)
                  Positioned(
                    top: 0,
                    right: 0,
                    bottom: 0,
                    width: 320,
                    child: _OrganizationInspector(
                      workspace: widget.workspace,
                      docked: true,
                      onClose: _clearSelection,
                      onDelete: _deleteSelection,
                      onDuplicate: _duplicateSelection,
                    ),
                  ),
                if (widget.state.validation.issues.isNotEmpty)
                  Positioned(
                    left: FrankUiTokens.inset,
                    bottom: FrankUiTokens.inset,
                    width: compact
                        ? (constraints.maxWidth - (FrankUiTokens.inset * 2))
                              .clamp(0.0, 320.0)
                              .toDouble()
                        : 320,
                    child: _ValidationPanel(
                      validation: widget.state.validation,
                      onFocus: _focusIssue,
                    ),
                  ),
                if (widget.graph.nodes.isEmpty)
                  Center(child: _EmptyOrganization(onAdd: _showAddPalette)),
              ],
            ),
          ),
        );
      },
    );
  }

  NodeFlowEvents<OrganizationFlowNodeData, OrganizationFlowRelationData>
  _events(bool compact) => NodeFlowEvents(
    onInit: () {
      final nodeId = widget.state.selectedNodeId;
      final relationId = widget.state.selectedRelationId;
      final groupId = widget.state.selectedGroupId;
      if (nodeId != null) _projection.controller.selectNode(nodeId);
      if (relationId != null) {
        _projection.controller.selectConnection(relationId);
      }
      if (groupId != null) {
        _projection.controller.selectNode('group-$groupId');
      }
    },
    node: NodeEvents(
      onDragStop: (node) {
        final domain = node.data.node;
        final group = node.data.group;
        if (domain == null && group != null) {
          final nodePositions = <String, OrganizationPoint>{};
          for (final flowNode in _projection.controller.nodes.values) {
            final flowDomain = flowNode.data.node;
            if (flowDomain?.groupId == group.id) {
              final position = flowNode.visualPosition.value;
              nodePositions[flowDomain!.id] = OrganizationPoint(
                position.dx,
                position.dy,
              );
            }
          }
          final position = node.visualPosition.value;
          context.read<OrganizationBloc>().add(
            OrganizationGroupMoved(
              group.id,
              OrganizationPoint(position.dx, position.dy),
              nodePositions: nodePositions,
            ),
          );
          return;
        }
        if (domain == null) return;
        // Vyuh keeps the raw drag position and the snapped visual position
        // separately. Persist the latter so the domain graph retains the
        // promised 20 px snap after the controller projection is rebuilt.
        final position = node.visualPosition.value;
        _queueDragPosition(
          node.id,
          OrganizationPoint(position.dx, position.dy),
        );
      },
      onDeleted: (node) {
        if (node.data.group != null) {
          context.read<OrganizationBloc>().add(
            OrganizationGroupsDeleted([node.data.group!.id]),
          );
          return;
        }
        if (node.data.node == null) return;
        context.read<OrganizationBloc>().add(
          OrganizationElementsDeleted(nodeIds: [node.id]),
        );
      },
      onContextMenu: (node, position) {
        if (node.data.group != null) {
          unawaited(_showGroupMenu(node.data.group!.id, position.offset));
          return;
        }
        if (node.data.node == null) return;
        unawaited(_showNodeMenu(node.id, position.offset));
      },
    ),
    connection: ConnectionEvents(
      onSelected: (connection) {
        if (connection == null) {
          _clearConnectionAnimations();
        } else {
          _setConnectionAnimation(connection, active: true);
        }
      },
      onMouseEnter: (connection) =>
          _setConnectionAnimation(connection, active: true),
      onMouseLeave: (connection) =>
          _setConnectionAnimation(connection, active: false),
      onBeforeComplete: (connection) {
        final source = connection.sourceNode.data.node;
        final target = connection.targetNode.data.node;
        if (source == null || target == null) {
          return const ConnectionValidationResult.deny(
            reason: 'Groups cannot be connected.',
          );
        }
        final kind = inferOrganizationRelationKind(
          source: source,
          target: target,
        );
        return kind == null
            ? const ConnectionValidationResult.deny(
                reason:
                    'Use staff-to-staff, staff-to-capability, or staff-to-approval.',
              )
            : const ConnectionValidationResult.allow();
      },
      onCreated: (connection) {
        final source = _projection.nodesById[connection.sourceNodeId];
        final target = _projection.nodesById[connection.targetNodeId];
        if (source == null || target == null) return;
        final kind = inferOrganizationRelationKind(
          source: source,
          target: target,
        );
        if (kind == null) return;
        final permissions = kind == OrganizationRelationKind.toolAccess
            ? [target.capability!.permissions.first]
            : const <String>[];
        context.read<OrganizationBloc>().add(
          OrganizationRelationAdded(
            OrganizationRelation(
              id: connection.id,
              kind: kind,
              sourceNodeId: source.id,
              targetNodeId: target.id,
              permissions: permissions,
            ),
          ),
        );
      },
      onDeleted: (connection) => context.read<OrganizationBloc>().add(
        OrganizationElementsDeleted(relationIds: [connection.id]),
      ),
      onContextMenu: (connection, position) {
        unawaited(_showConnectionMenu(connection.id, position.offset));
      },
    ),
    viewport: ViewportEvents(
      onMoveEnd: (viewport) => context.read<OrganizationBloc>().add(
        OrganizationViewportChanged(
          OrganizationViewport(
            x: viewport.x,
            y: viewport.y,
            zoom: viewport.zoom,
          ),
        ),
      ),
      onCanvasTap: (_) => _clearSelection(),
      onCanvasDoubleTap: (_) => _showAddPalette(),
    ),
    onSelectionChange: (selection) {
      final regularNodes = selection.nodes
          .where((node) => node.data.node != null)
          .toList();
      final groupNodes = selection.nodes
          .where((node) => node.data.group != null)
          .toList();
      final nodeId = regularNodes.isEmpty ? null : regularNodes.last.id;
      final groupId = groupNodes.isEmpty
          ? null
          : groupNodes.last.data.group!.id;
      final relationId = selection.connections.isEmpty
          ? null
          : selection.connections.last.id;
      context.read<OrganizationBloc>().add(
        OrganizationSelectionChanged(
          nodeId: relationId == null ? nodeId : null,
          relationId: relationId,
          groupId: relationId == null ? groupId : null,
          nodeIds: regularNodes.map((node) => node.id).toList(),
          relationIds: selection.connections
              .map((connection) => connection.id)
              .toList(),
          groupIds: groupNodes.map((node) => node.data.group!.id).toList(),
        ),
      );
      if (compact &&
          (nodeId != null || groupId != null || relationId != null)) {
        // Selection can also be restored from BLoC during editor init. Defer
        // the sheet until after that frame so Navigator is never invoked
        // while NodeFlow is still mounting its canvas.
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) _showCompactInspector();
        });
      }
    },
  );

  Widget _buildNode(
    BuildContext context,
    Node<OrganizationFlowNodeData> flowNode,
  ) {
    final node = flowNode.data.node;
    if (node == null && flowNode.data.group != null) {
      return OrganizationGroupVisual(
        node: flowNode as GroupNode<OrganizationFlowNodeData>,
      );
    }
    if (node == null) return const SizedBox.shrink();
    final employee = node.employeeId == null
        ? null
        : widget.workspace.employees
              .where((employee) => employee.id == node.employeeId)
              .firstOrNull;
    return Semantics(
      container: true,
      button: true,
      selected: flowNode.isSelected,
      label: switch (node.kind) {
        OrganizationNodeKind.staff =>
          '${node.label}, ${employee?.role ?? 'staff'}, ${employee?.status ?? 'available'}',
        OrganizationNodeKind.capability =>
          '${node.label} capability, ${node.providerLabel ?? 'profile not selected'}, ${node.configured ? 'configured' : 'setup required'}',
        OrganizationNodeKind.approval => 'Approval Desk, human checkpoint',
      },
      child: Padding(
        // Capability cards carry a compact status row (including the
        // approval-required marker for local execution tools). Give them a
        // little more vertical breathing room inside the fixed flow node so
        // the card remains legible at the editor's default zoom.
        padding: EdgeInsets.all(
          node.kind == OrganizationNodeKind.capability ? 10 : 14,
        ),
        child: switch (node.kind) {
          OrganizationNodeKind.staff => _StaffCard(
            node: node,
            employee: employee,
          ),
          OrganizationNodeKind.capability => _CapabilityCard(node: node),
          OrganizationNodeKind.approval => _ApprovalCard(node: node),
        },
      ),
    );
  }

  Widget _buildPort(
    BuildContext context,
    Node<OrganizationFlowNodeData> node,
    Port port,
  ) {
    final domain = node.data.node;
    final isOutput = port.isOutput;
    final controller =
        _projection.controller
            as NodeFlowController<OrganizationFlowNodeData, dynamic>;
    return Semantics(
      container: true,
      button: true,
      label:
          '${domain?.label ?? 'Organization group'} ${isOutput ? 'output' : 'input'} port',
      hint: isOutput
          ? 'Drag to connect this office element.'
          : 'Drop a relation on this input port.',
      child: PortWidget<OrganizationFlowNodeData>(
        port: port,
        theme:
            controller.theme?.portTheme ??
            frankOrganizationFlowTheme().portTheme,
        isConnected: controller.isPortConnected(node.id, port.id),
        controller: controller,
        nodeId: node.id,
        isOutput: isOutput,
        nodeBounds: node.getBounds(),
      ),
    );
  }

  Widget _buildConnectionLabel(
    BuildContext context,
    Connection<OrganizationFlowRelationData> connection,
    ConnectionLabel label,
    Rect position,
    VoidCallback? onTap,
  ) {
    final relation = connection.data?.relation;
    final color = switch (relation?.kind) {
      OrganizationRelationKind.handoff => organizationHandoffColor,
      OrganizationRelationKind.toolAccess => organizationToolColor,
      OrganizationRelationKind.review => organizationReviewColor,
      null => FrankColors.muted,
    };
    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
        decoration: BoxDecoration(
          color: FrankColors.panel.withValues(alpha: .94),
          borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
          border: Border.all(color: color.withValues(alpha: .55)),
        ),
        child: Text(label.text, style: TextStyle(color: color, fontSize: 9)),
      ),
    );
  }

  Future<void> _showAddPalette() async {
    final choice = await showDialog<_AddChoice>(
      context: context,
      builder: (_) => _AddPalette(
        employees: widget.workspace.employees,
        usedEmployeeIds: widget.graph.nodes
            .map((node) => node.employeeId)
            .whereType<String>()
            .toSet(),
      ),
    );
    if (choice == null || !mounted) return;
    final center = _projection.controller.getViewportCenter().offset;
    final sequence =
        '${DateTime.now().microsecondsSinceEpoch}-${++_newNodeSequence}';
    if (choice is _GroupChoice) {
      final label = await _showGroupNameDialog();
      if (!mounted || label == null) return;
      context.read<OrganizationBloc>().add(
        OrganizationGroupAdded(
          OrganizationGroup(
            id: 'custom-group-$sequence',
            label: label,
            position: OrganizationPoint(
              _snapToGrid(center.dx - 210),
              _snapToGrid(center.dy - 150),
            ),
            size: const OrganizationSize(420, 300),
            tone: OrganizationGroupTone.aubergine,
          ),
        ),
      );
      return;
    }
    final node = switch (choice) {
      _StaffChoice(:final employee) => OrganizationNode(
        id: 'staff-${employee.id}-$sequence',
        kind: OrganizationNodeKind.staff,
        label: employee.name,
        employeeId: employee.id,
        position: OrganizationPoint(center.dx - 110, center.dy - 63),
        groupId: _staffGroup(employee).id,
        configured: true,
      ),
      _CapabilityChoice(:final capability) => OrganizationNode(
        id: '${capability.name}-$sequence',
        kind: OrganizationNodeKind.capability,
        label: capability.label,
        capability: capability,
        position: OrganizationPoint(center.dx - 95, center.dy - 52),
        groupId: _capabilityGroup(capability).id,
        providerLabel: 'Profile not selected',
        approvalRequired: capability.isSensitive,
      ),
      _ApprovalChoice() => OrganizationNode(
        id: 'approval-$sequence',
        kind: OrganizationNodeKind.approval,
        label: 'Approval Desk',
        position: OrganizationPoint(center.dx - 95, center.dy - 52),
        groupId: OrganizationGroup.operationsReview.id,
        configured: true,
        approvalRequired: true,
      ),
      _GroupChoice() => throw StateError('Group choices are handled above.'),
    };
    context.read<OrganizationBloc>().add(OrganizationNodeAdded(node));
  }

  double _snapToGrid(double value) => (value / 20).round() * 20.0;

  Future<String?> _showGroupNameDialog({String? initialValue}) =>
      showDialog<String>(
        context: context,
        builder: (_) => _GroupNameDialog(initialValue: initialValue),
      );

  OrganizationGroup _staffGroup(OfficeEmployee employee) =>
      employee.id == 'ae-maya'
      ? OrganizationGroup.clientServices
      : employee.id == 'accountant-dimas'
      ? OrganizationGroup.operationsReview
      : OrganizationGroup.delivery;

  OrganizationGroup _capabilityGroup(OrganizationCapabilityKind capability) =>
      switch (capability) {
        OrganizationCapabilityKind.email ||
        OrganizationCapabilityKind.calendar => OrganizationGroup.clientServices,
        OrganizationCapabilityKind.taskboard =>
          OrganizationGroup.operationsReview,
        _ => OrganizationGroup.delivery,
      };

  Future<void> _showNodeMenu(String nodeId, Offset position) async {
    final node = widget.graph.nodes
        .where((node) => node.id == nodeId)
        .firstOrNull;
    if (node == null) return;
    final selectedNodeIds = {...widget.state.selectedNodeIds, nodeId};
    final action = await showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(
        position.dx,
        position.dy,
        position.dx,
        position.dy,
      ),
      items: [
        if (node.kind != OrganizationNodeKind.staff)
          const PopupMenuItem(value: 'duplicate', child: Text('Duplicate')),
        if (selectedNodeIds.length >= 2) ...[
          const PopupMenuDivider(),
          const PopupMenuItem(value: 'align-left', child: Text('Align left')),
          const PopupMenuItem(value: 'align-right', child: Text('Align right')),
          const PopupMenuItem(
            value: 'align-center',
            child: Text('Align center'),
          ),
          const PopupMenuItem(value: 'align-top', child: Text('Align top')),
          const PopupMenuItem(
            value: 'align-bottom',
            child: Text('Align bottom'),
          ),
          const PopupMenuItem(
            value: 'align-middle',
            child: Text('Align middle'),
          ),
        ],
        if (selectedNodeIds.length >= 3) ...[
          const PopupMenuItem(
            value: 'distribute-horizontal',
            child: Text('Distribute horizontally'),
          ),
          const PopupMenuItem(
            value: 'distribute-vertical',
            child: Text('Distribute vertically'),
          ),
        ],
        const PopupMenuItem(value: 'delete', child: Text('Delete')),
      ],
    );
    if (!mounted) return;
    if (action == 'duplicate') {
      context.read<OrganizationBloc>().add(OrganizationNodeDuplicated(nodeId));
    } else if (action == 'align-left' ||
        action == 'align-right' ||
        action == 'align-center') {
      context.read<OrganizationBloc>().add(
        OrganizationNodesAligned(selectedNodeIds.toList(), switch (action) {
          'align-left' => OrganizationAlignment.left,
          'align-right' => OrganizationAlignment.right,
          _ => OrganizationAlignment.centerX,
        }),
      );
    } else if (action == 'align-top' ||
        action == 'align-bottom' ||
        action == 'align-middle') {
      context.read<OrganizationBloc>().add(
        OrganizationNodesAligned(selectedNodeIds.toList(), switch (action) {
          'align-top' => OrganizationAlignment.top,
          'align-bottom' => OrganizationAlignment.bottom,
          _ => OrganizationAlignment.centerY,
        }),
      );
    } else if (action == 'distribute-horizontal' ||
        action == 'distribute-vertical') {
      context.read<OrganizationBloc>().add(
        OrganizationNodesDistributed(
          selectedNodeIds.toList(),
          action == 'distribute-horizontal'
              ? OrganizationDistributionAxis.horizontal
              : OrganizationDistributionAxis.vertical,
        ),
      );
    } else if (action == 'delete') {
      context.read<OrganizationBloc>().add(
        OrganizationElementsDeleted(nodeIds: [nodeId]),
      );
    }
  }

  Future<void> _showGroupMenu(String groupId, Offset position) async {
    final group = widget.graph.groupById(groupId);
    if (group == null || group.isBuiltIn) return;
    final action = await showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(
        position.dx,
        position.dy,
        position.dx,
        position.dy,
      ),
      items: [
        const PopupMenuItem(value: 'rename', child: Text('Rename')),
        const PopupMenuItem(value: 'delete', child: Text('Delete group')),
      ],
    );
    if (!mounted) return;
    if (action == 'rename') {
      final label = await _showGroupNameDialog(initialValue: group.label);
      if (!mounted || label == null) return;
      context.read<OrganizationBloc>().add(
        OrganizationGroupUpdated(group.copyWith(label: label)),
      );
    } else if (action == 'delete') {
      context.read<OrganizationBloc>().add(
        OrganizationGroupsDeleted([group.id]),
      );
    }
  }

  Future<void> _showConnectionMenu(String relationId, Offset position) async {
    final action = await showMenu<String>(
      context: context,
      position: RelativeRect.fromLTRB(
        position.dx,
        position.dy,
        position.dx,
        position.dy,
      ),
      items: const [
        PopupMenuItem(value: 'delete', child: Text('Delete relation')),
      ],
    );
    if (!mounted || action != 'delete') return;
    context.read<OrganizationBloc>().add(
      OrganizationElementsDeleted(relationIds: [relationId]),
    );
  }

  void _setConnectionAnimation(
    Connection<OrganizationFlowRelationData> connection, {
    required bool active,
  }) {
    if (_reducedMotion) {
      connection.animationEffect = null;
      return;
    }
    if (!active && connection.selected) return;
    connection.animationEffect = active
        ? PulseEffect(
            speed: 1,
            minOpacity: .72,
            maxOpacity: 1,
            widthVariation: 1.12,
          )
        : null;
  }

  void _queueDragPosition(String nodeId, OrganizationPoint position) {
    _pendingDragPositions[nodeId] = position;
    // endNodeDrag calls onDragStop once per selected node synchronously. Flush
    // on the next turn so that the whole multi-select drag becomes one BLoC
    // event and one undo snapshot.
    _dragFlushTimer ??= Timer(Duration.zero, () {
      _dragFlushTimer = null;
      if (!mounted || _pendingDragPositions.isEmpty) {
        _pendingDragPositions.clear();
        return;
      }
      final positions = Map<String, OrganizationPoint>.of(
        _pendingDragPositions,
      );
      _pendingDragPositions.clear();
      context.read<OrganizationBloc>().add(OrganizationNodesMoved(positions));
    });
  }

  void _clearConnectionAnimations() {
    for (final connection in _projection.controller.connections) {
      connection.animationEffect = null;
    }
  }

  void _showCompactInspector() {
    if (_compactInspectorOpen || !mounted) return;
    _compactInspectorOpen = true;
    unawaited(
      showModalBottomSheet<void>(
        context: context,
        backgroundColor: Colors.transparent,
        isScrollControlled: true,
        sheetAnimationStyle: _reducedMotion ? AnimationStyle.noAnimation : null,
        builder: (sheetContext) => FractionallySizedBox(
          heightFactor: .72,
          child: BlocProvider.value(
            value: context.read<OrganizationBloc>(),
            child: _OrganizationInspector(
              workspace: widget.workspace,
              docked: false,
              onClose: () => Navigator.of(sheetContext).pop(),
              onDelete: () {
                _deleteSelection();
                Navigator.of(sheetContext).pop();
              },
              onDuplicate: _duplicateSelection,
            ),
          ),
        ),
      ).whenComplete(() {
        _compactInspectorOpen = false;
        if (mounted) _clearSelection();
      }),
    );
  }

  void _focusIssue(OrganizationValidationIssue issue) {
    if (issue.nodeId != null) {
      _projection.controller.selectSpecificNodes([issue.nodeId!]);
      _projection.controller.centerOnNode(issue.nodeId!);
      context.read<OrganizationBloc>().add(
        OrganizationSelectionChanged(nodeId: issue.nodeId),
      );
    } else if (issue.relationId != null) {
      _projection.controller.selectConnection(issue.relationId!);
      context.read<OrganizationBloc>().add(
        OrganizationSelectionChanged(relationId: issue.relationId),
      );
    } else if (issue.groupId != null) {
      final flowId = 'group-${issue.groupId}';
      _projection.controller.selectSpecificNodes([flowId]);
      _projection.controller.centerOnNode(flowId);
      context.read<OrganizationBloc>().add(
        OrganizationSelectionChanged(groupId: issue.groupId),
      );
    }
  }

  void _undo() =>
      context.read<OrganizationBloc>().add(const OrganizationUndoRequested());

  void _redo() =>
      context.read<OrganizationBloc>().add(const OrganizationRedoRequested());

  void _fitView() {
    _projection.controller.fitToView();
    final viewport = _projection.controller.viewport;
    // Programmatic viewport changes do not pass through the editor's
    // interaction-end callback, so persist Fit view explicitly. This keeps
    // the last view when the user visits another Office section and returns.
    context.read<OrganizationBloc>().add(
      OrganizationViewportChanged(
        OrganizationViewport(x: viewport.x, y: viewport.y, zoom: viewport.zoom),
      ),
    );
  }

  void _clearSelection() {
    _projection.controller.clearSelection();
    context.read<OrganizationBloc>().add(const OrganizationSelectionChanged());
  }

  void _deleteSelection() {
    final nodeIds = widget.state.selectedNodeIds.isEmpty
        ? (widget.state.selectedNodeId == null
              ? const <String>[]
              : [widget.state.selectedNodeId!])
        : widget.state.selectedNodeIds;
    final relationIds = widget.state.selectedRelationIds.isEmpty
        ? (widget.state.selectedRelationId == null
              ? const <String>[]
              : [widget.state.selectedRelationId!])
        : widget.state.selectedRelationIds;
    final groupIds = widget.state.selectedGroupIds.isEmpty
        ? (widget.state.selectedGroupId == null
              ? const <String>[]
              : [widget.state.selectedGroupId!])
        : widget.state.selectedGroupIds;
    if (nodeIds.isEmpty && relationIds.isEmpty && groupIds.isEmpty) return;
    context.read<OrganizationBloc>().add(
      OrganizationElementsDeleted(
        nodeIds: nodeIds,
        relationIds: relationIds,
        groupIds: groupIds,
      ),
    );
  }

  void _duplicateSelection() {
    final nodeId = widget.state.selectedNodeId;
    if (nodeId == null) return;
    context.read<OrganizationBloc>().add(OrganizationNodeDuplicated(nodeId));
  }
}

Color _organizationStatusColor(String? status) =>
    switch (status?.toLowerCase()) {
      'available' || 'working' => FrankColors.green,
      'reviewing' => FrankColors.warningAmber,
      _ => FrankColors.muted,
    };

class _StaffCard extends StatelessWidget {
  const _StaffCard({required this.node, required this.employee});

  final OrganizationNode node;
  final OfficeEmployee? employee;

  @override
  Widget build(BuildContext context) {
    final statusColor = _organizationStatusColor(employee?.status);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            CircleAvatar(
              radius: 16,
              backgroundColor: FrankColors.panelRaised,
              child: Text(
                employee?.initials ?? node.label.characters.take(2).toString(),
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    node.label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.ink,
                      fontSize: FrankUiTokens.textSize,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  Text(
                    employee?.role ?? 'Staff',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        const Spacer(),
        Row(
          children: [
            _StatusDot(color: statusColor),
            const SizedBox(width: 6),
            Flexible(
              child: Text(
                employee?.status ?? 'Available',
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.muted, fontSize: 10),
              ),
            ),
            const SizedBox(width: 5),
            const Flexible(
              child: Text(
                'Codex · 5.6',
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.end,
                style: TextStyle(color: FrankColors.muted, fontSize: 10),
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _CapabilityCard extends StatelessWidget {
  const _CapabilityCard({required this.node});

  final OrganizationNode node;

  @override
  Widget build(BuildContext context) {
    final capability = node.capability;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(
              capability?.icon ?? FrankIcons.workflow,
              color: FrankColors.muted,
              size: FrankUiTokens.iconSize,
            ),
            const SizedBox(width: 9),
            Expanded(
              child: Text(
                node.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: FrankUiTokens.textSize,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 6),
        ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 164),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
            decoration: BoxDecoration(
              color: FrankColors.panelRaised,
              borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
              border: Border.all(color: FrankColors.border),
            ),
            child: Text(
              node.providerLabel?.isNotEmpty == true
                  ? node.providerLabel!
                  : 'Profile not selected',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(color: FrankColors.muted, fontSize: 9),
            ),
          ),
        ),
        if (capability != null) ...[
          const SizedBox(height: 2),
          Text(
            capability.permissions.join(' · '),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: FrankColors.muted, fontSize: 9),
          ),
        ],
        const Spacer(),
        Row(
          children: [
            Flexible(child: _SetupBadge(configured: node.configured)),
            if (node.approvalRequired) ...[
              const SizedBox(width: 5),
              const Flexible(
                child: Text(
                  'Approval required',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: organizationReviewColor,
                    fontSize: 8,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
            ],
          ],
        ),
      ],
    );
  }
}

class _ApprovalCard extends StatelessWidget {
  const _ApprovalCard({required this.node});

  final OrganizationNode node;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            const Icon(
              FrankIcons.approval,
              color: organizationReviewColor,
              size: FrankUiTokens.iconSize,
            ),
            const SizedBox(width: 9),
            Expanded(
              child: Text(
                node.label,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontWeight: FontWeight.w600,
                  fontSize: FrankUiTokens.textSize,
                ),
              ),
            ),
          ],
        ),
        const Spacer(),
        const Text(
          'Human checkpoint',
          style: TextStyle(color: organizationReviewColor, fontSize: 11),
        ),
      ],
    );
  }
}

class _OrganizationToolbar extends StatelessWidget {
  const _OrganizationToolbar({
    required this.state,
    required this.onAdd,
    required this.onUndo,
    required this.onRedo,
    required this.onValidate,
    required this.onFit,
    required this.onMinimap,
    required this.onPublish,
    required this.onRetry,
  });

  final OrganizationState state;
  final VoidCallback onAdd;
  final VoidCallback onUndo;
  final VoidCallback onRedo;
  final VoidCallback onValidate;
  final VoidCallback onFit;
  final VoidCallback onMinimap;
  final VoidCallback onPublish;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final failed =
        state.persistenceStatus == OrganizationPersistenceStatus.saveFailure ||
        state.persistenceStatus == OrganizationPersistenceStatus.publishFailure;
    return Semantics(
      container: true,
      label: 'Organization editor toolbar',
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: SizedBox(
          height: FrankUiTokens.toolbarHeight,
          child: _OrganizationPanel(
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _ToolbarButton(
                  key: const ValueKey('organization-add'),
                  icon: FrankIcons.plus,
                  label: 'Add',
                  onPressed: onAdd,
                  prominent: true,
                ),
                _ToolbarButton(
                  icon: FrankIcons.undo,
                  label: 'Undo',
                  onPressed: state.canUndo ? onUndo : null,
                ),
                _ToolbarButton(
                  icon: FrankIcons.redo,
                  label: 'Redo',
                  onPressed: state.canRedo ? onRedo : null,
                ),
                _ToolbarButton(
                  icon: FrankIcons.circleCheck,
                  label: 'Validate',
                  onPressed: onValidate,
                ),
                _ToolbarButton(
                  icon: FrankIcons.recenter,
                  label: 'Fit view',
                  onPressed: onFit,
                ),
                _ToolbarButton(
                  icon: FrankIcons.minimap,
                  label: 'Minimap',
                  onPressed: onMinimap,
                ),
                const SizedBox(width: 6),
                _DraftStatus(state: state),
                if (failed)
                  TextButton(
                    onPressed: onRetry,
                    style: _compactTextButtonStyle(),
                    child: const Text('Retry'),
                  ),
                const SizedBox(width: 4),
                FilledButton.icon(
                  key: const ValueKey('organization-publish'),
                  onPressed: state.canPublish ? onPublish : null,
                  style: _publishButtonStyle(),
                  icon: const Icon(
                    FrankIcons.publish,
                    size: FrankUiTokens.iconSize - 1,
                  ),
                  label: Text(
                    state.persistenceStatus ==
                            OrganizationPersistenceStatus.publishing
                        ? 'Publishing…'
                        : 'Publish',
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _OrganizationInspector extends StatelessWidget {
  const _OrganizationInspector({
    required this.workspace,
    this.docked = true,
    required this.onClose,
    required this.onDelete,
    required this.onDuplicate,
  });

  final OfficeWorkspace workspace;
  final bool docked;
  final VoidCallback onClose;
  final VoidCallback onDelete;
  final VoidCallback onDuplicate;

  @override
  Widget build(BuildContext context) {
    return BlocBuilder<OrganizationBloc, OrganizationState>(
      builder: (context, state) {
        final graph = state.graph;
        if (graph == null) return const SizedBox.shrink();
        final node = graph.nodes
            .where((node) => node.id == state.selectedNodeId)
            .firstOrNull;
        final relation = graph.relations
            .where((relation) => relation.id == state.selectedRelationId)
            .firstOrNull;
        final group = graph.groupById(state.selectedGroupId);
        if (node == null && relation == null && group == null) {
          return const SizedBox.shrink();
        }
        return Semantics(
          container: true,
          label: 'Organization inspector',
          child: _OrganizationPanel(
            padding: EdgeInsets.zero,
            radius: docked ? 0 : FrankUiTokens.panelRadius,
            borderRadius: docked
                ? BorderRadius.zero
                : const BorderRadius.vertical(
                    top: Radius.circular(FrankUiTokens.panelRadius),
                  ),
            border: docked
                ? const Border(
                    left: BorderSide(
                      color: FrankColors.border,
                      width: FrankUiTokens.borderWidth,
                    ),
                  )
                : null,
            child: Column(
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(18, 14, 10, 10),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          group != null
                              ? 'Group'
                              : node?.kind == OrganizationNodeKind.staff
                              ? 'Staff'
                              : node?.kind == OrganizationNodeKind.capability
                              ? 'Capability'
                              : node?.kind == OrganizationNodeKind.approval
                              ? 'Control'
                              : relation!.kind.label,
                          style: const TextStyle(
                            color: FrankColors.ink,
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Close inspector',
                        onPressed: onClose,
                        style: IconButton.styleFrom(
                          minimumSize: const Size.square(
                            FrankUiTokens.controlHeight,
                          ),
                          padding: EdgeInsets.zero,
                          tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                        ),
                        icon: const Icon(FrankIcons.close, size: 17),
                      ),
                    ],
                  ),
                ),
                const Divider(height: 1),
                Expanded(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(18),
                    child: group != null
                        ? _GroupInspector(group: group)
                        : node != null
                        ? _NodeInspector(node: node, workspace: workspace)
                        : _RelationInspector(relation: relation!, graph: graph),
                  ),
                ),
                const Divider(height: 1),
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: Wrap(
                    alignment: WrapAlignment.end,
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      if (node != null &&
                          node.kind != OrganizationNodeKind.staff)
                        TextButton.icon(
                          onPressed: onDuplicate,
                          style: _compactTextButtonStyle(),
                          icon: const Icon(FrankIcons.plus, size: 15),
                          label: const Text('Duplicate'),
                        ),
                      if (group != null && !group.isBuiltIn)
                        TextButton.icon(
                          onPressed: onDelete,
                          style: _compactTextButtonStyle(),
                          icon: const Icon(FrankIcons.archive, size: 15),
                          label: const Text('Delete'),
                        ),
                      if (group == null)
                        TextButton.icon(
                          onPressed: onDelete,
                          style: _compactTextButtonStyle(),
                          icon: const Icon(FrankIcons.archive, size: 15),
                          label: const Text('Delete'),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

InputDecoration _organizationInputDecoration({String? hintText}) =>
    InputDecoration(
      hintText: hintText,
      isDense: true,
      filled: true,
      fillColor: FrankColors.panelRaised,
      contentPadding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        borderSide: const BorderSide(color: FrankColors.border),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        borderSide: const BorderSide(color: FrankColors.border),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        borderSide: const BorderSide(color: FrankColors.aubergineAccent),
      ),
      hintStyle: const TextStyle(color: FrankColors.muted, fontSize: 11),
    );

class _GroupInspector extends StatelessWidget {
  const _GroupInspector({required this.group});

  final OrganizationGroup group;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _InspectorLabel('Name'),
        TextFormField(
          key: ValueKey('group-label-${group.id}'),
          initialValue: group.label,
          enabled: !group.isBuiltIn,
          decoration: _organizationInputDecoration(),
          onFieldSubmitted: (value) {
            final label = value.trim();
            if (label.isEmpty) return;
            context.read<OrganizationBloc>().add(
              OrganizationGroupUpdated(group.copyWith(label: label)),
            );
          },
        ),
        const SizedBox(height: 20),
        _InspectorLabel('Position'),
        Text(
          '${group.position.x.round()} × ${group.position.y.round()}',
          style: const TextStyle(color: FrankColors.ink),
        ),
        const SizedBox(height: 16),
        _InspectorLabel('Container size'),
        Text(
          '${group.size.width.round()} × ${group.size.height.round()}',
          style: const TextStyle(color: FrankColors.ink),
        ),
        const SizedBox(height: 20),
        _Notice(
          icon: group.isBuiltIn ? FrankIcons.circleDashed : FrankIcons.workflow,
          text: group.isBuiltIn
              ? 'Built-in container. Its name, position, and size stay fixed.'
              : 'Drag or resize this square container to organize the board.',
        ),
      ],
    );
  }
}

class _NodeInspector extends StatelessWidget {
  const _NodeInspector({required this.node, required this.workspace});

  final OrganizationNode node;
  final OfficeWorkspace workspace;

  @override
  Widget build(BuildContext context) {
    final employee = node.employeeId == null
        ? null
        : workspace.employees
              .where((employee) => employee.id == node.employeeId)
              .firstOrNull;
    final capability = node.capability;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _InspectorLabel('Name'),
        TextFormField(
          key: ValueKey('node-label-${node.id}'),
          initialValue: node.label,
          decoration: _organizationInputDecoration(),
          onFieldSubmitted: (value) => context.read<OrganizationBloc>().add(
            OrganizationNodeUpdated(node.copyWith(label: value.trim())),
          ),
        ),
        const SizedBox(height: 20),
        if (employee != null) ...[
          _InspectorLabel('Role'),
          Text(employee.role, style: const TextStyle(color: FrankColors.ink)),
          const SizedBox(height: 16),
          _InspectorLabel('Provider / model'),
          const Text(
            'Codex · GPT-5.6',
            style: TextStyle(color: FrankColors.ink),
          ),
        ],
        if (capability != null) ...[
          _InspectorLabel('Provider / profile'),
          TextFormField(
            key: ValueKey('node-provider-${node.id}'),
            initialValue: node.providerLabel,
            decoration: _organizationInputDecoration(
              hintText: 'Choose a connection profile',
            ),
            onFieldSubmitted: (value) {
              final profileLabel = value.trim();
              final hasProfile =
                  profileLabel.isNotEmpty &&
                  profileLabel.toLowerCase() != 'profile not selected';
              context.read<OrganizationBloc>().add(
                OrganizationNodeUpdated(
                  node.copyWith(
                    providerLabel: hasProfile ? profileLabel : null,
                    configured: hasProfile,
                  ),
                ),
              );
            },
          ),
          const SizedBox(height: 18),
          _InspectorLabel('Available permissions'),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final permission in capability.permissions)
                Chip(
                  label: Text(
                    permission,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 10,
                    ),
                  ),
                  backgroundColor: FrankColors.panelRaised,
                  side: const BorderSide(color: FrankColors.border),
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(
                      FrankUiTokens.controlRadius,
                    ),
                  ),
                  visualDensity: VisualDensity.compact,
                  padding: const EdgeInsets.symmetric(horizontal: 2),
                ),
            ],
          ),
          if (node.approvalRequired) ...[
            const SizedBox(height: 18),
            const _Notice(
              icon: FrankIcons.approval,
              text: 'Approval required by default for sensitive access.',
            ),
          ],
        ],
        if (node.kind == OrganizationNodeKind.approval)
          const _Notice(
            icon: FrankIcons.approval,
            text:
                'Pauses the flow until a person reviews and approves the handoff.',
          ),
        const SizedBox(height: 22),
        const _Notice(
          icon: FrankIcons.circleAlert,
          text:
              'Only a profile reference is stored. Secrets, tokens, passwords, and connection strings never enter this graph.',
        ),
      ],
    );
  }
}

class _RelationInspector extends StatelessWidget {
  const _RelationInspector({required this.relation, required this.graph});

  final OrganizationRelation relation;
  final OrganizationGraph graph;

  @override
  Widget build(BuildContext context) {
    final source = graph.nodes
        .where((node) => node.id == relation.sourceNodeId)
        .firstOrNull;
    final target = graph.nodes
        .where((node) => node.id == relation.targetNodeId)
        .firstOrNull;
    if (source == null || target == null) {
      return const _Notice(
        icon: FrankIcons.circleAlert,
        text: 'This relation points to a node that no longer exists.',
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _InspectorLabel('Flow'),
        Text(
          '${source.label}  →  ${target.label}',
          style: const TextStyle(color: FrankColors.ink, fontSize: 13),
        ),
        const SizedBox(height: 22),
        if (relation.kind == OrganizationRelationKind.handoff) ...[
          _InspectorLabel('Input / artifact received'),
          TextFormField(
            initialValue: relation.contract.inputSummary,
            minLines: 2,
            maxLines: 3,
            decoration: _organizationInputDecoration(),
            onFieldSubmitted: (value) => _updateContract(
              context,
              relation.contract.copyWith(inputSummary: value),
            ),
          ),
          const SizedBox(height: 18),
          _InspectorLabel('Expected output'),
          TextFormField(
            initialValue: relation.contract.expectedOutput,
            minLines: 2,
            maxLines: 3,
            decoration: _organizationInputDecoration(),
            onFieldSubmitted: (value) => _updateContract(
              context,
              relation.contract.copyWith(expectedOutput: value),
            ),
          ),
          const SizedBox(height: 18),
          _InspectorLabel('Context policy'),
          DropdownButtonFormField<OrganizationContextPolicy>(
            initialValue: relation.contract.contextPolicy,
            decoration: _organizationInputDecoration(),
            items: [
              for (final policy in OrganizationContextPolicy.values)
                DropdownMenuItem(value: policy, child: Text(policy.label)),
            ],
            onChanged: (policy) {
              if (policy == null) return;
              _updateContract(
                context,
                relation.contract.copyWith(contextPolicy: policy),
              );
            },
          ),
        ],
        if (relation.kind == OrganizationRelationKind.toolAccess) ...[
          _InspectorLabel('Permissions'),
          for (final permission in target.capability?.permissions ?? const [])
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              dense: true,
              value: relation.permissions.contains(permission),
              title: Text(permission),
              onChanged: (checked) {
                final permissions = [...relation.permissions];
                if (checked ?? false) {
                  if (!permissions.contains(permission)) {
                    permissions.add(permission);
                  }
                } else {
                  permissions.remove(permission);
                }
                context.read<OrganizationBloc>().add(
                  OrganizationRelationUpdated(
                    relation.copyWith(permissions: permissions),
                  ),
                );
              },
            ),
          if (target.approvalRequired)
            const _Notice(
              icon: FrankIcons.approval,
              text: 'This capability requires approval before use.',
            ),
        ],
        if (relation.kind == OrganizationRelationKind.review)
          const _Notice(
            icon: FrankIcons.approval,
            text: 'Review relations route work through a human checkpoint.',
          ),
      ],
    );
  }

  void _updateContract(
    BuildContext context,
    OrganizationHandoffContract contract,
  ) {
    context.read<OrganizationBloc>().add(
      OrganizationRelationUpdated(relation.copyWith(contract: contract)),
    );
  }
}

sealed class _AddChoice {
  const _AddChoice();
}

final class _StaffChoice extends _AddChoice {
  const _StaffChoice(this.employee);
  final OfficeEmployee employee;
}

final class _CapabilityChoice extends _AddChoice {
  const _CapabilityChoice(this.capability);
  final OrganizationCapabilityKind capability;
}

final class _ApprovalChoice extends _AddChoice {
  const _ApprovalChoice();
}

final class _GroupChoice extends _AddChoice {
  const _GroupChoice();
}

class _GroupNameDialog extends StatefulWidget {
  const _GroupNameDialog({this.initialValue});

  final String? initialValue;

  @override
  State<_GroupNameDialog> createState() => _GroupNameDialogState();
}

class _GroupNameDialogState extends State<_GroupNameDialog> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.initialValue,
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: FrankColors.panel,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        side: const BorderSide(
          color: FrankColors.border,
          width: FrankUiTokens.borderWidth,
        ),
      ),
      title: const Text('New group'),
      content: TextField(
        key: const ValueKey('organization-group-name'),
        controller: _controller,
        autofocus: true,
        maxLength: 48,
        textCapitalization: TextCapitalization.words,
        decoration: _organizationInputDecoration(hintText: 'e.g. Research'),
        onSubmitted: (_) => _submit(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          style: _compactTextButtonStyle(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('organization-group-create'),
          onPressed: _submit,
          style: _publishButtonStyle(),
          child: const Text('Create group'),
        ),
      ],
    );
  }

  void _submit() {
    final label = _controller.text.trim();
    if (label.isEmpty) return;
    Navigator.pop(context, label);
  }
}

class _AddPalette extends StatefulWidget {
  const _AddPalette({required this.employees, required this.usedEmployeeIds});

  final List<OfficeEmployee> employees;
  final Set<String> usedEmployeeIds;

  @override
  State<_AddPalette> createState() => _AddPaletteState();
}

class _AddPaletteState extends State<_AddPalette> {
  String query = '';

  @override
  Widget build(BuildContext context) {
    final normalized = query.trim().toLowerCase();
    bool matches(String value) =>
        normalized.isEmpty || value.toLowerCase().contains(normalized);
    final staff = widget.employees
        .where((employee) => !widget.usedEmployeeIds.contains(employee.id))
        .where((employee) => matches('${employee.name} ${employee.role}'))
        .toList();
    final capabilities = OrganizationCapabilityKind.values
        .where(
          (capability) => matches('${capability.label} ${capability.category}'),
        )
        .toList();
    final showApproval = matches('approval desk control human review');
    final showGroup = matches('group container structure organization');
    return Dialog(
      backgroundColor: FrankColors.panel,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        side: const BorderSide(
          color: FrankColors.border,
          width: FrankUiTokens.borderWidth,
        ),
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 500, maxHeight: 650),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 16, 18, 10),
              child: TextField(
                autofocus: true,
                decoration:
                    _organizationInputDecoration(
                      hintText: 'Search staff or office elements',
                    ).copyWith(
                      prefixIcon: const Icon(
                        FrankIcons.search,
                        size: FrankUiTokens.iconSize,
                      ),
                    ),
                onChanged: (value) => setState(() => query = value),
              ),
            ),
            const Divider(height: 1),
            Flexible(
              child: ListView(
                padding: const EdgeInsets.all(10),
                shrinkWrap: true,
                children: [
                  if (staff.isNotEmpty) ...[
                    const _PaletteHeading('TEAM'),
                    for (final employee in staff)
                      ListTile(
                        leading: const Icon(FrankIcons.user),
                        title: Text(employee.name),
                        subtitle: Text(employee.role),
                        onTap: () =>
                            Navigator.pop(context, _StaffChoice(employee)),
                      ),
                  ],
                  for (final category in const [
                    'Communication',
                    'Operations',
                    'Knowledge',
                    'Execution',
                  ]) ...[
                    if (capabilities.any(
                      (capability) => capability.category == category,
                    ))
                      _PaletteHeading(category.toUpperCase()),
                    for (final capability in capabilities.where(
                      (capability) => capability.category == category,
                    ))
                      ListTile(
                        leading: Icon(capability.icon),
                        title: Text(capability.label),
                        subtitle: Text(capability.permissions.join(' · ')),
                        onTap: () => Navigator.pop(
                          context,
                          _CapabilityChoice(capability),
                        ),
                      ),
                  ],
                  if (showApproval) ...[
                    const _PaletteHeading('CONTROL'),
                    ListTile(
                      leading: const Icon(
                        FrankIcons.approval,
                        color: organizationReviewColor,
                      ),
                      title: const Text('Approval Desk'),
                      subtitle: const Text('Human checkpoint'),
                      onTap: () =>
                          Navigator.pop(context, const _ApprovalChoice()),
                    ),
                  ],
                  if (showGroup) ...[
                    const _PaletteHeading('STRUCTURE'),
                    ListTile(
                      key: const ValueKey('add-group'),
                      leading: const Icon(FrankIcons.workflow),
                      title: const Text('Group'),
                      subtitle: const Text('Container for office elements'),
                      onTap: () => Navigator.pop(context, const _GroupChoice()),
                    ),
                  ],
                  if (staff.isEmpty &&
                      capabilities.isEmpty &&
                      !showApproval &&
                      !showGroup)
                    const Padding(
                      padding: EdgeInsets.all(26),
                      child: Text(
                        'No matching office element.',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: FrankColors.muted),
                      ),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

String organizationValidationSummary(OrganizationValidation validation) {
  final errors = validation.issues
      .where((issue) => issue.severity == OrganizationIssueSeverity.error)
      .length;
  final errorLabel = errors == 1 ? 'error' : 'errors';
  final warningLabel = validation.warningCount == 1 ? 'warning' : 'warnings';
  return '$errors $errorLabel · ${validation.warningCount} $warningLabel';
}

class _ValidationPanel extends StatefulWidget {
  const _ValidationPanel({required this.validation, required this.onFocus});

  final OrganizationValidation validation;
  final ValueChanged<OrganizationValidationIssue> onFocus;

  @override
  State<_ValidationPanel> createState() => _ValidationPanelState();
}

class _ValidationPanelState extends State<_ValidationPanel> {
  late bool _expanded = widget.validation.hasErrors;

  @override
  void didUpdateWidget(covariant _ValidationPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!oldWidget.validation.hasErrors && widget.validation.hasErrors) {
      _expanded = true;
    }
  }

  @override
  Widget build(BuildContext context) {
    final hasIssues = widget.validation.issues.isNotEmpty;
    return _OrganizationPanel(
      padding: EdgeInsets.zero,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Semantics(
            button: true,
            toggled: _expanded,
            label: 'Validation results',
            value: organizationValidationSummary(widget.validation),
            child: Material(
              color: Colors.transparent,
              child: InkWell(
                onTap: () => setState(() => _expanded = !_expanded),
                borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
                child: SizedBox(
                  height: FrankUiTokens.toolbarHeight,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 10),
                    child: Row(
                      children: [
                        Icon(
                          key: const ValueKey(
                            'organization-validation-status-icon',
                          ),
                          hasIssues
                              ? FrankIcons.circleAlert
                              : FrankIcons.circleCheck,
                          color: hasIssues
                              ? FrankColors.warningAmber
                              : FrankColors.muted,
                          size: FrankUiTokens.iconSize,
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            organizationValidationSummary(widget.validation),
                            style: const TextStyle(
                              color: FrankColors.ink,
                              fontSize: FrankUiTokens.textSize,
                            ),
                          ),
                        ),
                        Icon(
                          _expanded
                              ? FrankIcons.chevronUp
                              : FrankIcons.chevronDown,
                          color: FrankColors.muted,
                          size: FrankUiTokens.iconSize,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
          if (_expanded)
            Padding(
              padding: const EdgeInsets.fromLTRB(8, 0, 8, 6),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (final issue in widget.validation.issues.take(5))
                    _ValidationIssueRow(issue: issue, onFocus: widget.onFocus),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _ValidationIssueRow extends StatelessWidget {
  const _ValidationIssueRow({required this.issue, required this.onFocus});

  final OrganizationValidationIssue issue;
  final ValueChanged<OrganizationValidationIssue> onFocus;

  @override
  Widget build(BuildContext context) {
    final actionable =
        issue.nodeId != null ||
        issue.relationId != null ||
        issue.groupId != null;
    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: actionable ? () => onFocus(issue) : null,
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        child: ConstrainedBox(
          constraints: const BoxConstraints(
            minHeight: FrankUiTokens.controlHeight,
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 5),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Icon(
                  issue.severity == OrganizationIssueSeverity.error
                      ? FrankIcons.circleAlert
                      : FrankIcons.circleDashed,
                  size: FrankUiTokens.iconSize - 1,
                  color: FrankColors.warningAmber,
                ),
                const SizedBox(width: 7),
                Expanded(
                  child: Text(
                    issue.message,
                    style: const TextStyle(
                      color: FrankColors.ink,
                      fontSize: 11,
                      height: 1.35,
                    ),
                  ),
                ),
                if (actionable) ...[
                  const SizedBox(width: 5),
                  const Icon(
                    FrankIcons.chevronRight,
                    size: 14,
                    color: FrankColors.muted,
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _EmptyOrganization extends StatelessWidget {
  const _EmptyOrganization({required this.onAdd});
  final VoidCallback onAdd;

  @override
  Widget build(BuildContext context) {
    return _OrganizationPanel(
      padding: const EdgeInsets.all(28),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(
            FrankIcons.workflow,
            size: 32,
            color: FrankColors.aubergineAccent,
          ),
          const SizedBox(height: 14),
          const Text(
            'Build your agency flow',
            style: TextStyle(
              color: FrankColors.ink,
              fontSize: 18,
              fontWeight: FontWeight.w600,
            ),
          ),
          const SizedBox(height: 7),
          const Text(
            'Start with a staff member, then connect capabilities and approvals.',
            textAlign: TextAlign.center,
            style: TextStyle(color: FrankColors.muted, fontSize: 12),
          ),
          const SizedBox(height: 18),
          FilledButton.icon(
            onPressed: onAdd,
            style: _publishButtonStyle(),
            icon: const Icon(FrankIcons.plus),
            label: const Text('Add office element'),
          ),
        ],
      ),
    );
  }
}

class _OrganizationLoading extends StatelessWidget {
  const _OrganizationLoading();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Semantics(
        label: 'Loading organization',
        child: const CircularProgressIndicator(),
      ),
    );
  }
}

class _OrganizationFailure extends StatelessWidget {
  const _OrganizationFailure({required this.message});
  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: _OrganizationPanel(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(FrankIcons.circleAlert, color: FrankColors.warningAmber),
            const SizedBox(height: 10),
            Text(message, style: const TextStyle(color: FrankColors.ink)),
            const SizedBox(height: 14),
            FilledButton(
              onPressed: () => context.read<OrganizationBloc>().add(
                const OrganizationRetryRequested(),
              ),
              style: _publishButtonStyle(),
              child: const Text('Retry'),
            ),
          ],
        ),
      ),
    );
  }
}

class _OrganizationPanel extends StatelessWidget {
  const _OrganizationPanel({
    required this.child,
    this.padding = const EdgeInsets.all(6),
    this.radius = FrankUiTokens.panelRadius,
    this.borderRadius,
    this.border,
  });

  final Widget child;
  final EdgeInsets padding;
  final double radius;
  final BorderRadius? borderRadius;
  final Border? border;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: borderRadius ?? BorderRadius.circular(radius),
        border:
            border ??
            Border.all(
              color: FrankColors.border,
              width: FrankUiTokens.borderWidth,
            ),
      ),
      child: Padding(padding: padding, child: child),
    );
  }
}

class _ToolbarButton extends StatelessWidget {
  const _ToolbarButton({
    required this.icon,
    required this.label,
    required this.onPressed,
    this.prominent = false,
    super.key,
  });

  final IconData icon;
  final String label;
  final VoidCallback? onPressed;
  final bool prominent;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: label,
      child: TextButton.icon(
        onPressed: onPressed,
        style: _toolbarButtonStyle(prominent: prominent),
        icon: Icon(icon, size: FrankUiTokens.iconSize),
        label: Text(
          label,
          style: const TextStyle(fontSize: FrankUiTokens.textSize),
        ),
      ),
    );
  }
}

ButtonStyle _toolbarButtonStyle({bool prominent = false}) => ButtonStyle(
  minimumSize: const WidgetStatePropertyAll(
    Size(0, FrankUiTokens.controlHeight),
  ),
  padding: const WidgetStatePropertyAll(EdgeInsets.symmetric(horizontal: 8)),
  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
  shape: WidgetStatePropertyAll(
    RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
    ),
  ),
  foregroundColor: WidgetStateProperty.resolveWith((states) {
    if (states.contains(WidgetState.disabled)) {
      return FrankColors.muted.withValues(alpha: .42);
    }
    return prominent ? FrankColors.ink : FrankColors.muted;
  }),
  backgroundColor: WidgetStateProperty.resolveWith((states) {
    if (states.contains(WidgetState.disabled)) return Colors.transparent;
    if (states.contains(WidgetState.hovered) ||
        states.contains(WidgetState.pressed)) {
      return FrankColors.ink.withValues(
        alpha: prominent
            ? FrankUiTokens.selectedInkOpacity
            : FrankUiTokens.hoverInkOpacity,
      );
    }
    return prominent
        ? FrankColors.ink.withValues(alpha: FrankUiTokens.selectedInkOpacity)
        : Colors.transparent;
  }),
  overlayColor: const WidgetStatePropertyAll(Colors.transparent),
  textStyle: const WidgetStatePropertyAll(
    TextStyle(fontSize: FrankUiTokens.textSize),
  ),
);

ButtonStyle _compactTextButtonStyle() => TextButton.styleFrom(
  minimumSize: const Size(0, FrankUiTokens.controlHeight),
  padding: const EdgeInsets.symmetric(horizontal: 8),
  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
  shape: RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
  ),
  foregroundColor: FrankColors.muted,
  disabledForegroundColor: FrankColors.muted.withValues(alpha: .42),
  overlayColor: FrankColors.ink.withValues(
    alpha: FrankUiTokens.hoverInkOpacity,
  ),
  textStyle: const TextStyle(fontSize: FrankUiTokens.textSize),
);

ButtonStyle _publishButtonStyle() => FilledButton.styleFrom(
  minimumSize: const Size(0, FrankUiTokens.controlHeight),
  padding: const EdgeInsets.symmetric(horizontal: 12),
  tapTargetSize: MaterialTapTargetSize.shrinkWrap,
  shape: RoundedRectangleBorder(
    borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
  ),
  backgroundColor: FrankColors.aubergine,
  foregroundColor: FrankColors.ink,
  disabledBackgroundColor: FrankColors.border,
  disabledForegroundColor: FrankColors.muted.withValues(alpha: .52),
  overlayColor: Colors.transparent,
  textStyle: const TextStyle(
    fontSize: FrankUiTokens.textSize,
    fontWeight: FontWeight.w600,
  ),
);

class _DraftStatus extends StatelessWidget {
  const _DraftStatus({required this.state});
  final OrganizationState state;

  @override
  Widget build(BuildContext context) {
    final (label, color) = switch (state.persistenceStatus) {
      OrganizationPersistenceStatus.published => (
        'Published r${state.graph?.publishedRevision ?? 0}',
        FrankColors.green,
      ),
      OrganizationPersistenceStatus.clean => ('Draft saved', FrankColors.muted),
      OrganizationPersistenceStatus.dirty => (
        'Unsaved draft',
        FrankColors.warningAmber,
      ),
      OrganizationPersistenceStatus.saving => ('Saving…', FrankColors.blue),
      OrganizationPersistenceStatus.saveFailure => (
        'Save failed',
        FrankColors.warningAmber,
      ),
      OrganizationPersistenceStatus.publishing => (
        'Publishing…',
        FrankColors.blue,
      ),
      OrganizationPersistenceStatus.publishFailure => (
        'Publish failed',
        FrankColors.warningAmber,
      ),
    };
    return Semantics(
      liveRegion: true,
      label: label,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _StatusDot(color: color),
          const SizedBox(width: 6),
          Text(label, style: TextStyle(color: color, fontSize: 11)),
        ],
      ),
    );
  }
}

class _StatusDot extends StatelessWidget {
  const _StatusDot({required this.color});
  final Color color;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      child: const SizedBox.square(dimension: 7),
    );
  }
}

class _SetupBadge extends StatelessWidget {
  const _SetupBadge({required this.configured});
  final bool configured;

  @override
  Widget build(BuildContext context) {
    final color = configured ? FrankColors.green : FrankColors.warningAmber;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(
        color: color.withValues(alpha: .12),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        configured ? 'Connected' : 'Setup required',
        style: TextStyle(
          color: color,
          fontSize: 8,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _InspectorLabel extends StatelessWidget {
  const _InspectorLabel(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 7),
      child: Text(
        text.toUpperCase(),
        style: const TextStyle(
          color: FrankColors.muted,
          fontSize: 10,
          letterSpacing: .7,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _PaletteHeading extends StatelessWidget {
  const _PaletteHeading(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 13, 12, 5),
      child: Text(
        text,
        style: const TextStyle(
          color: FrankColors.muted,
          fontSize: 10,
          letterSpacing: .8,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _Notice extends StatelessWidget {
  const _Notice({required this.icon, required this.text});
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(11),
      decoration: BoxDecoration(
        color: FrankColors.panelRaised,
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            icon,
            color: FrankColors.warningAmber,
            size: FrankUiTokens.iconSize,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              text,
              style: const TextStyle(
                color: FrankColors.muted,
                fontSize: 11,
                height: 1.4,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
