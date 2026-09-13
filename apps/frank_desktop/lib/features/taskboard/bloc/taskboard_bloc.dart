import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/taskboard_models.dart';

enum TaskboardLoadStatus { initial, loading, ready, failure }

enum TaskboardDecisionStatus { idle, submitting, failure }

enum TaskboardMutationStatus { idle, submitting, failure }

sealed class TaskboardEvent {
  const TaskboardEvent();
}

final class TaskboardStarted extends TaskboardEvent {
  const TaskboardStarted();
}

/// Refreshes the current projection without tearing down the board. The
/// authenticated event cursor dispatches this event for low-latency updates;
/// the surface's bounded poll remains a recovery path for reconnect gaps.
final class TaskboardRefreshRequested extends TaskboardEvent {
  const TaskboardRefreshRequested();
}

final class TaskboardRetryRequested extends TaskboardEvent {
  const TaskboardRetryRequested();
}

final class TaskboardProjectFilterChanged extends TaskboardEvent {
  const TaskboardProjectFilterChanged(this.projectId);

  final String? projectId;
}

final class TaskboardAttentionFilterChanged extends TaskboardEvent {
  const TaskboardAttentionFilterChanged(this.enabled);

  final bool enabled;
}

final class TaskboardViewChanged extends TaskboardEvent {
  const TaskboardViewChanged(this.view);

  final TaskboardView view;
}

final class TaskboardTaskSelected extends TaskboardEvent {
  const TaskboardTaskSelected(this.taskId);

  final String? taskId;
}

final class TaskboardDecisionSubmitted extends TaskboardEvent {
  const TaskboardDecisionSubmitted({
    required this.taskId,
    required this.decision,
  });

  final String taskId;
  final TaskboardDecisionInput decision;
}

final class TaskboardClaimRequested extends TaskboardEvent {
  const TaskboardClaimRequested({required this.taskId, required this.agentId});

  final String taskId;
  final String agentId;
}

final class TaskboardReleaseRequested extends TaskboardEvent {
  const TaskboardReleaseRequested(this.taskId);

  final String taskId;
}

final class TaskboardCommentSubmitted extends TaskboardEvent {
  const TaskboardCommentSubmitted({required this.taskId, required this.body});

  final String taskId;
  final String body;
}

class TaskboardState {
  const TaskboardState({
    this.loadStatus = TaskboardLoadStatus.initial,
    this.decisionStatus = TaskboardDecisionStatus.idle,
    this.mutationStatus = TaskboardMutationStatus.idle,
    this.snapshot,
    this.projectId,
    this.attentionOnly = false,
    this.view = TaskboardView.board,
    this.selectedTaskId,
    this.decisionTaskId,
    this.error,
    this.decisionError,
    this.mutationTaskId,
    this.mutationError,
  });

  static const _unset = Object();

  final TaskboardLoadStatus loadStatus;
  final TaskboardDecisionStatus decisionStatus;
  final TaskboardMutationStatus mutationStatus;
  final TaskboardSnapshot? snapshot;
  final String? projectId;
  final bool attentionOnly;
  final TaskboardView view;
  final String? selectedTaskId;
  final String? decisionTaskId;
  final String? error;
  final String? decisionError;
  final String? mutationTaskId;
  final String? mutationError;

  bool get isLoading => loadStatus == TaskboardLoadStatus.loading;
  bool get hasError => loadStatus == TaskboardLoadStatus.failure;
  bool get isSubmitting => decisionStatus == TaskboardDecisionStatus.submitting;
  bool get isMutating => mutationStatus == TaskboardMutationStatus.submitting;
  bool get hasTasks => snapshot?.tasks.isNotEmpty ?? false;
  bool get hasActiveFilters => projectId != null || attentionOnly;

  TaskboardTask? get selectedTask => snapshot?.taskById(selectedTaskId);

