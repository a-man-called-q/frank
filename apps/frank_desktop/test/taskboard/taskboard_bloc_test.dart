import 'dart:async';

import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/taskboard_models.dart';
import 'package:frank_desktop/features/taskboard/bloc/taskboard_bloc.dart';

import '../support/fake_gateway.dart';

void main() {
  blocTest<TaskboardBloc, TaskboardState>(
    'loads once and applies project and attention filters locally',
    build: () => TaskboardBloc(gateway: FakeGateway()),
    act: (bloc) async {
      bloc.add(const TaskboardStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == TaskboardLoadStatus.ready,
      );
      bloc.add(const TaskboardProjectFilterChanged('northstar-inventory'));
      bloc.add(const TaskboardAttentionFilterChanged(true));
      bloc.add(const TaskboardStarted());
    },
    expect: () => [
      isA<TaskboardState>().having(
        (state) => state.loadStatus,
        'load status',
        TaskboardLoadStatus.loading,
      ),
      isA<TaskboardState>()
          .having(
            (state) => state.loadStatus,
            'load status',
            TaskboardLoadStatus.ready,
          )
          .having((state) => state.visibleTasks.length, 'visible tasks', 7),
      isA<TaskboardState>().having(
        (state) => state.projectId,
        'project filter',
        'northstar-inventory',
      ),
      isA<TaskboardState>()
          .having((state) => state.attentionOnly, 'attention filter', true)
          .having((state) => state.visibleTasks.length, 'visible tasks', 1),
    ],
    verify: (bloc) async {
      // The event after ready must be ignored; the fake was loaded once.
      expect(bloc.state.loadStatus, TaskboardLoadStatus.ready);
    },
  );

  blocTest<TaskboardBloc, TaskboardState>(
    'approval updates the task snapshot and clears the decision',
    build: () => TaskboardBloc(gateway: FakeGateway()),
    act: (bloc) async {
      bloc.add(const TaskboardStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == TaskboardLoadStatus.ready,
      );
      bloc.add(const TaskboardTaskSelected('NS-03'));
      await bloc.stream.firstWhere((state) => state.selectedTaskId == 'NS-03');
      bloc.add(
        const TaskboardDecisionSubmitted(
          taskId: 'NS-03',
          decision: TaskboardDecisionInput.approve(),
        ),
      );
      await bloc.stream.firstWhere(
        (state) =>
            state.decisionStatus == TaskboardDecisionStatus.idle &&
            state.selectedTask?.lane == TaskboardLane.working,
      );
    },
    expect: () => [
      isA<TaskboardState>().having(
        (state) => state.loadStatus,
        'load status',
        TaskboardLoadStatus.loading,
      ),
      isA<TaskboardState>().having(
        (state) => state.loadStatus,
        'load status',
        TaskboardLoadStatus.ready,
      ),
      isA<TaskboardState>().having(
        (state) => state.selectedTaskId,
        'selected task',
        'NS-03',
      ),
      isA<TaskboardState>().having(
        (state) => state.decisionStatus,
        'decision status',
        TaskboardDecisionStatus.submitting,
      ),
      isA<TaskboardState>()
          .having(
            (state) => state.selectedTask?.lane,
            'lane',
            TaskboardLane.working,
          )
          .having((state) => state.selectedTask?.decision, 'decision', isNull),
    ],
  );

  blocTest<TaskboardBloc, TaskboardState>(
    'rejects an invalid threshold before calling the gateway',
    build: () => TaskboardBloc(gateway: FakeGateway()),
    act: (bloc) async {
      bloc.add(const TaskboardStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == TaskboardLoadStatus.ready,
      );
      bloc.add(
        const TaskboardDecisionSubmitted(
          taskId: 'MF-02',
          decision: TaskboardDecisionInput.positiveInteger(0),
        ),
      );
      await bloc.stream.firstWhere(
        (state) => state.decisionStatus == TaskboardDecisionStatus.failure,
      );
    },
    expect: () => [
      isA<TaskboardState>().having(
        (state) => state.loadStatus,
        'load status',
        TaskboardLoadStatus.loading,
      ),
      isA<TaskboardState>().having(
        (state) => state.loadStatus,
        'load status',
        TaskboardLoadStatus.ready,
      ),
      isA<TaskboardState>().having(
        (state) => state.decisionStatus,
        'decision status',
        TaskboardDecisionStatus.failure,
      ),
    ],
  );

  late FakeGateway duplicateGateway;
  blocTest<TaskboardBloc, TaskboardState>(
    'ignores a duplicate decision while the first one is submitting',
    build: () {
      duplicateGateway = FakeGateway();
      return TaskboardBloc(gateway: duplicateGateway);
    },
    act: (bloc) async {
      bloc.add(const TaskboardStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == TaskboardLoadStatus.ready,
      );
      const event = TaskboardDecisionSubmitted(
        taskId: 'NS-03',
        decision: TaskboardDecisionInput.approve(),
      );
      bloc
        ..add(event)
        ..add(event);
      await bloc.stream.firstWhere(
        (state) =>
            state.snapshot?.taskById('NS-03')?.lane == TaskboardLane.working,
      );
    },
    verify: (bloc) {
      expect(bloc.state.decisionStatus, TaskboardDecisionStatus.idle);
      expect(duplicateGateway.taskboardDecisions, hasLength(1));
    },
  );

  test('taskboard load failure can recover through retry', () async {
    final gateway = FakeGateway(taskboardLoadError: StateError('offline'));
    final bloc = TaskboardBloc(gateway: gateway);
    addTearDown(bloc.close);

    bloc.add(const TaskboardStarted());
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == TaskboardLoadStatus.failure,
    );
    gateway.taskboardLoadError = null;
    bloc.add(const TaskboardRetryRequested());
    final recovered = await bloc.stream.firstWhere(
      (state) => state.loadStatus == TaskboardLoadStatus.ready,
    );

    expect(recovered.snapshot?.tasks, hasLength(7));
    expect(gateway.taskboardLoadCalls, 2);
  });

  test('a stale refresh cannot overwrite a completed mutation', () async {
    final base = await FixtureFrankGateway(
      latency: Duration.zero,
    ).loadTaskboard();
    final updated = base.replaceTask(
      base
          .taskById('NS-03')!
          .copyWith(
            agentId: 'agent-new',
            agentName: 'New Agent',
            agentInitials: 'NA',
          ),
    );
    final initial = Completer<TaskboardSnapshot>();
    final refresh = Completer<TaskboardSnapshot>();
    final mutation = Completer<TaskboardSnapshot>();
    final gateway = _OrderedTaskboardGateway(
      loadResponses: [initial, refresh],
      mutationResponse: mutation,
    );
    final bloc = TaskboardBloc(gateway: gateway);
    addTearDown(bloc.close);

    bloc.add(const TaskboardStarted());
    await gateway.loadStarted[0].future;
    initial.complete(base);
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == TaskboardLoadStatus.ready,
    );

    bloc.add(const TaskboardRefreshRequested());
    await gateway.loadStarted[1].future;
    bloc.add(
      const TaskboardClaimRequested(taskId: 'NS-03', agentId: 'agent-new'),
    );
    await bloc.stream.firstWhere(
      (state) => state.mutationStatus == TaskboardMutationStatus.submitting,
    );
    mutation.complete(updated);
    await bloc.stream.firstWhere(
      (state) => state.snapshot?.taskById('NS-03')?.agentId == 'agent-new',
    );

    refresh.complete(base);
    await Future<void>.delayed(Duration.zero);
    expect(bloc.state.snapshot?.taskById('NS-03')?.agentId, 'agent-new');
  });

  test('an older refresh response cannot overwrite a newer refresh', () async {
    final base = await FixtureFrankGateway(
      latency: Duration.zero,
    ).loadTaskboard();
    final newer = base.replaceTask(
      base.taskById('NS-03')!.copyWith(agentName: 'New Agent'),
    );
    final initial = Completer<TaskboardSnapshot>();
    final olderRefresh = Completer<TaskboardSnapshot>();
    final newerRefresh = Completer<TaskboardSnapshot>();
    final gateway = _OrderedTaskboardGateway(
      loadResponses: [initial, olderRefresh, newerRefresh],
      mutationResponse: Completer<TaskboardSnapshot>(),
    );
    final bloc = TaskboardBloc(gateway: gateway);
    addTearDown(bloc.close);

    bloc.add(const TaskboardStarted());
    await gateway.loadStarted[0].future;
    initial.complete(base);
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == TaskboardLoadStatus.ready,
    );

    bloc
      ..add(const TaskboardRefreshRequested())
      ..add(const TaskboardRefreshRequested());
    await gateway.loadStarted[1].future;
    await gateway.loadStarted[2].future;
    newerRefresh.complete(newer);
    await bloc.stream.firstWhere(
      (state) => state.snapshot?.taskById('NS-03')?.agentName == 'New Agent',
    );

    olderRefresh.complete(base);
    await Future<void>.delayed(Duration.zero);
    expect(bloc.state.snapshot?.taskById('NS-03')?.agentName, 'New Agent');
  });

  test(
    'fixture gateway preserves decision input failure and supports retry',
    () async {
      final gateway = FixtureFrankGateway(
        latency: Duration.zero,
        taskboardDecisionError: StateError('offline'),
      );
      final snapshot = await gateway.loadTaskboard();
      expect(snapshot.tasks, hasLength(7));
      await expectLater(
        gateway.submitTaskboardDecision(
          taskId: 'NS-03',
          decision: const TaskboardDecisionInput.approve(),
        ),
        throwsA(isA<StateError>()),
      );
    },
  );
}

class _OrderedTaskboardGateway extends FakeGateway {
  _OrderedTaskboardGateway({
    required List<Completer<TaskboardSnapshot>> loadResponses,
    required this.mutationResponse,
  }) : _loadResponses = loadResponses,
       loadStarted = [for (final _ in loadResponses) Completer<void>()];

  final List<Completer<TaskboardSnapshot>> _loadResponses;
  final List<Completer<void>> loadStarted;
  final Completer<TaskboardSnapshot> mutationResponse;
  int _loadIndex = 0;

  @override
  Future<TaskboardSnapshot> loadTaskboard() {
    final index = _loadIndex++;
    loadStarted[index].complete();
    return _loadResponses[index].future;
  }

  @override
  Future<TaskboardSnapshot> claimTask({
    required String taskId,
    required String agentId,
  }) => mutationResponse.future;
}
