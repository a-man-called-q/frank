part of 'frank_desktop_menu.dart';

class FrankDesktopMenuDismissScope extends StatefulWidget {
  const FrankDesktopMenuDismissScope({required this.child, super.key});

  final Widget child;

  /// Dismiss all registered desktop menus and Material tooltips below the
  /// nearest scope. The focus-restoration switch is false for scroll-driven
  /// dismissal so requesting focus cannot pull the row back into view.
  static void dismissAll(BuildContext context, {bool restoreFocus = true}) {
    context
        .findAncestorStateOfType<_FrankDesktopMenuDismissScopeState>()
        ?._dismissAll(restoreFocus: restoreFocus);
    Tooltip.dismissAllToolTips();
  }

  /// Registers a menu implemented by another overlay system, such as Forui.
  ///
  /// The registration is active until [release] is called. The dismiss
  /// callback is intentionally parameterless because external menus do not
  /// own Frank's trigger-focus restoration policy.
  static void register(
    BuildContext context,
    Object owner,
    VoidCallback dismiss,
  ) {
    registerAndGet(context, owner, dismiss);
  }

  /// Registers a menu and returns an unregistration callback that does not
  /// need to look up an ancestor later. This is useful for state objects that
  /// may be disposed after their element has already been deactivated.
  static VoidCallback? registerAndGet(
    BuildContext context,
    Object owner,
    VoidCallback dismiss,
  ) {
    final scope = _stateOf(context);
    if (scope == null) return null;
    scope._claim(owner, (_) => dismiss());
    return () => scope._release(owner);
  }

  /// Releases an external menu registration.
  static void release(BuildContext context, Object owner) {
    // A child can be disposed after its dismiss scope has already started
    // unmounting. In that narrow window ancestor lookup is unsafe, and the
    // scope's registry is discarded with it anyway.
    try {
      context
          .findAncestorStateOfType<_FrankDesktopMenuDismissScopeState>()
          ?._release(owner);
    } on FlutterError {
      // Nothing remains to release when the ancestor is deactivated.
    }
  }

  static _FrankDesktopMenuDismissScopeState? _stateOf(BuildContext context) =>
      context.findAncestorStateOfType<_FrankDesktopMenuDismissScopeState>();

  @override
  State<FrankDesktopMenuDismissScope> createState() =>
      _FrankDesktopMenuDismissScopeState();
}

class _FrankDesktopMenuDismissScopeState
    extends State<FrankDesktopMenuDismissScope>
    with WidgetsBindingObserver {
  final Map<Object, void Function(bool restoreFocus)> _activeMenus =
      <Object, void Function(bool restoreFocus)>{};

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  void _claim(Object owner, void Function(bool restoreFocus) dismiss) {
    for (final entry in List<MapEntry<Object, void Function(bool)>>.of(
      _activeMenus.entries,
    )) {
      if (entry.key == owner) continue;
      _activeMenus.remove(entry.key);
      entry.value(false);
    }
    _activeMenus[owner] = dismiss;
  }

  void _release(Object owner) {
    _activeMenus.remove(owner);
  }

  void _dismissAll({required bool restoreFocus}) {
    for (final dismiss in List<void Function(bool)>.of(_activeMenus.values)) {
      dismiss(restoreFocus);
    }
  }

  @override
  void didChangeMetrics() {
    // A window resize can move an anchored surface out of the viewport without
    // producing a scroll notification. Close it before the next layout paints.
    _dismissAll(restoreFocus: false);
    Tooltip.dismissAllToolTips();
  }

  @override
  Widget build(BuildContext context) =>
      NotificationListener<ScrollNotification>(
        onNotification: (notification) {
          if (notification is ScrollStartNotification ||
              notification is ScrollUpdateNotification ||
              notification is OverscrollNotification ||
              notification is ScrollMetricsNotification) {
            _dismissAll(restoreFocus: false);
            Tooltip.dismissAllToolTips();
          }
          return false;
        },
        child: widget.child,
      );
}

