import 'package:flutter_test/flutter_test.dart';

import 'package:frank_desktop/core/models/workflow_models.dart';

void main() {
  test('workflow projection decodes snake-case snapshot records', () {
    final projection = WorkflowProjection.fromSnapshot({
      'taskboards': [
        {
          'id': 'board-inbox',
          'name': 'Inbox',
          'dispatch_mode': 'pull',
          'created_at': '2026-09-12T08:00:00Z',
        },
      ],
      'work_offers': [
        {
          'id': 'offer-1',
          'task_id': 'task-1',
          'taskboard_id': 'board-inbox',
          'agent_id': 'agent-1',
          'status': 'pending',
          'attempt': 1,
          'expires_at': '2026-09-12T08:05:00Z',
        },
      ],
      'human_inputs': [
        {
          'id': 'input-1',
          'task_id': 'task-1',
          'kind': 'question',
          'prompt': 'Which warehouse?',
          'status': 'pending',
        },
      ],
      'organization_runtime': {
        'active_revision': 4,
        'status': 'draining',
        'pending_relocation_count': 2,
      },
    });

    expect(projection.boards.single.dispatchMode, WorkflowDispatchMode.pull);
    expect(projection.offers.single.isOpen, isTrue);
    expect(projection.inputsForTask('task-1').single.isPending, isTrue);
    expect(projection.runtime.activeRevision, 4);
    expect(projection.runtime.isDraining, isTrue);
  });

  test('unknown values fail safe to pull/task/question defaults', () {
    expect(
      WorkflowDispatchModeJson.fromWire('future'),
      WorkflowDispatchMode.pull,
    );
    expect(
      WorkflowWorkItemKindJson.fromWire('future'),
      WorkflowWorkItemKind.task,
    );
    expect(
      WorkflowOfferStatusJson.fromWire('future'),
      WorkflowOfferStatus.pending,
    );
  });

  test('wire timestamps accept Frank millisecond strings', () {
    final projection = WorkflowProjection.fromSnapshot({
      'taskboards': [
        {
          'id': 'board-inbox',
          'name': 'Inbox',
          'created_at': '1789200000000',
          'updated_at': 1789200001000,
        },
      ],
      'work_offers': [
        {
          'id': 'offer-1',
          'task_id': 'task-1',
          'taskboard_id': 'board-inbox',
          'agent_id': 'agent-1',
          'created_at': '1789200000000',
          'expires_at': '1789200300000',
        },
      ],
    });

    expect(
      projection.boards.single.createdAt,
      DateTime.fromMillisecondsSinceEpoch(1789200000000, isUtc: true),
    );
    expect(projection.offers.single.expiresAt?.isUtc, isTrue);
  });
}
