import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/gateway/http_frank_gateway.dart';
import 'package:frank_desktop/core/models/ledger_models.dart';
import '../support/fake_transport.dart';

void main() {
  test(
    'taskboard refresh bypasses the composition-root snapshot cache',
    () async {
      var snapshotCalls = 0;
      final transport = TestFrankTransport((method, path, {body}) async {
        expect(method, 'GET');
        expect(path, '/v2/snapshot');
        snapshotCalls++;
        return const {
          'server': <String, dynamic>{},
          'projects': <dynamic>[],
          'agents': <dynamic>[],
          'roles': <dynamic>[],
          'missions': <dynamic>[],
          'tasks': <dynamic>[],
          'task_feed': <dynamic>[],
          'usage': <dynamic>[],
        };
      });
      final gateway = HttpFrankGateway(transport);

      await gateway.loadWorkspace();
      await gateway.loadTaskboard();

      expect(snapshotCalls, 2);
    },
  );

  test(
    'maps measured and estimated remote usage without mixing them',
    () async {
      final transport = TestFrankTransport((method, path, {body}) async {
        expect(method, 'GET');
        expect(path, '/v2/snapshot');
        return const {
          'projects': [
            {'id': 'project-1', 'name': 'Project one'},
          ],
          'missions': [
            {
              'id': 'mission-1',
              'project_id': 'project-1',
              'objective': 'Mission one',
            },
          ],
          'tasks': [
            {
              'id': 'task-1',
              'mission_id': 'mission-1',
              'assigned_agent': 'agent-1',
              'title': 'Task one',
            },
          ],
          'agents': [
            {'id': 'agent-1', 'display_name': 'Agent one'},
          ],
          'usage': [
            {
              'id': 'attempt-1',
              'scope': 'task',
              'scope_id': 'task-1',
              'provider': 'codex',
              'model': 'gpt-test',
              'measured_input_tokens': 12,
              'cached_input_tokens': 3,
              'reasoning_tokens': 2,
              'measured_output_tokens': 5,
              'estimated_input_tokens': 100,
              'estimated_output_tokens': 40,
              'cost_micros': 7,
              'recorded_at': '2026-09-12T10:00:00Z',
            },
          ],
        };
      });

      final dashboard = await HttpFrankGateway(transport).loadLedgerDashboard();
      final entry = dashboard.operational!.entries.single;

      expect(entry.projectId, 'project-1');
      expect(entry.projectName, 'Project one');
      expect(entry.missionId, 'mission-1');
      expect(entry.taskId, 'task-1');
      expect(entry.taskName, 'Task one');
      expect(entry.agentId, 'agent-1');
      expect(entry.agentName, 'Agent one');
      expect(entry.model, 'gpt-test');
      expect(entry.measuredInputTokens, 12);
      expect(entry.cacheReadInputTokens, 3);
      expect(entry.reasoningTokens, 2);
      expect(entry.measuredOutputTokens, 5);
      expect(entry.estimatedInputTokens, 100);
      expect(entry.estimatedOutputTokens, 40);
      expect(entry.costMicros, 7);
      expect(entry.basis, LedgerAttributionBasis.unattributed);
      expect(entry.measuredTotals.measuredInputTokens, 12);
      expect(entry.measuredTotals.cacheReadInputTokens, 3);
      expect(entry.estimatedTotals.inputTokens, 100);

      expect(dashboard.session.turnCount, 0);
      expect(dashboard.session.totals, const LedgerTotals());
      expect(dashboard.lifetime.sessionCount, 0);
      expect(dashboard.lifetime.turnCount, 1);
      expect(dashboard.lifetime.totals.measuredInputTokens, 12);
      expect(dashboard.lifetime.totals.cacheReadInputTokens, 3);
      expect(dashboard.lifetime.attribution, isEmpty);
      expect(dashboard.lifetime.trend, isEmpty);
      expect(dashboard.lifetime.isIncomplete, isTrue);
      expect(dashboard.lifetime.evidenceCountsAvailable, isFalse);
      expect(dashboard.lifetime.injectedBytesAvailable, isFalse);
    },
  );

  test(
    'remote usage falls back to incomplete data without fake identity',
    () async {
      final transport = TestFrankTransport((method, path, {body}) async {
        expect(method, 'GET');
        expect(path, '/v2/snapshot');
        return const {
          'usage': [
            {
              'id': 'attempt-without-context',
              'scope': 'agent',
              'scope_id': 'agent-missing-from-snapshot',
              'provider': 'claude',
              'measured_input_tokens': 8,
              'measured_output_tokens': 2,
              'recorded_at': '2026-09-12T11:00:00Z',
            },
          ],
        };
      });

      final dashboard = await HttpFrankGateway(transport).loadLedgerDashboard();
      final entry = dashboard.operational!.entries.single;

      expect(entry.projectId, isNull);
      expect(entry.projectName, isNull);
      expect(entry.agentId, 'agent-missing-from-snapshot');
      expect(entry.agentName, isNull);
      expect(entry.measuredInputTokens, 8);
      expect(entry.measuredOutputTokens, 2);
      expect(entry.estimatedInputTokens, isNull);
      expect(entry.estimatedOutputTokens, isNull);
      expect(entry.isExcluded, isTrue);
      expect(dashboard.lifetime.attribution, isEmpty);
      expect(dashboard.lifetime.trend, isEmpty);
    },
  );

  test('empty remote usage remains an honest empty projection', () async {
    final transport = TestFrankTransport((method, path, {body}) async {
      expect(method, 'GET');
      expect(path, '/v2/snapshot');
      return const {'usage': <dynamic>[]};
    });

    final dashboard = await HttpFrankGateway(transport).loadLedgerDashboard();

    expect(dashboard.operational!.entries, isEmpty);
    expect(dashboard.lifetime.totals, const LedgerTotals());
    expect(dashboard.session.totals, const LedgerTotals());
    expect(dashboard.session.verdict, LedgerVerdict.insufficientEvidence);
    expect(dashboard.lifetime.verdict, LedgerVerdict.insufficientEvidence);
    expect(dashboard.lifetime.attribution, isEmpty);
    expect(dashboard.lifetime.trend, isEmpty);
  });
}
