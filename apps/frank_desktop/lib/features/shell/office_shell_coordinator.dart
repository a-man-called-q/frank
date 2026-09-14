part of 'office_shell.dart';

class _OfficeCoordinator extends StatelessWidget {
  const _OfficeCoordinator({
    this.authRepository,
    this.showDemoBanner = false,
    this.onLogout,
    this.onLogoutAll,
    this.onChangePassword,
    required this.sidebarEffectBuilder,
    this.onStatusChanged,
  });

  final SidebarEffectBuilder? sidebarEffectBuilder;
  final ValueChanged<ShellLoadStatus>? onStatusChanged;
  final AuthRepository? authRepository;
  final bool showDemoBanner;
  final Future<void> Function()? onLogout;
  final Future<void> Function()? onLogoutAll;
  final Future<void> Function(String currentPassword, String newPassword)?
  onChangePassword;

  @override
  Widget build(BuildContext context) {
    return MultiBlocListener(
      listeners: [
        BlocListener<ShellBloc, ShellState>(
          listenWhen: (previous, current) => previous.status != current.status,
          listener: (_, state) => onStatusChanged?.call(state.status),
        ),
        BlocListener<ShellBloc, ShellState>(
          listenWhen: (previous, current) =>
              previous.workspace != current.workspace &&
              current.workspace != null,
          listener: (context, state) {
            final workspace = state.workspace;
            if (workspace == null) return;
            context.read<ProjectsBloc>().add(ProjectsInitialized(workspace));
            context.read<ChatBloc>().add(ChatInitialized(workspace));
            // Organization is loaded only when its Settings section is active.
            // Office is the default destination, so the graph remains lazy at
            // startup and is fetched only when Settings → Organization opens.
            if (state.settingsSection == SettingsSection.organization) {
              context.read<OrganizationBloc>().add(const OrganizationStarted());
            }
          },
        ),
        BlocListener<ShellBloc, ShellState>(
          listenWhen: (previous, current) {
            final destinationChanged =
                previous.destination.runtimeType !=
                current.destination.runtimeType;
            final settingsSectionChanged = switch ((
              previous.destination,
              current.destination,
            )) {
              (
                SettingsDestination(section: final previousSection),
                SettingsDestination(section: final currentSection),
              ) =>
                previousSection != currentSection,
              _ => false,
            };
            return destinationChanged || settingsSectionChanged;
          },
          listener: _syncChatContext,
        ),
        BlocListener<ShellBloc, ShellState>(
          // Organization is intentionally lazy: Team, Ledger, Taskboard,
          // and Journal should not initialize its graph until the section is
          // actually opened. The workspace listener above covers a Settings
          // load that already targets Organization; this covers later entry.
          listenWhen: (previous, current) =>
              previous.settingsSection != SettingsSection.organization &&
              current.settingsSection == SettingsSection.organization &&
              current.workspace != null,
          listener: (context, _) =>
              context.read<OrganizationBloc>().add(const OrganizationStarted()),
        ),
        BlocListener<ProjectsBloc, ProjectsState>(
          listenWhen: (previous, current) =>
              previous.status != current.status ||
              previous.selectedProjectId != current.selectedProjectId ||
              previous.selectedMissionId != current.selectedMissionId,
          listener: _syncChatContext,
        ),
      ],
      child: _OfficeShellBody(
        authRepository: authRepository,
        showDemoBanner: showDemoBanner,
        onLogout: onLogout,
        onLogoutAll: onLogoutAll,
        onChangePassword: onChangePassword,
        sidebarEffectBuilder: sidebarEffectBuilder,
      ),
    );
  }

  void _syncChatContext(BuildContext context, Object _) {
    final shell = context.read<ShellBloc>().state;
    final projects = context.read<ProjectsBloc>().state;
    final project = projects.projectById(projects.selectedProjectId);
    final mission = projects.missionById(project, projects.selectedMissionId);
    final conversation = switch (shell.destination) {
      SettingsDestination() => null,
      OfficeDestination() =>
        project == null
            ? null
            : mission == null
            ? ProjectConversationContext(projectId: project.id)
            : MissionConversationContext(
                projectId: project.id,
                missionId: mission.id,
              ),
    };
    context.read<ChatBloc>().add(ChatContextChanged(conversation));
  }
}

