part of 'organization_surface.dart';

class _OrganizationToolbar extends StatefulWidget {
  const _OrganizationToolbar({
    required this.state,
    required this.controller,
    required this.viewMode,
    required this.onViewModeChanged,
    required this.canMutate,
    this.mutationDisabledReason,
    required this.onAdd,
    required this.onUndo,
    required this.onRedo,
    required this.onValidate,
    required this.onZoomBy,
    required this.onZoomTo,
    required this.onFit,
    required this.onPublish,
    required this.onRetry,
  });

  final OrganizationState state;
  final OrganizationViewMode viewMode;
  final ValueChanged<OrganizationViewMode> onViewModeChanged;
  final bool canMutate;
  final String? mutationDisabledReason;
  final NodeFlowController<
    OrganizationFlowNodeData,
    OrganizationFlowRelationData
  >
  controller;
  final VoidCallback? onAdd;
  final VoidCallback onUndo;
  final VoidCallback onRedo;
  final VoidCallback onValidate;
  final ValueChanged<double> onZoomBy;
  final ValueChanged<double> onZoomTo;
  final VoidCallback onFit;
  final VoidCallback onPublish;
  final VoidCallback onRetry;

  @override
  State<_OrganizationToolbar> createState() => _OrganizationToolbarState();
}

// Use the full single-row rail when this much width is available. Below the
// threshold the controls wrap into a compact two-line rail instead of
// clipping behind an overflow scroll or a full-width Publish bar.
const _organizationToolbarMinimumWidth = 940.0;

class _OrganizationToolbarState extends State<_OrganizationToolbar> {
  late bool _minimapVisible;

  @override
  void initState() {
    super.initState();
    _minimapVisible = widget.controller.minimap?.isVisible ?? false;
  }