class FrankDesktopMenu extends StatefulWidget {
  const FrankDesktopMenu({
    required this.child,
    required this.groups,
    this.controller,
    this.kind = FrankDesktopMenuKind.context,
    this.width = 248,
    this.matchTriggerWidth = false,
    this.openOnTap = false,
    this.openOnSecondaryTap = false,
    this.enabled = true,
    this.groupsBuilder,
    this.searchHint = 'Search',
    this.onOpenChanged,
    this.returnFocusNode,
    this.semanticsLabel,
    super.key,
  });

  final Widget child;
  final List<FrankMenuGroup> groups;
  final FrankDesktopMenuController? controller;
  final FrankDesktopMenuKind kind;
  final double width;
  final bool matchTriggerWidth;
  final bool openOnTap;
  final bool openOnSecondaryTap;
  final bool enabled;
  final List<FrankMenuGroup> Function(String query)? groupsBuilder;
  final String searchHint;
  final ValueChanged<bool>? onOpenChanged;
  final FocusNode? returnFocusNode;
  final String? semanticsLabel;

  @override
  State<FrankDesktopMenu> createState() => _FrankDesktopMenuState();
}

class _FrankDesktopMenuState extends State<FrankDesktopMenu> {
  static const _gap = 4.0;
  static const _viewportMargin = 8.0;
  OverlayEntry? _entry;
  _FrankDesktopMenuDismissScopeState? _dismissScope;
  final GlobalKey _anchorKey = GlobalKey();
  final Object _regionId = Object();

  bool get _isOpen => _entry != null;

  @override
  void initState() {
    super.initState();
    widget.controller?._openCallback = _open;
    widget.controller?._openAtCallback = _openAt;
    widget.controller?._closeCallback = _close;
    widget.controller?._toggleCallback = _toggle;
  }

  @override
  void didUpdateWidget(covariant FrankDesktopMenu oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller?._openCallback = null;
      oldWidget.controller?._openAtCallback = null;
      oldWidget.controller?._closeCallback = null;
      oldWidget.controller?._toggleCallback = null;
      widget.controller?._openCallback = _open;
      widget.controller?._openAtCallback = _openAt;
      widget.controller?._closeCallback = _close;
      widget.controller?._toggleCallback = _toggle;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final scope = FrankDesktopMenuDismissScope._stateOf(context);
    if (identical(scope, _dismissScope)) return;
    _dismissScope?._release(this);
    _dismissScope = scope;
    if (_entry != null) {
      _dismissScope?._claim(this, (restoreFocus) {
        _close(restoreFocus: restoreFocus);
      });
    }
  }

  @override
  void dispose() {
    _dismissScope?._release(this);
    widget.controller?._openCallback = null;
    widget.controller?._openAtCallback = null;
    widget.controller?._closeCallback = null;
    widget.controller?._toggleCallback = null;
    _entry?.remove();
    _entry?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    Widget child = widget.child;
    if (widget.openOnTap || widget.openOnSecondaryTap) {
      child = GestureDetector(
        behavior: HitTestBehavior.translucent,
        onTap: widget.enabled && widget.openOnTap ? _toggle : null,
        onSecondaryTap: widget.enabled && widget.openOnSecondaryTap
            ? _open
            : null,
        child: child,
      );
    }
    return TapRegion(
      groupId: _regionId,
      child: Listener(
        key: _anchorKey,
        behavior: HitTestBehavior.translucent,
        child: child,
      ),
    );
  }

  void _toggle() {
    if (_isOpen) {
      _close();
    } else {
      _open();
    }
  }

  void _open() {
    if (_isOpen) return;
    _openAt(null);
  }