class _OfficeShellBody extends StatefulWidget {
  const _OfficeShellBody({
    this.authRepository,
    this.showDemoBanner = false,
    this.onLogout,
    this.onLogoutAll,
    this.onChangePassword,
    required this.sidebarEffectBuilder,
  });

  final AuthRepository? authRepository;
  final bool showDemoBanner;
  final Future<void> Function()? onLogout;
  final Future<void> Function()? onLogoutAll;
  final Future<void> Function(String currentPassword, String newPassword)?
  onChangePassword;
  final SidebarEffectBuilder? sidebarEffectBuilder;

  @override
  State<_OfficeShellBody> createState() => _OfficeShellBodyState();
}

class _OfficeShellBodyState extends State<_OfficeShellBody> {
  late final WindowChromeState _windowChrome;
  late final OfficeSceneController _sceneController = OfficeSceneController();
  late final FocusNode _searchFocusNode = FocusNode(
    debugLabel: 'workspace-search',
  );

  @override
  void initState() {
    super.initState();
    _windowChrome = WindowChromeState()..addListener(_onWindowChromeChanged);
    unawaited(_windowChrome.attach());
    HardwareKeyboard.instance.addHandler(_handleGlobalKey);
  }

  void _onWindowChromeChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_handleGlobalKey);
    _windowChrome.removeListener(_onWindowChromeChanged);
    _windowChrome.dispose();
    _sceneController.dispose();
    _searchFocusNode.dispose();
    super.dispose();
  }

  bool _handleGlobalKey(KeyEvent event) {
    if (event is! KeyDownEvent) return false;
    final keyboard = HardwareKeyboard.instance;
    if (ShellShortcutRegistry.matchesToggleSidebar(event, keyboard)) {
      _toggleSidebar(context);
      return true;
    }
    if (ShellShortcutRegistry.matchesOpenSearch(event, keyboard)) {
      _focusSearch(context);
      return true;
    }
    return false;
  }

  @override
  Widget build(BuildContext context) {
    final shell = context.watch<ShellBloc>().state;
    return _buildBody(context, shell);
  }

  Widget _buildBody(BuildContext context, ShellState shell) {
    if (shell.isLoading) return const LoadingShell();
    if (shell.hasError || shell.workspace == null) {
      return ErrorShell(
        error: shell.error ?? 'Workspace unavailable',
        onRetry: () =>
            context.read<ShellBloc>().add(const ShellRetryRequested()),
      );
    }

    final workspace = shell.workspace!;
    final projects = context.watch<ProjectsBloc>().state;
    final chat = context.watch<ChatBloc>().state;
    final selectedProject = projects.projectById(projects.selectedProjectId);
    final selectedMission = projects.missionById(
      selectedProject,
      projects.selectedMissionId,
    );
    final conversation = _conversationFor(
      shell,
      selectedProject,
      selectedMission,
    );
    final generating = chat.generating && chat.pendingContext == conversation;

    final nativeSidebarEffect = widget.sidebarEffectBuilder != null;
    return FScaffold(
      childPad: false,
      scaffoldStyle: FScaffoldStyleDelta.delta(
        backgroundColor: nativeSidebarEffect
            ? const Color(0x00000000)
            : FrankColors.canvas,
      ),
      child: SafeArea(
        top: false,
        left: false,
        right: false,
        bottom: false,
        child: LayoutBuilder(
          builder: (context, _) {
            return Stack(
              fit: StackFit.expand,
              children: [
                Positioned.fill(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _SidebarSlot(
                        visible: shell.sidebarVisible,
                        width: shell.sidebarWidth,
                        sidebarEffectBuilder: widget.sidebarEffectBuilder,
                        onResizeEnd: (rawWidth) => context
                            .read<ShellBloc>()
                            .add(ShellSidebarResizeEnded(rawWidth)),
                        childBuilder: (width) => MainSidebar(
                          authRepository: widget.authRepository,
                          showDemoBanner: widget.showDemoBanner,
                          onLogout: widget.onLogout,
                          onLogoutAll: widget.onLogoutAll,
                          onChangePassword: widget.onChangePassword,
                          width: width,
                          nativeSidebarEffect:
                              widget.sidebarEffectBuilder != null,
                          searchFocusNode: _searchFocusNode,
                          isFullscreen: _windowChrome.isFullscreen,
                        ),
                      ),
                      Expanded(
                        child: Column(
                          children: [
                            Builder(
                              builder: (context) {
                                final contextStrip = SizedBox(
                                  key: const ValueKey('main-context-strip'),
                                  height: ShellContextBar.height,
                                  child: ColoredBox(
                                    color: nativeSidebarEffect
                                        ? FrankColors.sidebarGlass
                                        : FrankColors.sidebarSolid,
                                  ),
                                );
                                return contextStrip;
                              },
                            ),
                            Expanded(
                              child: ColoredBox(
                                key: const ValueKey('main-surface-background'),
                                color: nativeSidebarEffect
                                    ? FrankColors.canvas
                                    : const Color(0x00000000),
                                child: Stack(
                                  fit: StackFit.expand,
                                  children: [
                                    const Positioned.fill(
                                      child: FrankBackdrop(
                                        child: SizedBox.expand(),
                                      ),
                                    ),
                                    Positioned.fill(
                                      child: OfficeSceneFloor(
                                        key: const ValueKey(
                                          'shared-office-scene-floor',
                                        ),
                                        controller: _sceneController,
                                        activity: _sceneActivity(shell),
                                        blurSigma: _sceneBlur(shell),
                                        scrimColor: _sceneScrim(shell),
                                      ),
                                    ),
                                    Positioned.fill(
                                      child: _MainSurface(
                                        key: const ValueKey('main-surface'),
                                        workspace: workspace,
                                        project: selectedProject,
                                        mission: selectedMission,
                                        conversation: conversation,
                                        messages: chat.messagesFor(
                                          conversation,
                                        ),
                                        generating: generating,
                                        sceneController: _sceneController,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
                Positioned(
                  top: 0,
                  left: 0,
                  right: 0,
                  height: ShellContextBar.height,
                  child: ShellContextBar(
                    workspace: workspace,
                    project: shell.activeView == WorkspaceView.office
                        ? selectedProject
                        : null,
                    mission: shell.activeView == WorkspaceView.office
                        ? selectedMission
                        : null,
                    sidebarVisible: shell.sidebarVisible,
                    sidebarWidth: shell.sidebarWidth,
                    isFullscreen: _windowChrome.isFullscreen,
                    connectionStatus:
                        context.watch<ConnectionBloc>().state.status,
                    isFixture:
                        widget.showDemoBanner || context.read<FrankGateway>().isFixture,
                    onToggleSidebar: () {
                      context.read<ShellBloc>().add(
                        const ShellSidebarToggled(),
                      );
                    },
                    onDoubleTap: () => unawaited(_windowChrome.toggleZoom()),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }

  ConversationContext? _conversationFor(
    ShellState shell,
    OfficeProject? project,
    OfficeMission? mission,
  ) {
    if (shell.destination is! OfficeDestination) return null;
    if (project == null) return null;
    if (mission != null) {
      return MissionConversationContext(
        projectId: project.id,
        missionId: mission.id,
      );
    }
    return ProjectConversationContext(projectId: project.id);
  }

  OfficeSceneActivity _sceneActivity(ShellState shell) =>
      shell.destination is OfficeDestination
      ? OfficeSceneActivity.static
      : OfficeSceneActivity.paused;

  double _sceneBlur(ShellState shell) => switch (shell.destination) {
    OfficeDestination() => 0,
    SettingsDestination(section: _) => 12,
  };

  Color _sceneScrim(ShellState shell) => switch (shell.destination) {
    OfficeDestination() => const Color(0x00000000),
    SettingsDestination(section: _) => FrankColors.canvas.withValues(
      alpha: .94,
    ),
  };

  void _focusSearch(BuildContext context) {
    final shell = context.read<ShellBloc>();
    if (shell.state.activeView != WorkspaceView.office) {
      shell.add(const ShellViewSelected(WorkspaceView.office));
      context.read<ProjectsBloc>().add(const OfficeViewEntered());
    }
    if (!shell.state.sidebarVisible) {
      shell.add(const ShellSidebarToggled());
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _searchFocusNode.requestFocus();
    });
  }

  void _toggleSidebar(BuildContext context) {
    context.read<ShellBloc>().add(const ShellSidebarToggled());
  }
}

class _SidebarSlot extends StatefulWidget {
  const _SidebarSlot({
    required this.visible,
    required this.width,
    required this.onResizeEnd,
    required this.childBuilder,
    this.sidebarEffectBuilder,
  });

  final bool visible;
  final double width;
  final ValueChanged<double> onResizeEnd;
  final Widget Function(double width) childBuilder;
  final SidebarEffectBuilder? sidebarEffectBuilder;

  @override
  State<_SidebarSlot> createState() => _SidebarSlotState();
}

class _SidebarSlotState extends State<_SidebarSlot>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;
  late Tween<double> _widthTween;
  late double _targetContentWidth;
  late final FocusNode _handleFocusNode = FocusNode(
    debugLabel: 'sidebar-resize-handle',
  );
  double _dragOriginWidth = 0;
  double _dragOriginX = 0;
  double? _pointerDownX;
  double _previewWidth = 0;
  bool _dragging = false;
  bool _handleHovered = false;

  @override
  void initState() {
    super.initState();
    _targetContentWidth = SidebarLayout.normalizeWidth(widget.width);
    final initialWidth = widget.visible ? _targetContentWidth : 0.0;
    _widthTween = Tween(begin: initialWidth, end: initialWidth);
    _controller = AnimationController(
      vsync: this,
      duration: SidebarLayout.animationDuration,
      value: 1,
    );
  }

  @override
  void didUpdateWidget(covariant _SidebarSlot oldWidget) {
    super.didUpdateWidget(oldWidget);
    final width = SidebarLayout.normalizeWidth(widget.width);
    final visibilityChanged = oldWidget.visible != widget.visible;
    final widthChanged = width != _targetContentWidth;
    if (!visibilityChanged && !widthChanged) return;

    final from = _currentOuterWidth;
    _targetContentWidth = width;
    _startTransition(from, widget.visible ? width : 0);
  }

  @override
  void dispose() {
    _handleFocusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, _) {
        final visibleWidth = _dragging
            ? _previewWidth
            : _widthTween.transform(
                SidebarLayout.animationCurve.transform(_controller.value),
              );
        final visibleContentWidth = _dragging
            ? SidebarLayout.contentWidthForPreview(_previewWidth)
            : _targetContentWidth;
        final sidebar = SizedBox(
          key: const ValueKey('sidebar-slot'),
          width: visibleWidth,
          child: IgnorePointer(
            // OverflowBox keeps the child laid out at the 240px minimum, but
            // it must not intercept the context bar once the viewport closes.
            ignoring: visibleWidth <= 0,
            child: ClipRect(
              child: OverflowBox(
                alignment: Alignment.centerLeft,
                minWidth: visibleContentWidth,
                maxWidth: visibleContentWidth,
                child: Stack(
                  clipBehavior: Clip.none,
                  children: [
                    SizedBox(
                      width: visibleContentWidth,
                      child: widget.childBuilder(visibleContentWidth),
                    ),
                    if (visibleWidth > 0)
                      Positioned(
                        top: 0,
                        // Keep the handle at the visible divider while the
                        // child remains laid out at its 240px minimum.
                        right: visibleContentWidth - visibleWidth,
                        bottom: 0,
                        width: SidebarLayout.resizeHandleWidth,
                        child: _resizeHandle(visibleWidth),
                      ),
                  ],
                ),
              ),
            ),
          ),
        );
        return widget.sidebarEffectBuilder?.call(sidebar) ?? sidebar;
      },
    );
  }

  Widget _resizeHandle(double visibleWidth) {
    final active = _dragging || _handleHovered;
    final currentWidth = _dragging ? _previewWidth : visibleWidth;
    final increasedWidth = SidebarLayout.clampPreview(
      currentWidth + SidebarLayout.resizeStep,
    );
    final decreasedWidth = SidebarLayout.clampPreview(
      currentWidth - SidebarLayout.resizeStep,
    );
    return Semantics(
      container: true,
      label: 'Resize sidebar',
      value: '${currentWidth.round()} pixels',
      increasedValue: '${increasedWidth.round()} pixels',
      decreasedValue: '${decreasedWidth.round()} pixels',
      focusable: true,
      onIncrease: () => _resizeFromKeyboard(SidebarLayout.resizeStep),
      onDecrease: () => _resizeFromKeyboard(-SidebarLayout.resizeStep),
      child: Focus(
        focusNode: _handleFocusNode,
        canRequestFocus: true,
        onKeyEvent: (_, event) {
          if (event is! KeyDownEvent) return KeyEventResult.ignored;
          if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
            _resizeFromKeyboard(SidebarLayout.resizeStep);
            return KeyEventResult.handled;
          }
          if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
            _resizeFromKeyboard(-SidebarLayout.resizeStep);
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: MouseRegion(
          cursor: SystemMouseCursors.resizeColumn,
          onEnter: (_) => setState(() => _handleHovered = true),
          onExit: (_) => setState(() => _handleHovered = false),
          child: Listener(
            behavior: HitTestBehavior.opaque,
            onPointerDown: (event) {
              _pointerDownX = event.position.dx;
            },
            child: GestureDetector(
              behavior: HitTestBehavior.opaque,
              onHorizontalDragStart: (details) {
                _handleFocusNode.requestFocus();
                _controller.stop();
                _dragOriginWidth = _currentOuterWidth;
                _dragOriginX = _pointerDownX ?? details.globalPosition.dx;
                _previewWidth = _dragOriginWidth;
                setState(() => _dragging = true);
              },
              onHorizontalDragUpdate: (details) {
                final raw =
                    _dragOriginWidth + details.globalPosition.dx - _dragOriginX;
                setState(() {
                  _previewWidth = SidebarLayout.clampPreview(raw);
                });
              },
              onHorizontalDragEnd: (_) {
                final raw = _previewWidth;
                // Seed the next transition from the exact pointer position so a
                // snap or collapse never jumps before the BLoC commit arrives.
                setState(() {
                  _dragging = false;
                  _widthTween = Tween(begin: raw, end: raw);
                  _controller.value = 1;
                });
                _pointerDownX = null;
                widget.onResizeEnd(raw);
              },
              onHorizontalDragCancel: _cancelResize,
              onTap: () {
                _pointerDownX = null;
                _handleFocusNode.requestFocus();
              },
              child: ColoredBox(
                color: active
                    ? FrankColors.border.withValues(alpha: 0.9)
                    : const Color(0x00000000),
              ),
            ),
          ),
        ),
      ),
    );
  }

  void _cancelResize() {
    if (!_dragging) return;
    final from = _previewWidth;
    setState(() {
      _dragging = false;
      _pointerDownX = null;
    });
    _startTransition(from, widget.visible ? _targetContentWidth : 0);
  }

  void _resizeFromKeyboard(double delta) {
    final raw = SidebarLayout.clampPreview(_targetContentWidth + delta);
    widget.onResizeEnd(raw);
  }

  double get _currentOuterWidth => _widthTween
      .transform(SidebarLayout.animationCurve.transform(_controller.value))
      .clamp(0, SidebarLayout.maxWidth)
      .toDouble();

  void _startTransition(double from, double to) {
    _widthTween = Tween(begin: from, end: to);
    if (MediaQuery.maybeOf(context)?.disableAnimations ?? false) {
      _controller.value = 1;
      return;
    }
    _controller.forward(from: 0);
  }
}
