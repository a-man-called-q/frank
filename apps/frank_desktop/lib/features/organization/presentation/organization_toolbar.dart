part of 'organization_surface.dart';

class _OrganizationToolbar extends StatefulWidget {
  const _OrganizationToolbar({
    required this.state,
    required this.controller,
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
  final NodeFlowController<
    OrganizationFlowNodeData,
    OrganizationFlowRelationData
  >
  controller;
  final VoidCallback onAdd;
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

// Keep the rail wide enough for its fixed controls and the optional Retry
// action before falling back to horizontal scrolling at compact widths.
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
          final publish = FilledButton.icon(
            key: const ValueKey('organization-publish'),
            onPressed: state.canPublish ? widget.onPublish : null,
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
          );
          final rail = SizedBox(
            width: railWidth,
            height: FrankUiTokens.toolbarHeight,
            child: _OrganizationPanel(
              color: FrankColors.sidebarSolid,
              padding: const EdgeInsets.symmetric(horizontal: 4),
              child: Row(
                children: [
                  _ToolbarTextButton(
                    buttonKey: const ValueKey('organization-add'),
                    icon: FrankIcons.plus,
                    label: 'Add',
                    onPressed: widget.onAdd,
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
                    TextButton(
                      key: const ValueKey('organization-retry'),
                      onPressed: widget.onRetry,
                      style: _compactTextButtonStyle(),
                      child: const Text('Retry'),
                    ),
                  ],
                  if (!compactToolbar) ...[const _ToolbarDivider(), publish],
                ],
              ),
            ),
          );
          if (!compactToolbar) return rail;
          return SizedBox(
            width: availableWidth,
            height: FrankUiTokens.toolbarHeight,
            child: Stack(
              fit: StackFit.expand,
              children: [
                SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: rail,
                ),
                Positioned(
                  top: 0,
                  right: 0,
                  bottom: 0,
                  child: DecoratedBox(
                    decoration: const BoxDecoration(
                      color: FrankColors.sidebarSolid,
                      border: Border(
                        left: BorderSide(color: FrankColors.border),
                      ),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.only(left: 4, right: 4),
                      child: publish,
                    ),
                  ),
                ),
              ],
            ),
          );
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
              icon: Icons.remove,
              label: 'Zoom out',
              onPressed: () => onZoomBy(-.1),
            ),
            Tooltip(
              message: 'Zoom $percent%',
              child: SizedBox(
                width: 70,
                child: FrankDesktopSelectField<double>(
                  key: const ValueKey('organization-zoom'),
                  fieldKey: const ValueKey('organization-zoom-trigger'),
                  value: viewport.zoom,
                  options: [
                    if (!_presets.contains(viewport.zoom))
                      FrankDesktopSelectOption<double>(
                        value: viewport.zoom,
                        label: '$percent%',
                      ),
                    for (final preset in _presets)
                      FrankDesktopSelectOption<double>(
                        value: preset,
                        label: '${(preset * 100).round()}%',
                      ),
                  ],
                  onChanged: onZoomTo,
                  semanticsLabel: 'Zoom $percent%',
                ),
              ),
            ),
            _ToolbarIconButton(
              buttonKey: const ValueKey('organization-zoom-in'),
              icon: Icons.add,
              label: 'Zoom in',
              onPressed: () => onZoomBy(.1),
            ),
            Semantics(
              button: true,
              label: 'Reset zoom to 100%',
              child: TextButton(
                key: const ValueKey('organization-zoom-100'),
                onPressed: () => onZoomTo(1),
                style: _toolbarButtonStyle(selected: percent == 100),
                child: const Text('100%'),
              ),
            ),
          ],
        );
      },
    );
  }
}
