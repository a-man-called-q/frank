part of 'ledger_surface.dart';

class _LedgerHeader extends StatelessWidget {
  const _LedgerHeader({
    required this.tab,
    required this.period,
    required this.isFixture,
    required this.onTabChanged,
    required this.onPeriodChanged,
  });

  final LedgerTab tab;
  final LedgerPeriod period;
  final bool isFixture;
  final ValueChanged<LedgerTab> onTabChanged;
  final ValueChanged<LedgerPeriod> onPeriodChanged;

  @override
  Widget build(BuildContext context) {
    final controls = Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        _LedgerTabToggle(selected: tab, onChanged: onTabChanged),
        if (tab == LedgerTab.effectiveness)
          _LedgerPeriodToggle(selected: period, onChanged: onPeriodChanged),
        if (isFixture) const FrankSampleDataBadge(),
      ],
    );

    return OfficePageHeader(
      title: 'Ledger',
      description:
          'What was measured, what was estimated, and what Frank refuses to guess.',
      actions: controls,
    );
  }
}

class _LedgerTabToggle extends StatelessWidget {
  const _LedgerTabToggle({required this.selected, required this.onChanged});

  final LedgerTab selected;
  final ValueChanged<LedgerTab> onChanged;

  @override
  Widget build(BuildContext context) => FrankSegmentedControl<LedgerTab>(
    value: selected,
    items: const [
      (LedgerTab.operational, 'Operational', null),
      (LedgerTab.effectiveness, 'Frank Effectiveness', null),
    ],
    itemKeyBuilder: (tab) => ValueKey('ledger-tab-${tab.name}'),
    onChanged: onChanged,
  );
}

class _LedgerPeriodToggle extends StatelessWidget {
  const _LedgerPeriodToggle({required this.selected, required this.onChanged});

  final LedgerPeriod selected;
  final ValueChanged<LedgerPeriod> onChanged;

  @override
  Widget build(BuildContext context) => FrankSegmentedControl<LedgerPeriod>(
    value: selected,
    items: const [
      (LedgerPeriod.session, 'This session', null),
      (LedgerPeriod.lifetime, 'Lifetime', null),
    ],
    itemKeyBuilder: (period) => ValueKey('ledger-period-${period.name}'),
    onChanged: onChanged,
  );
}

// Operational rendering consumes the pure projection seam; a future gateway
// adapter can swap its source data without changing this surface.
class _LedgerOperationalView extends StatelessWidget {
  const _LedgerOperationalView({
    required this.data,
    required this.compact,
    required this.onFilterChanged,
  });

  final LedgerOperationalProjection data;
  final bool compact;
  final ValueChanged<LedgerFilter> onFilterChanged;

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('ledger-operational-view'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _LedgerOperationalIntro(data: data),
        const SizedBox(height: 14),
        _LedgerOperationalFilters(data: data, onChanged: onFilterChanged),
        const SizedBox(height: 14),
        _LedgerOperationalMetrics(data: data, compact: compact),
        const SizedBox(height: 14),
        _LedgerOperationalTrend(data: data),
        const SizedBox(height: 14),
        _LedgerOperationalGroups(data: data),
        if (data.excludedEntries.isNotEmpty) ...[
          const SizedBox(height: 12),
          _LedgerOperationalExclusions(data: data),
        ],
        const SizedBox(height: 12),
        const _LedgerOperationalHonestyNote(),
      ],
    );
  }
}

class _LedgerOperationalIntro extends StatelessWidget {
  const _LedgerOperationalIntro({required this.data});

  final LedgerOperationalProjection data;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        const Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _LedgerEyebrow('RUNTIME USAGE'),
              SizedBox(height: 6),
              Text(
                'Provider-reported usage by project and agent.',
                style: TextStyle(
                  color: FrankColors.ink,
                  fontSize: 15,
                  fontWeight: FontWeight.w500,
                ),
              ),
              SizedBox(height: 4),
              Text(
                'Measured and estimated fields stay separate; missing values stay visible.',
                style: TextStyle(color: FrankColors.muted, fontSize: 12),
              ),
            ],
          ),
        ),
        const SizedBox(width: 16),
        Text(
          '${data.entries.length} ${data.entries.length == 1 ? 'record' : 'records'}',
          style: const TextStyle(
            color: FrankColors.muted,
            fontFamily: FrankTypography.monoFontFamily,
            fontSize: 11,
          ),
        ),
      ],
    );
  }
}

