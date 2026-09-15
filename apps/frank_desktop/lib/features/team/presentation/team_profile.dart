part of 'team_surface.dart';

// The legacy portrait/capability/activity widgets remain below as private
// compatibility building blocks for fixture-only detail views. The
// role-first roster does not mount them in production.
// ignore_for_file: unused_element

class _TeamAgentCard extends StatefulWidget {
  const _TeamAgentCard({
    required this.profile,
    required this.reducedMotion,
    required this.compact,
    required this.onSelected,
  });

  final TeamAgentProfile profile;
  final bool reducedMotion;
  final bool compact;
  final VoidCallback onSelected;

  @override
  State<_TeamAgentCard> createState() => _TeamAgentCardState();
}

class _TeamAgentCardState extends State<_TeamAgentCard> {
  bool _hovered = false;
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    final profile = widget.profile;
    final active = _hovered || _focused;
    final duration = widget.reducedMotion
        ? Duration.zero
        : const Duration(milliseconds: 150);
    final member = Row(
      children: [
        FAvatar.raw(
          size: 34,
          child: Text(
            profile.initials,
            style: TextStyle(
              color: profile.accent,
              fontSize: 11,
              fontWeight: FontWeight.w700,
            ),
          ),
        ),
        const SizedBox(width: 10),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                profile.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                ),
              ),
              Text(
                profile.employeeId,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.muted, fontSize: 10),
              ),
            ],
          ),
        ),
      ],
    );
    final role = Text(
      profile.role,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: const TextStyle(color: FrankColors.ink, fontSize: 12),
    );
    final task = Text(
      profile.assignment,
      maxLines: 2,
      overflow: TextOverflow.ellipsis,
      style: const TextStyle(color: FrankColors.muted, fontSize: 11),
    );
    final model = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          profile.model,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: FrankColors.ink, fontSize: 11),
        ),
        Text(
          profile.modelSource,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: FrankColors.muted, fontSize: 10),
        ),
      ],
    );
    final row = Container(
      key: ValueKey('team-agent-row-${profile.employeeId}'),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
      decoration: BoxDecoration(
        color: FrankColors.panelRaised,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: active ? profile.accent : FrankColors.border,
          width: active ? 1.25 : 1,
        ),
      ),
      child: widget.compact
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Expanded(child: member),
                    Flexible(
                      child: FittedBox(
                        fit: BoxFit.scaleDown,
                        alignment: Alignment.centerRight,
                        child: _StatusPill(
                          status: profile.status,
                          compact: true,
                        ),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 10),
                Wrap(
                  spacing: 18,
                  runSpacing: 8,
                  children: [
                    SizedBox(width: 150, child: role),
                    SizedBox(width: 190, child: task),
                    SizedBox(width: 150, child: model),
                    Text(
                      'Revision ${profile.roleRevision}',
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 10,
                      ),
                    ),
                  ],
                ),
                Align(
                  alignment: Alignment.centerRight,
                  child: _ViewMemberButton(
                    key: ValueKey('team-member-view-${profile.employeeId}'),
                    label: 'View ${profile.name}',
                    onPressed: widget.onSelected,
                  ),
                ),
              ],
            )
          : Row(
              children: [
                Expanded(flex: 3, child: member),
                Expanded(
                  flex: 2,
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    alignment: Alignment.centerLeft,
                    child: _StatusPill(status: profile.status, compact: true),
                  ),
                ),
                Expanded(flex: 2, child: role),
                Expanded(flex: 3, child: task),
                Expanded(flex: 2, child: model),
                Expanded(
                  flex: 1,
                  child: Text(
                    '${profile.roleRevision}',
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 11,
                    ),
                  ),
                ),
                _ViewMemberButton(
                  key: ValueKey('team-member-view-${profile.employeeId}'),
                  label: 'View ${profile.name}',
                  onPressed: widget.onSelected,
                ),
              ],
            ),
    );
    return Semantics(
      container: true,
      button: true,
      focusable: true,
      label: '${profile.name}, ${profile.role}, ${profile.status.label}',
      hint: 'Open member details',
      child: FocusableActionDetector(
        onShowHoverHighlight: (value) => setState(() => _hovered = value),
        onShowFocusHighlight: (value) => setState(() => _focused = value),
        actions: <Type, Action<Intent>>{
          ActivateIntent: CallbackAction<ActivateIntent>(
            onInvoke: (_) {
              widget.onSelected();
              return null;
            },
          ),
        },
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onSelected,
          child: AnimatedContainer(
            duration: duration,
            curve: Curves.easeOutCubic,
            transform: active
                ? Matrix4.translationValues(0.0, -1.0, 0.0)
                : Matrix4.identity(),
            child: row,
          ),
        ),
      ),
    );
  }
}

