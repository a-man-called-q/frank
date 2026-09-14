part of 'ledger_surface.dart';

class _LedgerThresholds extends StatelessWidget {
  const _LedgerThresholds({required this.data});

  final LedgerPeriodData data;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 430;
        final children = [
          _LedgerThreshold(
            label: 'Sessions',
            current: data.evidenceCountsAvailable ? data.sessionCount : null,
            required: LedgerDashboardData.minimumSessionsForLifetimeVerdict,
          ),
          _LedgerThreshold(
            label: 'Turns',
            current: data.evidenceCountsAvailable ? data.turnCount : null,
            required: LedgerDashboardData.minimumTurnsForLifetimeVerdict,
          ),
        ];
        return Padding(
          padding: const EdgeInsets.all(20),
          child: compact
              ? Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    children[0],
                    const SizedBox(height: 18),
                    children[1],
                  ],
                )
              : Row(
                  children: [
                    Expanded(child: children[0]),
                    const SizedBox(width: 24),
                    Expanded(child: children[1]),
                  ],
                ),
        );
      },
    );
  }
}

class _LedgerThreshold extends StatelessWidget {
  const _LedgerThreshold({
    required this.label,
    required this.current,
    required this.required,
  });

  final String label;
  final int? current;
  final int required;

  @override
  Widget build(BuildContext context) {
    final ratio = current == null
        ? 0.0
        : (current! / required).clamp(0.0, 1.0).toDouble();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Wrap(
          alignment: WrapAlignment.spaceBetween,
          spacing: 8,
          runSpacing: 2,
          children: [
            Text(
              label,
              style: const TextStyle(color: FrankColors.ink, fontSize: 12),
            ),
            Text(
              current == null ? 'Not reported' : '$current / $required',
              style: const TextStyle(
                color: FrankColors.ink,
                fontFamily: FrankTypography.monoFontFamily,
                fontSize: 12,
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        ClipRRect(
          borderRadius: BorderRadius.circular(99),
          child: SizedBox(
            height: 5,
            child: Stack(
              fit: StackFit.expand,
              children: [
                const ColoredBox(color: FrankColors.border),
                FractionallySizedBox(
                  alignment: Alignment.centerLeft,
                  widthFactor: ratio,
                  child: const ColoredBox(color: FrankColors.warningAmber),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class _LedgerSessionEvidence extends StatelessWidget {
  const _LedgerSessionEvidence({required this.data});

  final LedgerPeriodData data;

  @override
  Widget build(BuildContext context) {
    final bases = data.attribution.map((row) => row.basis).toSet();
    final basisLabel = data.attribution.isEmpty
        ? 'Not reported'
        : bases.length == 1
        ? bases.single.label
        : 'multiple attribution bases';
    return Padding(
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'Session evidence',
            style: TextStyle(
              color: FrankColors.ink,
              fontSize: 13,
              fontWeight: FontWeight.w500,
            ),
          ),
          const SizedBox(height: 13),
          Wrap(
            spacing: 24,
            runSpacing: 16,
            children: [
              _LedgerEvidenceStat(
                label: 'Turns',
                value: data.evidenceCountsAvailable
                    ? '${data.turnCount}'
                    : 'Not reported',
              ),
              _LedgerEvidenceStat(label: 'Model', value: data.model),
              _LedgerEvidenceStat(label: 'Basis', value: basisLabel),
            ],
          ),
        ],
      ),
    );
  }
}

class _LedgerEvidenceStat extends StatelessWidget {
  const _LedgerEvidenceStat({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: const BoxConstraints(minWidth: 82, maxWidth: 190),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: const TextStyle(color: FrankColors.muted, fontSize: 11),
          ),
          const SizedBox(height: 5),
          Text(
            value,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: FrankColors.ink, fontSize: 13),
          ),
        ],
      ),
    );
  }
}

class _LedgerMeasures extends StatelessWidget {
  const _LedgerMeasures({required this.data, required this.compact});

  final LedgerPeriodData data;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final totals = data.totals;
    final cards = [
      _LedgerMetricCard(
        key: const ValueKey('ledger-metric-input'),
        label: 'Measured input',
        value: _formatCount(totals.measuredInputTokens),
        detail: 'input + cache creation tokens',
      ),
      _LedgerMetricCard(
        key: const ValueKey('ledger-metric-output'),
        label: 'Measured output',
        value: _formatCount(totals.measuredOutputTokens),
        detail: 'user-facing turns; sidechains separate',
      ),
      _LedgerMetricCard(
        key: const ValueKey('ledger-metric-injected'),
        label: 'Frank injected',
        value: data.injectedBytesAvailable
            ? '${_formatCount(totals.injectedBytes)} B'
            : 'Not reported',
        detail: data.injectedBytesAvailable
            ? '${_formatCount(totals.activationBytes)} activation + ${_formatCount(totals.reinforcementBytes)} reinforcement'
            : 'not included in the remote usage payload',
      ),
    ];
    return Container(
      key: const ValueKey('ledger-measures'),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: compact
          ? Column(children: _withDividers(cards))
          : Row(children: [for (final card in cards) Expanded(child: card)]),
    );
  }

  List<Widget> _withDividers(List<Widget> cards) {
    final result = <Widget>[];
    for (var index = 0; index < cards.length; index++) {
      if (index > 0) {
        result.add(const FDivider());
      }
      result.add(cards[index]);
    }
    return result;
  }
}

class _LedgerMetricCard extends StatelessWidget {
  const _LedgerMetricCard({
    required this.label,
    required this.value,
    required this.detail,
    super.key,
  });

  final String label;
  final String value;
  final String detail;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: const BoxDecoration(
        border: Border(right: BorderSide(color: FrankColors.border)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 6,
                height: 6,
                decoration: const BoxDecoration(
                  color: FrankColors.green,
                  shape: BoxShape.circle,
                ),
              ),
              const SizedBox(width: 7),
              Flexible(
                child: Text(
                  label,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 11,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            value,
            style: const TextStyle(
              color: FrankColors.ink,
              fontFamily: FrankTypography.monoFontFamily,
              fontSize: 20,
              fontWeight: FontWeight.w500,
              letterSpacing: -0.3,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            detail,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: FrankColors.muted,
              fontSize: 11,
              height: 1.35,
            ),
          ),
        ],
      ),
    );
  }
}

class _LedgerTrendPanel extends StatelessWidget {
  const _LedgerTrendPanel({required this.data});

