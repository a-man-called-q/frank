import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../core/fixtures/fixture_ledger.dart';
import '../../core/models/ledger_models.dart';
import '../../core/models/workspace_models.dart';
import 'ledger_projection.dart';
import 'ledger_trend_painter.dart';

/// Read-only, fixture-backed Ledger surface for the Office destination.
///
/// The presentation model is intentionally injectable. The current milestone
/// uses deterministic local data; a later gateway adapter can provide the same
/// [LedgerDashboardData] without changing this widget's rendering contract.
class LedgerSurface extends StatefulWidget {
  const LedgerSurface({required this.workspace, this.data, super.key});

  final OfficeWorkspace workspace;
  final LedgerDashboardData? data;

  @override
  State<LedgerSurface> createState() => _LedgerSurfaceState();
}

class _LedgerSurfaceState extends State<LedgerSurface> {
  late LedgerDashboardData _dashboard;
  // Effectiveness is the honest default: opening Ledger immediately shows
  // lifetime evidence and its threshold rather than an unqualified savings
  // headline.
  LedgerTab _tab = LedgerTab.effectiveness;
  LedgerPeriod _period = LedgerPeriod.lifetime;
  LedgerFilter _filter = const LedgerFilter();

  @override
  void initState() {
    super.initState();
    _dashboard = widget.data ?? fixtureLedgerDashboard(widget.workspace);
  }

  @override
  void didUpdateWidget(covariant LedgerSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.data != oldWidget.data ||
        (widget.data == null && widget.workspace != oldWidget.workspace)) {
      _dashboard = widget.data ?? fixtureLedgerDashboard(widget.workspace);
    }
  }

  @override
  Widget build(BuildContext context) {
    final periodData = _dashboard.forPeriod(_period);
    final operationalData =
        _dashboard.operational ??
        LedgerOperationalData(entries: const [], asOf: DateTime.utc(1970));
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Ledger',
      child: LayoutBuilder(
        builder: (context, constraints) {
          final width = constraints.maxWidth.isFinite
              ? constraints.maxWidth
              : MediaQuery.sizeOf(context).width;
          final compact = width < 700;
          return SingleChildScrollView(
            primary: false,
            child: Column(
              key: const ValueKey('ledger-surface'),
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _LedgerHeader(
                  tab: _tab,
                  period: _period,
                  compact: compact,
                  onTabChanged: (tab) => setState(() => _tab = tab),
                  onPeriodChanged: (period) => setState(() => _period = period),
                ),
                const SizedBox(height: 22),
                if (_tab == LedgerTab.operational)
                  _LedgerOperationalView(
                    data: LedgerProjection.operational(
                      operationalData,
                      filter: _filter,
                    ),
                    compact: compact,
                    onFilterChanged: (filter) =>
                        setState(() => _filter = filter),
                  )
                else ...[
                  _LedgerEvidenceCard(data: periodData, compact: compact),
                  const SizedBox(height: 14),
                  _LedgerMeasures(data: periodData, compact: compact),
                  const SizedBox(height: 14),
                  _LedgerTrendPanel(data: periodData),
                  const SizedBox(height: 14),
                  _LedgerAttributionPanel(data: periodData),
                  const SizedBox(height: 12),
                  const _LedgerHonestyNote(),
                ],
              ],
            ),
          );
        },
      ),
    );
  }
}

class _LedgerHeader extends StatelessWidget {
  const _LedgerHeader({
    required this.tab,
    required this.period,
    required this.compact,
    required this.onTabChanged,
    required this.onPeriodChanged,
  });

  final LedgerTab tab;
  final LedgerPeriod period;
  final bool compact;
  final ValueChanged<LedgerTab> onTabChanged;
  final ValueChanged<LedgerPeriod> onPeriodChanged;

  @override
  Widget build(BuildContext context) {
    final copy = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Ledger',
          style: TextStyle(
            color: FrankColors.ink,
            fontSize: 30,
            fontWeight: FontWeight.w500,
            letterSpacing: -0.7,
          ),
        ),
        const SizedBox(height: 7),
        const Text(
          'What was measured, what was estimated, and what Frank refuses to guess.',
          style: TextStyle(
            color: FrankColors.muted,
            fontSize: 14,
            height: 20 / 14,
          ),
        ),
      ],
    );
    final controls = Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        _LedgerTabToggle(selected: tab, onChanged: onTabChanged),
        if (tab == LedgerTab.effectiveness)
          _LedgerPeriodToggle(selected: period, onChanged: onPeriodChanged),
      ],
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (compact) copy else Row(children: [Expanded(child: copy)]),
        const SizedBox(height: 16),
        Align(
          alignment: compact ? Alignment.centerLeft : Alignment.centerRight,
          child: controls,
        ),
      ],
    );
  }
}

