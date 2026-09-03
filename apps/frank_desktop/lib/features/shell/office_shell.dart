import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../app/theme.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/workspace_models.dart';
import '../chat/account_chat.dart';
import '../chat/bloc/chat_bloc.dart';
import '../floor/office_scene_floor.dart';
import '../ledger/ledger_surface.dart';
import '../organization/bloc/organization_bloc.dart';
import '../organization/organization_surface.dart';
import '../projects/bloc/projects_bloc.dart';
import '../projects/presentation/project_dialogs.dart';
import '../team/team_surface.dart';
import 'bloc/shell_bloc.dart';
import 'main_sidebar.dart';
import 'presentation/frank_desktop_menu.dart';
import 'presentation/shell_context_bar.dart';
import 'shortcut_registry.dart';
import 'sidebar_effect.dart';
import 'sidebar_layout.dart';
import 'window_chrome.dart';

/// Composition root for the remote-only desktop shell.
///
/// The gateway is the only dependency that crosses into the application. The
/// three feature blocs are kept independent; this widget is the sole place
/// where their lifecycle and cross-feature context are coordinated.
class OfficeShell extends StatelessWidget {
  const OfficeShell({
    required this.gateway,
    this.sidebarEffectBuilder,
    super.key,
  });

  final FrankGateway gateway;
  final SidebarEffectBuilder? sidebarEffectBuilder;

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      providers: [
        BlocProvider(
          create: (_) => ShellBloc(gateway: gateway)..add(const ShellStarted()),
        ),
        BlocProvider(create: (_) => ProjectsBloc()),
        BlocProvider(create: (_) => ChatBloc(gateway: gateway)),
        BlocProvider(create: (_) => OrganizationBloc(gateway: gateway)),
      ],
      child: _OfficeCoordinator(sidebarEffectBuilder: sidebarEffectBuilder),
    );
  }
}

class _OfficeCoordinator extends StatelessWidget {
  const _OfficeCoordinator({required this.sidebarEffectBuilder});

  final SidebarEffectBuilder? sidebarEffectBuilder;