class _LedgerOperationalFilters extends StatelessWidget {
  const _LedgerOperationalFilters({
    required this.data,
    required this.onChanged,
  });

  final LedgerOperationalProjection data;
  final ValueChanged<LedgerFilter> onChanged;

  static const _all = '__all__';

  @override
  Widget build(BuildContext context) {
    final projects = <String, String>{
      for (final entry in data.entries)
        if (entry.projectId != null && entry.projectName != null)
          entry.projectId!: entry.projectName!,
    };
    final agents = <String, String>{
      for (final entry in data.entries)
        if (entry.agentId != null && entry.agentName != null)
          entry.agentId!: entry.agentName!,
    };
    final providers = <String, String>{
      for (final entry in data.entries) entry.provider: entry.provider,
    };
    return Container(
      key: const ValueKey('ledger-operational-filters'),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(11),
        border: Border.all(color: FrankColors.border),
      ),
      child: Wrap(
        spacing: 7,
        runSpacing: 7,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          const Text(
            'Range',
            style: TextStyle(color: FrankColors.muted, fontSize: 11),
          ),
          for (final range in LedgerRange.values)
            _LedgerRangeButton(
              range: range,
              selected: data.filter.range == range,
              onPressed: () => onChanged(data.filter.copyWith(range: range)),
            ),
          _LedgerFilterMenu(
            key: const ValueKey('ledger-filter-project'),
            allLabel: 'All projects',
            selectedValue: data.filter.projectId ?? _all,
            options: projects,
            onSelected: (value) => onChanged(
              data.filter.copyWith(projectId: value == _all ? null : value),
            ),
          ),
          _LedgerFilterMenu(
            key: const ValueKey('ledger-filter-agent'),
            allLabel: 'All agents',
            selectedValue: data.filter.agentId ?? _all,
            options: agents,
            onSelected: (value) => onChanged(
              data.filter.copyWith(agentId: value == _all ? null : value),
            ),
          ),
          _LedgerFilterMenu(
            key: const ValueKey('ledger-filter-provider'),
            allLabel: 'All providers',
            selectedValue: data.filter.provider ?? _all,
            options: providers,
            onSelected: (value) => onChanged(
              data.filter.copyWith(provider: value == _all ? null : value),
            ),
          ),
          if (data.filter.hasFilters)
            FButton(
              key: const ValueKey('ledger-filter-reset'),
              onPress: () => onChanged(const LedgerFilter()),
              variant: FButtonVariant.ghost,
              size: FButtonSizeVariant.sm,
              child: const Text('Reset'),
            ),
        ],
      ),
    );
  }
}

class _LedgerRangeButton extends StatelessWidget {
  const _LedgerRangeButton({
    required this.range,
    required this.selected,
    required this.onPressed,
  });

  final LedgerRange range;
  final bool selected;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return FButton(
      key: ValueKey('ledger-range-${range.name}'),
      onPress: onPressed,
      variant: selected ? FButtonVariant.secondary : FButtonVariant.ghost,
      size: FButtonSizeVariant.sm,
      child: Flexible(
        child: Text(range.label, maxLines: 1, overflow: TextOverflow.ellipsis),
      ),
    );
  }
}

class _LedgerFilterMenu extends StatelessWidget {
  const _LedgerFilterMenu({
    required this.allLabel,
    required this.selectedValue,
    required this.options,
    required this.onSelected,
    super.key,
  });

  final String allLabel;
  final String selectedValue;
  final Map<String, String> options;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    return FSelect<String>.rich(
      key: key,
      control: FSelectControl<String>.lifted(
        value: selectedValue,
        onChange: (value) {
          if (value != null) onSelected(value);
        },
      ),
      format: (value) => value == _LedgerOperationalFilters._all
          ? allLabel
          : options[value] ?? value,
      children: [
        FSelectItem<String>.item(
          value: _LedgerOperationalFilters._all,
          title: Text(allLabel),
        ),
        for (final option in options.entries)
          FSelectItem<String>.item(
            value: option.key,
            title: Text(option.value),
          ),
      ],
      hint: allLabel,
      label: Text(allLabel),
    );
  }
}

class _LedgerOperationalMetrics extends StatelessWidget {
  const _LedgerOperationalMetrics({required this.data, required this.compact});

