import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_ledger.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/ledger_models.dart';

void main() {
  test('fixture keeps measured totals and estimates separate', () async {
    final workspace = await FixtureFrankGateway(
      latency: Duration.zero,
    ).loadWorkspace();
    final dashboard = fixtureLedgerDashboard(workspace);

    expect(dashboard.lifetime.verdict, LedgerVerdict.insufficientEvidence);
    expect(dashboard.session.verdict, LedgerVerdict.comparisonRequired);
    expect(dashboard.lifetime.sessionCount, 12);
    expect(dashboard.lifetime.turnCount, 124);
    expect(dashboard.lifetime.totals.measuredInputTokens, 1266610);
    expect(dashboard.lifetime.totals.measuredOutputTokens, 286420);
    expect(dashboard.lifetime.totals.injectedBytes, 104800);
    expect(dashboard.lifetime.attribution, hasLength(5));
    expect(
      dashboard.lifetime.attribution
          .where((row) => row.isExcluded)
          .map((row) => row.basis),
      containsAll(<LedgerAttributionBasis>[
        LedgerAttributionBasis.unattributed,
        LedgerAttributionBasis.sidechain,
      ]),
    );
    expect(
      dashboard.lifetime.attribution.first.estimatedSaved,
      const TokenRange(84000, 127000),
    );
  });

  test('lifetime evidence requires both thresholds', () {
    const belowSessions = LedgerPeriodData(
      period: LedgerPeriod.lifetime,
      sessionCount: 19,
      turnCount: 200,
      totals: LedgerTotals(),
      trend: [],
      attribution: [],
    );
    const belowTurns = LedgerPeriodData(
      period: LedgerPeriod.lifetime,
      sessionCount: 20,
      turnCount: 199,
      totals: LedgerTotals(),
      trend: [],
      attribution: [],
    );
    const enough = LedgerPeriodData(
      period: LedgerPeriod.lifetime,
      sessionCount: 20,
      turnCount: 200,
      totals: LedgerTotals(),
      trend: [],
      attribution: [],
    );

    expect(belowSessions.verdict, LedgerVerdict.insufficientEvidence);
    expect(belowTurns.verdict, LedgerVerdict.insufficientEvidence);
    expect(enough.verdict, LedgerVerdict.comparisonRequired);
  });
}