  void _openAt(Offset? globalPosition) {
    if (!mounted || !widget.enabled) return;
    if (_isOpen) _close(restoreFocus: false);
    final anchor = _anchorKey.currentContext?.findRenderObject();
    final overlay = Overlay.of(context).context.findRenderObject();
    if (anchor is! RenderBox || overlay is! RenderBox) return;

    final anchorRect = globalPosition == null
        ? anchor.localToGlobal(Offset.zero, ancestor: overlay) & anchor.size
        : overlay.globalToLocal(globalPosition) & Size.zero;
    final requestedMenuWidth = widget.matchTriggerWidth
        ? anchor.size.width
        : widget.width;
    final maxMenuWidth = (overlay.size.width - 2 * _viewportMargin)
        .clamp(1.0, double.infinity)
        .toDouble();
    final menuWidth = requestedMenuWidth.clamp(1.0, maxMenuWidth).toDouble();
    final menuHeight = _estimatedHeight;
    final placement = _placement(
      anchorRect,
      Size(menuWidth, menuHeight),
      overlay.size,
    );

    _dismissScope?._claim(this, (restoreFocus) {
      _close(restoreFocus: restoreFocus);
    });

    final entry = OverlayEntry(
      builder: (context) => Stack(
        clipBehavior: Clip.none,
        children: [
          Positioned(
            left: placement.left,
            top: placement.top,
            width: menuWidth,
            child: TapRegion(
              groupId: _regionId,
              onTapOutside: (_) => _close(),
              child: _FrankDesktopMenuSurface(
                groups: widget.groups,
                maxHeight: placement.maxHeight,
                kind: widget.kind,
                semanticsLabel: widget.semanticsLabel,
                groupsBuilder: widget.groupsBuilder,
                searchHint: widget.searchHint,
                onClose: _close,
              ),
            ),
          ),
        ],
      ),
    );
    _entry = entry;
    Overlay.of(context).insert(entry);
    widget.onOpenChanged?.call(true);
  }

  void _close({bool restoreFocus = true}) {
    final entry = _entry;
    if (entry == null) return;
    _entry = null;
    _dismissScope?._release(this);
    entry.remove();
    entry.dispose();
    widget.onOpenChanged?.call(false);
    if (restoreFocus && mounted) widget.returnFocusNode?.requestFocus();
  }

  double get _estimatedHeight {
    final itemCount = widget.groups.fold<int>(
      0,
      (total, group) => total + group.items.length,
    );
    final separators = widget.groups
        .where((group) => group.items.isNotEmpty)
        .length;
    return (itemCount * 32 + (separators > 0 ? separators - 1 : 0) * 9 + 8)
        .toDouble();
  }

  _MenuPlacement _placement(Rect anchor, Size menu, Size viewport) {
    final candidates = widget.kind == FrankDesktopMenuKind.account
        ? <_MenuPlacement Function()>[
            () => _candidate(
              anchor.left,
              anchor.top - _gap - menu.height,
              menu,
              viewport,
            ),
            () => _candidate(anchor.left, anchor.bottom + _gap, menu, viewport),
            () => _candidate(anchor.right + _gap, anchor.top, menu, viewport),
            () => _candidate(
              anchor.left - _gap - menu.width,
              anchor.top,
              menu,
              viewport,
            ),
          ]
        : widget.kind == FrankDesktopMenuKind.select
        ? <_MenuPlacement Function()>[
            () => _candidate(anchor.left, anchor.bottom + _gap, menu, viewport),
            () => _candidate(
              anchor.left,
              anchor.top - _gap - menu.height,
              menu,
              viewport,
            ),
            () => _candidate(anchor.right + _gap, anchor.top, menu, viewport),
            () => _candidate(
              anchor.left - _gap - menu.width,
              anchor.top,
              menu,
              viewport,
            ),
          ]
        : <_MenuPlacement Function()>[
            () => _candidate(anchor.right + _gap, anchor.top, menu, viewport),
            () => _candidate(
              anchor.left - _gap - menu.width,
              anchor.top,
              menu,
              viewport,
            ),
            () => _candidate(anchor.left, anchor.bottom + _gap, menu, viewport),
            () => _candidate(
              anchor.left,
              anchor.top - _gap - menu.height,
              menu,
              viewport,
            ),
          ];

    for (final candidate in candidates) {
      final placement = candidate();
      if (_fits(placement, menu, viewport)) return placement;
    }

    // A menu that is taller than the available space should stay on the
    // chosen side of its anchor and scroll internally. Only clamp the
    // horizontal coordinate when the vertical side is still valid; blindly
    // clamping the first candidate can move a right/left menu over its row.
    for (final candidate in candidates) {
      final placement = candidate();
      if (_isVisibleAndSeparate(placement, menu, anchor, viewport)) {
        return placement;
      }
    }

    // The vertical candidates are the last safe fallback because their gap
    // from the anchor is preserved even when the trigger is at a viewport
    // edge. This branch is only reached when the menu is wider than the
    // available viewport or the viewport is exceptionally short.
    final vertical = [
      _candidate(anchor.left, anchor.bottom + _gap, menu, viewport),
      _candidate(anchor.left, anchor.top - _gap - menu.height, menu, viewport),
    ];
    for (final placement in vertical) {
      if (placement.top >= _viewportMargin &&
          placement.top < viewport.height - _viewportMargin) {
        return _MenuPlacement(
          left: placement.left
              .clamp(
                _viewportMargin,
                (viewport.width - menu.width - _viewportMargin).clamp(
                  _viewportMargin,
                  double.infinity,
                ),
              )
              .toDouble(),
          top: placement.top,
          maxHeight: placement.maxHeight,
        );
      }
    }

    // This is a last-resort protection for a test host shorter than a single
    // menu item. It keeps the surface inside the viewport; normal supported
    // windows take one of the non-overlapping branches above.
    return _MenuPlacement(
      left: _viewportMargin,
      top: _viewportMargin,
      maxHeight: _availableHeight(
        viewport.height - 2 * _viewportMargin,
        menu.height,
      ),
    );
  }

