part of 'organization_surface.dart';

class _StaffCard extends StatelessWidget {
  const _StaffCard({required this.node, required this.employee, this.profile});

  final OrganizationNode node;
  final OfficeEmployee? employee;
  final TeamAgentProfile? profile;

  @override
  Widget build(BuildContext context) {
    final status = employee?.status ?? 'Available';
    final textScaleFactor = MediaQuery.textScalerOf(context).scale(1);
    final modelSummary = Text(
      profile == null ? 'Unconfigured' : profile!.modelSummary,
      maxLines: textScaleFactor > 1.25 ? 2 : 1,
      overflow: TextOverflow.ellipsis,
      textAlign: textScaleFactor > 1.25 ? TextAlign.start : TextAlign.end,
      style: const TextStyle(color: FrankColors.muted, fontSize: 10),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            FrankAgentAvatar(
              initials:
                  employee?.initials ??
                  node.label.characters.take(2).toString(),
              imageAsset: profile?.imageAsset,
              color: profile?.accent ?? FrankColors.aubergineAccent,
              size: 32,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    node.label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.ink,
                      fontSize: FrankUiTokens.textSize,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  Text(
                    employee?.role ?? 'Staff',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        const Spacer(),
        if (textScaleFactor > 1.25)
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              FrankStatusBadge(
                label: status,
                tone: _organizationStatusTone(status),
                icon: _organizationStatusIcon(status),
                compact: true,
              ),
              const SizedBox(height: 4),
              modelSummary,
            ],
          )
        else
          Row(
            children: [
              FrankStatusBadge(
                label: status,
                tone: _organizationStatusTone(status),
                icon: _organizationStatusIcon(status),
                compact: true,
              ),
              const Spacer(),
              const SizedBox(width: 5),
              Flexible(child: modelSummary),
            ],
          ),
      ],
    );
  }
}

class _CapabilityCard extends StatelessWidget {
  const _CapabilityCard({required this.node});

  final OrganizationNode node;

  @override
  Widget build(BuildContext context) {
    final capability = node.capability;
    final textScaleFactor = MediaQuery.textScalerOf(context).scale(1);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(
              capability?.icon ?? FrankIcons.workflow,
              color: FrankColors.muted,
              size: FrankUiTokens.iconSize,
            ),
            const SizedBox(width: 9),
            Expanded(
              child: Text(
                node.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: FrankUiTokens.textSize,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 6),
        ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 164),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
            decoration: BoxDecoration(
              color: FrankColors.panelRaised,
              borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
              border: Border.all(color: FrankColors.border),
            ),
            child: Text(
              node.connectorProfileLabel?.isNotEmpty == true
                  ? node.connectorProfileLabel!
                  : 'Profile not selected',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(color: FrankColors.muted, fontSize: 9),
            ),
          ),
        ),
        if (capability != null) ...[
          const SizedBox(height: 2),
          Text(
            capability.permissions.join(' · '),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: FrankColors.muted, fontSize: 9),
          ),
        ],
        const Spacer(),
        if (textScaleFactor > 1.25)
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _SetupBadge(configured: node.configured),
              if (node.approvalRequired) ...[
                const SizedBox(height: 4),
                const Text(
                  'Approval required',
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: organizationReviewColor,
                    fontSize: 8,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ],
          )
        else
          Row(
            children: [
              Flexible(child: _SetupBadge(configured: node.configured)),
              if (node.approvalRequired) ...[
                const SizedBox(width: 5),
                const Flexible(
                  child: Text(
                    'Approval required',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: organizationReviewColor,
                      fontSize: 8,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ],
            ],
          ),
      ],
    );
  }
}

class _ApprovalCard extends StatelessWidget {
  const _ApprovalCard({required this.node});

  final OrganizationNode node;

  @override
  Widget build(BuildContext context) {
    final textScaleFactor = MediaQuery.textScalerOf(context).scale(1);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            const Icon(
              FrankIcons.approval,
              color: organizationReviewColor,
              size: FrankUiTokens.iconSize,
            ),
            const SizedBox(width: 9),
            Expanded(
              child: Text(
                node.label,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontWeight: FontWeight.w600,
                  fontSize: FrankUiTokens.textSize,
                ),
              ),
            ),
          ],
        ),
        const Spacer(),
        Text(
          'Human checkpoint',
          maxLines: textScaleFactor > 1.25 ? 2 : 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: organizationReviewColor, fontSize: 11),
        ),
      ],
    );
  }
}

/// Compact renderer for the executable Organization v2 nodes. These cards are
/// intentionally reference-first: the role/board/workflow id is shown as a
/// stable badge while the daemon remains the source of truth for its details.
class _WorkflowCard extends StatelessWidget {
  const _WorkflowCard({required this.node});

  final OrganizationNode node;

  @override
  Widget build(BuildContext context) {
    final (icon, accent, kindLabel, reference) = switch (node.kind) {
      OrganizationNodeKind.role => (
        FrankIcons.user,
        organizationRoleColor,
        'Role worker',
        node.roleId,
      ),
      OrganizationNodeKind.taskboard => (
        FrankIcons.dashboard,
        organizationTaskboardColor,
        'Shared taskboard',
        node.taskboardId,
      ),
      OrganizationNodeKind.childWorkflow => (
        FrankIcons.workflow,
        organizationChildWorkflowColor,
        'Child workflow',
        node.childWorkflowId,
      ),
      _ => (FrankIcons.workflow, FrankColors.muted, 'Workflow node', null),
    };
    final scale = MediaQuery.textScalerOf(context).scale(1);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(icon, color: accent, size: FrankUiTokens.iconSize),
            const SizedBox(width: 9),
            Expanded(
              child: Text(
                node.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: FrankUiTokens.textSize,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
        ),
        const Spacer(),
        Text(
          kindLabel,
          maxLines: scale > 1.25 ? 2 : 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(color: accent, fontSize: 10),
        ),
        const SizedBox(height: 5),
        Container(
          width: double.infinity,
          padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 4),
          decoration: BoxDecoration(
            color: FrankColors.panelRaised,
            borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
            border: Border.all(color: accent.withValues(alpha: .35)),
          ),
          child: Text(
            reference == null || reference.isEmpty
                ? 'Reference not selected'
                : reference,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: FrankColors.muted, fontSize: 9),
          ),
        ),
      ],
    );
  }
}