  final LedgerPeriodData data;

  @override
  Widget build(BuildContext context) {
    final chartLabel = data.period == LedgerPeriod.lifetime
        ? 'Weekly measured output tokens with a separate estimated savings range'
        : 'Session measured output tokens with a separate estimated savings range';
    return Container(
      key: const ValueKey('ledger-trend-panel'),
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 13),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            alignment: WrapAlignment.spaceBetween,
            runSpacing: 8,
            children: [
              const Text(
                'Usage and possible savings',
                style: TextStyle(
                  color: FrankColors.ink,
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                ),
              ),
              const _LedgerChartLegend(),
            ],
          ),
          const SizedBox(height: 12),
          Semantics(
            image: true,
            label: chartLabel,
            child: data.trend.isEmpty
                ? SizedBox(
                    height: 96,
                    child: Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          const Icon(
                            FrankIcons.timelineOutlined,
                            size: 20,
                            color: FrankColors.muted,
                          ),
                          const SizedBox(height: 7),
                          Text(
                            data.isIncomplete
                                ? 'Trend unavailable from server'
                                : 'No measured usage yet',
                            style: const TextStyle(
                              color: FrankColors.muted,
                              fontSize: 12,
                            ),
                          ),
                        ],
                      ),
                    ),
                  )
                : SizedBox(
                    height: 190,
                    child: CustomPaint(
                      key: const ValueKey('ledger-trend-chart'),
                      painter: LedgerAxisTrendPainter(data.trend),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _LedgerChartLegend extends StatelessWidget {
  const _LedgerChartLegend();

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: 14,
      runSpacing: 6,
      children: const [
        _LedgerLegendItem(
          color: FrankColors.green,
          label: 'Measured output',
          line: true,
        ),
        _LedgerLegendItem(
          color: FrankColors.aubergineAccent,
          label: 'Estimated saved range',
          line: false,
        ),
      ],
    );
  }
}

class _LedgerLegendItem extends StatelessWidget {
  const _LedgerLegendItem({
    required this.color,
    required this.label,
    required this.line,
  });

  final Color color;
  final String label;
  final bool line;

  @override
  Widget build(BuildContext context) {
    final marker = Container(
      width: 16,
      height: line ? 2 : 7,
      decoration: BoxDecoration(
        color: line ? color : color.withValues(alpha: .16),
        border: line ? null : Border.all(color: color),
      ),
    );
    final metrics = OfficeLayoutMetricsScope.maybeOf(context);
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact =
            metrics?.isCompact == true ||
            (constraints.hasBoundedWidth && constraints.maxWidth < 260);
        if (!compact) {
          return Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              marker,
              const SizedBox(width: 6),
              Text(
                label,
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            marker,
            const SizedBox(width: 6),
            Expanded(
              child: Text(
                label,
                softWrap: true,
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ),
          ],
        );
      },
    );
  }
}
