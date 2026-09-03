import '../../core/models/ledger_models.dart';

/// Pure, deterministic projections for the fixture-backed Ledger surface.
///
/// This layer is deliberately independent of Flutter and of the gateway. It
/// is the seam where a future Snapshot/SessionReport mapper can be introduced
/// without changing the presentation widgets.
abstract final class LedgerProjection {
  static LedgerOperationalProjection operational(
    LedgerOperationalData data, {
    LedgerFilter filter = const LedgerFilter(),
  }) {
    final entries = data.entries
        .where((entry) => _matches(entry, data.asOf, filter))
        .toList(growable: false);

    final attributed = entries
        .where((entry) => !entry.isExcluded)
        .toList(growable: false);
    final excluded = entries
        .where((entry) => entry.isExcluded)
        .toList(growable: false);
    final groups = _groups(entries);

    return LedgerOperationalProjection(
      filter: filter,
      entries: entries,
      groups: groups,
      measuredInput: _metric(attributed, (entry) => entry.measuredInputTokens),
      measuredOutput: _metric(
        attributed,
        (entry) => entry.measuredOutputTokens,
      ),
      estimatedInput: _metric(
        attributed,
        (entry) => entry.estimatedInputTokens,
      ),
      estimatedOutput: _metric(
        attributed,
        (entry) => entry.estimatedOutputTokens,
      ),
      cost: _cost(attributed),
      trend: _trend(attributed),
      excludedEntries: excluded,
    );
  }

  static LedgerEffectivenessProjection effectiveness(LedgerPeriodData data) {
    final included = data.attribution
        .where((row) => !row.isExcluded)
        .toList(growable: false);
    final excluded = data.attribution
        .where((row) => row.isExcluded)
        .toList(growable: false);
    final savings = _range(included.map((row) => row.estimatedSaved));
    final basisCounts = <LedgerAttributionBasis, int>{};
    for (final row in data.attribution) {
      basisCounts.update(row.basis, (count) => count + 1, ifAbsent: () => 1);
    }

    return LedgerEffectivenessProjection(
      data: data,
      attributedRows: included,
      excludedRows: excluded,
      estimatedSavings: savings,
      basisCounts: basisCounts,
    );
  }

  static bool _matches(
    LedgerUsageEntry entry,
    DateTime asOf,
    LedgerFilter filter,
  ) {
    if (filter.projectId != null && entry.projectId != filter.projectId) {
      return false;
    }
    if (filter.agentId != null && entry.agentId != filter.agentId) {
      return false;
    }
    if (filter.provider != null && entry.provider != filter.provider) {
      return false;
    }

    final recorded = entry.recordedAt.toUtc();
    final end = asOf.toUtc();
    if (recorded.isAfter(end)) return false;
    return switch (filter.range) {
      LedgerRange.allTime => true,
      LedgerRange.last7Days => !recorded.isBefore(
        end.subtract(const Duration(days: 7)),
      ),
      LedgerRange.last30Days => !recorded.isBefore(
        end.subtract(const Duration(days: 30)),
      ),
    };
  }

  static LedgerMetricSummary _metric(
    List<LedgerUsageEntry> entries,
    int? Function(LedgerUsageEntry entry) select,
  ) {
    final values = entries.map(select).whereType<int>().toList(growable: false);
    return LedgerMetricSummary(
      value: values.isEmpty
          ? null
          : values.fold<int>(0, (sum, value) => sum + value),
      reportedRows: values.length,
      totalRows: entries.length,
    );
  }

  static LedgerCostSummary _cost(List<LedgerUsageEntry> entries) {
    final values = entries
        .map((entry) => entry.costMicros)
        .whereType<int>()
        .toList(growable: false);
    return LedgerCostSummary(
      micros: values.isEmpty
          ? null
          : values.fold<int>(0, (sum, value) => sum + value),
      reportedRows: values.length,
      totalRows: entries.length,
    );
  }

  static List<LedgerUsageGroup> _groups(List<LedgerUsageEntry> entries) {
    final grouped = <String, List<LedgerUsageEntry>>{};
    for (final entry in entries) {
      final key = entry.agentId ?? 'unattributed';
      grouped.putIfAbsent(key, () => <LedgerUsageEntry>[]).add(entry);
    }

    return [
      for (final group in grouped.entries)
        LedgerUsageGroup(
          key: group.key,
          label: group.value.first.agentName,
          entries: List.unmodifiable(group.value),
          excluded: group.value.every((entry) => entry.isExcluded),
        ),
    ];
  }