  @override
  Widget build(BuildContext context) {
    return MultiBlocListener(
      listeners: [
        BlocListener<ShellBloc, ShellState>(
          listenWhen: (previous, current) =>
              previous.workspace != current.workspace &&
              current.workspace != null,
          listener: (context, state) {
            final workspace = state.workspace;
            if (workspace == null) return;
            context.read<ProjectsBloc>().add(ProjectsInitialized(workspace));
            context.read<ChatBloc>().add(ChatInitialized(workspace));
            // Organization is loaded only when its section is active. The
            // default Office destination is Organization, so starting it here
            // avoids a visible second loading pass after the workspace shell
            // has already appeared while keeping Team/Ledger/etc. lazy.
            if (state.officeSection == OfficeSection.organization) {
              context.read<OrganizationBloc>().add(const OrganizationStarted());
            }
          },
        ),
        BlocListener<ShellBloc, ShellState>(
          listenWhen: (previous, current) {
            final destinationChanged =
                previous.destination.runtimeType !=
                current.destination.runtimeType;
            final officeSectionChanged = switch ((
              previous.destination,
              current.destination,
            )) {
              (
                OfficeDestination(section: final previousSection),
                OfficeDestination(section: final currentSection),
              ) =>
                previousSection != currentSection,
              _ => false,
            };
            return destinationChanged || officeSectionChanged;
          },
          listener: _syncChatContext,
        ),
        BlocListener<ShellBloc, ShellState>(
          // Organization is intentionally lazy: Team, Ledger, Taskboard,
          // and Journal should not initialize its graph until the section is
          // actually opened. The workspace listener above covers the default
          // destination; this listener covers a later navigation into it.
          listenWhen: (previous, current) =>
              previous.officeSection != OfficeSection.organization &&
              current.officeSection == OfficeSection.organization &&
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
      child: _OfficeShellBody(sidebarEffectBuilder: sidebarEffectBuilder),
    );
  }

  void _syncChatContext(BuildContext context, Object _) {
    final shell = context.read<ShellBloc>().state;
    final projects = context.read<ProjectsBloc>().state;
    final project = projects.projectById(projects.selectedProjectId);
    final mission = projects.missionById(project, projects.selectedMissionId);
    final conversation = switch (shell.destination) {
      OfficeDestination() => null,
      ProjectsDestination() =>
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
  const _OfficeShellBody({required this.sidebarEffectBuilder});

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
    return FrankDesktopMenuDismissScope(child: _buildBody(context, shell));
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
    return Scaffold(
      backgroundColor: nativeSidebarEffect
          ? Colors.transparent
          : FrankColors.canvas,
      body: SafeArea(
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
                            const SizedBox(
                              key: ValueKey('main-context-strip'),
                              height: ShellContextBar.height,
                              child: ColoredBox(color: FrankColors.canvas),
                            ),
                            Expanded(
                              child: ColoredBox(
                                key: const ValueKey('main-surface-background'),
                                color: nativeSidebarEffect
                                    ? FrankColors.canvas
                                    : Colors.transparent,
                                child: Stack(
                                  fit: StackFit.expand,
                                  children: [
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
                    project: shell.activeView == WorkspaceView.projects
                        ? selectedProject
                        : null,
                    mission: shell.activeView == WorkspaceView.projects
                        ? selectedMission
                        : null,
                    sidebarVisible: shell.sidebarVisible,
                    sidebarWidth: shell.sidebarWidth,
                    isFullscreen: _windowChrome.isFullscreen,
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
    if (shell.destination is! ProjectsDestination) return null;
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
      shell.destination is ProjectsDestination
      ? OfficeSceneActivity.static
      : OfficeSceneActivity.paused;

  double _sceneBlur(ShellState shell) => switch (shell.destination) {
    ProjectsDestination() => 0,
    OfficeDestination(section: OfficeSection.organization) => 10,
    OfficeDestination() => 12,
  };

  Color _sceneScrim(ShellState shell) => switch (shell.destination) {
    ProjectsDestination() => Colors.transparent,
    OfficeDestination(section: OfficeSection.organization) =>
      FrankColors.canvas.withValues(alpha: .72),
    OfficeDestination() => FrankColors.canvas.withValues(alpha: .28),
  };

  void _focusSearch(BuildContext context) {
    final shell = context.read<ShellBloc>();
    if (shell.state.activeView != WorkspaceView.projects) {
      shell.add(const ShellViewSelected(WorkspaceView.projects));
      context.read<ProjectsBloc>().add(const ProjectsViewEntered());
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
                    : Colors.transparent,
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

class _MainSurface extends StatelessWidget {
  const _MainSurface({
    required this.workspace,
    required this.project,
    required this.mission,
    required this.conversation,
    required this.messages,
    required this.generating,
    required this.sceneController,
    super.key,
  });

  final OfficeWorkspace workspace;
  final OfficeProject? project;
  final OfficeMission? mission;
  final ConversationContext? conversation;
  final List<OfficeMessage> messages;
  final bool generating;
  final OfficeSceneController sceneController;

  @override
  Widget build(BuildContext context) {
    final shell = context.watch<ShellBloc>().state;
    if (shell.destination case OfficeDestination(:final section)) {
      return _OfficeSectionSurface(section: section, workspace: workspace);
    }
    if (project == null || conversation == null) {
      return const NoProjectSurface();
    }

    return AccountExecutiveChat(
      executive: workspace.accountExecutive,
      project: project!,
      mission: shell.activeView == WorkspaceView.projects ? mission : null,
      officeView: shell.activeView == WorkspaceView.office,
      messages: messages,
      generating: generating,
      // The shell owns one retained floor for every destination. Keeping the
      // chat rail floor-free prevents a project switch from replacing the
      // scene/controller that carries the camera state.
      renderFloor: false,
      // Reused so the floor-reset button (rendered beside the composer) acts
      // on the same camera the shell's shared floor is displaying.
      sceneController: sceneController,
      onSend: (text) => context.read<ChatBloc>().add(
        ChatMessageSubmitted(context: conversation!, text: text),
      ),
      onStop: () =>
          context.read<ChatBloc>().add(const ChatMessageStopRequested()),
    );
  }
}

class NoProjectSurface extends StatelessWidget {
  const NoProjectSurface({super.key});

  @override
  Widget build(BuildContext context) {
    return const ColoredBox(
      color: FrankColors.canvas,
      child: Center(
        child: Text(
          'No projects yet',
          style: TextStyle(color: FrankColors.muted, fontSize: 18),
        ),
      ),
    );
  }
}

class _OfficeSectionSurface extends StatelessWidget {
  const _OfficeSectionSurface({required this.section, required this.workspace});

  final OfficeSection section;
  final OfficeWorkspace workspace;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: '${section.label} office section',
      child: LayoutBuilder(
        builder: (context, constraints) {
          if (section == OfficeSection.organization) {
            return KeyedSubtree(
              key: const ValueKey('office-section-surface-organization'),
              child: ColoredBox(
                key: const ValueKey('office-section-content-host'),
                color: FrankColors.canvas.withValues(alpha: .18),
                child: AnimatedSwitcher(
                  key: const ValueKey('office-section-content-switcher'),
                  duration:
                      (MediaQuery.maybeOf(context)?.disableAnimations ?? false)
                      ? Duration.zero
                      : const Duration(milliseconds: 180),
                  reverseDuration:
                      (MediaQuery.maybeOf(context)?.disableAnimations ?? false)
                      ? Duration.zero
                      : const Duration(milliseconds: 180),
                  child: KeyedSubtree(
                    key: const ValueKey('office-section-content-organization'),
                    child: OrganizationSurface(workspace: workspace),
                  ),
                ),
              ),
            );
          }
          final compact =
              (constraints.maxWidth.isFinite && constraints.maxWidth < 700) ||
              (constraints.maxHeight.isFinite && constraints.maxHeight < 560);
          final contentPadding = compact ? 18.0 : 32.0;
          final motionDisabled =
              MediaQuery.maybeOf(context)?.disableAnimations ?? false;
          final contentMaxWidth =
              section == OfficeSection.team || section == OfficeSection.ledger
              ? 1240.0
              : 760.0;

          return KeyedSubtree(
            key: ValueKey('office-section-surface-${section.name}'),
            child: ColoredBox(
              key: const ValueKey('office-section-content-host'),
              color: FrankColors.panel.withValues(alpha: 0.84),
              child: AnimatedSwitcher(
                key: const ValueKey('office-section-content-switcher'),
                duration: motionDisabled
                    ? Duration.zero
                    : const Duration(milliseconds: 180),
                reverseDuration: motionDisabled
                    ? Duration.zero
                    : const Duration(milliseconds: 180),
                switchInCurve: Curves.easeOutCubic,
                switchOutCurve: Curves.easeInCubic,
                transitionBuilder: (child, animation) {
                  return FadeTransition(
                    opacity: animation,
                    child: AnimatedBuilder(
                      animation: animation,
                      child: child,
                      builder: (context, child) => Transform.translate(
                        offset: Offset(0, 8 * (1 - animation.value)),
                        child: child,
                      ),
                    ),
                  );
                },
                child: SingleChildScrollView(
                  key: ValueKey('office-section-content-${section.name}'),
                  padding: EdgeInsets.all(contentPadding),
                  child: Center(
                    child: ConstrainedBox(
                      constraints: BoxConstraints(maxWidth: contentMaxWidth),
                      child: section == OfficeSection.team
                          ? TeamSurface(workspace: workspace)
                          : section == OfficeSection.ledger
                          ? LedgerSurface(workspace: workspace)
                          : Column(
                              mainAxisSize: MainAxisSize.min,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  section.label,
                                  style: const TextStyle(
                                    color: FrankColors.ink,
                                    fontSize: 22,
                                    fontWeight: FontWeight.w500,
                                  ),
                                ),
                                const SizedBox(height: 8),
                                Text(
                                  key: ValueKey(
                                    'office-section-description-${section.name}',
                                  ),
                                  section.description,
                                  style: const TextStyle(
                                    color: FrankColors.muted,
                                    fontSize: 13,
                                    height: 1.45,
                                  ),
                                ),
                              ],
                            ),
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
