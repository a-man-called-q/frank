part of 'organization_surface.dart';

class _ValidationPanel extends StatefulWidget {
  const _ValidationPanel({required this.validation, required this.onFocus});

  final OrganizationValidation validation;
  final ValueChanged<OrganizationValidationIssue> onFocus;

  @override
  State<_ValidationPanel> createState() => _ValidationPanelState();
}

class _ValidationPanelState extends State<_ValidationPanel> {
  late bool _expanded = widget.validation.hasErrors;

  @override
  void didUpdateWidget(covariant _ValidationPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!oldWidget.validation.hasErrors && widget.validation.hasErrors) {
      _expanded = true;
    }
  }

  @override
  Widget build(BuildContext context) {
    final hasIssues = widget.validation.issues.isNotEmpty;
    return _OrganizationPanel(
      padding: EdgeInsets.zero,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Semantics(
            button: true,
            toggled: _expanded,
            label: 'Validation results',
            value: organizationValidationSummary(widget.validation),
            child: GestureDetector(
              onTap: () => setState(() => _expanded = !_expanded),
              child: SizedBox(
                height: FrankUiTokens.toolbarHeight,
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  child: Row(
                    children: [
                      Icon(
                        key: const ValueKey(
                          'organization-validation-status-icon',
                        ),
                        hasIssues
                            ? FrankIcons.circleAlert
                            : FrankIcons.circleCheck,
                        color: hasIssues
                            ? FrankColors.warningAmber
                            : FrankColors.muted,
                        size: FrankUiTokens.iconSize,
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          organizationValidationSummary(widget.validation),
                          style: const TextStyle(
                            color: FrankColors.ink,
                            fontSize: FrankUiTokens.textSize,
                          ),
                        ),
                      ),
                      Icon(
                        _expanded
                            ? FrankIcons.chevronUp
                            : FrankIcons.chevronDown,
                        color: FrankColors.muted,
                        size: FrankUiTokens.iconSize,
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          if (_expanded)
            Padding(
              padding: const EdgeInsets.fromLTRB(8, 0, 8, 6),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (final issue in widget.validation.issues.take(5))
                    _ValidationIssueRow(issue: issue, onFocus: widget.onFocus),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class _ValidationIssueRow extends StatelessWidget {
  const _ValidationIssueRow({required this.issue, required this.onFocus});

  final OrganizationValidationIssue issue;
  final ValueChanged<OrganizationValidationIssue> onFocus;

  @override
  Widget build(BuildContext context) {
    final actionable =
        issue.nodeId != null ||
        issue.relationId != null ||
        issue.groupId != null;
    return GestureDetector(
      onTap: actionable ? () => onFocus(issue) : null,
      child: ConstrainedBox(
        constraints: const BoxConstraints(
          minHeight: FrankUiTokens.controlHeight,
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 5),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(
                issue.severity == OrganizationIssueSeverity.error
                    ? FrankIcons.circleAlert
                    : FrankIcons.circleDashed,
                size: FrankUiTokens.iconSize - 1,
                color: FrankColors.warningAmber,
              ),
              const SizedBox(width: 7),
              Expanded(
                child: Text(
                  issue.message,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontSize: 11,
                    height: 1.35,
                  ),
                ),
              ),
              if (actionable) ...[
                const SizedBox(width: 5),
                const Icon(
                  FrankIcons.chevronRight,
                  size: 14,
                  color: FrankColors.muted,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _EmptyOrganization extends StatelessWidget {
  const _EmptyOrganization({required this.onAdd});
  final VoidCallback? onAdd;

  @override
  Widget build(BuildContext context) {
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 440),
      child: _OrganizationPanel(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(
              FrankIcons.workflow,
              size: 32,
              color: FrankColors.aubergineAccent,
            ),
            const SizedBox(height: 14),
            const Text(
              'Build your agency flow',
              style: TextStyle(
                color: FrankColors.ink,
                fontSize: 18,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 7),
            const Text(
              'Start with a member, then connect roles, tools, and internal taskboards.',
              textAlign: TextAlign.center,
              style: TextStyle(color: FrankColors.muted, fontSize: 12),
            ),
            const SizedBox(height: 18),
            FButton(
              onPress: onAdd,
              semanticsLabel: onAdd == null
                  ? 'Reconnect before adding an office element'
                  : 'Add office element',
              semanticsTooltip: onAdd == null
                  ? 'Reconnect before adding an office element'
                  : 'Add office element',
              prefix: const Icon(FrankIcons.plus),
              child: const Text('Add office element'),
            ),
          ],
        ),
      ),
    );
  }
}

class _OrganizationLoading extends StatelessWidget {
  const _OrganizationLoading();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Semantics(
        label: 'Loading organization',
        child: const FCircularProgress(),
      ),
    );
  }
}

class _OrganizationFailure extends StatelessWidget {
  const _OrganizationFailure({required this.message});
  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: _OrganizationPanel(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(FrankIcons.circleAlert, color: FrankColors.warningAmber),
            const SizedBox(height: 10),
            Text(message, style: const TextStyle(color: FrankColors.ink)),
            const SizedBox(height: 14),
            FButton(
              onPress: () => context.read<OrganizationBloc>().add(
                const OrganizationRetryRequested(),
              ),
              child: const Text('Retry'),
            ),
          ],
        ),
      ),
    );
  }
}

class _OrganizationPanel extends StatelessWidget {
  const _OrganizationPanel({
    required this.child,
    this.padding = const EdgeInsets.all(6),
    this.color = FrankColors.panel,
    this.radius = FrankUiTokens.panelRadius,
    this.borderRadius,
    this.border,
  });

  final Widget child;
  final EdgeInsets padding;
  final Color color;
  final double radius;
  final BorderRadius? borderRadius;
  final Border? border;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: color,
        borderRadius: borderRadius ?? BorderRadius.circular(radius),
        border:
            border ??
            Border.all(
              color: FrankColors.border,
              width: FrankUiTokens.borderWidth,
            ),
      ),
      child: Padding(padding: padding, child: child),
    );
  }
}

class _ToolbarTextButton extends StatelessWidget {
  const _ToolbarTextButton({
    required this.buttonKey,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.semanticsTooltip,
    this.selected = false,
  });

  final Key buttonKey;
  final IconData icon;
  final String label;
  final VoidCallback? onPressed;
  final String? semanticsTooltip;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      enabled: onPressed != null,
      label: label,
      hint: semanticsTooltip,
      child: FButton(
        key: buttonKey,
        onPress: onPressed,
        variant: selected ? FButtonVariant.secondary : FButtonVariant.ghost,
        size: FButtonSizeVariant.sm,
        prefix: Icon(icon, size: FrankUiTokens.iconSize),
        child: Text(
          label,
          style: const TextStyle(fontSize: FrankUiTokens.textSize),
        ),
      ),
    );
  }
}

