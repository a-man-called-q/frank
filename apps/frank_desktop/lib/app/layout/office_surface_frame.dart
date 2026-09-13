import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';

/// The two kinds of content that can occupy an Office surface.
///
/// A page owns one vertical reading flow. A canvas is a full-bleed viewport
/// whose gestures and scrolling belong to the feature rendered inside it.
enum OfficeSurfaceMode { canvas, page }

enum OfficeLayoutClass { compact, regular, wide }

/// Layout facts derived from the width allocated by the shell.
///
/// This deliberately takes the [BoxConstraints] supplied by the surface
/// rather than reading the window size. A surface therefore responds to a
/// resized or collapsed sidebar using the width it can actually render.
@immutable
class OfficeLayoutMetrics {
  const OfficeLayoutMetrics({
    required this.availableWidth,
    required this.layoutClass,
    required this.gutter,
    required this.maxContentWidth,
  });

  factory OfficeLayoutMetrics.fromConstraints(BoxConstraints constraints) {
    final width = constraints.hasBoundedWidth && constraints.maxWidth.isFinite
        ? constraints.maxWidth
        : double.infinity;
    return OfficeLayoutMetrics.fromWidth(width);
  }

  factory OfficeLayoutMetrics.fromWidth(double width) {
    final normalizedWidth = width.isFinite && width >= 0
        ? width
        : double.infinity;
    final layoutClass = normalizedWidth < 720
        ? OfficeLayoutClass.compact
        : normalizedWidth < 1200
        ? OfficeLayoutClass.regular
        : OfficeLayoutClass.wide;
    return OfficeLayoutMetrics(
      availableWidth: normalizedWidth,
      layoutClass: layoutClass,
      gutter: layoutClass == OfficeLayoutClass.compact ? 16 : 24,
      maxContentWidth: 1240,
    );
  }

  final double availableWidth;
  final OfficeLayoutClass layoutClass;
  final double gutter;
  final double maxContentWidth;

  bool get isCompact => layoutClass == OfficeLayoutClass.compact;
  bool get isWide => layoutClass == OfficeLayoutClass.wide;

  /// The width available to a centered page after its two gutters.
  double get contentWidth => availableWidth.isFinite
      ? math.max(0, math.min(maxContentWidth, availableWidth - gutter * 2))
      : maxContentWidth;
}

/// Provides the frame's pre-gutter width classification to feature widgets.
///
/// A header or an internal breakpoint should classify the area allocated by
/// the shell, not the narrower readable column after page gutters. The scope
/// keeps that distinction explicit without coupling features to the shell.
class OfficeLayoutMetricsScope extends InheritedWidget {
  const OfficeLayoutMetricsScope({
    required this.metrics,
    required super.child,
    super.key,
  });

  final OfficeLayoutMetrics metrics;

  static OfficeLayoutMetrics? maybeOf(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<OfficeLayoutMetricsScope>()
      ?.metrics;

  @override
  bool updateShouldNotify(OfficeLayoutMetricsScope oldWidget) =>
      metrics != oldWidget.metrics;
}

/// Shared frame for all Office feature surfaces.
class OfficeSurfaceFrame extends StatelessWidget {
  const OfficeSurfaceFrame.canvas({
    required this.child,
    this.backgroundColor = Colors.transparent,
    this.header,
    this.overlay,
    super.key,
  }) : mode = OfficeSurfaceMode.canvas,
       fullWidth = false,
       slivers = const <Widget>[],
       scrollKey = null,
       scrollController = null;

  const OfficeSurfaceFrame.page({
    required this.header,
    required this.slivers,
    this.fullWidth = false,
    this.scrollKey,
    this.scrollController,
    super.key,
  }) : mode = OfficeSurfaceMode.page,
       child = null,
       backgroundColor = Colors.transparent,
       overlay = null;

  final OfficeSurfaceMode mode;
  final Widget? child;
  final Color backgroundColor;
  final Widget? header;

  /// Optional feature overlay rendered over the whole canvas surface.
  ///
  /// The slot intentionally has no barrier or focus trap. A feature can use
  /// it for a right-side inspector while leaving the exposed canvas beneath
  /// interactive. The frame owns the bounds, so the overlay also covers the
  /// page header and toolbar rather than only the expanded canvas body.
  final Widget? overlay;

  /// Whether a page should use all available width after its gutters.
  ///
  /// Reading pages default to a 1240 logical pixel column for comfortable
  /// scanning. Dense surfaces such as Team and Ledger can opt into the full
  /// shell width while retaining the same responsive gutters.
  final bool fullWidth;