class _LedgerTabToggle extends StatelessWidget {
  const _LedgerTabToggle({required this.selected, required this.onChanged});

  final LedgerTab selected;
  final ValueChanged<LedgerTab> onChanged;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(9),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final tab in LedgerTab.values)
            Semantics(
              button: true,
              selected: selected == tab,
              label: tab.label,
              child: TextButton(
                key: ValueKey('ledger-tab-${tab.name}'),
                onPressed: () => onChanged(tab),
                style: TextButton.styleFrom(
                  minimumSize: const Size(0, 30),
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  foregroundColor: selected == tab
                      ? FrankColors.ink
                      : FrankColors.muted,
                  backgroundColor: selected == tab
                      ? FrankColors.aubergineSoft
                      : Colors.transparent,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(6),
                  ),
                  textStyle: const TextStyle(fontSize: 12),
                ),
                child: Text(tab.label),
              ),
            ),
        ],
      ),
    );
  }
}

class _LedgerPeriodToggle extends StatelessWidget {
  const _LedgerPeriodToggle({required this.selected, required this.onChanged});

  final LedgerPeriod selected;
  final ValueChanged<LedgerPeriod> onChanged;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(9),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final period in LedgerPeriod.values)
            Semantics(
              button: true,
              selected: selected == period,
              label: period.label,
              child: TextButton(
                key: ValueKey('ledger-period-${period.name}'),
                onPressed: () => onChanged(period),
                style: TextButton.styleFrom(
                  minimumSize: const Size(0, 30),
                  padding: const EdgeInsets.symmetric(horizontal: 11),
                  foregroundColor: selected == period
                      ? FrankColors.ink
                      : FrankColors.muted,
                  backgroundColor: selected == period
                      ? FrankColors.aubergineSoft
                      : Colors.transparent,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(6),
                  ),
                  textStyle: const TextStyle(fontSize: 12),
                ),
                child: Text(period.label),
              ),
            ),
        ],
      ),
    );
  }
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
      for (final entry in data.entries) entry.projectId: entry.projectName,
    };
    final agents = <String, String>{
      for (final entry in data.entries)
        if (entry.agentId != null) entry.agentId!: entry.agentName,
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
            label: data.filter.projectId == null
                ? 'All projects'
                : projects[data.filter.projectId] ?? 'Project',
            options: projects,
            onSelected: (value) => onChanged(
              data.filter.copyWith(projectId: value == _all ? null : value),
            ),
          ),
          _LedgerFilterMenu(
            key: const ValueKey('ledger-filter-agent'),
            label: data.filter.agentId == null
                ? 'All agents'
                : agents[data.filter.agentId] ?? 'Agent',
            options: agents,
            onSelected: (value) => onChanged(
              data.filter.copyWith(agentId: value == _all ? null : value),
            ),
          ),
          _LedgerFilterMenu(
            key: const ValueKey('ledger-filter-provider'),
            label: data.filter.provider == null
                ? 'All providers'
                : data.filter.provider!,
            options: providers,
            onSelected: (value) => onChanged(
              data.filter.copyWith(provider: value == _all ? null : value),
            ),
          ),
          if (data.filter.hasFilters)
            TextButton(
              key: const ValueKey('ledger-filter-reset'),
              onPressed: () => onChanged(const LedgerFilter()),
              style: TextButton.styleFrom(
                minimumSize: const Size(0, 30),
                padding: const EdgeInsets.symmetric(horizontal: 9),
                foregroundColor: FrankColors.muted,
              ),
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
    return TextButton(
      key: ValueKey('ledger-range-${range.name}'),
      onPressed: onPressed,
      style: TextButton.styleFrom(
        minimumSize: const Size(0, 30),
        padding: const EdgeInsets.symmetric(horizontal: 9),
        foregroundColor: selected ? FrankColors.ink : FrankColors.muted,
        backgroundColor: selected
            ? FrankColors.aubergineSoft
            : Colors.transparent,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
        textStyle: const TextStyle(fontSize: 11),
      ),
      child: Text(range.label),
    );
  }
}

class _LedgerFilterMenu extends StatelessWidget {
  const _LedgerFilterMenu({
    required this.label,
    required this.options,
    required this.onSelected,
    super.key,
  });

