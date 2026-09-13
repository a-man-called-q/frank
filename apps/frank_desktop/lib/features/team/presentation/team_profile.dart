part of 'team_surface.dart';

class _TeamAgentCard extends StatefulWidget {
  const _TeamAgentCard({
    required this.profile,
    required this.reducedMotion,
    required this.onSelected,
  });

  final TeamAgentProfile profile;
  final bool reducedMotion;
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
    return Semantics(
      container: true,
      button: true,
      focusable: true,
      label: '${profile.name}, ${profile.role}, ${profile.status.label}',
      hint: 'Open character profile',
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
            key: ValueKey('team-agent-card-${profile.employeeId}'),
            duration: duration,
            curve: Curves.easeOutCubic,
            transform: active
                ? Matrix4.translationValues(0.0, -2.0, 0.0)
                : Matrix4.identity(),
            decoration: BoxDecoration(
              color: FrankColors.panelRaised,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(
                color: active ? profile.accent : FrankColors.border,
                width: active ? 1.25 : 1,
              ),
              boxShadow: active
                  ? [
                      BoxShadow(
                        color: profile.accent.withValues(alpha: 0.16),
                        blurRadius: 22,
                        offset: const Offset(0, 9),
                      ),
                    ]
                  : const [],
            ),
            clipBehavior: Clip.antiAlias,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                SizedBox(height: 176, child: _PortraitPanel(profile: profile)),
                _CardDetails(profile: profile),
              ],
            ),
          ),
        ),
      ),
    );
  }
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
        Padding(
          padding: const EdgeInsets.only(top: 10, left: 10, right: 10),
          child: Image.asset(
            profile.imageAsset,
            fit: BoxFit.contain,
            alignment: Alignment.bottomCenter,
            errorBuilder: (context, error, stackTrace) => Center(
              child: Text(
                profile.initials,
                style: TextStyle(
                  color: profile.accent,
                  fontSize: 42,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ),
        ),
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
          if (profile.specialization != null) ...[
            const SizedBox(height: 2),
            Text(
              profile.specialization!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: profile.accent, fontSize: 11),
            ),
          ],
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
    required this.onSaveAgentModel,
    required this.onSaveRoleModel,
    required this.onUpdateAgent,
    required this.onTabSelected,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final TeamProfileTab selectedTab;
  final bool reducedMotion;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;
  final TeamAgentUpdate? onUpdateAgent;
  final ValueChanged<TeamProfileTab> onTabSelected;

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('team-character-profile'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ProfileHero(profile: profile),
        const SizedBox(height: 24),
        _ProfileTabs(
          selectedTab: selectedTab,
          reducedMotion: reducedMotion,
          onSelected: onTabSelected,
        ),
        const SizedBox(height: 20),
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
              onSaveAgentModel: onSaveAgentModel,
              onSaveRoleModel: onSaveRoleModel,
              onUpdateAgent: onUpdateAgent,
            ),
          ),
        ),
      ],
    );
  }
}

class _ProfileHeader extends StatelessWidget {
  const _ProfileHeader({
    required this.profile,
    required this.onBack,
    this.onArchive,
  });