  /// Slivers rendered in the page body below the pinned [header].
  ///
  /// Feature-level horizontal scrolling remains local to the sliver that
  /// needs it; the frame owns only the vertical reading flow.
  final List<Widget> slivers;
  final Key? scrollKey;
  final ScrollController? scrollController;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final metrics = OfficeLayoutMetrics.fromConstraints(constraints);
        return OfficeLayoutMetricsScope(
          metrics: metrics,
          child: switch (mode) {
            OfficeSurfaceMode.canvas => ColoredBox(
              key: const ValueKey('office-surface-frame-canvas'),
              color: backgroundColor,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  header == null
                      ? SizedBox.expand(child: child)
                      : Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Padding(
                              padding: EdgeInsets.fromLTRB(
                                metrics.gutter,
                                metrics.gutter,
                                metrics.gutter,
                                0,
                              ),
                              // Canvas surfaces use the full width allocated
                              // by the shell so their toolbar/actions line up
                              // with the viewport rather than the page's 1240
                              // px reading column. Page headers are
                              // constrained in [_OfficePageFrame] below.
                              child: SizedBox(
                                width: double.infinity,
                                child: header,
                              ),
                            ),
                            Expanded(child: child!),
                          ],
                        ),
                  if (overlay != null)
                    Positioned.fill(child: SizedBox.expand(child: overlay!)),
                ],
              ),
            ),
            OfficeSurfaceMode.page => _OfficePageFrame(
              key: const ValueKey('office-surface-frame-page'),
              metrics: metrics,
              header: header,
              slivers: slivers,
              fullWidth: fullWidth,
              scrollKey: scrollKey,
              scrollController: scrollController,
            ),
          },
        );
      },
    );
  }
}

/// Places a feature's detail inspector in a drawer over the complete feature.
///
/// The drawer is attached to the root overlay so it can cover the frame's
/// header and toolbar while retaining the feature surface's own constraints.
/// On desktop the surface remains mounted and interactive beside the drawer;
/// on compact surfaces it remains mounted at its original geometry while the
/// detail view covers it, so its viewport and input state survive.
class OfficeInspectorDrawerOverlay extends StatefulWidget {
  const OfficeInspectorDrawerOverlay({
    required this.child,
    required this.inspector,
    required this.inspectorLabel,
    this.inspectorKey,
    super.key,
  });

  static const desktopBreakpoint = 1088.0;
  static const defaultPanelWidth = 360.0;
  static const minPanelWidth = 320.0;
  static const maxPanelWidth = 480.0;
  static const resizeStep = 16.0;
  static const resizeHandleWidth = 8.0;

  final Widget child;
  final Widget? inspector;
  final String inspectorLabel;
  final Key? inspectorKey;

  @override
  State<OfficeInspectorDrawerOverlay> createState() =>
      _OfficeInspectorDrawerOverlayState();
}