  List<TaskboardTask> get visibleTasks {
    final all = snapshot?.tasks ?? const <TaskboardTask>[];
    return [
      for (final task in all)
        if ((projectId == null || task.projectId == projectId) &&
            (!attentionOnly || task.needsAttention))
          task,
    ];
  }

  List<String> get projectIds => snapshot?.projectIds ?? const <String>[];

  int get attentionCount =>
      visibleTasks.where((task) => task.needsAttention).length;

  TaskboardState copyWith({
    TaskboardLoadStatus? loadStatus,
    TaskboardDecisionStatus? decisionStatus,
    TaskboardMutationStatus? mutationStatus,
    TaskboardSnapshot? snapshot,
    Object? projectId = _unset,
    bool? attentionOnly,
    TaskboardView? view,
    Object? selectedTaskId = _unset,
    Object? decisionTaskId = _unset,
    Object? error = _unset,
    Object? decisionError = _unset,
    Object? mutationTaskId = _unset,
    Object? mutationError = _unset,
  }) {
    return TaskboardState(
      loadStatus: loadStatus ?? this.loadStatus,
      decisionStatus: decisionStatus ?? this.decisionStatus,
      mutationStatus: mutationStatus ?? this.mutationStatus,
      snapshot: snapshot ?? this.snapshot,
      projectId: identical(projectId, _unset)
          ? this.projectId
          : projectId as String?,
      attentionOnly: attentionOnly ?? this.attentionOnly,
      view: view ?? this.view,
      selectedTaskId: identical(selectedTaskId, _unset)
          ? this.selectedTaskId
          : selectedTaskId as String?,
      decisionTaskId: identical(decisionTaskId, _unset)
          ? this.decisionTaskId
          : decisionTaskId as String?,
      error: identical(error, _unset) ? this.error : error as String?,
      decisionError: identical(decisionError, _unset)
          ? this.decisionError
          : decisionError as String?,
      mutationTaskId: identical(mutationTaskId, _unset)
          ? this.mutationTaskId
          : mutationTaskId as String?,
      mutationError: identical(mutationError, _unset)
          ? this.mutationError
          : mutationError as String?,
    );
  }
}

class TaskboardBloc extends Bloc<TaskboardEvent, TaskboardState> {
  TaskboardBloc({required TaskboardGateway gateway})
    : _gateway = gateway,
      super(const TaskboardState()) {
    on<TaskboardStarted>(_onStarted);
    on<TaskboardRefreshRequested>(_onRefreshRequested);
    on<TaskboardRetryRequested>(_onRetry);
    on<TaskboardProjectFilterChanged>(_onProjectFilterChanged);
    on<TaskboardAttentionFilterChanged>(_onAttentionFilterChanged);
    on<TaskboardViewChanged>(_onViewChanged);
    on<TaskboardTaskSelected>(_onTaskSelected);
    on<TaskboardDecisionSubmitted>(_onDecisionSubmitted);
    on<TaskboardClaimRequested>(_onClaimRequested);
    on<TaskboardReleaseRequested>(_onReleaseRequested);
    on<TaskboardCommentSubmitted>(_onCommentSubmitted);
  }

  final TaskboardGateway _gateway;
  int _projectionGeneration = 0;

  Stream<void> get updates => _gateway.watchTaskboard();

