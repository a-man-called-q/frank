import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:forui/forui.dart';

import '../../../app/icons.dart';
import '../../../app/layout/office_surface_frame.dart';
import '../../../app/office_ui.dart';
import '../../../app/theme.dart';
import '../../../core/models/taskboard_models.dart';
import '../../../core/models/team_models.dart';
import '../../../core/models/workspace_models.dart';
import '../bloc/taskboard_bloc.dart';

part 'taskboard_board.dart';
part 'taskboard_list.dart';
part 'taskboard_inspector.dart';

typedef TaskboardAgentLookup = TeamAgentProfile? Function(String employeeId);

String? _taskboardFriendlyError(String? error, {required String fallback}) {
  if (error == null) return null;
  return frankFriendlyError(error, fallback: fallback);
}

/// Fixture-first taskboard for the Settings destination.
///
/// The surface owns presentation state through [TaskboardBloc]. It does not
/// read projects or missions from [ProjectsBloc], so filtering this board
/// cannot change the conversation context in Office.
class TaskboardSurface extends StatefulWidget {
  const TaskboardSurface({required this.workspace, this.profiles, super.key});

  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;

  @override
  State<TaskboardSurface> createState() => _TaskboardSurfaceState();
}

class _TaskboardSurfaceState extends State<TaskboardSurface> {
  final FocusNode _surfaceFocusNode = FocusNode(
    debugLabel: 'taskboard-surface',
  );
  final Map<String, FocusNode> _taskFocusNodes = <String, FocusNode>{};
  final Map<String, TextEditingController> _decisionControllers =
      <String, TextEditingController>{};
  final Map<String, TextEditingController> _commentControllers =
      <String, TextEditingController>{};
  Timer? _refreshTimer;
  StreamSubscription<void>? _eventSubscription;

  @override
  void initState() {
    super.initState();
    context.read<TaskboardBloc>().add(const TaskboardStarted());
    // Settings sections are mounted lazily, but the bloc survives section
    // switches. Ask for a fresh projection on every mount so reopening the
    // board does not wait for the 15-second recovery poll when no event was
    // emitted while it was hidden.
    context.read<TaskboardBloc>().add(const TaskboardRefreshRequested());
    // A bounded refresh keeps an authenticated desktop view current even
    // while the event-cursor transport is reconnecting. It is deliberately
    // owned by the surface and cancelled with it, so hidden settings pages do
    // not keep polling the daemon.
    _refreshTimer = Timer.periodic(const Duration(seconds: 15), (_) {
      if (mounted) {
        context.read<TaskboardBloc>().add(const TaskboardRefreshRequested());
      }
    });
    _eventSubscription = context.read<TaskboardBloc>().updates.listen((_) {
      if (mounted) {
        context.read<TaskboardBloc>().add(const TaskboardRefreshRequested());
      }
    });
  }

  @override
  void dispose() {
    _surfaceFocusNode.dispose();
    _refreshTimer?.cancel();
    _eventSubscription?.cancel();
    for (final node in _taskFocusNodes.values) {
      node.dispose();
    }
    for (final controller in _decisionControllers.values) {
      controller.dispose();
    }
    for (final controller in _commentControllers.values) {
      controller.dispose();
    }
    super.dispose();
  }

  FocusNode _focusNodeFor(String taskId) => _taskFocusNodes.putIfAbsent(
    taskId,
    () => FocusNode(debugLabel: 'taskboard-task-$taskId'),
  );

  TextEditingController _controllerFor(String taskId) =>
      _decisionControllers.putIfAbsent(taskId, TextEditingController.new);

  TextEditingController _commentControllerFor(String taskId) =>
      _commentControllers.putIfAbsent(taskId, TextEditingController.new);

  TeamAgentProfile? _profileFor(String employeeId) => widget.profiles
      ?.where((profile) => profile.employeeId == employeeId)
      .firstOrNull;

