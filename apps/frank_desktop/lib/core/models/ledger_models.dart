import 'package:flutter/foundation.dart';

/// The two presentation surfaces in Office > Ledger.
enum LedgerTab { operational, effectiveness }

extension LedgerTabMetadata on LedgerTab {
  String get label => switch (this) {
    LedgerTab.operational => 'Operational',
    LedgerTab.effectiveness => 'Frank Effectiveness',
  };

  String get description => switch (this) {
    LedgerTab.operational =>
      'Runtime usage recorded by provider, project, and agent.',
    LedgerTab.effectiveness =>
      'Measured conversation usage and Frank’s separate evidence range.',
  };
}

/// Time windows used by the operational projection.
enum LedgerRange { last7Days, last30Days, allTime }

extension LedgerRangeMetadata on LedgerRange {
  String get label => switch (this) {
    LedgerRange.last7Days => 'Last 7 days',
    LedgerRange.last30Days => 'Last 30 days',
    LedgerRange.allTime => 'All time',
  };
}

/// The two evidence windows shown by the Ledger surface.
enum LedgerPeriod { session, lifetime }

extension LedgerPeriodMetadata on LedgerPeriod {
  String get label => switch (this) {
    LedgerPeriod.session => 'This session',
    LedgerPeriod.lifetime => 'Lifetime',
  };
}

/// The only conclusions the fixture surface is allowed to make.
///
/// A single session can show evidence, but it cannot support a lifetime
/// verdict. Lifetime data only becomes comparable after both sample-size
/// thresholds have been met.
enum LedgerVerdict { insufficientEvidence, comparisonRequired }

extension LedgerVerdictMetadata on LedgerVerdict {
  String get label => switch (this) {
    LedgerVerdict.insufficientEvidence => 'Not enough data yet',
    LedgerVerdict.comparisonRequired => 'Evidence ready',
  };

  String get description => switch (this) {
    LedgerVerdict.insufficientEvidence =>
        'Frank won’t claim a net saving until both evidence thresholds are met.',
    LedgerVerdict.comparisonRequired =>
        'Measured usage is available. Compare it with the separate benchmark range; lifetime claims still require both evidence thresholds.',
  };
}

/// The possible provenance of an attribution row.
///
/// [unattributed] and [sidechain] are deliberately first-class values. They
/// stay visible in the table, but their measured tokens never enter an
/// estimated-savings range.
enum LedgerAttributionBasis {
  transitionLog,
  flagMtime,
  wholeSession,
  unattributed,
  sidechain,
}

extension LedgerAttributionBasisMetadata on LedgerAttributionBasis {
  String get label => switch (this) {
    LedgerAttributionBasis.transitionLog => 'transition log',
    LedgerAttributionBasis.flagMtime => 'flag mtime',
    LedgerAttributionBasis.wholeSession => 'whole session',
    LedgerAttributionBasis.unattributed => 'unattributed',
    LedgerAttributionBasis.sidechain => 'sidechain',
  };

  bool get isExcluded => switch (this) {
    LedgerAttributionBasis.unattributed ||
    LedgerAttributionBasis.sidechain => true,
    _ => false,
  };
}

/// A closed interval for an estimate.
///
/// Estimates never collapse to a single number. An exact measured value is
/// represented by [TokenRange.exact] only for APIs that need a common display
/// type; measured totals themselves remain integers in [LedgerTotals].
@immutable
class TokenRange {
  const TokenRange(this.low, this.high) : assert(low >= 0 && high >= low);

  const TokenRange.exact(int value) : this(value, value);

  final int low;
  final int high;

  int get lower => low;
  int get upper => high;
  bool get isExact => low == high;

  TokenRange clampTo(int minimum, int maximum) =>
      TokenRange(low.clamp(minimum, maximum), high.clamp(minimum, maximum));

  @override
  bool operator ==(Object other) =>
      other is TokenRange && other.low == low && other.high == high;

  @override
  int get hashCode => Object.hash(low, high);

  @override
  String toString() => 'TokenRange($low, $high)';
}