class _ViewMemberButton extends StatelessWidget {
  const _ViewMemberButton({required this.onPressed, required this.label, super.key});

  final VoidCallback onPressed;
  final String label;

  @override
  Widget build(BuildContext context) => FButton(
    key: key,
    onPress: onPressed,
    semanticsLabel: label,
    semanticsTooltip: 'Open member details',
    variant: FButtonVariant.ghost,
    size: FButtonSizeVariant.sm,
    child: const Text('View'),
  );
}

class _PortraitPanel extends StatelessWidget {
  const _PortraitPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        DecoratedBox(
          decoration: BoxDecoration(
            color: profile.accent.withValues(alpha: 0.11),
            gradient: LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [
                profile.accent.withValues(alpha: 0.18),
                profile.accent.withValues(alpha: 0.03),
              ],
            ),
          ),
        ),
        if (profile.imageAsset.trim().isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(top: 10, left: 10, right: 10),
            child: Image.asset(
              profile.imageAsset,
              fit: BoxFit.contain,
              alignment: Alignment.bottomCenter,
              errorBuilder: (context, error, stackTrace) =>
                  _InitialsAvatar(profile: profile),
            ),
          )
        else
          _InitialsAvatar(profile: profile),
        Positioned(
          top: 14,
          left: 14,
          child: _StatusPill(status: profile.status, compact: true),
        ),
      ],
    );
  }
}

class _CardDetails extends StatelessWidget {
  const _CardDetails({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  profile.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontSize: 16,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              const _ModelBadge(),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            profile.role,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(color: profile.accent, fontSize: 12),
          ),
          const SizedBox(height: 9),
          Text(
            profile.assignment,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: FrankColors.muted, fontSize: 12),
          ),
        ],
      ),
    );
  }
}

class _ModelBadge extends StatelessWidget {
  const _ModelBadge();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 4),
      decoration: BoxDecoration(
        color: FrankColors.canvas.withValues(alpha: 0.55),
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: FrankColors.border),
      ),
      child: Text(
        'OpenRouter',
        style: const TextStyle(
          color: FrankColors.muted,
          fontSize: 10,
          fontWeight: FontWeight.w500,
        ),
      ),
    );
  }
}

class _StatusPill extends StatelessWidget {
  const _StatusPill({required this.status, this.compact = false});

  final TeamAgentStatus status;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final tone = switch (status) {
      TeamAgentStatus.available => FrankStatusTone.success,
      TeamAgentStatus.working => FrankStatusTone.working,
      TeamAgentStatus.idle ||
      TeamAgentStatus.offline => FrankStatusTone.neutral,
      TeamAgentStatus.reviewing => FrankStatusTone.attention,
      TeamAgentStatus.blocked => FrankStatusTone.failure,
    };
    return FrankStatusBadge(
      label: status.label,
      tone: tone,
      icon: status.icon,
      compact: compact,
    );
  }
}