  final TeamAgentProfile profile;
  final VoidCallback onBack;
  final VoidCallback? onArchive;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Semantics(
          button: true,
          label: 'Back to Team roster',
          child: IconButton(
            key: const ValueKey('team-profile-back'),
            tooltip: 'Back to Team roster',
            onPressed: onBack,
            icon: const Icon(Icons.arrow_back, size: FrankUiTokens.iconSize),
            color: FrankColors.ink,
            style: IconButton.styleFrom(
              minimumSize: const Size.square(FrankUiTokens.controlHeight),
              padding: EdgeInsets.zero,
              tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              backgroundColor: FrankColors.panelRaised,
              side: const BorderSide(color: FrankColors.border),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(
                  FrankUiTokens.controlRadius,
                ),
              ),
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
          IconButton(
            key: const ValueKey('team-profile-archive'),
            tooltip: 'Archive member',
            onPressed: onArchive,
            icon: const Icon(Icons.archive_outlined, size: 18),
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
        final wide = constraints.maxWidth >= 760;
        final portrait = _ProfilePortrait(profile: profile);
        final details = _ProfileHeroDetails(profile: profile);
        if (!wide) {
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [portrait, const SizedBox(height: 18), details],
          );
        }
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            SizedBox(width: 350, child: portrait),
            const SizedBox(width: 28),
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
    return AspectRatio(
      aspectRatio: 1.13,
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
        child: Image.asset(
          profile.imageAsset,
          fit: BoxFit.contain,
          alignment: Alignment.bottomCenter,
          errorBuilder: (context, error, stackTrace) => Center(
            child: Text(
              profile.initials,
              style: TextStyle(
                color: profile.accent,
                fontSize: 64,
                fontWeight: FontWeight.w600,
              ),
            ),
          ),
        ),
      ),
    );
  }
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
        const SizedBox(height: 15),
        Text(
          profile.name,
          style: const TextStyle(
            color: FrankColors.ink,
            fontSize: 30,
            fontWeight: FontWeight.w500,
            letterSpacing: -0.7,
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
        if (profile.specialization != null) ...[
          const SizedBox(height: 3),
          Text(
            profile.specialization!,
            style: TextStyle(
              color: profile.accent,
              fontSize: 13,
              fontWeight: FontWeight.w500,
            ),
          ),
        ],
        const SizedBox(height: 12),
        Text(
          profile.tagline,
          style: const TextStyle(
            color: FrankColors.muted,
            fontSize: 15,
            height: 1.45,
          ),
        ),
        const SizedBox(height: 24),
        _AssignmentPanel(profile: profile),
        const SizedBox(height: 12),
        LayoutBuilder(
          builder: (context, constraints) {
            final textScale =
                MediaQuery.maybeOf(context)?.textScaler.scale(1) ?? 1;
            final compact =
                textScale > 1 ||
                (constraints.hasBoundedWidth && constraints.maxWidth < 520);
            final modelSummary = Row(
              children: [
                const Icon(
                  Icons.memory_outlined,
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
            );
            final pack = Row(
              children: [
                const Icon(
                  Icons.auto_awesome_outlined,
                  size: 14,
                  color: FrankColors.muted,
                ),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(
                    '${profile.promptPack} · ${profile.level}',
                    softWrap: true,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 12,
                    ),
                  ),
                ),
              ],
            );
            if (compact) {
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [modelSummary, const SizedBox(height: 8), pack],
              );
            }
            return Row(
              children: [
                const Icon(
                  Icons.memory_outlined,
                  size: 15,
                  color: FrankColors.muted,
                ),
                const SizedBox(width: 7),
                Text(
                  profile.modelSummary,
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 12,
                  ),
                ),
                const SizedBox(width: 14),
                const Icon(
                  Icons.auto_awesome_outlined,
                  size: 14,
                  color: FrankColors.muted,
                ),
                const SizedBox(width: 6),
                Text(
                  '${profile.promptPack} · ${profile.level}',
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 12,
                  ),
                ),
              ],
            );
          },
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
                  'CURRENT ASSIGNMENT',
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
          const Icon(Icons.arrow_outward, size: 16, color: FrankColors.muted),
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
      child: Wrap(
        spacing: 4,
        runSpacing: 4,
        children: [
          for (final tab in TeamProfileTab.values)
            _ProfileTabButton(
              tab: tab,
              selected: tab == selectedTab,
              reducedMotion: reducedMotion,
              onPressed: () => onSelected(tab),
            ),
        ],
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
          color: selected ? FrankColors.aubergineSoft : Colors.transparent,
          borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
          border: selected
              ? Border.all(
                  color: FrankColors.aubergineAccent.withValues(alpha: 0.52),
                )
              : null,
        ),
        child: TextButton.icon(
          key: ValueKey('team-profile-tab-${tab.name}'),
          onPressed: onPressed,
          icon: Icon(
            tab.icon,
            size: FrankUiTokens.iconSize,
            color: selected ? FrankColors.ink : FrankColors.muted,
          ),
          label: Text(tab.label),
          style: TextButton.styleFrom(
            foregroundColor: selected ? FrankColors.ink : FrankColors.muted,
            minimumSize: const Size(0, FrankUiTokens.controlHeight),
            padding: const EdgeInsets.symmetric(horizontal: 11),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            textStyle: const TextStyle(fontSize: 12),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
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
    required this.onSaveAgentModel,
    required this.onSaveRoleModel,
    required this.onUpdateAgent,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final TeamProfileTab tab;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;
  final TeamAgentUpdate? onUpdateAgent;

  @override
  Widget build(BuildContext context) {
    return switch (tab) {
      TeamProfileTab.overview => _OverviewPanel(profile: profile),
      TeamProfileTab.identity => _IdentityPanel(
        profile: profile,
        roles: roles,
        onUpdateAgent: onUpdateAgent,
      ),
      TeamProfileTab.setup => _SetupPanel(
        profile: profile,
        catalog: catalog,
        catalogError: catalogError,
        catalogLoading: catalogLoading,
        onSaveAgentModel: onSaveAgentModel,
        onSaveRoleModel: onSaveRoleModel,
      ),
      TeamProfileTab.capabilities => _CapabilitiesPanel(profile: profile),
      TeamProfileTab.activity => _ActivityPanel(profile: profile),
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
          eyebrow: 'AT A GLANCE',
          title: 'A little context goes a long way.',
          detail: profile.tagline,
        ),
        const SizedBox(height: 18),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            _InfoCard(
              icon: Icons.work_outline,
              label: 'Current project',
              value: profile.currentProject,
              accent: profile.accent,
            ),
            _InfoCard(
              icon: Icons.assignment_outlined,
              label: 'Assignment',
              value: profile.assignment,
              accent: profile.accent,
            ),
            _InfoCard(
              icon: Icons.memory_outlined,
              label: 'Runtime',
              value: profile.modelSummary,
              accent: profile.accent,
            ),
          ],
        ),
        const SizedBox(height: 24),
        _TraitSection(profile: profile),
      ],
    );
  }
}

