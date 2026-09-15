part of 'ledger_surface.dart';

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
        : FrankColors.statusSuccess;
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
