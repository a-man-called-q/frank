part of 'ledger_surface.dart';

class _LedgerAttributionPanel extends StatelessWidget {
  const _LedgerAttributionPanel({required this.data});

  final LedgerPeriodData data;

  @override
  Widget build(BuildContext context) {
    final measuredOutput = data.attribution.fold<int>(
      0,
      (sum, row) => sum + row.measuredOutputTokens,
    );
    final attributedOutput = data.attribution
        .where((row) => !row.isExcluded)
        .fold<int>(0, (sum, row) => sum + row.measuredOutputTokens);
    final attributedPercent = measuredOutput == 0
        ? 0
        : (attributedOutput / measuredOutput * 100).round();
    return Container(
      key: const ValueKey('ledger-attribution-panel'),
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 14, 16, 13),
            child: Wrap(
              alignment: WrapAlignment.spaceBetween,
              runSpacing: 7,
              children: [
                const Text(
                  'Attribution breakdown',
                  style: TextStyle(
                    color: FrankColors.ink,
                    fontSize: 13,
                    fontWeight: FontWeight.w500,
                  ),
                ),
                if (data.attribution.isEmpty)
                  const Text(
                    'Not reported by server',
                    style: TextStyle(color: FrankColors.muted, fontSize: 11),
                  )
                else
                  Text.rich(
                    TextSpan(
                      children: [
                        TextSpan(
                          text: '$attributedPercent%',
                          style: const TextStyle(
                            color: FrankColors.green,
                            fontWeight: FontWeight.w500,
                          ),
                        ),
                        const TextSpan(
                          text: ' attributed · ',
                          style: TextStyle(color: FrankColors.muted),
                        ),
                        TextSpan(
                          text: '${100 - attributedPercent}%',
                          style: const TextStyle(
                            color: FrankColors.warningAmber,
                            fontWeight: FontWeight.w500,
                          ),
                        ),
                        const TextSpan(
                          text: ' excluded from estimates',
                          style: TextStyle(color: FrankColors.muted),
                        ),
                      ],
                      style: const TextStyle(fontSize: 11),
                    ),
                  ),
              ],
            ),
          ),
          const FDivider(),
          if (data.attribution.isEmpty)
            const Padding(
              padding: EdgeInsets.all(18),
              child: Text(
                'The remote usage payload does not include attribution basis. No savings estimate is shown.',
                style: TextStyle(color: FrankColors.muted, fontSize: 12),
              ),
            )
          else
            Semantics(
              container: true,
              label: 'Ledger attribution table',
              child: SingleChildScrollView(
                key: const ValueKey('ledger-attribution-scroll'),
                scrollDirection: Axis.horizontal,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(minWidth: 900),
                  child: Table(
                    defaultVerticalAlignment: TableCellVerticalAlignment.middle,
                    columnWidths: const {
                      0: FlexColumnWidth(2.2),
                      1: FixedColumnWidth(68),
                      2: FixedColumnWidth(118),
                      3: FixedColumnWidth(100),
                      4: FixedColumnWidth(118),
                      5: FixedColumnWidth(130),
                      6: FlexColumnWidth(1.2),
                    },
                    border: TableBorder(
                      horizontalInside: BorderSide(color: FrankColors.border),
                    ),
                    children: [
                      TableRow(children: _LedgerTableRow.header().cells),
                      for (final row in data.attribution)
                        TableRow(children: _LedgerTableRow.data(row).cells),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _LedgerTableRow {
  const _LedgerTableRow._(this.cells);

  final List<Widget> cells;

  factory _LedgerTableRow.header() => _LedgerTableRow._([
    const _LedgerTableCell('Agent / bucket', header: true),
    const _LedgerTableCell('Mode', header: true),
    const _LedgerTableCell('Measured input', header: true, numeric: true),
    const _LedgerTableCell('Cache read', header: true, numeric: true),
    const _LedgerTableCell('Measured output', header: true, numeric: true),
    const _LedgerTableCell('Estimated saved', header: true, numeric: true),
    const _LedgerTableCell('Basis', header: true),
  ]);

  factory _LedgerTableRow.data(LedgerAttributionRow row) {
    final excluded = row.isExcluded;
    final valueColor = excluded ? FrankColors.warningAmber : FrankColors.ink;
    return _LedgerTableRow._([
      _LedgerTableCell(
        row.label,
        color: excluded ? FrankColors.warningAmber : FrankColors.muted,
        maxLines: 1,
      ),
      _LedgerTableCell(row.mode, color: FrankColors.muted),
      _LedgerTableCell(
        _formatCount(row.measuredInput),
        color: valueColor,
        numeric: true,
      ),
      _LedgerTableCell(
        _formatCount(row.cacheReadTokens),
        color: valueColor,
        numeric: true,
      ),
      _LedgerTableCell(
        _formatCount(row.measuredOutput),
        color: valueColor,
        numeric: true,
      ),
      _LedgerTableCell(
        row.isExcluded ? 'excluded' : _formatRange(row.estimatedSaved),
        color: row.isExcluded
            ? FrankColors.warningAmber
            : FrankColors.aubergineAccent,
        numeric: true,
      ),
      _LedgerTableCell(
        row.basis.label,
        color: row.isExcluded ? FrankColors.warningAmber : FrankColors.muted,
        marker: row.isExcluded ? FrankColors.warningAmber : FrankColors.green,
      ),
    ]);
  }
}

class _LedgerTableCell extends StatelessWidget {
  const _LedgerTableCell(
    this.text, {
    this.header = false,
    this.numeric = false,
    this.color,
    this.marker,
    this.maxLines,
  });

  final String text;
  final bool header;
  final bool numeric;
  final Color? color;
  final Color? marker;
  final int? maxLines;

  @override
  Widget build(BuildContext context) {
    final child = Text(
      text,
      maxLines: maxLines,
      overflow: maxLines == null ? TextOverflow.visible : TextOverflow.ellipsis,
      textAlign: numeric ? TextAlign.right : TextAlign.left,
      style: TextStyle(
        color: color ?? (header ? FrankColors.muted : FrankColors.ink),
        fontFamily: numeric && !header ? FrankTypography.monoFontFamily : null,
        fontSize: header ? 10 : 11,
        fontWeight: header ? FontWeight.w400 : FontWeight.w400,
        letterSpacing: header ? .2 : 0,
      ),
    );
    final content = marker == null
        ? child
        : Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Container(
                width: 5,
                height: 5,
                decoration: BoxDecoration(
                  color: marker,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 6),
              Flexible(child: child),
            ],
          );
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 11),
      child: content,
    );
  }
}

class _LedgerHonestyNote extends StatelessWidget {
  const _LedgerHonestyNote();

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: 'Measured and estimated values are kept separate',
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(FrankIcons.circle, size: 14, color: FrankColors.muted),
          const SizedBox(width: 8),
          const Expanded(
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text:
                        'Measured and estimated values are never added together. ',
                    style: TextStyle(
                      color: FrankColors.ink,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                  TextSpan(
                    text:
                        'Estimated savings come from the active pack benchmark and always remain a range.',
                    style: TextStyle(color: FrankColors.muted),
                  ),
                ],
                style: TextStyle(fontSize: 11, height: 1.45),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _LedgerEyebrow extends StatelessWidget {
  const _LedgerEyebrow(this.text);

  final String text;

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: const TextStyle(
        color: FrankColors.muted,
        fontSize: 10,
        letterSpacing: 1.1,
      ),
    );
  }
}

String _formatSummary(LedgerMetricSummary summary) {
  final value = summary.value;
  return value == null ? 'Not reported' : _formatCount(value);
}

String _formatMetric(LedgerMetricSummary summary) {
  final value = summary.value;
  return value == null ? '—' : _formatCount(value);
}

String _formatOptional(int? value) =>
    value == null ? 'not reported' : _formatCount(value);

String _coverageLabel(LedgerMetricSummary summary) {
  if (summary.totalRows == 0) return 'no records in range';
  if (summary.complete) return '${summary.totalRows} rows reported';
  return '${summary.reportedRows} of ${summary.totalRows} rows reported';
}

String _formatCost(LedgerCostSummary summary) {
  final micros = summary.micros;
  if (micros == null) return 'Not reported';
  final dollars = micros / 1000000;
  // Keep small fixture amounts legible. Rounding every sub-cent aggregate to
  // "$0.00" would look indistinguishable from a reported zero.
  final decimals = micros == 0
      ? 2
      : dollars.abs() >= 0.01
      ? 2
      : dollars.abs() >= 0.001
      ? 4
      : 6;
  return '\$${dollars.toStringAsFixed(decimals)}';
}

String _costCoverageLabel(LedgerCostSummary summary) {
  if (summary.totalRows == 0) return 'no records in range';
  if (summary.complete) return '${summary.totalRows} rows reported';
  return '${summary.reportedRows} of ${summary.totalRows} rows reported';
}

String _formatCount(int value) {
  final sign = value < 0 ? '-' : '';
  final digits = value.abs().toString();
  final groups = <String>[];
  for (var end = digits.length; end > 0; end -= 3) {
    final start = math.max(0, end - 3);
    groups.insert(0, digits.substring(start, end));
  }
  return '$sign${groups.join(',')}';
}

String _formatRange(TokenRange? range) {
  if (range == null) return '—';
  return '${formatCompactCount(range.low)}–${formatCompactCount(range.high)}';
}