class _OfficeInspectorDrawerOverlayState
    extends State<OfficeInspectorDrawerOverlay> {
  double _panelWidth = OfficeInspectorDrawerOverlay.defaultPanelWidth;
  late final OverlayPortalController _overlayController =
      OverlayPortalController(debugLabel: 'office-inspector-drawer');
  late final FocusNode _resizeFocusNode = FocusNode(
    debugLabel: 'office-inspector-resize',
  );

  @override
  void initState() {
    super.initState();
    // Showing the portal before it attaches means a selection that appears
    // during a rebuild is laid out in the same frame. The overlay child is a
    // zero-size widget while there is no inspector.
    _overlayController.show();
  }

  @override
  void dispose() {
    _resizeFocusNode.dispose();
    super.dispose();
  }

  double _maxPanelWidth(double availableWidth) {
    if (!availableWidth.isFinite) {
      return OfficeInspectorDrawerOverlay.maxPanelWidth;
    }
    return math.min(OfficeInspectorDrawerOverlay.maxPanelWidth, availableWidth);
  }

  double _clampPanelWidth(double width, double availableWidth) {
    final maxWidth = _maxPanelWidth(availableWidth);
    if (maxWidth < OfficeInspectorDrawerOverlay.minPanelWidth) {
      return maxWidth;
    }
    return width
        .clamp(OfficeInspectorDrawerOverlay.minPanelWidth, maxWidth)
        .toDouble();
  }

  void _setPanelWidth(double width, double availableWidth) {
    if (!mounted) return;
    final next = _clampPanelWidth(width, availableWidth);
    if ((next - _panelWidth).abs() < .1) return;
    setState(() => _panelWidth = next);
  }

  KeyEventResult _onResizeKey(KeyEvent event, double availableWidth) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (key == LogicalKeyboardKey.arrowLeft) {
      _setPanelWidth(
        _panelWidth + OfficeInspectorDrawerOverlay.resizeStep,
        availableWidth,
      );
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.arrowRight) {
      _setPanelWidth(
        _panelWidth - OfficeInspectorDrawerOverlay.resizeStep,
        availableWidth,
      );
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  Widget _mainPane({required bool hidden}) {
    return ExcludeFocus(
      excluding: hidden,
      child: ExcludeSemantics(
        excluding: hidden,
        child: IgnorePointer(ignoring: hidden, child: widget.child),
      ),
    );
  }

  Widget _resizeHandle({required double availableWidth}) {
    final panelWidth = _clampPanelWidth(_panelWidth, availableWidth);
    final increased = _clampPanelWidth(
      panelWidth + OfficeInspectorDrawerOverlay.resizeStep,
      availableWidth,
    );
    final decreased = _clampPanelWidth(
      panelWidth - OfficeInspectorDrawerOverlay.resizeStep,
      availableWidth,
    );
    return Semantics(
      container: true,
      label: '${widget.inspectorLabel} width',
      value: '${panelWidth.round()} pixels',
      increasedValue: '${increased.round()} pixels',
      decreasedValue: '${decreased.round()} pixels',
      onIncrease: () => _setPanelWidth(increased, availableWidth),
      onDecrease: () => _setPanelWidth(decreased, availableWidth),
      child: Focus(
        focusNode: _resizeFocusNode,
        onKeyEvent: (_, event) => _onResizeKey(event, availableWidth),
        child: MouseRegion(
          cursor: SystemMouseCursors.resizeLeftRight,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: _resizeFocusNode.requestFocus,
            onHorizontalDragUpdate: (details) =>
                _setPanelWidth(_panelWidth - details.delta.dx, availableWidth),
            child: const DecoratedBox(
              key: ValueKey('office-inspector-resize-handle'),
              decoration: BoxDecoration(
                border: Border(left: BorderSide(color: FrankColors.border)),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _drawer({required OverlayChildLayoutInfo info}) {
    final inspector = widget.inspector;
    if (inspector == null) return const SizedBox.shrink();

    final featureRect = MatrixUtils.transformRect(
      info.childPaintTransform,
      Offset.zero & info.childSize,
    );
    final featureWidth = info.childSize.width;
    final desktop =
        featureWidth >= OfficeInspectorDrawerOverlay.desktopBreakpoint;
    final requestedWidth = desktop
        ? _clampPanelWidth(_panelWidth, featureWidth)
        : featureRect.width;
    final overlayWidth = info.overlaySize.width;
    final rightEdge = featureRect.right.clamp(0.0, overlayWidth).toDouble();
    final panelWidth = math.min(requestedWidth, rightEdge);
    if (panelWidth <= 0) return const SizedBox.shrink();

    final inspectorChild = KeyedSubtree(
      key: widget.inspectorKey,
      child: inspector,
    );
    final content = Material(
      type: MaterialType.transparency,
      child: desktop
          ? inspectorChild
          : Focus(
              debugLabel: 'office-inspector',
              autofocus: true,
              child: inspectorChild,
            ),
    );
    final panel = DecoratedBox(
      decoration: const BoxDecoration(
        border: Border(left: BorderSide(color: FrankColors.border)),
        boxShadow: [
          BoxShadow(
            color: Color(0x30000000),
            offset: Offset(-3, 0),
            blurRadius: 12,
          ),
        ],
      ),
      child: Stack(
        fit: StackFit.expand,
        children: [
          content,
          if (desktop)
            Positioned(
              left: 0,
              top: 0,
              bottom: 0,
              width: OfficeInspectorDrawerOverlay.resizeHandleWidth,
              child: _resizeHandle(availableWidth: featureWidth),
            ),
        ],
      ),
    );
    return Positioned(
      left: rightEdge - panelWidth,
      top: 0,
      bottom: 0,
      width: panelWidth,
      child: panel,
    );
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final availableWidth =
            constraints.hasBoundedWidth && constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : double.infinity;
        final desktop =
            availableWidth >= OfficeInspectorDrawerOverlay.desktopBreakpoint;
        final hidden = widget.inspector != null && !desktop;
        return OverlayPortal.overlayChildLayoutBuilder(
          controller: _overlayController,
          overlayLocation: OverlayChildLocation.rootOverlay,
          overlayChildBuilder: (context, info) => _drawer(info: info),
          child: _mainPane(hidden: hidden),
        );
      },
    );
  }
}

class _OfficePageFrame extends StatefulWidget {
  const _OfficePageFrame({
    required this.metrics,
    required this.header,
    required this.slivers,
    required this.fullWidth,
    required this.scrollKey,
    required this.scrollController,
    super.key,
  });

  final OfficeLayoutMetrics metrics;
  final Widget? header;
  final List<Widget> slivers;
  final bool fullWidth;
  final Key? scrollKey;
  final ScrollController? scrollController;

  @override
  State<_OfficePageFrame> createState() => _OfficePageFrameState();
}

class _OfficePageFrameState extends State<_OfficePageFrame> {
  final _fallbackScrollController = ScrollController();

  OfficeLayoutMetrics get metrics => widget.metrics;

  @override
  void dispose() {
    _fallbackScrollController.dispose();
    super.dispose();
  }

  /// Keeps a page's content aligned with its header while retaining the
  /// feature's own sliver geometry. Reading pages absorb unused width after
  /// the 1240 px readable maximum; full-width pages keep only the responsive
  /// gutter on each side.
  Widget _constrainedSliver({
    required Widget sliver,
    required double top,
    required double bottom,
  }) {
    return SliverLayoutBuilder(
      builder: (context, constraints) {
        final crossAxisExtent = constraints.crossAxisExtent;
        final rightGutter = widget.fullWidth
            ? metrics.gutter
            : crossAxisExtent.isFinite
            ? math.max(
                metrics.gutter,
                crossAxisExtent - metrics.maxContentWidth - metrics.gutter,
              )
            : metrics.gutter;
        return SliverPadding(
          padding: EdgeInsets.fromLTRB(
            metrics.gutter,
            top,
            rightGutter,
            bottom,
          ),
          sliver: sliver,
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final header = widget.header;
    final slivers = widget.slivers;
    final scrollController =
        widget.scrollController ?? _fallbackScrollController;
    final gutter = metrics.gutter;
    final bodyTop = header == null ? gutter : 20.0;
    final contentSlivers = <Widget>[];
    if (slivers.isEmpty) {
      contentSlivers.add(
        _constrainedSliver(
          top: bodyTop,
          bottom: gutter,
          sliver: const SliverToBoxAdapter(child: SizedBox.shrink()),
        ),
      );
    } else {
      for (var index = 0; index < slivers.length; index++) {
        contentSlivers.add(
          _constrainedSliver(
            top: index == 0 ? bodyTop : 0,
            bottom: index == slivers.length - 1 ? gutter : 0,
            sliver: slivers[index],
          ),
        );
      }
    }
    return ColoredBox(
      key: const ValueKey('office-page-surface'),
      color: FrankColors.panel,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (header != null)
            Padding(
              padding: EdgeInsets.fromLTRB(gutter, gutter, gutter, 0),
              child: Align(
                alignment: Alignment.centerLeft,
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    maxWidth: widget.fullWidth
                        ? double.infinity
                        : metrics.maxContentWidth,
                  ),
                  child: SizedBox(width: double.infinity, child: header),
                ),
              ),
            ),
          Expanded(
            child: ScrollConfiguration(
              behavior: ScrollConfiguration.of(
                context,
              ).copyWith(scrollbars: false),
              child: Scrollbar(
                controller: scrollController,
                child: CustomScrollView(
                  key: widget.scrollKey,
                  controller: scrollController,
                  primary: false,
                  slivers: contentSlivers,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Shared heading name for page and canvas surfaces.
typedef OfficeSurfaceHeader = OfficePageHeader;

/// Standard heading used by reading-oriented Office pages.
class OfficePageHeader extends StatelessWidget {
  const OfficePageHeader({
    required this.title,
    this.description,
    this.actions,
    super.key,
  });

  final String title;
  final String? description;
  final Widget? actions;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final scopedMetrics = OfficeLayoutMetricsScope.maybeOf(context);
        final regular = scopedMetrics == null
            ? constraints.hasBoundedWidth && constraints.maxWidth >= 720
            : !scopedMetrics.isCompact;
        final copy = Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              title,
              style: const TextStyle(
                color: FrankColors.ink,
                fontSize: FrankUiTokens.pageTitleSize,
                height:
                    FrankUiTokens.pageTitleLineHeight /
                    FrankUiTokens.pageTitleSize,
                fontWeight: FontWeight.w500,
                letterSpacing: -0.35,
              ),
            ),
            if (description != null) ...[
              const SizedBox(height: 6),
              Text(
                description!,
                softWrap: true,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: FrankUiTokens.bodyTextSize,
                  height: 19 / FrankUiTokens.bodyTextSize,
                ),
              ),
            ],
          ],
        );
        if (actions == null) return copy;
        if (regular) {
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(child: copy),
              const SizedBox(width: 16),
              Flexible(
                child: Align(alignment: Alignment.topRight, child: actions),
              ),
            ],
          );
        }
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            copy,
            const SizedBox(height: 12),
            Align(alignment: Alignment.centerLeft, child: actions),
          ],
        );
      },
    );
  }
}
