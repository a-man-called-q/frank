import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_ledger.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/ledger/ledger_projection.dart';
import 'package:frank_desktop/core/models/ledger_models.dart';

void main() {
  test(
    'operational projection keeps excluded rows out of derived totals',
    () async {
      final workspace = await FixtureFrankGateway(
        latency: Duration.zero,
      ).loadWorkspace();
      final dashboard = fixtureLedgerDashboard(workspace);
      final projection = LedgerProjection.operational(
        dashboard.operational!,
        filter: const LedgerFilter(range: LedgerRange.allTime),
      );

      expect(projection.entries, hasLength(9));
      expect(projection.excludedEntries, hasLength(2));
      expect(projection.measuredInput.value, 219460);
      expect(projection.measuredOutput.value, 46590);
      expect(projection.measuredInput.totalRows, 7);
      expect(projection.cost.micros, 4740);
      expect(projection.cost.reportedRows, 5);
      expect(projection.cost.totalRows, 7);
      expect(projection.excludedMeasuredInput, 26200);
      expect(projection.excludedMeasuredOutput, 3800);
    },
  );

  test(
    'operational filters are deterministic and preserve missing values',
    () async {
      final workspace = await FixtureFrankGateway(
        latency: Duration.zero,
      ).loadWorkspace();
      final dashboard = fixtureLedgerDashboard(workspace);
      final projection = LedgerProjection.operational(
        dashboard.operational!,
        filter: const LedgerFilter(
          range: LedgerRange.allTime,
          projectId: 'atlas-handoff',
          provider: 'Codex',
        ),
      );

      expect(projection.entries, hasLength(1));
      expect(projection.measuredInput.value, 31200);
      expect(projection.cost.micros, isNull);
      expect(projection.cost.reportedRows, 0);
      expect(projection.cost.totalRows, 1);
    },
  );

  test('effectiveness projection excludes unattributed savings', () async {
    final workspace = await FixtureFrankGateway(
      latency: Duration.zero,
    ).loadWorkspace();
    final dashboard = fixtureLedgerDashboard(workspace);
    final projection = LedgerProjection.effectiveness(dashboard.lifetime);

    expect(projection.attributedRows, hasLength(3));
    expect(projection.excludedRows, hasLength(2));
    expect(projection.estimatedSavings, const TokenRange(115000, 175000));
    expect(projection.evidenceThresholdMet, isFalse);
  });
}