/// A usage row shaped like the fields available in protocol [UsageView].
///
/// Nullable token and cost fields are intentional. A provider that did not
/// report a quantity is not the same thing as a provider that reported zero.
@immutable
class LedgerUsageEntry {
  const LedgerUsageEntry({
    required this.id,
    required this.recordedAt,
    required this.provider,
    required this.projectId,
    required this.projectName,
    this.missionId,
    this.missionName,
    this.taskId,
    this.taskName,
    this.agentId,
    required this.agentName,
    this.measuredInputTokens,
    this.measuredOutputTokens,
    this.estimatedInputTokens,
    this.estimatedOutputTokens,
    this.costMicros,
    this.basis = LedgerAttributionBasis.transitionLog,
  });

  final String id;
  final DateTime recordedAt;
  final String provider;
  final String projectId;
  final String projectName;
  final String? missionId;
  final String? missionName;
  final String? taskId;
  final String? taskName;
  final String? agentId;
  final String agentName;
  final int? measuredInputTokens;
  final int? measuredOutputTokens;
  final int? estimatedInputTokens;
  final int? estimatedOutputTokens;
  final int? costMicros;
  final LedgerAttributionBasis basis;

  bool get isExcluded => basis.isExcluded;

  /// `null` is rendered as “not reported”; this is never coalesced to zero.
  bool get hasReportedCost => costMicros != null;

  String get scopeLabel => taskName ?? missionName ?? projectName;

  LedgerTotals get measuredTotals => LedgerTotals(
    inputTokens: measuredInputTokens ?? 0,
    outputTokens: measuredOutputTokens ?? 0,
  );

  LedgerTotals get estimatedTotals => LedgerTotals(
    inputTokens: estimatedInputTokens ?? 0,
    outputTokens: estimatedOutputTokens ?? 0,
  );
}

/// Operational source rows. [asOf] makes range filtering deterministic in
/// both the fixture and widget tests.
@immutable
class LedgerOperationalData {
  const LedgerOperationalData({required this.entries, required this.asOf});

  final List<LedgerUsageEntry> entries;
  final DateTime asOf;
}

/// A nullable aggregate with coverage information kept beside its value.
///
/// The UI can therefore say “not reported” or “7 of 9 rows” instead of
/// presenting a misleading zero.
@immutable
class LedgerMetricSummary {
  const LedgerMetricSummary({
    required this.value,
    required this.reportedRows,
    required this.totalRows,
  });

  final int? value;
  final int reportedRows;
  final int totalRows;

  bool get hasValue => value != null;
  bool get complete => totalRows > 0 && reportedRows == totalRows;
  double? get coverage => totalRows == 0 ? null : reportedRows / totalRows;
}

/// Cost coverage is kept separate from token evidence.
@immutable
class LedgerCostSummary {
  const LedgerCostSummary({
    required this.micros,
    required this.reportedRows,
    required this.totalRows,
  });

  final int? micros;
  final int reportedRows;
  final int totalRows;

  int get missingRows => totalRows - reportedRows;
  bool get hasValue => micros != null;
  bool get complete => totalRows > 0 && reportedRows == totalRows;
}

/// The controls owned by the local Ledger surface.
@immutable
class LedgerFilter {
  const LedgerFilter({
    this.range = LedgerRange.last30Days,
    this.projectId,
    this.agentId,
    this.provider,
  });

  final LedgerRange range;
  final String? projectId;
  final String? agentId;
  final String? provider;

  bool get hasFilters =>
      range != LedgerRange.last30Days ||
      projectId != null ||
      agentId != null ||
      provider != null;

  LedgerFilter copyWith({
    LedgerRange? range,
    Object? projectId = _unset,
    Object? agentId = _unset,
    Object? provider = _unset,
  }) {
    return LedgerFilter(
      range: range ?? this.range,
      projectId: identical(projectId, _unset)
          ? this.projectId
          : projectId as String?,
      agentId: identical(agentId, _unset) ? this.agentId : agentId as String?,
      provider: identical(provider, _unset)
          ? this.provider
          : provider as String?,
    );
  }