  Future<void> _onStarted(
    TaskboardStarted event,
    Emitter<TaskboardState> emit,
  ) async {
    if (state.isLoading || state.loadStatus == TaskboardLoadStatus.ready) {
      return;
    }
    final requestGeneration = ++_projectionGeneration;
    emit(state.copyWith(loadStatus: TaskboardLoadStatus.loading, error: null));
    try {
      final snapshot = await _gateway.loadTaskboard();
      if (requestGeneration != _projectionGeneration) return;
      final retainedProjectId = snapshot.projectIds.contains(state.projectId)
          ? state.projectId
          : null;
      emit(
        state.copyWith(
          loadStatus: TaskboardLoadStatus.ready,
          snapshot: snapshot,
          projectId: retainedProjectId,
          selectedTaskId: _visibleSelection(snapshot),
          error: null,
        ),
      );
    } catch (error) {
      if (requestGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          loadStatus: TaskboardLoadStatus.failure,
          error: error.toString(),
        ),
      );
    }
  }

  Future<void> _onRetry(
    TaskboardRetryRequested event,
    Emitter<TaskboardState> emit,
  ) async {
    ++_projectionGeneration;
    emit(
      state.copyWith(
        loadStatus: TaskboardLoadStatus.initial,
        error: null,
        decisionStatus: TaskboardDecisionStatus.idle,
        decisionTaskId: null,
        decisionError: null,
        mutationStatus: TaskboardMutationStatus.idle,
        mutationTaskId: null,
        mutationError: null,
      ),
    );
    add(const TaskboardStarted());
  }

  Future<void> _onRefreshRequested(
    TaskboardRefreshRequested event,
    Emitter<TaskboardState> emit,
  ) async {
    if (state.loadStatus != TaskboardLoadStatus.ready ||
        state.isLoading ||
        state.isMutating ||
        state.isSubmitting) {
      return;
    }
    final requestGeneration = ++_projectionGeneration;
    try {
      final snapshot = await _gateway.loadTaskboard();
      if (requestGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          snapshot: snapshot,
          selectedTaskId: _visibleSelection(snapshot),
          error: null,
        ),
      );
    } catch (_) {
      if (requestGeneration != _projectionGeneration) return;
      // Keep the last known board visible during a transient poll failure;
      // the next tick or an explicit retry will recover the transport.
    }
  }

  void _onProjectFilterChanged(
    TaskboardProjectFilterChanged event,
    Emitter<TaskboardState> emit,
  ) {
    final selected = state.snapshot?.taskById(state.selectedTaskId);
    emit(
      state.copyWith(
        projectId: event.projectId,
        selectedTaskId:
            selected?.projectId == event.projectId || event.projectId == null
            ? selected?.id
            : null,
      ),
    );
  }

  void _onAttentionFilterChanged(
    TaskboardAttentionFilterChanged event,
    Emitter<TaskboardState> emit,
  ) {
    final selected = state.snapshot?.taskById(state.selectedTaskId);
    emit(
      state.copyWith(
        attentionOnly: event.enabled,
        selectedTaskId: event.enabled && !(selected?.needsAttention ?? false)
            ? null
            : selected?.id,
      ),
    );
  }

  void _onViewChanged(
    TaskboardViewChanged event,
    Emitter<TaskboardState> emit,
  ) => emit(state.copyWith(view: event.view));

  void _onTaskSelected(
    TaskboardTaskSelected event,
    Emitter<TaskboardState> emit,
  ) {
    final task = state.snapshot?.taskById(event.taskId);
    emit(state.copyWith(selectedTaskId: task?.id));
  }

  Future<void> _onDecisionSubmitted(
    TaskboardDecisionSubmitted event,
    Emitter<TaskboardState> emit,
  ) async {
    if (state.isSubmitting) return;
    final task = state.snapshot?.taskById(event.taskId);
    if (task == null || task.decision == null) return;
    if (task.decision!.requiresInput &&
        (event.decision.value == null || event.decision.value! <= 0)) {
      emit(
        state.copyWith(
          decisionStatus: TaskboardDecisionStatus.failure,
          decisionTaskId: event.taskId,
          decisionError: 'Enter a positive whole number.',
        ),
      );
      return;
    }
    final operationGeneration = ++_projectionGeneration;
    emit(
      state.copyWith(
        decisionStatus: TaskboardDecisionStatus.submitting,
        decisionTaskId: event.taskId,
        decisionError: null,
      ),
    );
    try {
      final snapshot = await _gateway.submitTaskboardDecision(
        taskId: event.taskId,
        decision: event.decision,
      );
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          decisionStatus: TaskboardDecisionStatus.idle,
          snapshot: snapshot,
          decisionTaskId: null,
          decisionError: null,
          selectedTaskId: _visibleSelection(snapshot),
        ),
      );
    } catch (error) {
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          decisionStatus: TaskboardDecisionStatus.failure,
          decisionTaskId: event.taskId,
          decisionError: error.toString(),
        ),
      );
    }
  }

  Future<void> _onClaimRequested(
    TaskboardClaimRequested event,
    Emitter<TaskboardState> emit,
  ) async {
    if (state.isMutating) return;
    if (state.snapshot?.taskById(event.taskId) == null) return;
    final operationGeneration = ++_projectionGeneration;
    emit(
      state.copyWith(
        mutationStatus: TaskboardMutationStatus.submitting,
        mutationTaskId: event.taskId,
        mutationError: null,
      ),
    );
    try {
      final snapshot = await _gateway.claimTask(
        taskId: event.taskId,
        agentId: event.agentId,
      );
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          mutationStatus: TaskboardMutationStatus.idle,
          mutationTaskId: null,
          mutationError: null,
          snapshot: snapshot,
          selectedTaskId: _visibleSelection(snapshot),
        ),
      );
    } catch (error) {
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          mutationStatus: TaskboardMutationStatus.failure,
          mutationTaskId: event.taskId,
          mutationError: error.toString(),
        ),
      );
    }
  }

  Future<void> _onReleaseRequested(
    TaskboardReleaseRequested event,
    Emitter<TaskboardState> emit,
  ) async {
    if (state.isMutating) return;
    if (state.snapshot?.taskById(event.taskId) == null) return;
    final operationGeneration = ++_projectionGeneration;
    emit(
      state.copyWith(
        mutationStatus: TaskboardMutationStatus.submitting,
        mutationTaskId: event.taskId,
        mutationError: null,
      ),
    );
    try {
      final snapshot = await _gateway.releaseTask(taskId: event.taskId);
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          mutationStatus: TaskboardMutationStatus.idle,
          mutationTaskId: null,
          mutationError: null,
          snapshot: snapshot,
          selectedTaskId: _visibleSelection(snapshot),
        ),
      );
    } catch (error) {
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          mutationStatus: TaskboardMutationStatus.failure,
          mutationTaskId: event.taskId,
          mutationError: error.toString(),
        ),
      );
    }
  }

  Future<void> _onCommentSubmitted(
    TaskboardCommentSubmitted event,
    Emitter<TaskboardState> emit,
  ) async {
    if (state.isMutating) return;
    final body = event.body.trim();
    if (body.isEmpty || state.snapshot?.taskById(event.taskId) == null) return;
    final operationGeneration = ++_projectionGeneration;
    emit(
      state.copyWith(
        mutationStatus: TaskboardMutationStatus.submitting,
        mutationTaskId: event.taskId,
        mutationError: null,
      ),
    );
    try {
      final snapshot = await _gateway.addTaskComment(
        taskId: event.taskId,
        body: body,
      );
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          mutationStatus: TaskboardMutationStatus.idle,
          mutationTaskId: null,
          mutationError: null,
          snapshot: snapshot,
          selectedTaskId: _visibleSelection(snapshot),
        ),
      );
    } catch (error) {
      if (operationGeneration != _projectionGeneration) return;
      emit(
        state.copyWith(
          mutationStatus: TaskboardMutationStatus.failure,
          mutationTaskId: event.taskId,
          mutationError: error.toString(),
        ),
      );
    }
  }

  String? _visibleSelection(TaskboardSnapshot snapshot) {
    final task = snapshot.taskById(state.selectedTaskId);
    if (task == null) return null;
    if (state.projectId != null && task.projectId != state.projectId) {
      return null;
    }
    if (state.attentionOnly && !task.needsAttention) return null;
    return task.id;
  }
}