class _IdentityPanel extends StatelessWidget {
  const _IdentityPanel({
    required this.profile,
    required this.roles,
    this.onUpdateAgent,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final TeamAgentUpdate? onUpdateAgent;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      key: const ValueKey('team-profile-identity'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _PanelHeading(
            eyebrow: 'IDENTITY',
            title: 'The character behind the work.',
            detail: 'A compact presentation of this agent’s working persona.',
          ),
          const SizedBox(height: 26),
          Row(
            children: [
              Expanded(
                child: _DetailRow(label: 'Role template', value: profile.role),
              ),
              if (onUpdateAgent != null)
                OutlinedButton.icon(
                  key: const ValueKey('team-member-edit'),
                  onPressed: () => _edit(context),
                  icon: const Icon(Icons.edit_outlined, size: 16),
                  label: const Text('Edit member'),
                ),
            ],
          ),
          const SizedBox(height: 15),
          _DetailRow(label: 'Prompt pack', value: profile.promptPack),
          const SizedBox(height: 22),
          const Text(
            'PERSONALITY MARKERS',
            style: TextStyle(
              color: FrankColors.muted,
              fontSize: 10,
              fontWeight: FontWeight.w600,
              letterSpacing: 1,
            ),
          ),
          const SizedBox(height: 10),
          _TraitChips(profile: profile),
        ],
      ),
    );
  }

  Future<void> _edit(BuildContext context) async {
    final patch = await showDialog<TeamAgentPatch>(
      context: context,
      builder: (_) => _AgentEditDialog(profile: profile, roles: roles),
    );
    if (patch == null || onUpdateAgent == null || !context.mounted) return;
    try {
      await onUpdateAgent!(profile.employeeId, patch);
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Member could not be updated: $error')),
        );
      }
    }
  }
}