class _ToolbarIconButton extends StatelessWidget {
  const _ToolbarIconButton({
    required this.buttonKey,
    required this.icon,
    required this.label,
    required this.onPressed,
    this.selected = false,
    this.toggled,
  });

  final Key buttonKey;
  final IconData icon;
  final String label;
  final VoidCallback? onPressed;
  final bool selected;
  final bool? toggled;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      enabled: onPressed != null,
      label: label,
      toggled: toggled,
      excludeSemantics: true,
      child: FButton.icon(
        key: buttonKey,
        onPress: onPressed,
        variant: selected ? FButtonVariant.secondary : FButtonVariant.ghost,
        size: FButtonSizeVariant.sm,
        semanticsTooltip: label,
        child: Icon(icon, size: FrankUiTokens.iconSize),
      ),
    );
  }
}

class _ToolbarDivider extends StatelessWidget {
  const _ToolbarDivider();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 6),
      child: SizedBox(
        width: FrankUiTokens.borderWidth,
        height: 20,
        child: const ColoredBox(color: FrankColors.border),
      ),
    );
  }
}

class _DraftStatus extends StatelessWidget {
  const _DraftStatus({required this.state});
  final OrganizationState state;

  @override
  Widget build(BuildContext context) {
    final (label, color) = switch (state.persistenceStatus) {
      OrganizationPersistenceStatus.published => (
        'Published r${state.graph?.publishedRevision ?? 0}',
        FrankColors.green,
      ),
      OrganizationPersistenceStatus.clean => ('Draft saved', FrankColors.muted),
      OrganizationPersistenceStatus.dirty => (
        'Unsaved draft',
        FrankColors.warningAmber,
      ),
      OrganizationPersistenceStatus.saving => ('Saving…', FrankColors.blue),
      OrganizationPersistenceStatus.saveFailure => (
        'Save failed',
        FrankColors.warningAmber,
      ),
      OrganizationPersistenceStatus.publishing => (
        'Publishing…',
        FrankColors.blue,
      ),
      OrganizationPersistenceStatus.publishFailure => (
        'Publish failed',
        FrankColors.warningAmber,
      ),
    };
    return Semantics(
      liveRegion: true,
      label: label,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _StatusDot(color: color),
          const SizedBox(width: 6),
          Text(label, style: TextStyle(color: color, fontSize: 11)),
        ],
      ),
    );
  }
}

class _StatusDot extends StatelessWidget {
  const _StatusDot({required this.color});
  final Color color;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
      child: const SizedBox.square(dimension: 7),
    );
  }
}

class _SetupBadge extends StatelessWidget {
  const _SetupBadge({required this.configured});
  final bool configured;

  @override
  Widget build(BuildContext context) {
    final color = configured ? FrankColors.green : FrankColors.warningAmber;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(
        color: color.withValues(alpha: .12),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        configured ? 'Connected' : 'Setup required',
        style: TextStyle(
          color: color,
          fontSize: 8,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _InspectorLabel extends StatelessWidget {
  const _InspectorLabel(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 7),
      child: Text(
        text.toUpperCase(),
        style: const TextStyle(
          color: FrankColors.muted,
          fontSize: 10,
          letterSpacing: .7,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _Notice extends StatelessWidget {
  const _Notice({required this.icon, required this.text});
  final IconData icon;
  final String text;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(11),
      decoration: BoxDecoration(
        color: FrankColors.panelRaised,
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            icon,
            color: FrankColors.warningAmber,
            size: FrankUiTokens.iconSize,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              text,
              style: const TextStyle(
                color: FrankColors.muted,
                fontSize: 11,
                height: 1.4,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