  @override
  void didUpdateWidget(covariant _OrganizationToolbar oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.controller, widget.controller)) {
      _minimapVisible = widget.controller.minimap?.isVisible ?? false;
    }
  }

  void _toggleMinimap() {
    final minimap = widget.controller.minimap;
    if (minimap == null) return;
    minimap.toggle();
    setState(() => _minimapVisible = minimap.isVisible);
  }

  @override
  Widget build(BuildContext context) {
    final state = widget.state;
    final failed =
        state.persistenceStatus == OrganizationPersistenceStatus.saveFailure ||
        state.persistenceStatus == OrganizationPersistenceStatus.publishFailure;
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Organization editor toolbar',
      child: LayoutBuilder(
        builder: (context, constraints) {
          final availableWidth = constraints.hasBoundedWidth
              ? constraints.maxWidth
              : _organizationToolbarMinimumWidth;
          final textScaleFactor =
              MediaQuery.maybeOf(context)?.textScaler.scale(1) ?? 1;
          final minimumWidth =
              _organizationToolbarMinimumWidth *
              (textScaleFactor > 1 ? textScaleFactor : 1);
          final railWidth = availableWidth < minimumWidth
              ? minimumWidth
              : availableWidth;
          final compactToolbar = railWidth != availableWidth;
          final publish = FButton(
            key: const ValueKey('organization-publish'),
            onPress: state.canPublish && widget.canMutate
                ? widget.onPublish
                : null,
            semanticsLabel: widget.canMutate
                ? 'Publish organization'
                : widget.mutationDisabledReason ??
                      'Reconnect before publishing organization',
            semanticsTooltip: widget.canMutate
                ? 'Publish organization'
                : widget.mutationDisabledReason ??
                      'Reconnect before publishing organization',
            size: compactToolbar
                ? FButtonSizeVariant.sm
                : FButtonSizeVariant.md,
            prefix: const Icon(
              FrankIcons.publish,
              size: FrankUiTokens.iconSize - 1,
            ),
            child: Text(
              state.persistenceStatus ==
                      OrganizationPersistenceStatus.publishing
                  ? 'Publishing…'
                  : 'Publish',
            ),
          );
          final viewToggle = FrankSegmentedControl<OrganizationViewMode>(
            key: const ValueKey('organization-view-mode'),
            value: widget.viewMode,
            items: const [
              (
                OrganizationViewMode.canvas,
                'Canvas',
                FrankIcons.accountTreeOutlined,
              ),
              (
                OrganizationViewMode.outline,
                'Outline',
                FrankIcons.listAltOutlined,
              ),
            ],
            onChanged: widget.onViewModeChanged,
          );
          final compactControls = <Widget>[
            _ToolbarTextButton(
              buttonKey: const ValueKey('organization-add'),
              icon: FrankIcons.plus,
              label: 'Add',
              onPressed: widget.onAdd,
              semanticsTooltip: widget.canMutate
                  ? 'Add organization element'
                  : widget.mutationDisabledReason ??
                        'Reconnect before adding an organization element',
              selected: true,
            ),
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-undo'),
              icon: FrankIcons.undo,
              label: 'Undo',
              onPressed: state.canUndo ? widget.onUndo : null,
            ),
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-redo'),
              icon: FrankIcons.redo,
              label: 'Redo',
              onPressed: state.canRedo ? widget.onRedo : null,
            ),
            const _ToolbarDivider(),
            _ToolbarTextButton(
              buttonKey: const ValueKey('organization-validate'),
              icon: FrankIcons.circleCheck,
              label: 'Validate',
              onPressed: widget.onValidate,
            ),
            const _ToolbarDivider(),
            viewToggle,
            const _ToolbarDivider(),
            _OrganizationZoomControls(
              controller: widget.controller,
              onZoomBy: widget.onZoomBy,
              onZoomTo: widget.onZoomTo,
            ),
            const _ToolbarDivider(),
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-fit'),
              icon: FrankIcons.recenter,
              label: 'Fit view',
              onPressed: widget.onFit,
            ),
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-minimap'),
              icon: FrankIcons.minimap,
              label: 'Minimap',
              onPressed: widget.controller.minimap == null
                  ? null
                  : _toggleMinimap,
              selected: _minimapVisible,
              toggled: _minimapVisible,
            ),
            _DraftStatus(state: state),
            if (failed) ...[
              const SizedBox(width: 4),
              FButton(
                key: const ValueKey('organization-retry'),
                onPress: widget.onRetry,
                variant: FButtonVariant.ghost,
                size: FButtonSizeVariant.sm,
                child: const Text('Retry'),
              ),
            ],
            const _ToolbarDivider(),
            publish,
          ];
          final rail = ConstrainedBox(
            constraints: const BoxConstraints(
              minHeight: FrankUiTokens.toolbarHeight,
            ),
            child: _OrganizationPanel(
              color: FrankColors.sidebarSolid,
              padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 2),
              child: compactToolbar
                  ? Wrap(
                      alignment: WrapAlignment.start,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      spacing: 2,
                      runSpacing: 2,
                      children: compactControls,
                    )
                  : Row(
                      children: [
                        _ToolbarTextButton(
                          buttonKey: const ValueKey('organization-add'),
                          icon: FrankIcons.plus,
                          label: 'Add',
                          onPressed: widget.onAdd,
                          semanticsTooltip: widget.canMutate
                              ? 'Add organization element'
                              : widget.mutationDisabledReason ??
                                    'Reconnect before adding an organization element',
                          selected: true,
                        ),
                        _ToolbarIconButton(
                          buttonKey: const ValueKey('organization-undo'),
                          icon: FrankIcons.undo,
                          label: 'Undo',
                          onPressed: state.canUndo ? widget.onUndo : null,
                        ),
                        _ToolbarIconButton(
                          buttonKey: const ValueKey('organization-redo'),
                          icon: FrankIcons.redo,
                          label: 'Redo',
                          onPressed: state.canRedo ? widget.onRedo : null,
                        ),
                        const _ToolbarDivider(),
                        _ToolbarTextButton(
                          buttonKey: const ValueKey('organization-validate'),
                          icon: FrankIcons.circleCheck,
                          label: 'Validate',
                          onPressed: widget.onValidate,
                        ),
                        const _ToolbarDivider(),
                        Flexible(fit: FlexFit.loose, child: viewToggle),
                        const _ToolbarDivider(),
                        _OrganizationZoomControls(
                          controller: widget.controller,
                          onZoomBy: widget.onZoomBy,
                          onZoomTo: widget.onZoomTo,
                        ),
                        const _ToolbarDivider(),
                        _ToolbarIconButton(
                          buttonKey: const ValueKey('organization-fit'),
                          icon: FrankIcons.recenter,
                          label: 'Fit view',
                          onPressed: widget.onFit,
                        ),
                        _ToolbarIconButton(
                          buttonKey: const ValueKey('organization-minimap'),
                          icon: FrankIcons.minimap,
                          label: 'Minimap',
                          onPressed: widget.controller.minimap == null
                              ? null
                              : _toggleMinimap,
                          selected: _minimapVisible,
                          toggled: _minimapVisible,
                        ),
                        if (!compactToolbar) const Spacer(),
                        _DraftStatus(state: state),
                        if (failed) ...[
                          const SizedBox(width: 4),
                          FButton(
                            key: const ValueKey('organization-retry'),
                            onPress: widget.onRetry,
                            variant: FButtonVariant.ghost,
                            size: FButtonSizeVariant.sm,
                            child: const Text('Retry'),
                          ),
                        ],
                        if (!compactToolbar) ...[
                          const _ToolbarDivider(),
                          publish,
                        ],
                      ],
                    ),
            ),
          );
          if (!compactToolbar) return rail;
          return SizedBox(width: availableWidth, child: rail);
        },
      ),
    );
  }
}