  static const _unset = Object();
}

/// A grouped view used by the operational drill-down table.
@immutable
class LedgerUsageGroup {
  const LedgerUsageGroup({
    required this.key,
    required this.label,
    required this.entries,
    required this.excluded,
  });

  final String key;
  final String label;
  final List<LedgerUsageEntry> entries;
  final bool excluded;

  int? _sum(int? Function(LedgerUsageEntry entry) select) {
    final values = entries.map(select).whereType<int>().toList();
    if (values.isEmpty) return null;
    return values.fold<int>(0, (sum, value) => sum + value);
  }

  LedgerMetricSummary metric(int? Function(LedgerUsageEntry entry) select) {
    return LedgerMetricSummary(
      value: _sum(select),
      reportedRows: entries.where((entry) => select(entry) != null).length,
      totalRows: entries.length,
    );
  }
}

/// Measured usage for a period or attribution bucket.
///
/// `inputTokens` and `cacheCreationInputTokens` are kept separate because
/// that is how provider session JSONL reports them. [measuredInputTokens]
/// follows the ledger contract and adds those two measured quantities. Cache
/// reads are displayed separately and are intentionally not folded into that
/// headline, matching the Rust ledger's `measured_input_total`.
@immutable
class LedgerTotals {
  const LedgerTotals({
    this.inputTokens = 0,
    this.cacheCreationInputTokens = 0,
    this.cacheReadInputTokens = 0,
    this.outputTokens = 0,
    this.activationBytes = 0,
    this.reinforcementBytes = 0,
  });

  final int inputTokens;
  final int cacheCreationInputTokens;
  final int cacheReadInputTokens;
  final int outputTokens;
  final int activationBytes;
  final int reinforcementBytes;

  int get measuredInputTokens => inputTokens + cacheCreationInputTokens;
  int get measuredOutputTokens => outputTokens;
  int get injectedBytes => activationBytes + reinforcementBytes;
  int get exactInjectedBytes => injectedBytes;

  // These aliases make the presentation DTO straightforward to map from
  // transport names without introducing a transport dependency here.
  int get cacheReadTokens => cacheReadInputTokens;
  int get cacheCreationTokens => cacheCreationInputTokens;

  LedgerTotals operator +(LedgerTotals other) => LedgerTotals(
    inputTokens: inputTokens + other.inputTokens,
    cacheCreationInputTokens:
        cacheCreationInputTokens + other.cacheCreationInputTokens,
    cacheReadInputTokens: cacheReadInputTokens + other.cacheReadInputTokens,
    outputTokens: outputTokens + other.outputTokens,
    activationBytes: activationBytes + other.activationBytes,
    reinforcementBytes: reinforcementBytes + other.reinforcementBytes,
  );