  final String label;
  final Map<String, String> options;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) {
    return Material(
      type: MaterialType.transparency,
      child: PopupMenuButton<String>(
        onSelected: onSelected,
        tooltip: label,
        itemBuilder: (context) => [
          const PopupMenuItem<String>(
            value: _LedgerOperationalFilters._all,
            child: Text('All'),
          ),
          for (final option in options.entries)
            PopupMenuItem<String>(value: option.key, child: Text(option.value)),
        ],
        child: Container(
          constraints: const BoxConstraints(minHeight: 30, maxWidth: 190),
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
          decoration: BoxDecoration(
            color: Colors.transparent,
            borderRadius: BorderRadius.circular(6),
            border: Border.all(color: FrankColors.border),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
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
              const SizedBox(width: 6),
              const Icon(
                FrankIcons.chevronDown,
                size: 13,
                color: FrankColors.muted,
              ),
            ],
          ),
        ),
      ),
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
        result.add(const Divider(height: 1, color: FrankColors.border));
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
          const Divider(height: 1, color: FrankColors.border),
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
              if (index < data.groups.length - 1)
                const Divider(height: 1, color: FrankColors.border),
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
        data.excludedEntries.map((entry) => entry.agentName).toSet().toList()
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

class _LedgerEvidenceCard extends StatelessWidget {
  const _LedgerEvidenceCard({required this.data, required this.compact});

  final LedgerPeriodData data;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final lifetime = data.period == LedgerPeriod.lifetime;
    final verdict = data.verdict;
    final verdictColor = verdict == LedgerVerdict.insufficientEvidence
        ? FrankColors.warningAmber
        : FrankColors.green;
    final verdictContent = Container(
      key: const ValueKey('ledger-verdict'),
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
        color: verdict == LedgerVerdict.insufficientEvidence
            ? FrankColors.warningAmberSoft.withValues(alpha: .52)
            : FrankColors.aubergineSoft.withValues(alpha: .55),
        border: compact
            ? const Border(bottom: BorderSide(color: FrankColors.border))
            : const Border(right: BorderSide(color: FrankColors.border)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          _LedgerEyebrow(lifetime ? 'LIFETIME VERDICT' : 'SESSION STATUS'),
          const SizedBox(height: 8),
          Row(
            children: [
              Icon(
                verdict == LedgerVerdict.insufficientEvidence
                    ? FrankIcons.circleDashed
                    : FrankIcons.circleCheck,
                size: 16,
                color: verdictColor,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  verdict.label,
                  style: TextStyle(
                    color: verdictColor,
                    fontSize: 16,
                    fontWeight: FontWeight.w500,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 7),
          Text(
            verdict.description,
            style: const TextStyle(
              color: FrankColors.muted,
              fontSize: 12,
              height: 1.5,
            ),
          ),
        ],
      ),
    );

    final evidenceContent = lifetime
        ? _LedgerThresholds(data: data)
        : _LedgerSessionEvidence(data: data);

    return Container(
      key: const ValueKey('ledger-evidence-card'),
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(
          color: verdict == LedgerVerdict.insufficientEvidence
              ? FrankColors.warningAmber.withValues(alpha: .34)
              : FrankColors.border,
        ),
      ),
      child: compact
          ? Column(children: [verdictContent, evidenceContent])
          : Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SizedBox(width: 282, child: verdictContent),
                Expanded(child: evidenceContent),
              ],
            ),
    );
  }
}

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
            current: data.sessionCount,
            required: LedgerDashboardData.minimumSessionsForLifetimeVerdict,
          ),
          _LedgerThreshold(
            label: 'Turns',
            current: data.turnCount,
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
  final int current;
  final int required;

  @override
  Widget build(BuildContext context) {
    final ratio = (current / required).clamp(0.0, 1.0).toDouble();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(
              label,
              style: const TextStyle(color: FrankColors.ink, fontSize: 12),
            ),
            Text(
              '$current / $required',
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
    final basisLabel = bases.length == 1
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
              _LedgerEvidenceStat(label: 'Turns', value: '${data.turnCount}'),
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
        value: '${_formatCount(totals.injectedBytes)} B',
        detail:
            '${_formatCount(totals.activationBytes)} activation + ${_formatCount(totals.reinforcementBytes)} reinforcement',
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
        result.add(const Divider(height: 1, color: FrankColors.border));
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
            child: SizedBox(
              height: 190,
              child: data.trend.isEmpty
                  ? const Center(
                      child: Text(
                        'No measured usage yet',
                        style: TextStyle(
                          color: FrankColors.muted,
                          fontSize: 12,
                        ),
                      ),
                    )
                  : CustomPaint(
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
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 16,
          height: line ? 2 : 7,
          decoration: BoxDecoration(
            color: line ? color : color.withValues(alpha: .16),
            border: line ? null : Border.all(color: color),
          ),
        ),
        const SizedBox(width: 6),
        Text(
          label,
          style: const TextStyle(color: FrankColors.muted, fontSize: 11),
        ),
      ],
    );
  }
}

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
          const Divider(height: 1, color: FrankColors.border),
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
