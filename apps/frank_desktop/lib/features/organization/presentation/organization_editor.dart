part of 'organization_surface.dart';

class _OrganizationEditor extends StatefulWidget {
  const _OrganizationEditor({
    required this.graph,
    required this.state,
    required this.workspace,
    this.profiles,
    this.roles = const [],
    this.workflowProjection = const WorkflowProjection(),
    required this.lookupIndex,
    super.key,
  });

  final OrganizationGraph graph;
  final OrganizationState state;
  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;
  final List<TeamRoleSummary> roles;
  final WorkflowProjection workflowProjection;
  final OrganizationLookupIndex lookupIndex;

  @override
  State<_OrganizationEditor> createState() => _OrganizationEditorState();
}

class _OrganizationEditorState extends State<_OrganizationEditor> {
  late OrganizationFlowProjection _projection;
  OrganizationGraph? _projectedGraph;
  bool _reducedMotion = false;
  bool _wideLayout = true;
  double _textScaleFactor = 1;
  int _newNodeSequence = 0;
  final FocusNode _editorFocusNode = FocusNode(
    debugLabel: 'organization-editor',
  );
  final Map<String, OrganizationPoint> _pendingDragPositions = {};
  Timer? _dragFlushTimer;
  late final FrankDesktopMenuController _contextMenuController;
  List<FrankMenuGroup> _contextMenuGroups = const [];

