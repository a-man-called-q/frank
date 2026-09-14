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
    this.viewMode = OrganizationViewMode.canvas,
    this.onViewModeChanged,
    this.canMutate = true,
    this.mutationDisabledReason,
    super.key,
  });

  final OrganizationGraph graph;
  final OrganizationState state;
  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;
  final List<TeamRoleSummary> roles;
  final WorkflowProjection workflowProjection;
  final OrganizationLookupIndex lookupIndex;
  final OrganizationViewMode viewMode;
  final ValueChanged<OrganizationViewMode>? onViewModeChanged;
  final bool canMutate;
  final String? mutationDisabledReason;

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
  List<FItemGroupMixin> _contextMenuItems = const [];
  String? _connectionFeedback;

  void _announce(String message) {
    if (!mounted) return;
    setState(() => _connectionFeedback = message);
    _announceLive(message);
  }

  void _announceLive(String message) {
    if (!mounted) return;
    unawaited(
      SemanticsService.sendAnnouncement(
        View.of(context),
        message,
        Directionality.of(context),
      ),
    );
  }

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
    if (!widget.canMutate) return;
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
    _projection.controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (widget.viewMode == OrganizationViewMode.outline) {
      return _buildOutline(context);
    }
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
                FContextMenu(
                  key: const ValueKey('organization-context-menu'),
                  groupId: const ValueKey('organization-context-menu-group'),
                  semanticsLabel: 'Organization context menu',
                  menuBuilder: (_, _, _) => _contextMenuItems,
                  child: Semantics(
                    container: true,
                    label:
                        'Organization canvas. ${widget.graph.nodes.length} nodes and ${widget.graph.relations.length} relations.',
                    hint:
                        'Use Outline view for keyboard editing and connection feedback.',
                    child: ExcludeSemantics(
                      // The live shell owns the stable canvas container and
                      // exposes keyboard editing through Outline. Standalone
                      // embeds (including the fixture harness) retain the
                      // flow library's port semantics for pointer tests and
                      // screen-reader discovery.
                      excluding: widget.onViewModeChanged != null,
                      child: RepaintBoundary(
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
                    ),
                  ),
                ),
                Positioned(
                  left: gutter,
                  top: 20,
                  right: gutter,
                  child: _OrganizationToolbar(
                    state: widget.state,
                    controller: _projection.controller,
                    viewMode: widget.viewMode,
                    onViewModeChanged: widget.onViewModeChanged ?? (_) {},
                    canMutate: widget.canMutate,
                    mutationDisabledReason: widget.mutationDisabledReason,
                    onAdd: widget.canMutate ? _showAddPalette : null,
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
                if (_connectionFeedback case final feedback?)
                  Positioned(
                    left: gutter,
                    top: 68,
                    right: gutter,
                    child: FrankActionFeedback(
                      message: feedback,
                      tone: FrankStatusTone.failure,
                      action: FButton(
                        onPress: () =>
                            setState(() => _connectionFeedback = null),
                        variant: FButtonVariant.ghost,
                        size: FButtonSizeVariant.sm,
                        child: const Text('Dismiss'),
                      ),
                    ),
                  ),
                if ((widget.state.persistenceStatus ==
                            OrganizationPersistenceStatus.saveFailure ||
                        widget.state.persistenceStatus ==
                            OrganizationPersistenceStatus.publishFailure) &&
                    widget.state.error != null)
                  Positioned(
                    left: gutter,
                    top: 124,
                    right: gutter,
                    child: FrankActionFeedback(
                      message: frankFriendlyError(
                        widget.state.error,
                        fallback: 'The organization could not be saved.',
                      ),
                      tone: FrankStatusTone.failure,
                      action: FButton(
                        onPress: () => context.read<OrganizationBloc>().add(
                          const OrganizationRetryRequested(),
                        ),
                        variant: FButtonVariant.ghost,
                        size: FButtonSizeVariant.sm,
                        child: const Text('Retry'),
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
                  Center(
                    child: _EmptyOrganization(
                      onAdd: widget.canMutate ? _showAddPalette : null,
                    ),
                  ),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _buildOutline(BuildContext context) {
    final gutter =
        OfficeLayoutMetricsScope.maybeOf(context)?.gutter ??
        FrankUiTokens.inset;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(gutter, 20, gutter, 12),
          child: _OrganizationToolbar(
            state: widget.state,
            controller: _projection.controller,
            viewMode: widget.viewMode,
            onViewModeChanged: widget.onViewModeChanged ?? (_) {},
            canMutate: widget.canMutate,
            mutationDisabledReason: widget.mutationDisabledReason,
            onAdd: widget.canMutate ? _showAddPalette : null,
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
        if (widget.state.persistenceStatus ==
                OrganizationPersistenceStatus.saveFailure ||
            widget.state.persistenceStatus ==
                OrganizationPersistenceStatus.publishFailure)
          Padding(
            padding: EdgeInsets.fromLTRB(gutter, 0, gutter, 12),
            child: FrankActionFeedback(
              message: frankFriendlyError(
                widget.state.error,
                fallback: 'The organization could not be saved.',
              ),
              tone: FrankStatusTone.failure,
              action: FButton(
                onPress: () => context.read<OrganizationBloc>().add(
                  const OrganizationRetryRequested(),
                ),
                variant: FButtonVariant.ghost,
                size: FButtonSizeVariant.sm,
                child: const Text('Retry'),
              ),
            ),
          ),
        Expanded(
          child: Padding(
            padding: EdgeInsets.symmetric(horizontal: gutter),
            child: _OrganizationOutline(
              graph: widget.graph,
              state: widget.state,
              canMutate: widget.canMutate,
              mutationDisabledReason: widget.mutationDisabledReason,
            ),
          ),
        ),
      ],
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
        if (!widget.canMutate) return;
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
        if (!widget.canMutate) return;
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
        if (!widget.canMutate) {
          _announce(
            'Connection not created: ${widget.mutationDisabledReason ?? 'changes are paused until the server reconnects'}.',
          );
          return const ConnectionValidationResult.deny(
            reason: 'Reconnect before changing the organization.',
          );
        }
        final source = connection.sourceNode.data.node;
        final target = connection.targetNode.data.node;
        if (source == null || target == null) {
          _announce('Connection not created: groups cannot be connected.');
          return const ConnectionValidationResult.deny(
            reason: 'Groups cannot be connected.',
          );
        }
        final kind = inferOrganizationRelationKind(
          source: source,
          target: target,
        );
        if (kind == null) {
          _announce(
            'Connection not created: connect roles and internal taskboards using pickup, drop, or rework routes.',
          );
          return const ConnectionValidationResult.deny(
            reason:
                'Connect roles and internal taskboards using pickup, drop, or rework routes.',
          );
        }
        if (kind == OrganizationRelationKind.toolAccess &&
            (target.capability?.permissions.isEmpty ?? true)) {
          _announce(
            'Connection not created: this capability has no permission.',
          );
          return const ConnectionValidationResult.deny(
            reason: 'This capability has no permission.',
          );
        }
        return const ConnectionValidationResult.allow();
      },
      onCreated: (connection) {
        if (!widget.canMutate) return;
        // A successful retry replaces any earlier validation banner. Keep the
        // live announcement below, but do not leave stale failure UI pinned
        // over the canvas after the graph has accepted the relation.
        if (mounted) setState(() => _connectionFeedback = null);
        final source = _projection.nodesById[connection.sourceNodeId];
        final target = _projection.nodesById[connection.targetNodeId];
        if (source == null || target == null) {
          _announce(
            'Connection not created: the selected node is unavailable.',
          );
          return;
        }
        final kind = inferOrganizationRelationKind(
          source: source,
          target: target,
        );
        if (kind == null) {
          _announce('Connection not created: this relation is not valid.');
          return;
        }
        final targetPermissions = target.capability?.permissions ?? const [];
        final permissions =
            kind == OrganizationRelationKind.toolAccess &&
                targetPermissions.isNotEmpty
            ? [targetPermissions.first]
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
        _announceLive(
          'Connection created from ${source.label} to ${target.label} as ${kind.name}.',
        );
      },
      onConnectEnd: (targetNode, targetPort, _) {
        if (!widget.canMutate || targetNode != null || targetPort != null) {
          return;
        }
        _announce(
          'Connection not created: drop on a compatible target port or use Outline view for keyboard editing.',
        );
      },
      onDeleted: (connection) {
        if (!widget.canMutate) return;
        context.read<OrganizationBloc>().add(
          OrganizationElementsDeleted(relationIds: [connection.id]),
        );
      },
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
      onCanvasDoubleTap: (_) {
        if (widget.canMutate) _showAddPalette();
      },
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
        OrganizationNodeKind.approval => 'Retired control node',
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
    final choice = await showFDialog<_AddChoice>(
      context: context,
      barrierDismissible: true,
      barrierLabel: 'Dismiss add to office',
      builder: (_, _, _) => _AddPalette(
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
      _announceLive('Added group $label.');
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && _editorFocusNode.canRequestFocus) {
          _editorFocusNode.requestFocus();
        }
      });
      return;
    }
    final node = switch (choice) {
      _StaffChoice(:final employee) => OrganizationNode(
        id: 'staff-${employee.id}-$sequence',
        kind: OrganizationNodeKind.staff,
        label: employee.name,
        employeeId: employee.id,
        position: OrganizationPoint(center.dx - 110, center.dy - 63),
        // New nodes are intentionally ungrouped. Placement is an explicit
        // user action and must not depend on fixture ids or names.
        groupId: null,
        configured: true,
      ),
      _CapabilityChoice(:final capability) => OrganizationNode(
        id: '${capability.name}-$sequence',
        kind: OrganizationNodeKind.capability,
        label: capability.label,
        capability: capability,
        position: OrganizationPoint(center.dx - 95, center.dy - 52),
        groupId: null,
        connectorProfileLabel: 'Profile not selected',
        approvalRequired: capability.isSensitive,
      ),
      _RoleChoice(:final role) => OrganizationNode(
        id: 'role-${role.id}-$sequence',
        kind: OrganizationNodeKind.role,
        label: role.name,
        roleId: role.id,
        position: OrganizationPoint(center.dx - 110, center.dy - 63),
        groupId: null,
        configured: true,
      ),
      _TaskboardChoice(:final board) => OrganizationNode(
        id: 'taskboard-${board.id}-$sequence',
        kind: OrganizationNodeKind.taskboard,
        label: board.name,
        taskboardId: board.id,
        position: OrganizationPoint(center.dx - 105, center.dy - 59),
        groupId: null,
        configured: true,
      ),
      _ChildWorkflowChoice(:final workflowId, :final label) => OrganizationNode(
        id: 'workflow-$workflowId-$sequence',
        kind: OrganizationNodeKind.childWorkflow,
        label: label,
        childWorkflowId: workflowId,
        position: OrganizationPoint(center.dx - 115, center.dy - 63),
        groupId: null,
        configured: true,
      ),
      _GroupChoice(:final label) => throw StateError(
        'Group choices are handled above: $label.',
      ),
    };
    context.read<OrganizationBloc>().add(OrganizationNodeAdded(node));
    _announceLive('Added ${node.label} to the organization.');
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _editorFocusNode.canRequestFocus) {
        _editorFocusNode.requestFocus();
      }
    });
  }

  double _snapToGrid(double value) => (value / 20).round() * 20.0;

  Future<String?> _showGroupNameDialog({String? initialValue}) =>
      showFDialog<String>(
        context: context,
        builder: (_, _, _) => _GroupNameDialog(initialValue: initialValue),
      );

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
    final groups = <FItemGroupMixin>[
      FItemGroup(
        children: [
          if (node.kind != OrganizationNodeKind.staff)
            FItem(
              title: const Text('Duplicate'),
              prefix: const Icon(FrankIcons.copy),
              onPress: () => context.read<OrganizationBloc>().add(
                OrganizationNodeDuplicated(nodeId),
              ),
            ),
        ],
      ),
      if (selectedNodeIds.length >= 2)
        FItemGroup(
          children: [
            FItem(
              title: const Text('Align left'),
              onPress: () => addAlignment(OrganizationAlignment.left),
            ),
            FItem(
              title: const Text('Align right'),
              onPress: () => addAlignment(OrganizationAlignment.right),
            ),
            FItem(
              title: const Text('Align center'),
              onPress: () => addAlignment(OrganizationAlignment.centerX),
            ),
            FItem(
              title: const Text('Align top'),
              onPress: () => addAlignment(OrganizationAlignment.top),
            ),
            FItem(
              title: const Text('Align bottom'),
              onPress: () => addAlignment(OrganizationAlignment.bottom),
            ),
            FItem(
              title: const Text('Align middle'),
              onPress: () => addAlignment(OrganizationAlignment.centerY),
            ),
          ],
        ),
      if (selectedNodeIds.length >= 3)
        FItemGroup(
          children: [
            FItem(
              title: const Text('Distribute horizontally'),
              onPress: () =>
                  addDistribution(OrganizationDistributionAxis.horizontal),
            ),
            FItem(
              title: const Text('Distribute vertically'),
              onPress: () =>
                  addDistribution(OrganizationDistributionAxis.vertical),
            ),
          ],
        ),
      FItemGroup(
        children: [
          FItem(
            title: const Text('Delete'),
            variant: FItemVariant.destructive,
            prefix: const Icon(FrankIcons.archive),
            onPress: () => context.read<OrganizationBloc>().add(
              OrganizationElementsDeleted(nodeIds: [nodeId]),
            ),
          ),
        ],
      ),
    ];
    _openContextMenu(groups, position);
  }

  void _openContextMenu(List<FItemGroupMixin> groups, Offset position) {
    setState(() => _contextMenuItems = groups);
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
      FItemGroup(
        children: [
          FItem(
            title: const Text('Rename'),
            onPress: () => unawaited(rename()),
          ),
          FItem(
            title: const Text('Delete group'),
            variant: FItemVariant.destructive,
            prefix: const Icon(FrankIcons.archive),
            onPress: () => context.read<OrganizationBloc>().add(
              OrganizationGroupsDeleted([group.id]),
            ),
          ),
        ],
      ),
    ], position);
  }

  void _showConnectionMenu(String relationId, Offset position) {
    _openContextMenu([
      FItemGroup(
        children: [
          FItem(
            title: const Text('Delete relation'),
            variant: FItemVariant.destructive,
            prefix: const Icon(FrankIcons.archive),
            onPress: () => context.read<OrganizationBloc>().add(
              OrganizationElementsDeleted(relationIds: [relationId]),
            ),
          ),
        ],
      ),
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
    if (!widget.canMutate) return;
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
    if (!widget.canMutate) return;
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
    if (!widget.canMutate) return;
    final nodeId = widget.state.selectedNodeId;
    if (nodeId == null) return;
    context.read<OrganizationBloc>().add(OrganizationNodeDuplicated(nodeId));
  }
}

/// Native, keyboard-first projection of the same organization graph used by
/// the canvas. It deliberately has no second graph or persistence path.
class _OrganizationOutline extends StatefulWidget {
  const _OrganizationOutline({
    required this.graph,
    required this.state,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final OrganizationGraph graph;
  final OrganizationState state;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  State<_OrganizationOutline> createState() => _OrganizationOutlineState();
}

class _OrganizationOutlineState extends State<_OrganizationOutline> {
  String? _connectionSourceId;

  void _announce(String message) {
    if (!mounted) return;
    unawaited(
      SemanticsService.sendAnnouncement(
        View.of(context),
        message,
        Directionality.of(context),
      ),
    );
  }

  OrganizationNode? _node(String id) =>
      widget.graph.nodes.where((node) => node.id == id).firstOrNull;

  void _selectNode(String id) {
    context.read<OrganizationBloc>().add(
      OrganizationSelectionChanged(
        nodeId: id,
        nodeIds: [id],
        relationId: null,
        relationIds: const [],
        groupId: null,
        groupIds: const [],
      ),
    );
  }

  void _connect(OrganizationNode source, OrganizationNode target) {
    if (!widget.canMutate) return;
    final kind = inferOrganizationRelationKind(source: source, target: target);
    if (kind == null) return;
    final targetPermissions =
        target.capability?.permissions ?? const <String>[];
    if (kind == OrganizationRelationKind.toolAccess &&
        targetPermissions.isEmpty) {
      _announce('Connection not created: this capability has no permission.');
      return;
    }
    final relationId = 'outline-${source.id}-${target.id}';
    if (widget.graph.relations.any((relation) => relation.id == relationId)) {
      return;
    }
    final permissions = kind == OrganizationRelationKind.toolAccess
        ? [targetPermissions.first]
        : const <String>[];
    context.read<OrganizationBloc>().add(
      OrganizationRelationAdded(
        OrganizationRelation(
          id: relationId,
          kind: kind,
          sourceNodeId: source.id,
          targetNodeId: target.id,
          permissions: permissions,
        ),
      ),
    );
    setState(() => _connectionSourceId = null);
    unawaited(
      SemanticsService.sendAnnouncement(
        View.of(context),
        'Connection created from ${source.label} to ${target.label}.',
        Directionality.of(context),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final grouped = <String, List<OrganizationNode>>{};
    for (final node in widget.graph.nodes) {
      (grouped[node.groupId ?? 'ungrouped'] ??= []).add(node);
    }
    final groupLabels = {
      for (final group in widget.graph.groups) group.id: group.label,
    };
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Organization outline',
      hint: 'Select, edit, delete, or start a connection from a node.',
      child: ListView(
        padding: const EdgeInsets.only(bottom: 24),
        children: [
          if (_connectionSourceId case final sourceId?)
            FrankActionFeedback(
              message:
                  'Connection source: ${_node(sourceId)?.label ?? sourceId}. Choose a valid target or cancel.',
              tone: FrankStatusTone.working,
              action: FButton(
                onPress: () => setState(() => _connectionSourceId = null),
                variant: FButtonVariant.ghost,
                size: FButtonSizeVariant.sm,
                child: const Text('Cancel connection'),
              ),
            ),
          for (final entry in grouped.entries) ...[
            const SizedBox(height: 12),
            Text(
              groupLabels[entry.key] ?? 'UNGROUPED',
              style: const TextStyle(
                color: FrankColors.muted,
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: 1,
              ),
            ),
            const SizedBox(height: 6),
            for (final node in entry.value)
              _outlineNode(node, source: _connectionSourceId),
          ],
          const SizedBox(height: 18),
          const Text(
            'RELATIONS',
            style: TextStyle(
              color: FrankColors.muted,
              fontSize: 11,
              fontWeight: FontWeight.w700,
              letterSpacing: 1,
            ),
          ),
          const SizedBox(height: 6),
          for (final relation in widget.graph.relations)
            _outlineRelation(relation),
        ],
      ),
    );
  }

  Widget _outlineNode(OrganizationNode node, {String? source}) {
    final sourceNode = source == null ? null : _node(source);
    final kind = sourceNode == null
        ? null
        : inferOrganizationRelationKind(source: sourceNode, target: node);
    final isSource = source == node.id;
    final canConnect =
        sourceNode != null &&
        !isSource &&
        kind != null &&
        (kind != OrganizationRelationKind.toolAccess ||
            node.capability?.permissions.isNotEmpty == true);
    final reason = isSource
        ? 'This is the active connection source.'
        : sourceNode == null
        ? null
        : canConnect
        ? 'Connect ${sourceNode.label} to ${node.label}'
        : 'This node is not a valid target for the selected source.';
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Semantics(
        container: true,
        label: '${node.label}, ${node.kind.name}',
        hint: reason,
        child: FrankPanel(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
          child: Wrap(
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 6,
            runSpacing: 6,
            children: [
              FButton(
                onPress: () => _selectNode(node.id),
                variant: widget.state.selectedNodeId == node.id
                    ? FButtonVariant.secondary
                    : FButtonVariant.ghost,
                child: Text(node.label),
              ),
              FButton(
                onPress: () => _selectNode(node.id),
                variant: FButtonVariant.outline,
                size: FButtonSizeVariant.sm,
                child: const Text('Edit'),
              ),
              FButton(
                onPress: widget.canMutate
                    ? () => context.read<OrganizationBloc>().add(
                        OrganizationElementsDeleted(nodeIds: [node.id]),
                      )
                    : null,
                semanticsTooltip: widget.canMutate
                    ? 'Delete ${node.label}'
                    : widget.mutationDisabledReason ??
                          'Reconnect before deleting an organization element',
                variant: FButtonVariant.destructive,
                size: FButtonSizeVariant.sm,
                child: const Text('Delete'),
              ),
              FButton(
                onPress: !widget.canMutate
                    ? null
                    : sourceNode == null
                    ? () => setState(() => _connectionSourceId = node.id)
                    : canConnect
                    ? () => _connect(sourceNode, node)
                    : null,
                variant: canConnect
                    ? FButtonVariant.primary
                    : FButtonVariant.outline,
                size: FButtonSizeVariant.sm,
                child: Text(
                  !widget.canMutate
                      ? 'Changes paused'
                      : sourceNode == null
                      ? 'Start connection'
                      : canConnect
                      ? 'Connect'
                      : isSource
                      ? 'Source selected'
                      : 'Connect unavailable',
                ),
                semanticsTooltip: !widget.canMutate
                    ? widget.mutationDisabledReason ??
                          'Reconnect before changing the organization'
                    : reason,
              ),
              if (reason != null && sourceNode != null)
                Text(
                  reason,
                  style: TextStyle(
                    color: canConnect
                        ? FrankColors.muted
                        : FrankColors.warningAmber,
                    fontSize: FrankUiTokens.metadataTextSize,
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _outlineRelation(OrganizationRelation relation) {
    final source = _node(relation.sourceNodeId)?.label ?? relation.sourceNodeId;
    final target = _node(relation.targetNodeId)?.label ?? relation.targetNodeId;
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: FrankPanel(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
        child: Row(
          children: [
            Expanded(child: Text('$source → $target · ${relation.kind.name}')),
            FButton(
              onPress: () => context.read<OrganizationBloc>().add(
                OrganizationSelectionChanged(
                  relationId: relation.id,
                  relationIds: [relation.id],
                  nodeId: null,
                  nodeIds: const [],
                ),
              ),
              variant: FButtonVariant.outline,
              size: FButtonSizeVariant.sm,
              child: const Text('Inspect'),
            ),
            const SizedBox(width: 4),
            FButton(
              onPress: widget.canMutate
                  ? () => context.read<OrganizationBloc>().add(
                      OrganizationElementsDeleted(relationIds: [relation.id]),
                    )
                  : null,
              semanticsTooltip: widget.canMutate
                  ? 'Delete relation'
                  : widget.mutationDisabledReason ??
                        'Reconnect before deleting a relation',
              variant: FButtonVariant.destructive,
              size: FButtonSizeVariant.sm,
              child: const Text('Delete'),
            ),
          ],
        ),
      ),
    );
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
      'available' => FrankIcons.checkCircleOutline,
      'working' => FrankIcons.boltOutlined,
      'reviewing' => FrankIcons.rateReviewOutlined,
      'blocked' || 'failed' => FrankIcons.errorOutline,
      _ => FrankIcons.circleOutlined,
    };