class _OrganizationZoomControls extends StatelessWidget {
  const _OrganizationZoomControls({
    required this.controller,
    required this.onZoomBy,
    required this.onZoomTo,
  });

  final NodeFlowController<
    OrganizationFlowNodeData,
    OrganizationFlowRelationData
  >
  controller;
  final ValueChanged<double> onZoomBy;
  final ValueChanged<double> onZoomTo;

  static const _presets = <double>[.5, .75, 1, 1.25, 1.5];

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<GraphViewport>(
      valueListenable: controller.cameraViewportListenable,
      builder: (context, viewport, _) {
        final percent = (viewport.zoom * 100).round();
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-zoom-out'),
              icon: FrankIcons.remove,
              label: 'Zoom out',
              onPressed: () => onZoomBy(-.1),
            ),
            Semantics(
              container: true,
              label: 'Zoom $percent%',
              value: '$percent%',
              child: MediaQuery.withClampedTextScaling(
                maxScaleFactor: 1,
                child: SizedBox(
                  width: 70,
                  child: ExcludeSemantics(
                    // ForUI 0.25's select field builds a nested merging node
                    // which conflicts with the flow editor's explicit
                    // semantics tree. The surrounding node retains the
                    // accessible zoom label/value while the select remains
                    // fully focusable and interactive.
                    excluding: true,
                    child: FSelect<double>.rich(
                      key: const ValueKey('organization-zoom'),
                      size: FTextFieldSizeVariant.sm,
                      control: FSelectControl<double>.lifted(
                        value: viewport.zoom,
                        onChange: (value) {
                          if (value != null) onZoomTo(value);
                        },
                      ),
                      format: (value) => '${(value * 100).round()}%',
                      children: [
                        if (!_presets.contains(viewport.zoom))
                          FSelectItem<double>.item(
                            value: viewport.zoom,
                            title: Text('$percent%'),
                          ),
                        for (final preset in _presets)
                          FSelectItem<double>.item(
                            value: preset,
                            title: Text('${(preset * 100).round()}%'),
                          ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-zoom-in'),
              icon: FrankIcons.add,
              label: 'Zoom in',
              onPressed: () => onZoomBy(.1),
            ),
            Semantics(
              button: true,
              label: 'Reset zoom to 100%',
              child: FButton(
                key: const ValueKey('organization-zoom-100'),
                onPress: () => onZoomTo(1),
                variant: percent == 100
                    ? FButtonVariant.secondary
                    : FButtonVariant.ghost,
                size: FButtonSizeVariant.sm,
                child: const Text('100%'),
              ),
            ),
          ],
        );
      },
    );
  }
}