  _MenuPlacement _candidate(double left, double top, Size menu, Size viewport) {
    return _MenuPlacement(
      left: left,
      top: top,
      maxHeight: _availableHeight(
        viewport.height - top - _viewportMargin,
        menu.height,
      ),
    );
  }

  bool _fits(_MenuPlacement placement, Size menu, Size viewport) {
    return placement.left >= _viewportMargin &&
        placement.top >= _viewportMargin &&
        placement.left + menu.width <= viewport.width - _viewportMargin &&
        placement.top + menu.height <= viewport.height - _viewportMargin;
  }

  bool _isVisibleAndSeparate(
    _MenuPlacement placement,
    Size menu,
    Rect anchor,
    Size viewport,
  ) {
    final visible =
        placement.left >= _viewportMargin &&
        placement.left + menu.width <= viewport.width - _viewportMargin &&
        placement.top >= _viewportMargin &&
        placement.top < viewport.height - _viewportMargin;
    if (!visible) return false;
    final menuRect = Rect.fromLTWH(
      placement.left,
      placement.top,
      menu.width,
      menu.height,
    );
    return !menuRect.overlaps(anchor);
  }

  double _availableHeight(double available, double menuHeight) {
    return available.clamp(1.0, menuHeight).toDouble();
  }
}

class _MenuPlacement {
  const _MenuPlacement({
    required this.left,
    required this.top,
    required this.maxHeight,
  });

  final double left;
  final double top;
  final double maxHeight;
}

class _FrankDesktopMenuSurface extends StatefulWidget {
  const _FrankDesktopMenuSurface({
    required this.groups,
    required this.maxHeight,
    required this.kind,
    required this.onClose,
    this.groupsBuilder,
    this.searchHint = 'Search',
    this.semanticsLabel,
  });

  final List<FrankMenuGroup> groups;
  final double maxHeight;
  final FrankDesktopMenuKind kind;
  final VoidCallback onClose;
  final List<FrankMenuGroup> Function(String query)? groupsBuilder;
  final String searchHint;
  final String? semanticsLabel;

  @override
  State<_FrankDesktopMenuSurface> createState() =>
      _FrankDesktopMenuSurfaceState();
}

class _FrankDesktopMenuSurfaceState extends State<_FrankDesktopMenuSurface> {
  late final FocusNode _focusNode;
  late final List<FrankMenuItem> _items;
  late final TextEditingController _searchController;
  int _focusedIndex = 0;