  final LedgerOperationalProjection data;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final cards = [
      _LedgerOperationalMetricCard(
        key: const ValueKey('ledger-operational-measured-input'),
        label: 'Measured input',
        value: _formatSummary(data.measuredInput),
        detail: _coverageLabel(data.measuredInput),
        color: FrankColors.blue,
      ),
      _LedgerOperationalMetricCard(
        key: const ValueKey('ledger-operational-measured-output'),
        label: 'Measured output',
        value: _formatSummary(data.measuredOutput),
        detail: _coverageLabel(data.measuredOutput),
        color: FrankColors.green,
      ),
      _LedgerOperationalMetricCard(
        key: const ValueKey('ledger-operational-cost'),
        label: 'Reported cost',
        value: _formatCost(data.cost),
        detail: _costCoverageLabel(data.cost),
        color: FrankColors.aubergineAccent,
      ),
      _LedgerOperationalMetricCard(
        key: const ValueKey('ledger-operational-estimated'),
        label: 'Estimated input',
        value: _formatSummary(data.estimatedInput),
        detail: 'estimated field · never added to measured',
        color: FrankColors.warningAmber,
      ),
      _LedgerOperationalMetricCard(
        key: const ValueKey('ledger-operational-estimated-output'),
        label: 'Estimated output',
        value: _formatSummary(data.estimatedOutput),
        detail: 'estimated field · never added to measured',
        color: FrankColors.warningAmber,
      ),
    ];
    final content = compact
        ? Column(children: _operationalMetricDividers(cards))
        : Row(children: [for (final card in cards) Expanded(child: card)]);
    return Container(
      key: const ValueKey('ledger-operational-metrics'),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: content,
    );
  }

  List<Widget> _operationalMetricDividers(
    List<_LedgerOperationalMetricCard> cards,
  ) {
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

class _LedgerOperationalMetricCard extends StatelessWidget {
  const _LedgerOperationalMetricCard({
    required this.label,
    required this.value,
    required this.detail,
    required this.color,
    super.key,
  });

  final String label;
  final String value;
  final String detail;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(15),
      decoration: const BoxDecoration(
        border: Border(right: BorderSide(color: FrankColors.border)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Container(
                width: 6,
                height: 6,
                decoration: BoxDecoration(color: color, shape: BoxShape.circle),
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
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: FrankColors.ink,
              fontFamily: FrankTypography.monoFontFamily,
              fontSize: 18,
              fontWeight: FontWeight.w500,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            detail,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: FrankColors.muted,
              fontSize: 10,
              height: 1.35,
            ),
          ),
        ],
      ),
    );
  }
}

class _LedgerOperationalTrend extends StatelessWidget {
  const _LedgerOperationalTrend({required this.data});

  final LedgerOperationalProjection data;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const ValueKey('ledger-operational-trend'),
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 14),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              const Expanded(
                child: Text(
                  'Measured usage trend',
                  style: TextStyle(
                    color: FrankColors.ink,
                    fontSize: 13,
                    fontWeight: FontWeight.w500,
                  ),
                ),
              ),
              Text(
                '${data.trend.length} points',
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Semantics(
            image: true,
            label: 'Operational measured input and output trend',
            child: SizedBox(
              height: 174,
              child: data.trend.isEmpty
                  ? const Center(
                      child: Text(
                        'No measured usage in this range',
                        style: TextStyle(
                          color: FrankColors.muted,
                          fontSize: 12,
                        ),
                      ),
                    )
                  : CustomPaint(
                      painter: LedgerTrendPainter(points: data.trend),
                    ),
            ),
          ),
          const SizedBox(height: 8),
          const Wrap(
            spacing: 14,
            runSpacing: 5,
            children: [
              _LedgerLegendItem(
                color: FrankColors.blue,
                label: 'Measured input',
                line: true,
              ),
              _LedgerLegendItem(
                color: FrankColors.aubergineAccent,
                label: 'Measured output',
                line: true,
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _LedgerOperationalGroups extends StatelessWidget {
  const _LedgerOperationalGroups({required this.data});

  final LedgerOperationalProjection data;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const ValueKey('ledger-operational-groups'),
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Padding(
            padding: EdgeInsets.fromLTRB(16, 14, 16, 12),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'By agent',
                    style: TextStyle(
                      color: FrankColors.ink,
                      fontSize: 13,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
                Text(
                  'Input',
                  style: TextStyle(color: FrankColors.muted, fontSize: 10),
                ),
                SizedBox(width: 70),
                Text(
                  'Output',
                  style: TextStyle(color: FrankColors.muted, fontSize: 10),
                ),
              ],
            ),
          ),
          const FDivider(),
          if (data.groups.isEmpty)
            const Padding(
              padding: EdgeInsets.all(18),
              child: Text(
                'No usage records in this range.',
                style: TextStyle(color: FrankColors.muted, fontSize: 12),
              ),
            )
          else
            for (var index = 0; index < data.groups.length; index++) ...[
              _LedgerOperationalGroupRow(group: data.groups[index]),
              if (index < data.groups.length - 1) const FDivider(),
            ],
        ],
      ),
    );
  }
}