class _TeamProfile extends StatelessWidget {
  const _TeamProfile({
    required this.profile,
    required this.roles,
    required this.selectedTab,
    required this.reducedMotion,
    required this.catalog,
    required this.catalogError,
    required this.catalogLoading,
    this.providerConfigured,
    this.providerError,
    required this.onSaveAgentModel,
    required this.onSaveRoleModel,
    required this.onUpdateAgent,
    this.onEditRole,
    required this.onClose,
    this.onArchive,
    required this.onTabSelected,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final TeamProfileTab selectedTab;
  final bool reducedMotion;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final bool? providerConfigured;
  final Object? providerError;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;
  final TeamAgentUpdate? onUpdateAgent;
  final VoidCallback? onEditRole;
  final VoidCallback onClose;
  final VoidCallback? onArchive;
  final ValueChanged<TeamProfileTab> onTabSelected;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: FrankColors.panel,
      child: Column(
        key: const ValueKey('team-member-details'),
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _ProfileHeader(
            profile: profile,
            onBack: onClose,
            onArchive: onArchive,
            canMutate: canMutate,
            mutationDisabledReason: mutationDisabledReason,
          ),
          const FDivider(),
          Expanded(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  _ProfileHero(profile: profile),
                  const SizedBox(height: 16),
                  _ProfileTabs(
                    selectedTab: selectedTab,
                    reducedMotion: reducedMotion,
                    onSelected: onTabSelected,
                  ),
                  const SizedBox(height: 16),
                  AnimatedSwitcher(
                    duration: reducedMotion
                        ? Duration.zero
                        : const Duration(milliseconds: 150),
                    child: KeyedSubtree(
                      key: ValueKey('team-profile-panel-${selectedTab.name}'),
                      child: _ProfileTabBody(
                        profile: profile,
                        roles: roles,
                        tab: selectedTab,
                        catalog: catalog,
                        catalogError: catalogError,
                        catalogLoading: catalogLoading,
                        providerConfigured: providerConfigured,
                        providerError: providerError,
                        onSaveAgentModel: onSaveAgentModel,
                        onSaveRoleModel: onSaveRoleModel,
                        onUpdateAgent: onUpdateAgent,
                        onEditRole: onEditRole,
                        canMutate: canMutate,
                        mutationDisabledReason: mutationDisabledReason,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ProfileHeader extends StatelessWidget {
  const _ProfileHeader({
    required this.profile,
    required this.onBack,
    this.onArchive,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final TeamAgentProfile profile;
  final VoidCallback onBack;
  final VoidCallback? onArchive;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Semantics(
          button: true,
          label: 'Back to Team roster',
          child: FButton.icon(
            key: const ValueKey('team-profile-back'),
            semanticsTooltip: 'Back to Team roster',
            onPress: onBack,
            child: const Icon(
              FrankIcons.arrowBack,
              size: FrankUiTokens.iconSize,
            ),
          ),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Text(
            profile.name,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: FrankColors.ink, fontSize: 12),
          ),
        ),
        if (onArchive != null)
          FButton.icon(
            key: const ValueKey('team-profile-archive'),
            semanticsLabel: 'Archive member',
            semanticsTooltip: canMutate
                ? 'Archive member'
                : mutationDisabledReason ??
                      'Reconnect before archiving a member',
            onPress: canMutate ? onArchive : null,
            child: const Icon(FrankIcons.archiveOutlined, size: 18),
          ),
      ],
    );
  }
}

class _ProfileHero extends StatelessWidget {
  const _ProfileHero({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        // This view lives in the member drawer. Keep the identity summary
        // compact instead of allocating a large portrait slab.
        final wide = constraints.maxWidth >= 480;
        final portrait = _ProfilePortrait(profile: profile);
        final details = _ProfileHeroDetails(profile: profile);
        if (!wide) {
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              portrait,
              const SizedBox(width: 12),
              Expanded(child: details),
            ],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            portrait,
            const SizedBox(width: 14),
            Expanded(child: details),
          ],
        );
      },
    );
  }
}

class _ProfilePortrait extends StatelessWidget {
  const _ProfilePortrait({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: 68,
      child: Container(
        decoration: BoxDecoration(
          color: profile.accent.withValues(alpha: 0.11),
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: profile.accent.withValues(alpha: 0.42)),
          gradient: LinearGradient(
            begin: Alignment.topLeft,
            end: Alignment.bottomRight,
            colors: [
              profile.accent.withValues(alpha: 0.18),
              FrankColors.panelRaised,
            ],
          ),
        ),
        clipBehavior: Clip.antiAlias,
        child: profile.imageAsset.trim().isEmpty
            ? _InitialsAvatar(profile: profile, fontSize: 24)
            : Image.asset(
                profile.imageAsset,
                fit: BoxFit.contain,
                alignment: Alignment.bottomCenter,
                errorBuilder: (context, error, stackTrace) =>
                    _InitialsAvatar(profile: profile, fontSize: 24),
              ),
      ),
    );
  }
}

class _InitialsAvatar extends StatelessWidget {
  const _InitialsAvatar({required this.profile, this.fontSize = 42});

  final TeamAgentProfile profile;
  final double fontSize;

  @override
  Widget build(BuildContext context) => Center(
    child: Text(
      profile.initials,
      style: TextStyle(
        color: profile.accent,
        fontSize: fontSize,
        fontWeight: FontWeight.w600,
      ),
    ),
  );
}

class _ProfileHeroDetails extends StatelessWidget {
  const _ProfileHeroDetails({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        _StatusPill(status: profile.status),
        const SizedBox(height: 8),
        Text(
          profile.name,
          style: const TextStyle(
            color: FrankColors.ink,
            fontSize: 21,
            fontWeight: FontWeight.w500,
            letterSpacing: -0.3,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          profile.role,
          style: TextStyle(
            color: profile.accent,
            fontSize: 14,
            fontWeight: FontWeight.w500,
          ),
        ),
        if (profile.tagline.trim().isNotEmpty) ...[
          const SizedBox(height: 12),
          Text(
            profile.tagline,
            style: const TextStyle(
              color: FrankColors.muted,
              fontSize: 15,
              height: 1.45,
            ),
          ),
        ],
        const SizedBox(height: 12),
        _AssignmentPanel(profile: profile),
        const SizedBox(height: 12),
        Row(
          children: [
            const Icon(
              FrankIcons.memoryOutlined,
              size: 15,
              color: FrankColors.muted,
            ),
            const SizedBox(width: 7),
            Flexible(
              child: Text(
                profile.modelSummary,
                softWrap: true,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: 12,
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _AssignmentPanel extends StatelessWidget {
  const _AssignmentPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 13),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(11),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        children: [
          Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(
              color: profile.accent,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 11),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'CURRENT TASK',
                  style: TextStyle(
                    color: FrankColors.muted,
                    fontSize: 9,
                    fontWeight: FontWeight.w600,
                    letterSpacing: 1,
                  ),
                ),
                const SizedBox(height: 5),
                Text(
                  profile.assignment,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontSize: 13,
                    fontWeight: FontWeight.w500,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  profile.currentProject,
                  style: TextStyle(color: profile.accent, fontSize: 11),
                ),
              ],
            ),
          ),
          const Icon(
            FrankIcons.arrowOutward,
            size: 16,
            color: FrankColors.muted,
          ),
        ],
      ),
    );
  }
}

class _ProfileTabs extends StatelessWidget {
  const _ProfileTabs({
    required this.selectedTab,
    required this.reducedMotion,
    required this.onSelected,
  });

  final TeamProfileTab selectedTab;
  final bool reducedMotion;
  final ValueChanged<TeamProfileTab> onSelected;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: const ValueKey('team-profile-tabs'),
      padding: const EdgeInsets.all(4),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        border: Border.all(color: FrankColors.border),
      ),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final buttons = [
            for (final tab in TeamProfileTab.values)
              _ProfileTabButton(
                tab: tab,
                selected: tab == selectedTab,
                reducedMotion: reducedMotion,
                onPressed: () => onSelected(tab),
              ),
          ];
          if (constraints.maxWidth >= 500) {
            return Row(
              children: [for (final button in buttons) Expanded(child: button)],
            );
          }
          return SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final button in buttons)
                  SizedBox(width: 108, child: button),
              ],
            ),
          );
        },
      ),
    );
  }
}