  static List<LedgerTrendPoint> _trend(List<LedgerUsageEntry> entries) {
    final grouped = <DateTime, List<LedgerUsageEntry>>{};
    for (final entry in entries) {
      final date = DateTime.utc(
        entry.recordedAt.toUtc().year,
        entry.recordedAt.toUtc().month,
        entry.recordedAt.toUtc().day,
      );
      grouped.putIfAbsent(date, () => <LedgerUsageEntry>[]).add(entry);
    }
    final dates = grouped.keys.toList()..sort();
    return [
      for (final date in dates)
        LedgerTrendPoint(
          label: '${_month(date.month)} ${date.day}',
          measuredInputTokens: _sum(
            grouped[date]!,
            (entry) => entry.measuredInputTokens,
          ),
          measuredOutputTokens:
              _sum(grouped[date]!, (entry) => entry.measuredOutputTokens) ?? 0,
        ),
    ];
  }

  static int? _sum(
    List<LedgerUsageEntry> entries,
    int? Function(LedgerUsageEntry entry) select,
  ) {
    final values = entries.map(select).whereType<int>().toList(growable: false);
    if (values.isEmpty) return null;
    return values.fold<int>(0, (sum, value) => sum + value);
  }

  static TokenRange? _range(Iterable<TokenRange?> values) {
    final ranges = values.whereType<TokenRange>().toList(growable: false);
    if (ranges.isEmpty) return null;
    return TokenRange(
      ranges.fold<int>(0, (sum, value) => sum + value.low),
      ranges.fold<int>(0, (sum, value) => sum + value.high),
    );
  }

  static String _month(int month) => const [
    '',
    'Jan',
    'Feb',
    'Mar',
    'Apr',
    'May',
    'Jun',
    'Jul',
    'Aug',
    'Sep',
    'Oct',
    'Nov',
    'Dec',
  ][month];
}

class LedgerOperationalProjection {
  const LedgerOperationalProjection({
    required this.filter,
    required this.entries,
    required this.groups,
    required this.measuredInput,
    required this.measuredOutput,
    required this.estimatedInput,
    required this.estimatedOutput,
    required this.cost,
    required this.trend,
    required this.excludedEntries,
  });

  final LedgerFilter filter;
  final List<LedgerUsageEntry> entries;
  final List<LedgerUsageGroup> groups;
  final LedgerMetricSummary measuredInput;
  final LedgerMetricSummary measuredOutput;
  final LedgerMetricSummary estimatedInput;
  final LedgerMetricSummary estimatedOutput;
  final LedgerCostSummary cost;
  final List<LedgerTrendPoint> trend;
  final List<LedgerUsageEntry> excludedEntries;

  int? _excludedSum(int? Function(LedgerUsageEntry entry) select) {
    final values = excludedEntries.map(select).whereType<int>().toList();
    if (values.isEmpty) return null;
    return values.fold<int>(0, (sum, value) => sum + value);
  }

  int? get excludedMeasuredInput =>
      _excludedSum((entry) => entry.measuredInputTokens);

  int? get excludedMeasuredOutput =>
      _excludedSum((entry) => entry.measuredOutputTokens);

  bool get isEmpty => entries.isEmpty;
}

class LedgerEffectivenessProjection {
  const LedgerEffectivenessProjection({
    required this.data,
    required this.attributedRows,
    required this.excludedRows,
    required this.estimatedSavings,
    required this.basisCounts,
  });

  final LedgerPeriodData data;
  final List<LedgerAttributionRow> attributedRows;
  final List<LedgerAttributionRow> excludedRows;
  final TokenRange? estimatedSavings;
  final Map<LedgerAttributionBasis, int> basisCounts;

  bool get evidenceThresholdMet => data.hasLifetimeEvidence;
  bool get isEmpty => data.attribution.isEmpty;

  int get attributedMeasuredInput =>
      attributedRows.fold<int>(0, (sum, row) => sum + row.measuredInput);

  int get attributedMeasuredOutput =>
      attributedRows.fold<int>(0, (sum, row) => sum + row.measuredOutput);

  int get excludedMeasuredInput =>
      excludedRows.fold<int>(0, (sum, row) => sum + row.measuredInput);

  int get excludedMeasuredOutput =>
      excludedRows.fold<int>(0, (sum, row) => sum + row.measuredOutput);
}