  @override
  void initState() {
    super.initState();
    _contextMenuController = FrankDesktopMenuController();
    _replaceProjection(reducedMotion: false);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reducedMotion =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    final mediaWidth = MediaQuery.maybeOf(context)?.size.width ?? 0;
    final textScaleFactor =
        MediaQuery.maybeOf(context)?.textScaler.scale(1) ?? 1;
    final view = View.maybeOf(context);
    final viewWidth = view == null || view.devicePixelRatio <= 0
        ? 1600.0
        : view.physicalSize.width / view.devicePixelRatio;
    final wideLayout = (mediaWidth > 0 ? mediaWidth : viewWidth) >= 900;
    if (reducedMotion == _reducedMotion &&
        wideLayout == _wideLayout &&
        textScaleFactor == _textScaleFactor &&
        _projectedGraph != null) {
      return;
    }
    _replaceProjection(
      reducedMotion: reducedMotion,
      wideLayout: wideLayout,
      textScaleFactor: textScaleFactor,
    );
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

  List<String> _selectedIds(List<String> ids, String? id) {
    if (ids.isNotEmpty) return ids;
    if (id == null) return const <String>[];
    return <String>[id];
  }

  void _syncControllerSelection() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _projection.controller.clearSelection();
      final nodeIds = _selectedIds(
        widget.state.selectedNodeIds,
        widget.state.selectedNodeId,
      );
      final relationIds = _selectedIds(
        widget.state.selectedRelationIds,
        widget.state.selectedRelationId,
      );
      final groupIds = _selectedIds(
        widget.state.selectedGroupIds,
        widget.state.selectedGroupId,
      );
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

  void _replaceProjection({
    required bool reducedMotion,
    bool? wideLayout,
    double? textScaleFactor,
  }) {
    final previous = _projectedGraph == null ? null : _projection.controller;
    final nextWideLayout = wideLayout ?? _wideLayout;
    final nextTextScaleFactor = textScaleFactor ?? _textScaleFactor;
    _projection = OrganizationFlowAdapter.project(
      widget.graph,
      reducedMotion: reducedMotion,
      wideLayout: nextWideLayout,
      textScaleFactor: nextTextScaleFactor,
      onGroupResized: _onGroupResized,
    );
    _reducedMotion = reducedMotion;
    _wideLayout = nextWideLayout;
    _textScaleFactor = nextTextScaleFactor;
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
    _editorFocusNode.dispose();
    _contextMenuController.close();
    _projection.controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 900;
        final gutter =
            OfficeLayoutMetricsScope.maybeOf(context)?.gutter ??
            FrankUiTokens.inset;
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
            focusNode: _editorFocusNode,
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
                        events: _events(),
                      ),
                ),
                Positioned(
                  left: 0,
                  top: 0,
                  width: 1,
                  height: 1,
                  child: FrankDesktopMenu(
                    controller: _contextMenuController,
                    width: 248,
                    groups: _contextMenuGroups,
                    semanticsLabel: 'Organization context menu',
                    child: const SizedBox(width: 1, height: 1),
                  ),
                ),
                Positioned(
                  left: gutter,
                  top: 20,
                  right: gutter,
                  child: _OrganizationToolbar(
                    state: widget.state,
                    controller: _projection.controller,
                    onAdd: _showAddPalette,
                    onUndo: _undo,
                    onRedo: _redo,
                    onValidate: () => context.read<OrganizationBloc>().add(
                      const OrganizationValidateRequested(),
                    ),
                    onZoomBy: _zoomBy,
                    onZoomTo: _zoomTo,
                    onFit: _fitView,
                    onPublish: () => context.read<OrganizationBloc>().add(
                      const OrganizationPublishRequested(),
                    ),
                    onRetry: () => context.read<OrganizationBloc>().add(
                      const OrganizationRetryRequested(),
                    ),
                  ),
                ),
                if (widget.state.validation.issues.isNotEmpty)
                  Positioned(
                    left: gutter,
                    bottom: gutter,
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
  _events() => NodeFlowEvents(
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
          _showGroupMenu(node.data.group!.id, position.offset);
          return;
        }
        if (node.data.node == null) return;
        _showNodeMenu(node.id, position.offset);
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
                    'Use legacy staff routes or v2 taskboard/role routes (pickup, drop, rework).',
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
        _showConnectionMenu(connection.id, position.offset);
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
        : widget.lookupIndex.employeeById[node.employeeId];
    final profile = node.employeeId == null
        ? null
        : widget.lookupIndex.profileByEmployeeId[node.employeeId];
    return Semantics(
      container: true,
      button: true,
      selected: flowNode.isSelected,
      label: switch (node.kind) {
        OrganizationNodeKind.staff =>
          '${node.label}, ${employee?.role ?? 'staff'}, ${employee?.status ?? 'available'}',
        OrganizationNodeKind.capability =>
          '${node.label} capability, ${node.connectorProfileLabel ?? 'profile not selected'}, ${node.configured ? 'configured' : 'setup required'}',
        OrganizationNodeKind.approval => 'Approval Desk, human checkpoint',
        OrganizationNodeKind.role =>
          '${node.label} role, executable worker template',
        OrganizationNodeKind.taskboard =>
          '${node.label} taskboard, shared hand-off surface',
        OrganizationNodeKind.childWorkflow =>
          '${node.label} child workflow, composed subflow',
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
            profile: profile,
          ),
          OrganizationNodeKind.capability => _CapabilityCard(node: node),
          OrganizationNodeKind.approval => _ApprovalCard(node: node),
          OrganizationNodeKind.role ||
          OrganizationNodeKind.taskboard ||
          OrganizationNodeKind.childWorkflow => _WorkflowCard(node: node),
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
      OrganizationRelationKind.pickup => organizationTaskboardColor,
      OrganizationRelationKind.drop => organizationTaskboardColor,
      OrganizationRelationKind.rework => organizationChildWorkflowColor,
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
    final focusBefore = FocusManager.instance.primaryFocus;
    final reducedMotion =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    final choice = await showGeneralDialog<_AddChoice>(
      context: context,
      barrierDismissible: true,
      barrierLabel: 'Dismiss add to office',
      barrierColor: Colors.black.withValues(alpha: .48),
      transitionDuration: reducedMotion
          ? Duration.zero
          : const Duration(milliseconds: 120),
      transitionBuilder: reducedMotion
          ? (_, _, _, child) => child
          : (_, animation, _, child) {
              final curve = CurvedAnimation(
                parent: animation,
                curve: Curves.easeOutCubic,
                reverseCurve: Curves.easeInCubic,
              );
              return FadeTransition(
                opacity: curve,
                child: ScaleTransition(
                  scale: Tween<double>(begin: .98, end: 1).animate(curve),
                  child: child,
                ),
              );
            },
      pageBuilder: (_, _, _) => _AddPalette(
        employees: widget.workspace.employees,
        usedEmployeeIds: widget.graph.nodes
            .map((node) => node.employeeId)
            .whereType<String>()
            .toSet(),
        roles: widget.roles,
        taskboards: widget.workflowProjection.boards,
      ),
    );
    if (mounted && focusBefore != null && focusBefore.canRequestFocus) {
      final nodeToRestore = focusBefore;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && nodeToRestore.canRequestFocus) {
          nodeToRestore.requestFocus();
        }
      });
    }
    if (choice == null || !mounted) return;
    final center = _projection.controller.getViewportCenter().offset;
    final sequence =
        '${DateTime.now().microsecondsSinceEpoch}-${++_newNodeSequence}';
    if (choice case _GroupChoice(:final label)) {
      context.read<OrganizationBloc>().add(
        OrganizationGroupAdded(
          OrganizationGroup(
            id: 'custom-group-$sequence',
            label: label.trim(),
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
        connectorProfileLabel: 'Profile not selected',
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
      _RoleChoice(:final role) => OrganizationNode(
        id: 'role-${role.id}-$sequence',
        kind: OrganizationNodeKind.role,
        label: role.name,
        roleId: role.id,
        position: OrganizationPoint(center.dx - 110, center.dy - 63),
        groupId: OrganizationGroup.delivery.id,
        configured: true,
      ),
      _TaskboardChoice(:final board) => OrganizationNode(
        id: 'taskboard-${board.id}-$sequence',
        kind: OrganizationNodeKind.taskboard,
        label: board.name,
        taskboardId: board.id,
        position: OrganizationPoint(center.dx - 105, center.dy - 59),
        groupId: OrganizationGroup.operationsReview.id,
        configured: true,
      ),
      _ChildWorkflowChoice(:final workflowId, :final label) => OrganizationNode(
        id: 'workflow-$workflowId-$sequence',
        kind: OrganizationNodeKind.childWorkflow,
        label: label,
        childWorkflowId: workflowId,
        position: OrganizationPoint(center.dx - 115, center.dy - 63),
        groupId: OrganizationGroup.delivery.id,
        configured: true,
      ),
      _GroupChoice(:final label) => throw StateError(
        'Group choices are handled above: $label.',
      ),
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
      switch (employee.id) {
        'ae-maya' => OrganizationGroup.clientServices,
        'accountant-dimas' => OrganizationGroup.operationsReview,
        _ => OrganizationGroup.delivery,
      };

  OrganizationGroup _capabilityGroup(OrganizationCapabilityKind capability) =>
      switch (capability) {
        OrganizationCapabilityKind.email ||
        OrganizationCapabilityKind.calendar => OrganizationGroup.clientServices,
        OrganizationCapabilityKind.taskboard =>
          OrganizationGroup.operationsReview,
        _ => OrganizationGroup.delivery,
      };

  void _showNodeMenu(String nodeId, Offset position) {
    final node = widget.lookupIndex.nodeById[nodeId];
    if (node == null) return;
    final selectedNodeIds = {...widget.state.selectedNodeIds, nodeId};
    void addAlignment(OrganizationAlignment alignment) => context
        .read<OrganizationBloc>()
        .add(OrganizationNodesAligned(selectedNodeIds.toList(), alignment));
    void addDistribution(OrganizationDistributionAxis axis) => context
        .read<OrganizationBloc>()
        .add(OrganizationNodesDistributed(selectedNodeIds.toList(), axis));
    final groups = <FrankMenuGroup>[
      FrankMenuGroup([
        if (node.kind != OrganizationNodeKind.staff)
          FrankMenuItem(
            label: 'Duplicate',
            onPressed: () => context.read<OrganizationBloc>().add(
              OrganizationNodeDuplicated(nodeId),
            ),
          ),
      ]),
      if (selectedNodeIds.length >= 2)
        FrankMenuGroup([
          FrankMenuItem(
            label: 'Align left',
            onPressed: () => addAlignment(OrganizationAlignment.left),
          ),
          FrankMenuItem(
            label: 'Align right',
            onPressed: () => addAlignment(OrganizationAlignment.right),
          ),
          FrankMenuItem(
            label: 'Align center',
            onPressed: () => addAlignment(OrganizationAlignment.centerX),
          ),
          FrankMenuItem(
            label: 'Align top',
            onPressed: () => addAlignment(OrganizationAlignment.top),
          ),
          FrankMenuItem(
            label: 'Align bottom',
            onPressed: () => addAlignment(OrganizationAlignment.bottom),
          ),
          FrankMenuItem(
            label: 'Align middle',
            onPressed: () => addAlignment(OrganizationAlignment.centerY),
          ),
        ]),
      if (selectedNodeIds.length >= 3)
        FrankMenuGroup([
          FrankMenuItem(
            label: 'Distribute horizontally',
            onPressed: () =>
                addDistribution(OrganizationDistributionAxis.horizontal),
          ),
          FrankMenuItem(
            label: 'Distribute vertically',
            onPressed: () =>
                addDistribution(OrganizationDistributionAxis.vertical),
          ),
        ]),
      FrankMenuGroup([
        FrankMenuItem(
          label: 'Delete',
          destructive: true,
          onPressed: () => context.read<OrganizationBloc>().add(
            OrganizationElementsDeleted(nodeIds: [nodeId]),
          ),
        ),
      ]),
    ];
    _openContextMenu(groups, position);
  }

  void _openContextMenu(List<FrankMenuGroup> groups, Offset position) {
    setState(() => _contextMenuGroups = groups);
    // The menu surface reads its groups from the rebuilt FrankDesktopMenu.
    // Wait for that rebuild before inserting the overlay so a pointer-opened
    // menu cannot capture the previous node/group/relation action list.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _contextMenuController.openAt(position);
    });
  }

  void _showGroupMenu(String groupId, Offset position) {
    final group = widget.graph.groupById(groupId);
    if (group == null || group.isBuiltIn) return;
    Future<void> rename() async {
      final label = await _showGroupNameDialog(initialValue: group.label);
      if (!mounted || label == null) return;
      context.read<OrganizationBloc>().add(
        OrganizationGroupUpdated(group.copyWith(label: label)),
      );
    }

    _openContextMenu([
      FrankMenuGroup([
        FrankMenuItem(label: 'Rename', onPressed: () => unawaited(rename())),
        FrankMenuItem(
          label: 'Delete group',
          destructive: true,
          onPressed: () => context.read<OrganizationBloc>().add(
            OrganizationGroupsDeleted([group.id]),
          ),
        ),
      ]),
    ], position);
  }

  void _showConnectionMenu(String relationId, Offset position) {
    _openContextMenu([
      FrankMenuGroup([
        FrankMenuItem(
          label: 'Delete relation',
          destructive: true,
          onPressed: () => context.read<OrganizationBloc>().add(
            OrganizationElementsDeleted(relationIds: [relationId]),
          ),
        ),
      ]),
    ], position);
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
    _persistViewport();
  }

  void _zoomBy(double delta) {
    _projection.controller.zoomBy(delta);
    _persistViewport();
  }

  void _zoomTo(double zoom) {
    _projection.controller.zoomTo(zoom);
    _persistViewport();
  }

  void _persistViewport() {
    final viewport = _projection.controller.viewport;
    // Programmatic viewport changes do not pass through the editor's
    // interaction-end callback, so persist them explicitly. This keeps the
    // last view when the user visits another Settings section and returns.
    context.read<OrganizationBloc>().add(
      OrganizationViewportChanged(
        OrganizationViewport(x: viewport.x, y: viewport.y, zoom: viewport.zoom),
      ),
    );
  }

  void _clearSelection() {
    _projection.controller.clearSelection();
    context.read<OrganizationBloc>().add(const OrganizationSelectionChanged());
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _editorFocusNode.canRequestFocus) {
        _editorFocusNode.requestFocus();
      }
    });
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

FrankStatusTone _organizationStatusTone(String? status) =>
    switch (status?.toLowerCase()) {
      'available' => FrankStatusTone.success,
      'working' => FrankStatusTone.working,
      'reviewing' => FrankStatusTone.attention,
      'blocked' || 'failed' => FrankStatusTone.failure,
      _ => FrankStatusTone.neutral,
    };

IconData _organizationStatusIcon(String? status) =>
    switch (status?.toLowerCase()) {
      'available' => Icons.check_circle_outline,
      'working' => Icons.bolt_outlined,
      'reviewing' => Icons.rate_review_outlined,
      'blocked' || 'failed' => Icons.error_outline,
      _ => Icons.circle_outlined,
    };
