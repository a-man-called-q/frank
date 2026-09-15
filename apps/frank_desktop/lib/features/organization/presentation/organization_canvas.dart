part of 'organization_surface.dart';

class _OrganizationSurfaceState extends State<OrganizationSurface> {
  final GlobalKey<_OrganizationEditorState> _editorKey =
      GlobalKey<_OrganizationEditorState>();
  final FocusNode _surfaceFocusNode = FocusNode(
    debugLabel: 'organization-surface',
  );
  OrganizationLookupIndex? _lookup;
  OrganizationGraph? _lookupGraph;
  List<TeamAgentProfile>? _lookupProfiles;
  List<ConnectorProfile>? _lookupConnectors;

  OrganizationLookupIndex _indexFor(
    OrganizationGraph graph,
    OrganizationState state,
    List<TeamAgentProfile>? visibleProfiles,
  ) {
    final profiles = visibleProfiles ??
        widget.profiles?.where((profile) => !profile.archived).toList(growable: false);
    if (_lookup == null ||
        !identical(_lookupGraph, graph) ||
        !listEquals(_lookupProfiles, profiles) ||
        !identical(_lookupConnectors, state.connectorProfiles)) {
      _lookup = OrganizationLookupIndex.build(
        graph: graph,
        employees: widget.workspace.employees,
        profiles: profiles ?? const [],
        connectorProfiles: state.connectorProfiles,
      );
      _lookupGraph = graph;
      _lookupProfiles = profiles;
      _lookupConnectors = state.connectorProfiles;
    }
    return _lookup!;
  }

  @override
  void initState() {
    super.initState();
    final bloc = context.read<OrganizationBloc>();
    if (bloc.state.loadStatus == OrganizationLoadStatus.initial) {
      bloc.add(const OrganizationStarted());
    }
  }

  @override
  void dispose() {
    _surfaceFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return BlocBuilder<OrganizationBloc, OrganizationState>(
      builder: (context, state) {
        final visibleProfiles = widget.profiles
            ?.where((profile) => !profile.archived)
            .toList(growable: false);
        final inspectorVisible =
            state.selectedNodeId != null ||
            state.selectedRelationId != null ||
            state.selectedGroupId != null;
        return LayoutBuilder(
          builder: (context, constraints) {
            final desktopPane =
                constraints.maxWidth >=
                OfficeInspectorDrawerOverlay.desktopBreakpoint;
            final inspector = inspectorVisible && state.graph != null
                ? _OrganizationInspector(
                    workspace: widget.workspace,
                    profiles: visibleProfiles,
                    connectorProfiles: state.connectorProfiles,
                    lookupIndex: _indexFor(
                      state.graph!,
                      state,
                      visibleProfiles,
                    ),
                    docked: desktopPane,
                    onClose: () => _editorKey.currentState?._clearSelection(),
                    onDelete: () => _editorKey.currentState?._deleteSelection(),
                    onDuplicate: () =>
                        _editorKey.currentState?._duplicateSelection(),
                    canMutate: widget.canMutate,
                    mutationDisabledReason: widget.mutationDisabledReason,
                  )
                : null;
            final frame = OfficeSurfaceFrame.canvas(
              backgroundColor: const Color(0x00000000),
              header: OfficePageHeader(
                title: 'Organization',
                description:
                    'Configure agents, connections, and taskboard assignments.',
                actions: widget.isFixture ? const FrankSampleDataBadge() : null,
              ),
              child: Semantics(
                container: true,
                explicitChildNodes: true,
                label: 'Organization flow editor',
                child: switch (state.loadStatus) {
                  OrganizationLoadStatus.initial ||
                  OrganizationLoadStatus.loading =>
                    const _OrganizationLoading(),
                  OrganizationLoadStatus.failure =>
                    frankIsUnsupportedError(state.error)
                        ? const FrankUnavailableState(
                            title: 'Organization unavailable',
                            message:
                                'Organization editor isn’t available on this server build.',
                            icon: FrankIcons.accountTreeOutlined,
                          )
                        : _OrganizationFailure(
                            message: frankFriendlyError(
                              state.error,
                              fallback: 'The organization could not be loaded.',
                            ),
                          ),
                  OrganizationLoadStatus.ready =>
                    state.graph == null
                        ? const _OrganizationLoading()
                        : _OrganizationEditor(
                            key: _editorKey,
                            graph: state.graph!,
                            state: state,
                            workspace: widget.workspace,
                            profiles: visibleProfiles,
                            roles: widget.roles,
                            workflowProjection: widget.workflowProjection,
                            lookupIndex: _indexFor(
                              state.graph!,
                              state,
                              visibleProfiles,
                            ),
                            viewMode:
                                widget.viewMode ?? OrganizationViewMode.canvas,
                            onViewModeChanged: widget.onViewModeChanged,
                            canMutate: widget.canMutate,
                            mutationDisabledReason:
                                widget.mutationDisabledReason,
                          ),
                },
              ),
            );
            return Focus(
              focusNode: _surfaceFocusNode,
              onKeyEvent: (_, event) {
                if (event is KeyDownEvent &&
                    event.logicalKey == LogicalKeyboardKey.escape &&
                    inspectorVisible) {
                  _editorKey.currentState?._clearSelection();
                  return KeyEventResult.handled;
                }
                return KeyEventResult.ignored;
              },
              child: OfficeInspectorDrawerOverlay(
                child: frame,
                inspector: inspector,
                inspectorKey: const ValueKey('organization-inspector-rail'),
                inspectorLabel: 'Organization inspector',
              ),
            );
          },
        );
      },
    );
  }
}