  @override
  void initState() {
    super.initState();
    _focusNode = FocusNode(debugLabel: 'Frank desktop menu');
    _searchController = TextEditingController();
    _items = [for (final group in widget.groups) ...group.items];
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focusNode.requestFocus();
    });
  }

  @override
  void dispose() {
    _focusNode.dispose();
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final groups =
        widget.groupsBuilder?.call(_searchController.text) ?? widget.groups;
    _items
      ..clear()
      ..addAll([for (final group in groups) ...group.items]);
    if (_items.isNotEmpty && _focusedIndex >= _items.length) {
      _focusedIndex = _items.length - 1;
    }
    return Semantics(
      container: true,
      label: widget.semanticsLabel,
      child: Material(
        color: Colors.transparent,
        child: Focus(
          focusNode: _focusNode,
          onKeyEvent: (_, event) => _handleKey(event),
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: widget.maxHeight),
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: FrankColors.panelRaised,
                borderRadius: BorderRadius.circular(9),
                border: Border.all(color: FrankColors.border),
                boxShadow: const [
                  BoxShadow(
                    color: Color(0x66000000),
                    blurRadius: 20,
                    offset: Offset(0, 8),
                  ),
                ],
              ),
              child: ClipRRect(
                borderRadius: BorderRadius.circular(8),
                child: SingleChildScrollView(
                  padding: const EdgeInsets.symmetric(vertical: 3),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (widget.groupsBuilder != null)
                        Padding(
                          padding: const EdgeInsets.fromLTRB(8, 7, 8, 5),
                          child: TextField(
                            controller: _searchController,
                            autofocus: true,
                            onChanged: (_) => setState(() {}),
                            style: const TextStyle(
                              color: FrankColors.ink,
                              fontSize: 12,
                            ),
                            decoration: InputDecoration(
                              isDense: true,
                              hintText: widget.searchHint,
                              hintStyle: const TextStyle(
                                color: FrankColors.muted,
                                fontSize: 12,
                              ),
                              filled: true,
                              fillColor: FrankColors.panel,
                              contentPadding: const EdgeInsets.symmetric(
                                horizontal: 8,
                                vertical: 7,
                              ),
                              border: OutlineInputBorder(
                                borderRadius: BorderRadius.all(
                                  Radius.circular(6),
                                ),
                                borderSide: BorderSide(
                                  color: FrankColors.border,
                                ),
                              ),
                              enabledBorder: OutlineInputBorder(
                                borderRadius: BorderRadius.all(
                                  Radius.circular(6),
                                ),
                                borderSide: BorderSide(
                                  color: FrankColors.border,
                                ),
                              ),
                              focusedBorder: OutlineInputBorder(
                                borderRadius: BorderRadius.all(
                                  Radius.circular(6),
                                ),
                                borderSide: BorderSide(
                                  color: FrankColors.border,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ..._buildGroups(groups),
                      if (_items.isEmpty)
                        const Padding(
                          padding: EdgeInsets.fromLTRB(12, 8, 12, 10),
                          child: Align(
                            alignment: Alignment.centerLeft,
                            child: Text(
                              'No matches',
                              style: TextStyle(
                                color: FrankColors.muted,
                                fontSize: 12,
                              ),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  List<Widget> _buildGroups(List<FrankMenuGroup> groups) {
    final children = <Widget>[];
    var itemIndex = 0;
    for (var groupIndex = 0; groupIndex < groups.length; groupIndex++) {
      if (groupIndex > 0 && groups[groupIndex].items.isNotEmpty) {
        children.add(
          const Divider(
            height: 9,
            thickness: 1,
            indent: 4,
            endIndent: 4,
            color: FrankColors.border,
          ),
        );
      }
      for (final item in groups[groupIndex].items) {
        final index = itemIndex++;
        children.add(_buildItem(item, index));
      }
    }
    return children;
  }

  Widget _buildItem(FrankMenuItem item, int index) {
    final selected = widget.kind == FrankDesktopMenuKind.select && item.checked;
    final active = selected || (index == _focusedIndex && item.isEnabled);
    return KeyedSubtree(
      key: item.key,
      child: Semantics(
        button: true,
        enabled: item.isEnabled,
        label: item.semanticsLabel ?? item.label,
        child: MouseRegion(
          onEnter: (_) {
            if (item.isEnabled) setState(() => _focusedIndex = index);
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: item.isEnabled
                ? () {
                    widget.onClose();
                    item.onPressed?.call();
                  }
                : null,
            child: Container(
              height: 30,
              margin: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
              padding: const EdgeInsets.symmetric(horizontal: 6),
              decoration: BoxDecoration(
                color: active ? FrankColors.ink.withValues(alpha: 0.08) : null,
                borderRadius: BorderRadius.circular(6),
              ),
              child: Row(
                children: [
                  SizedBox(
                    width: 18,
                    child: item.icon == null
                        ? (item.checked
                              ? const Icon(
                                  FrankIcons.check,
                                  size: 14,
                                  color: FrankColors.ink,
                                )
                              : null)
                        : Icon(
                            item.icon,
                            size: 15,
                            color: item.destructive && item.isEnabled
                                ? FrankColors.failure
                                : item.isEnabled
                                ? FrankColors.ink
                                : FrankColors.muted,
                          ),
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      item.label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        color: item.destructive && item.isEnabled
                            ? FrankColors.failure
                            : !item.isEnabled
                            ? FrankColors.muted
                            : widget.kind == FrankDesktopMenuKind.select &&
                                  !item.checked
                            ? FrankColors.muted
                            : FrankColors.ink,
                        fontSize: 12,
                        fontWeight:
                            widget.kind == FrankDesktopMenuKind.select &&
                                item.checked
                            ? FontWeight.w600
                            : FontWeight.normal,
                      ),
                    ),
                  ),
                  if (item.shortcut != null) ...[
                    const SizedBox(width: 8),
                    Text(
                      item.shortcut!,
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 11,
                      ),
                    ),
                  ],
                  if (item.badge != null) ...[
                    const SizedBox(width: 8),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 5,
                        vertical: 2,
                      ),
                      decoration: BoxDecoration(
                        color: item.checked
                            ? FrankColors.aubergineAccent.withValues(
                                alpha: 0.18,
                              )
                            : FrankColors.panel,
                        borderRadius: BorderRadius.circular(4),
                      ),
                      child: Text(
                        item.badge!,
                        style: TextStyle(
                          color: item.checked
                              ? FrankColors.aubergineAccent
                              : FrankColors.muted,
                          fontSize: 10,
                          fontWeight: FontWeight.w500,
                        ),
                      ),
                    ),
                  ],
                  if (item.hasSubmenu)
                    const Padding(
                      padding: EdgeInsets.only(left: 6),
                      child: Icon(
                        FrankIcons.chevronRight,
                        size: 15,
                        color: FrankColors.muted,
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  KeyEventResult _handleKey(KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      widget.onClose();
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.arrowDown) {
      _move(1);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.arrowUp) {
      _move(-1);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.home) {
      _moveTo(0);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.end) {
      _moveTo(_items.length - 1);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.space) {
      if (_items.isEmpty) return KeyEventResult.handled;
      final item = _items[_focusedIndex];
      if (item.isEnabled) {
        widget.onClose();
        item.onPressed?.call();
      }
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  void _move(int delta) {
    if (_items.isEmpty) return;
    var next = _focusedIndex;
    for (var attempts = 0; attempts < _items.length; attempts++) {
      next = (next + delta) % _items.length;
      if (next < 0) next += _items.length;
      if (_items[next].isEnabled) {
        _moveTo(next);
        return;
      }
    }
  }

  void _moveTo(int index) {
    if (_items.isEmpty) return;
    final clamped = index.clamp(0, _items.length - 1).toInt();
    if (_items[clamped].isEnabled) setState(() => _focusedIndex = clamped);
  }
}
