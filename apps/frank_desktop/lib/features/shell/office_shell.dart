import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../app/theme.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/workspace_models.dart';
import '../chat/account_chat.dart';
import '../chat/bloc/chat_bloc.dart';
import '../projects/bloc/projects_bloc.dart';
import '../projects/presentation/project_dialogs.dart';
import '../settings/settings_surface.dart';
import 'bloc/shell_bloc.dart';
import 'main_sidebar.dart';
import 'presentation/shell_context_bar.dart';
import 'shortcut_registry.dart';
import 'sidebar_layout.dart';
import 'window_chrome.dart';

/// Composition root for the remote-only desktop shell.
///
/// The gateway is the only dependency that crosses into the application. The
/// three feature blocs are kept independent; this widget is the sole place
/// where their lifecycle and cross-feature context are coordinated.
class OfficeShell extends StatelessWidget {
  const OfficeShell({required this.gateway, super.key});

  final FrankGateway gateway;

  @override
  Widget build(BuildContext context) {
    return MultiBlocProvider(
      providers: [
        BlocProvider(
          create: (_) => ShellBloc(gateway: gateway)..add(const ShellStarted()),
        ),
        BlocProvider(create: (_) => ProjectsBloc()),
        BlocProvider(create: (_) => ChatBloc(gateway: gateway)),
      ],
      child: const _OfficeCoordinator(),
    );
  }
}

class _OfficeCoordinator extends StatelessWidget {
  const _OfficeCoordinator();

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
          },
        ),
        BlocListener<ShellBloc, ShellState>(
          listenWhen: (previous, current) =>
              previous.destination.runtimeType !=
              current.destination.runtimeType,
          listener: _syncChatContext,
        ),
        BlocListener<ProjectsBloc, ProjectsState>(
          listenWhen: (previous, current) =>
              previous.status != current.status ||
              previous.selectedProjectId != current.selectedProjectId ||
              previous.selectedMissionId != current.selectedMissionId,
          listener: _syncChatContext,
        ),
      ],
      child: const _OfficeShellBody(),
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
            : ProjectConversationContext(projectId: project.id),
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
  const _OfficeShellBody();

  @override
  State<_OfficeShellBody> createState() => _OfficeShellBodyState();
}

class _OfficeShellBodyState extends State<_OfficeShellBody> {
  late final WindowChromeState _windowChrome;
  late final FocusNode _searchFocusNode = FocusNode(
    debugLabel: 'workspace-search',
  );

  @override
  void initState() {
    super.initState();
    _windowChrome = WindowChromeState()..addListener(_onWindowChromeChanged);
    unawaited(_windowChrome.attach());
  }

  void _onWindowChromeChanged() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _windowChrome.removeListener(_onWindowChromeChanged);
    _windowChrome.dispose();
    _searchFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final shell = context.watch<ShellBloc>().state;
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

    return CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        ShellShortcutRegistry.openSearchMac: () => _focusSearch(context),
        ShellShortcutRegistry.openSearchControl: () => _focusSearch(context),
        ShellShortcutRegistry.toggleSidebarMac: () => _toggleSidebar(context),
        ShellShortcutRegistry.toggleSidebarControl: () =>
            _toggleSidebar(context),
      },
      child: Focus(
        autofocus: true,
        onKeyEvent: (_, event) => _handleShellKey(context, event),
        child: Scaffold(
          body: SafeArea(
            top: false,
            left: false,
            right: false,
            bottom: false,
            child: LayoutBuilder(
              builder: (context, _) {
                final mainSurface = Column(
                  children: [
                    ShellContextBar(
                      workspace: workspace,
                      project: selectedProject,
                      mission: selectedMission,
                      sidebarVisible: shell.sidebarVisible,
                      isFullscreen: _windowChrome.isFullscreen,
                      onToggleSidebar: () {
                        context.read<ShellBloc>().add(
                          const ShellSidebarToggled(),
                        );
                      },
                      onDoubleTap: () => unawaited(_windowChrome.toggleZoom()),
                    ),
                    Expanded(
                      child: _MainSurface(
                        key: const ValueKey('main-surface'),
                        workspace: workspace,
                        project: selectedProject,
                        mission: selectedMission,
                        conversation: conversation,
                        messages: chat.messagesFor(conversation),
                        generating: generating,
                      ),
                    ),
                  ],
                );

                return Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    _SidebarSlot(
                      visible: shell.sidebarVisible,
                      width: shell.sidebarWidth,
                      onResizeEnd: (rawWidth) => context.read<ShellBloc>().add(
                        ShellSidebarResizeEnded(rawWidth),
                      ),
                      childBuilder: (width) => MainSidebar(
                        width: width,
                        searchFocusNode: _searchFocusNode,
                        isFullscreen: _windowChrome.isFullscreen,
                      ),
                    ),
                    Expanded(child: mainSurface),
                  ],
                );
              },
            ),
          ),
        ),
      ),
    );
  }

  ConversationContext? _conversationFor(
    ShellState shell,
    OfficeProject? project,
    OfficeMission? mission,
  ) {
    if (project == null || shell.settingsSection != null) return null;
    if (shell.activeView == WorkspaceView.projects && mission != null) {
      return MissionConversationContext(
        projectId: project.id,
        missionId: mission.id,
      );
    }
    return ProjectConversationContext(projectId: project.id);
  }

  void _focusSearch(BuildContext context) {
    final shell = context.read<ShellBloc>();
    if (shell.state.settingsSection != null) {
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

  KeyEventResult _handleShellKey(BuildContext context, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final keyboard = HardwareKeyboard.instance;
    if (event.logicalKey == LogicalKeyboardKey.keyB &&
        (keyboard.isControlPressed || keyboard.isMetaPressed)) {
      _toggleSidebar(context);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.keyK &&
        (keyboard.isControlPressed || keyboard.isMetaPressed)) {
      _focusSearch(context);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }
}

class _SidebarSlot extends StatefulWidget {
  const _SidebarSlot({
    required this.visible,
    required this.width,
    required this.onResizeEnd,
    required this.childBuilder,
  });

  final bool visible;
  final double width;
  final ValueChanged<double> onResizeEnd;
  final Widget Function(double width) childBuilder;

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
        return SizedBox(
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
    super.key,
  });

  final OfficeWorkspace workspace;
  final OfficeProject? project;
  final OfficeMission? mission;
  final ConversationContext? conversation;
  final List<OfficeMessage> messages;
  final bool generating;

  @override
  Widget build(BuildContext context) {
    final shell = context.watch<ShellBloc>().state;
    final settingsSection = shell.settingsSection;
    if (settingsSection != null) {
      return SettingsSurface(section: settingsSection, workspace: workspace);
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
      onSend: (text) => context.read<ChatBloc>().add(
        ChatMessageSubmitted(context: conversation!, text: text),
      ),
      onStop: () =>
          context.read<ChatBloc>().add(const ChatMessageStopRequested()),
    );
  }
}