  LedgerTotals copyWith({
    int? inputTokens,
    int? cacheCreationInputTokens,
    int? cacheReadInputTokens,
    int? outputTokens,
    int? activationBytes,
    int? reinforcementBytes,
  }) {
    return LedgerTotals(
      inputTokens: inputTokens ?? this.inputTokens,
      cacheCreationInputTokens:
          cacheCreationInputTokens ?? this.cacheCreationInputTokens,
      cacheReadInputTokens: cacheReadInputTokens ?? this.cacheReadInputTokens,
      outputTokens: outputTokens ?? this.outputTokens,
      activationBytes: activationBytes ?? this.activationBytes,
      reinforcementBytes: reinforcementBytes ?? this.reinforcementBytes,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is LedgerTotals &&
      other.inputTokens == inputTokens &&
      other.cacheCreationInputTokens == cacheCreationInputTokens &&
      other.cacheReadInputTokens == cacheReadInputTokens &&
      other.outputTokens == outputTokens &&
      other.activationBytes == activationBytes &&
      other.reinforcementBytes == reinforcementBytes;

  @override
  int get hashCode => Object.hash(
    inputTokens,
    cacheCreationInputTokens,
    cacheReadInputTokens,
    outputTokens,
    activationBytes,
    reinforcementBytes,
  );
}

/// One point in the measured-output trend and its independent estimate band.
@immutable
class LedgerTrendPoint {
  const LedgerTrendPoint({
    required this.label,
    required this.measuredOutputTokens,
    this.measuredInputTokens,
    this.estimatedSaved,
  });

  final String label;
  final int measuredOutputTokens;
  final int? measuredInputTokens;
  final TokenRange? estimatedSaved;

  int get measuredOutput => measuredOutputTokens;
}

/// One row in the audit table.
@immutable
class LedgerAttributionRow {
  const LedgerAttributionRow({
    required this.label,
    required this.mode,
    this.agentId,
    this.measuredInputTokens = 0,
    this.cacheCreationInputTokens = 0,
    this.cacheReadInputTokens = 0,
    this.measuredOutputTokens = 0,
    this.estimatedSaved,
    required this.basis,
    this.excluded = false,
  });

  /// Human-readable agent or bucket label, e.g. `Maya Chen · Account Executive`.
  final String label;
  final String mode;
  final String? agentId;
  final int measuredInputTokens;
  final int cacheCreationInputTokens;
  final int cacheReadInputTokens;
  final int measuredOutputTokens;
  final TokenRange? estimatedSaved;
  final LedgerAttributionBasis basis;
  final bool excluded;

  String get agent => label;
  int get measuredInput => measuredInputTokens + cacheCreationInputTokens;
  int get cacheReadTokens => cacheReadInputTokens;
  int get measuredOutput => measuredOutputTokens;
  bool get isExcluded => excluded || basis.isExcluded;

  LedgerTotals get totals => LedgerTotals(
    inputTokens: measuredInputTokens,
    cacheCreationInputTokens: cacheCreationInputTokens,
    cacheReadInputTokens: cacheReadInputTokens,
    outputTokens: measuredOutputTokens,
  );
}

/// A complete fixture-backed data set for one evidence period.
@immutable
class LedgerPeriodData {
  const LedgerPeriodData({
    required this.period,
    required this.sessionCount,
    required this.turnCount,
    required this.totals,
    required this.trend,
    required this.attribution,
    this.model = 'Claude · default',
    this.benchmarkModelMatches,
  });

  final LedgerPeriod period;
  final int sessionCount;
  final int turnCount;
  final LedgerTotals totals;
  final List<LedgerTrendPoint> trend;
  final List<LedgerAttributionRow> attribution;
  final String model;
  final bool? benchmarkModelMatches;

  int get sessions => sessionCount;
  int get turns => turnCount;
  List<LedgerTrendPoint> get trendPoints => trend;
  List<LedgerAttributionRow> get attributionRows => attribution;
  LedgerTotals get measured => totals;

  bool get hasLifetimeEvidence =>
      sessionCount >= LedgerDashboardData.minimumSessionsForLifetimeVerdict &&
      turnCount >= LedgerDashboardData.minimumTurnsForLifetimeVerdict;

  LedgerVerdict get verdict =>
      period == LedgerPeriod.lifetime && !hasLifetimeEvidence
      ? LedgerVerdict.insufficientEvidence
      : LedgerVerdict.comparisonRequired;
}

/// Presentation-only root DTO. The eventual remote DTO can map into this
/// shape without changing [LedgerSurface].
@immutable
class LedgerDashboardData {
  const LedgerDashboardData({
    required this.session,
    required this.lifetime,
    this.operational,
  });

  static const minimumSessionsForLifetimeVerdict = 20;
  static const minimumTurnsForLifetimeVerdict = 200;

  final LedgerPeriodData session;
  final LedgerPeriodData lifetime;
  final LedgerOperationalData? operational;

  LedgerPeriodData forPeriod(LedgerPeriod period) => switch (period) {
    LedgerPeriod.session => session,
    LedgerPeriod.lifetime => lifetime,
  };

  LedgerPeriodData get sessionData => session;
  LedgerPeriodData get lifetimeData => lifetime;
}
