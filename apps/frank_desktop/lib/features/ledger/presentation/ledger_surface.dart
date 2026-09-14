import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import '../../../app/icons.dart';
import '../../../app/layout/office_surface_frame.dart';
import '../../../app/office_ui.dart';
import '../../../app/theme.dart';
import '../../../core/models/ledger_models.dart';
import '../../../core/models/workspace_models.dart';
import '../ledger_projection.dart';
import 'ledger_trend_painter.dart';

part 'ledger_operational.dart';
part 'ledger_evidence.dart';
part 'ledger_metrics.dart';
part 'ledger_table.dart';

LedgerDashboardData _emptyLedgerDashboard() => const LedgerDashboardData(
  session: LedgerPeriodData(
    period: LedgerPeriod.session,
    sessionCount: 0,
    turnCount: 0,
    totals: LedgerTotals(),
    trend: [],
    attribution: [],
  ),
  lifetime: LedgerPeriodData(
    period: LedgerPeriod.lifetime,
    sessionCount: 0,
    turnCount: 0,
    totals: LedgerTotals(),
    trend: [],
    attribution: [],
  ),
);

/// Read-only Ledger surface for the Office destination.
///
/// The presentation model is intentionally injectable. The current milestone
/// uses deterministic local data supplied by the shell composition root; a
/// later gateway adapter can provide the same [LedgerDashboardData] without
/// changing this widget's rendering contract.
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
    _dashboard = widget.data ?? _emptyLedgerDashboard();
  }

  @override
  void didUpdateWidget(covariant LedgerSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.data != oldWidget.data ||
        widget.workspace != oldWidget.workspace) {
      _dashboard = widget.data ?? _emptyLedgerDashboard();
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
          final compact = width < 720;
          final content = Column(
            key: const ValueKey('ledger-surface'),
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_tab == LedgerTab.operational)
                _LedgerOperationalView(
                  data: LedgerProjection.operational(
                    operationalData,
                    filter: _filter,
                  ),
                  compact: compact,
                  onFilterChanged: (filter) => setState(() => _filter = filter),
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
          );
          return OfficeSurfaceFrame.page(
            key: const ValueKey('ledger-page-frame'),
            fullWidth: true,
            scrollKey: const ValueKey('ledger-scroll'),
            header: _LedgerHeader(
              tab: _tab,
              period: _period,
              isFixture: widget.data?.isFixture ?? false,
              onTabChanged: (tab) => setState(() => _tab = tab),
              onPeriodChanged: (period) => setState(() => _period = period),
            ),
            slivers: [SliverToBoxAdapter(child: content)],
          );
        },
      ),
    );
  }
}