  void _closeDetail(String? taskId) {
    context.read<TaskboardBloc>().add(const TaskboardTaskSelected(null));
    if (taskId == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focusNodeFor(taskId).requestFocus();
    });
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final featureWidth =
            constraints.hasBoundedWidth && constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : double.infinity;
        final desktopPane =
            featureWidth >= OfficeInspectorDrawerOverlay.desktopBreakpoint;
        return BlocBuilder<TaskboardBloc, TaskboardState>(
          builder: (context, state) {
            final visibleTasks = state.visibleTasks;
            final selected = state.selectedTask;
            final detailVisible =
                state.loadStatus == TaskboardLoadStatus.ready &&
                selected != null &&
                visibleTasks.any((task) => task.id == selected.id);
            final inspector = detailVisible
                ? _TaskboardInspector(
                    task: selected,
                    profile: _profileFor(selected.agentId),
                    profiles: widget.profiles ?? const <TeamAgentProfile>[],
                    compact: !desktopPane,
                    controller: _controllerFor(selected.id),
                    commentController: _commentControllerFor(selected.id),
                    decisionStatus: state.decisionTaskId == selected.id
                        ? state.decisionStatus
                        : TaskboardDecisionStatus.idle,
                    decisionError: state.decisionTaskId == selected.id
                        ? _taskboardFriendlyError(
                            state.decisionError,
                            fallback: 'The decision could not be saved.',
                          )
                        : null,
                    mutationStatus: state.mutationTaskId == selected.id
                        ? state.mutationStatus
                        : TaskboardMutationStatus.idle,
                    mutationError: state.mutationTaskId == selected.id
                        ? _taskboardFriendlyError(
                            state.mutationError,
                            fallback: 'The task update could not be saved.',
                          )
                        : null,
                    onClose: () => _closeDetail(selected.id),
                    onSubmit: (input) => context.read<TaskboardBloc>().add(
                      TaskboardDecisionSubmitted(
                        taskId: selected.id,
                        decision: input,
                      ),
                    ),
                    onClaim: (agentId) => context.read<TaskboardBloc>().add(
                      TaskboardClaimRequested(
                        taskId: selected.id,
                        agentId: agentId,
                      ),
                    ),
                    onRelease: () => context.read<TaskboardBloc>().add(
                      TaskboardReleaseRequested(selected.id),
                    ),
                    onComment: (body) => context.read<TaskboardBloc>().add(
                      TaskboardCommentSubmitted(
                        taskId: selected.id,
                        body: body,
                      ),
                    ),
                  )
                : null;
            return Focus(
              focusNode: _surfaceFocusNode,
              autofocus: true,
              onKeyEvent: (_, event) {
                if (event is KeyDownEvent &&
                    event.logicalKey == LogicalKeyboardKey.escape &&
                    context.read<TaskboardBloc>().state.selectedTaskId !=
                        null) {
                  _closeDetail(
                    context.read<TaskboardBloc>().state.selectedTaskId,
                  );
                  return KeyEventResult.handled;
                }
                return KeyEventResult.ignored;
              },
              child: OfficeInspectorDrawerOverlay(
                child: OfficeSurfaceFrame.canvas(
                  backgroundColor: const Color(0x00000000),
                  header: OfficePageHeader(
                    title: 'Taskboard',
                    description: 'Work in motion, across your office.',
                    actions: state.snapshot?.isFixture == true
                        ? const FrankSampleDataBadge()
                        : null,
                  ),
                  child: Semantics(
                    container: true,
                    label: 'Taskboard',
                    child: Builder(
                      builder: (context) => _TaskboardContent(
                        workspace: widget.workspace,
                        profileFor: _profileFor,
                        state: state,
                        visibleTasks: visibleTasks,
                        compact:
                            OfficeLayoutMetricsScope.maybeOf(
                              context,
                            )?.isCompact ??
                            true,
                        onProjectChanged: (projectId) => context
                            .read<TaskboardBloc>()
                            .add(TaskboardProjectFilterChanged(projectId)),
                        onAttentionChanged: (enabled) => context
                            .read<TaskboardBloc>()
                            .add(TaskboardAttentionFilterChanged(enabled)),
                        onViewChanged: (view) => context
                            .read<TaskboardBloc>()
                            .add(TaskboardViewChanged(view)),
                        onRetry: () => context.read<TaskboardBloc>().add(
                          const TaskboardRetryRequested(),
                        ),
                        onClearFilters: () {
                          final bloc = context.read<TaskboardBloc>();
                          bloc.add(const TaskboardProjectFilterChanged(null));
                          bloc.add(
                            const TaskboardAttentionFilterChanged(false),
                          );
                        },
                        onSelectTask: (taskId) => context
                            .read<TaskboardBloc>()
                            .add(TaskboardTaskSelected(taskId)),
                        focusNodeFor: _focusNodeFor,
                      ),
                    ),
                  ),
                ),
                inspector: inspector,
                inspectorKey: const ValueKey('taskboard-inspector-rail'),
                inspectorLabel: 'Taskboard inspector',
              ),
            );
          },
        );
      },
    );
  }
}