class _LedgerOperationalGroupRow extends StatelessWidget {
  const _LedgerOperationalGroupRow({required this.group});

  final LedgerUsageGroup group;

  @override
  Widget build(BuildContext context) {
    final input = group.metric((entry) => entry.measuredInputTokens);
    final output = group.metric((entry) => entry.measuredOutputTokens);
    final color = group.excluded ? FrankColors.warningAmber : FrankColors.ink;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      child: Row(
        children: [
          Container(
            width: 6,
            height: 6,
            decoration: BoxDecoration(
              color: group.excluded
                  ? FrankColors.warningAmber
                  : FrankColors.green,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 9),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  group.label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: color, fontSize: 12),
                ),
                const SizedBox(height: 3),
                Text(
                  '${group.entries.length} ${group.entries.length == 1 ? 'record' : 'records'}${group.excluded ? ' · excluded' : ''}',
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 10,
                  ),
                ),
              ],
            ),
          ),
          SizedBox(
            width: 70,
            child: Text(
              _formatMetric(input),
              textAlign: TextAlign.right,
              style: TextStyle(
                color: color,
                fontFamily: FrankTypography.monoFontFamily,
                fontSize: 11,
              ),
            ),
          ),
          const SizedBox(width: 14),
          SizedBox(
            width: 70,
            child: Text(
              _formatMetric(output),
              textAlign: TextAlign.right,
              style: TextStyle(
                color: color,
                fontFamily: FrankTypography.monoFontFamily,
                fontSize: 11,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _LedgerOperationalExclusions extends StatelessWidget {
  const _LedgerOperationalExclusions({required this.data});

  final LedgerOperationalProjection data;

  @override
  Widget build(BuildContext context) {
    final labels =
        data.excludedEntries
            .map((entry) => entry.agentName)
            .whereType<String>()
            .toSet()
            .toList()
          ..sort();
    final input = data.excludedMeasuredInput;
    final output = data.excludedMeasuredOutput;
    return Container(
      key: const ValueKey('ledger-operational-exclusions'),
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: FrankColors.warningAmberSoft.withValues(alpha: .45),
        borderRadius: BorderRadius.circular(11),
        border: Border.all(
          color: FrankColors.warningAmber.withValues(alpha: .3),
        ),
      ),
      child: Wrap(
        spacing: 10,
        runSpacing: 5,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          const Icon(
            FrankIcons.circleDashed,
            size: 14,
            color: FrankColors.warningAmber,
          ),
          for (final label in labels)
            Text(
              label,
              style: const TextStyle(
                color: FrankColors.ink,
                fontSize: 11,
                fontWeight: FontWeight.w500,
              ),
            ),
          const Text(
            'excluded from derived totals',
            style: TextStyle(color: FrankColors.muted, fontSize: 11),
          ),
          Text(
            'input ${_formatOptional(input)} · output ${_formatOptional(output)}',
            style: const TextStyle(color: FrankColors.muted, fontSize: 11),
          ),
        ],
      ),
    );
  }
}

class _LedgerOperationalHonestyNote extends StatelessWidget {
  const _LedgerOperationalHonestyNote();

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label:
          'Operational measured, estimated, and excluded values stay separate',
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Icon(FrankIcons.circle, size: 14, color: FrankColors.muted),
          const SizedBox(width: 8),
          const Expanded(
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text:
                        'Measured, estimated, and excluded values are never combined. ',
                    style: TextStyle(
                      color: FrankColors.ink,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                  TextSpan(
                    text:
                        'A missing cost is shown as not reported, not as zero.',
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