class _ProfileTabButton extends StatelessWidget {
  const _ProfileTabButton({
    required this.tab,
    required this.selected,
    required this.reducedMotion,
    required this.onPressed,
  });

  final TeamProfileTab tab;
  final bool selected;
  final bool reducedMotion;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      selected: selected,
      label: '${tab.label} tab',
      child: AnimatedContainer(
        duration: reducedMotion
            ? Duration.zero
            : const Duration(milliseconds: 120),
        decoration: BoxDecoration(
          color: selected ? FrankColors.aubergineSoft : const Color(0x00000000),
          borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
          border: selected
              ? Border.all(
                  color: FrankColors.aubergineAccent.withValues(alpha: 0.52),
                )
              : null,
        ),
        child: FButton(
          key: ValueKey('team-profile-tab-${tab.name}'),
          onPress: onPressed,
          variant: selected ? FButtonVariant.secondary : FButtonVariant.ghost,
          prefix: Icon(tab.icon, size: FrankUiTokens.iconSize),
          child: Flexible(
            child: Text(
              tab.label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileTabBody extends StatelessWidget {
  const _ProfileTabBody({
    required this.profile,
    required this.roles,
    required this.tab,
    required this.catalog,
    required this.catalogError,
    required this.catalogLoading,
    this.providerConfigured,
    this.providerError,
    required this.onSaveAgentModel,
    required this.onSaveRoleModel,
    required this.onUpdateAgent,
    this.onEditRole,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final TeamProfileTab tab;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final bool? providerConfigured;
  final Object? providerError;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;
  final TeamAgentUpdate? onUpdateAgent;
  final VoidCallback? onEditRole;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    return switch (tab) {
      TeamProfileTab.overview => _OverviewPanel(profile: profile),
      TeamProfileTab.identity => _IdentityPanel(
        profile: profile,
        roles: roles,
        onUpdateAgent: onUpdateAgent,
        canMutate: canMutate,
        mutationDisabledReason: mutationDisabledReason,
      ),
      TeamProfileTab.setup => _SetupPanel(
        profile: profile,
        catalog: catalog,
        catalogError: catalogError,
        catalogLoading: catalogLoading,
        providerConfigured: providerConfigured,
        providerError: providerError,
        onSaveAgentModel: onSaveAgentModel,
        onSaveRoleModel: onSaveRoleModel,
        onEditRole: onEditRole,
        canMutate: canMutate,
        mutationDisabledReason: mutationDisabledReason,
      ),
    };
  }
}

class _OverviewPanel extends StatelessWidget {
  const _OverviewPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('team-profile-overview'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _PanelHeading(
          eyebrow: 'RUNTIME STATE',
          title: 'Operational facts.',
          detail: profile.tagline.trim().isEmpty
              ? 'Facts are shown only when reported by the daemon.'
              : profile.tagline,
        ),
        const SizedBox(height: 18),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            _InfoCard(
              icon: FrankIcons.workOutline,
              label: 'Current project',
              value: profile.currentProject,
              accent: profile.accent,
            ),
            _InfoCard(
              icon: FrankIcons.assignmentOutlined,
              label: 'Assignment',
              value: profile.assignment,
              accent: profile.accent,
            ),
            _InfoCard(
              icon: FrankIcons.memoryOutlined,
              label: 'Runtime',
              value: profile.modelSummary,
              accent: profile.accent,
            ),
          ],
        ),
      ],
    );
  }
}

class _IdentityPanel extends StatelessWidget {
  const _IdentityPanel({
    required this.profile,
    required this.roles,
    this.onUpdateAgent,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final TeamAgentUpdate? onUpdateAgent;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      key: const ValueKey('team-profile-identity'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _PanelHeading(
            eyebrow: 'IDENTITY',
            title: 'Member configuration.',
            detail: 'Identity and role assignment reported by the daemon.',
          ),
          const SizedBox(height: 26),
          Row(
            children: [
              Expanded(
                child: _DetailRow(label: 'Role', value: profile.role),
              ),
              if (onUpdateAgent != null)
                FButton(
                  key: const ValueKey('team-member-edit'),
                  onPress: canMutate ? () => _edit(context) : null,
                  semanticsTooltip: canMutate
                      ? 'Edit member'
                      : mutationDisabledReason ??
                            'Reconnect before editing a member',
                  variant: FButtonVariant.outline,
                  prefix: const Icon(FrankIcons.editOutlined, size: 16),
                  child: const Text('Edit member'),
                ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _edit(BuildContext context) async {
    await Navigator.of(context).push<void>(
      PageRouteBuilder<void>(
        pageBuilder: (_, _, _) => _TeamFormPage(
          form: _AgentEditDialog(
            profile: profile,
            roles: roles,
            onSave: (patch) async {
              await onUpdateAgent!(profile.employeeId, patch);
            },
          ),
        ),
        transitionDuration: Duration.zero,
        reverseTransitionDuration: Duration.zero,
      ),
    );
  }
}

class _CapabilitiesPanel extends StatelessWidget {
  const _CapabilitiesPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      key: const ValueKey('team-profile-capabilities'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _PanelHeading(
            eyebrow: 'CAPABILITIES',
            title: 'Tools available to this member.',
            detail:
                'Reported capabilities are read-only until the daemon exposes updates.',
          ),
          const SizedBox(height: 20),
          if (profile.capabilities.isEmpty)
            const Text(
              'No capabilities reported.',
              style: TextStyle(color: FrankColors.muted),
            )
          else
            Wrap(
              spacing: 10,
              runSpacing: 10,
              children: [
                for (final capability in profile.capabilities)
                  DecoratedBox(
                    decoration: BoxDecoration(
                      color: FrankColors.panelRaised,
                      borderRadius: BorderRadius.circular(10),
                      border: Border.all(color: FrankColors.border),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 10,
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            capability.icon,
                            size: FrankUiTokens.iconSize,
                            color: FrankColors.aubergineAccent,
                          ),
                          const SizedBox(width: 8),
                          Text(
                            capability.label,
                            style: const TextStyle(color: FrankColors.ink),
                          ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
        ],
      ),
    );
  }
}

class _ActivityPanel extends StatelessWidget {
  const _ActivityPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      key: const ValueKey('team-profile-activity'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _PanelHeading(
            eyebrow: 'ACTIVITY',
            title: 'Recent work signals.',
            detail: 'Only activity reported by the daemon is shown here.',
          ),
          const SizedBox(height: 18),
          if (profile.activity.isEmpty)
            const Text(
              'No activity reported.',
              style: TextStyle(color: FrankColors.muted),
            )
          else
            for (final event in profile.activity)
              Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    color: FrankColors.panelRaised,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: FrankColors.border),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Icon(
                          event.icon,
                          size: FrankUiTokens.iconSize,
                          color: FrankColors.aubergineAccent,
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                event.label,
                                style: const TextStyle(
                                  color: FrankColors.ink,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              const SizedBox(height: 3),
                              Text(
                                event.detail,
                                style: const TextStyle(
                                  color: FrankColors.muted,
                                  fontSize: 12,
                                ),
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(width: 10),
                        Text(
                          event.timeLabel,
                          style: const TextStyle(
                            color: FrankColors.muted,
                            fontSize: 11,
                          ),
                        ),
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
