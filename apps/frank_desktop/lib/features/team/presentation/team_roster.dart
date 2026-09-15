part of 'team_surface.dart';

enum TeamRosterTab { members, roles }

enum TeamProfileTab { overview, identity, setup }

extension TeamProfileTabMetadata on TeamProfileTab {
  String get label => switch (this) {
    TeamProfileTab.overview => 'Overview',
    TeamProfileTab.identity => 'Identity',
    TeamProfileTab.setup => 'Setup',
  };

  IconData get icon => switch (this) {
    TeamProfileTab.overview => FrankIcons.personOutline,
    TeamProfileTab.identity => FrankIcons.badgeOutlined,
    TeamProfileTab.setup => FrankIcons.tuneOutlined,
  };
}

class _TeamRoster extends StatelessWidget {
  const _TeamRoster({
    required this.profiles,
    required this.reducedMotion,
    required this.onSelect,
  });

  final List<TeamAgentProfile> profiles;
  final bool reducedMotion;
  final ValueChanged<TeamAgentProfile> onSelect;

  @override
  Widget build(BuildContext context) {
    return SliverLayoutBuilder(
      builder: (context, constraints) {
        final width =
            OfficeLayoutMetricsScope.maybeOf(context)?.availableWidth ??
            constraints.crossAxisExtent;
        final textScale = MediaQuery.maybeOf(context)?.textScaler.scale(1) ?? 1;

        if (profiles.isEmpty) {
          return const SliverToBoxAdapter(
            child: _EmptyTeamState(key: ValueKey('team-roster')),
          );
        }
        return SliverMainAxisGroup(
          slivers: [
            SliverToBoxAdapter(
              child: _TeamRosterHeader(
                compact: width < 760 || textScale > 1.25,
              ),
            ),
            SliverList(
              key: const ValueKey('team-agent-list'),
              delegate: SliverChildBuilderDelegate(
                (context, index) => Padding(
                  padding: EdgeInsets.only(
                    bottom: index == profiles.length - 1 ? 0 : 8,
                  ),
                  child: _TeamAgentCard(
                    profile: profiles[index],
                    reducedMotion: reducedMotion,
                    compact: width < 760 || textScale > 1.25,
                    onSelected: () => onSelect(profiles[index]),
                  ),
                ),
                childCount: profiles.length,
              ),
            ),
          ],
        );
      },
    );
  }
}

class _TeamRosterHeader extends StatelessWidget {
  const _TeamRosterHeader({required this.compact});

  final bool compact;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(14, 16, 14, 8),
    child: Row(
      children: [
        const Expanded(flex: 3, child: _RosterColumnLabel('MEMBER')),
        const Expanded(flex: 2, child: _RosterColumnLabel('STATUS')),
        const Expanded(flex: 2, child: _RosterColumnLabel('ROLE')),
        if (!compact) ...[
          const Expanded(flex: 3, child: _RosterColumnLabel('CURRENT TASK')),
          const Expanded(flex: 2, child: _RosterColumnLabel('EFFECTIVE MODEL')),
          const Expanded(flex: 1, child: _RosterColumnLabel('REVISION')),
        ],
        const SizedBox(width: 74),
      ],
    ),
  );
}

class _RosterColumnLabel extends StatelessWidget {
  const _RosterColumnLabel(this.label);

  final String label;

  @override
  Widget build(BuildContext context) => Text(
    label,
    style: const TextStyle(
      color: FrankColors.muted,
      fontSize: 10,
      fontWeight: FontWeight.w700,
      letterSpacing: 0.8,
    ),
  );
}

class _TeamRoles extends StatelessWidget {
  const _TeamRoles({
    required this.roles,
    required this.profiles,
    this.models = const [],
    this.onUpdateRole,
    this.onArchiveRole,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final List<TeamRoleSummary> roles;
  final List<TeamAgentProfile> profiles;
  final List<OpenRouterModel> models;
  final TeamRoleUpdate? onUpdateRole;
  final TeamRoleArchive? onArchiveRole;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    final activeRoles = roles.where((role) => !role.archived).toList();
    if (activeRoles.isEmpty) {
      return const SliverToBoxAdapter(
        child: _EmptyTeamState(key: ValueKey('team-roles-empty')),
      );
    }
    return SliverPadding(
      padding: const EdgeInsets.only(top: 16, bottom: 8),
      sliver: SliverList.builder(
        itemCount: activeRoles.length,
        itemBuilder: (context, index) {
          final role = activeRoles[index];
          final memberCount = profiles
              .where((profile) => profile.roleId == role.id)
              .length;
          return Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: _RoleCard(
              key: ValueKey('team-role-card-${role.id}'),
              role: role,
              memberCount: memberCount,
              onEdit: onUpdateRole == null ? null : () => _edit(context, role),
              onArchive: onArchiveRole == null
                  ? null
                  : () => _archive(context, role),
              canMutate: canMutate,
              mutationDisabledReason: mutationDisabledReason,
            ),
          );
        },
      ),
    );
  }

  Future<void> _edit(BuildContext context, TeamRoleSummary role) async {
    await Navigator.of(context).push<void>(
      PageRouteBuilder<void>(
        pageBuilder: (_, _, _) => _TeamFormPage(
          form: _RoleEditDialog(
            role: role,
            models: models,
            onSave: (patch) async {
              await onUpdateRole!(role.id, patch);
            },
          ),
        ),
        transitionDuration: Duration.zero,
        reverseTransitionDuration: Duration.zero,
      ),
    );
  }

  Future<void> _archive(BuildContext context, TeamRoleSummary role) async {
    final confirmed = await showFrankDialog<bool>(
      context: context,
      builder: (dialogContext) => FrankDialogScaffold(
        title: Text(
          'Archive ${role.name}?',
          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        ),
        content: const Text(
          'Agents and tasks still using this role must be moved first.',
        ),
        actions: [
          FButton(
            onPress: () => Navigator.pop(dialogContext, false),
            variant: FButtonVariant.ghost,
            child: const Text('Cancel'),
          ),
          FButton(
            onPress: () => Navigator.pop(dialogContext, true),
            variant: FButtonVariant.destructive,
            child: const Text('Archive role'),
          ),
        ],
      ),
    );
    if (confirmed != true || onArchiveRole == null || !context.mounted) {
      return;
    }
    try {
      await onArchiveRole!(role.id);
    } catch (error) {
      if (context.mounted) {
        showFrankToast(context, 'Role could not be archived: $error');
      }
    }
  }
}

class _RoleCard extends StatelessWidget {
  const _RoleCard({
    required this.role,
    required this.memberCount,
    this.onEdit,
    this.onArchive,
    this.canMutate = true,
    this.mutationDisabledReason,
    super.key,
  });

  final TeamRoleSummary role;
  final int memberCount;
  final VoidCallback? onEdit;
  final VoidCallback? onArchive;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(FrankUiTokens.panelPadding),
    decoration: BoxDecoration(
      color: FrankColors.panel,
      borderRadius: BorderRadius.circular(14),
      border: Border.all(color: FrankColors.border),
    ),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        FAvatar.raw(
          size: 40,
          child: const Icon(FrankIcons.accountTreeOutlined, size: 19),
        ),
        const SizedBox(width: 14),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                role.name,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 10),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                children: [
                  _RolePill(
                    label: role.defaultModel == null ||
                            role.defaultModel!.trim().isEmpty
                        ? 'Model unavailable'
                        : role.defaultModel!,
                  ),
                  _RolePill(label: '$memberCount members'),
                  if (role.revision > 0)
                    _RolePill(label: 'Revision ${role.revision}'),
                  _RolePill(label: _rolePolicySummary(role.policy)),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(width: 8),
        if (onEdit != null)
          FButton.icon(
            key: ValueKey('team-role-edit-${role.id}'),
            semanticsLabel: 'Edit role',
            onPress: canMutate ? onEdit : null,
            semanticsTooltip: canMutate
                ? 'Edit role'
                : mutationDisabledReason ?? 'Reconnect before changing a role',
            child: const Icon(FrankIcons.editOutlined, size: 18),
          ),
        if (onArchive != null)
          FButton.icon(
            key: ValueKey('team-role-archive-${role.id}'),
            semanticsLabel: 'Archive role',
            onPress: canMutate ? onArchive : null,
            semanticsTooltip: canMutate
                ? 'Archive role'
                : mutationDisabledReason ?? 'Reconnect before changing a role',
            child: const Icon(FrankIcons.archiveOutlined, size: 18),
          ),
      ],
    ),
  );
}

String _rolePolicySummary(Map<String, Object?> policy) {
  final filesystem = policy['filesystem']?.toString() ?? 'workspace-write';
  final shell = policy['shell']?.toString() ?? 'ask';
  final network = policy['network']?.toString() ?? 'ask';
  return '${filesystem.replaceAll('-', ' ')} · shell $shell · network $network';
}

class _RolePill extends StatelessWidget {
  const _RolePill({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
    decoration: BoxDecoration(
      color: FrankColors.panelRaised,
      borderRadius: BorderRadius.circular(7),
      border: Border.all(color: FrankColors.border),
    ),
    child: Text(
      label,
      style: const TextStyle(color: FrankColors.muted, fontSize: 11),
    ),
  );
}

class _TeamHeader extends StatelessWidget {
  const _TeamHeader({
    required this.agentCount,
    required this.isFixture,
    required this.workingCount,
    required this.availableCount,
    required this.roles,
    this.models = const [],
    required this.selectedTab,
    required this.onTabSelected,
    this.onCreateRole,
    this.onCreateAgent,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final int agentCount;
  final bool isFixture;
  final int workingCount;
  final int availableCount;
  final List<TeamRoleSummary> roles;
  final List<OpenRouterModel> models;
  final TeamRosterTab selectedTab;
  final ValueChanged<TeamRosterTab> onTabSelected;
  final TeamRoleCreate? onCreateRole;
  final TeamAgentCreate? onCreateAgent;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    final compact =
        OfficeLayoutMetricsScope.maybeOf(context)?.isCompact ?? false;
    final stats = <Widget>[
      _TeamStat(label: 'Agents', value: '$agentCount'),
      if (!compact) ...[
        _TeamStat(label: 'Active', value: '$workingCount'),
        _TeamStat(label: 'Available', value: '$availableCount'),
      ],
      if (isFixture) const FrankSampleDataBadge(),
    ];
    final showRoleTab = roles.isNotEmpty || onCreateRole != null;
    final managementEnabled =
        roles.isNotEmpty || onCreateRole != null || onCreateAgent != null;
    final tabs = showRoleTab
        ? FrankSegmentedControl<TeamRosterTab>(
            key: const ValueKey('team-roster-tabs'),
            value: selectedTab,
            items: const [
              (TeamRosterTab.members, 'Members', FrankIcons.peopleOutline),
              (TeamRosterTab.roles, 'Roles', FrankIcons.accountTreeOutlined),
            ],
            onChanged: onTabSelected,
          )
        : null;
    final createAction = selectedTab == TeamRosterTab.roles
        ? onCreateRole == null
              ? null
              : FButton(
                  key: const ValueKey('team-create-role'),
                  onPress: canMutate ? () => _createRole(context) : null,
                  size: FButtonSizeVariant.sm,
                  prefix: const Icon(FrankIcons.accountTreeOutlined, size: 16),
                  child: const Text('New role'),
                )
        : onCreateAgent == null
        ? null
        : FButton(
            key: const ValueKey('team-create-agent'),
            onPress: canMutate ? () => _createAgent(context) : null,
            size: FButtonSizeVariant.sm,
            prefix: const Icon(FrankIcons.personAddAlt1, size: 16),
            child: const Text('Add member'),
          );
    // Keep the header controls in one compact flow. A nested Column here used
    // to produce two full-width stacked bars (tabs, then actions) at the
    // tablet breakpoint, which made the roster feel disconnected from its
    // controls and pushed the first row below the fold.
    final controls = Wrap(
      alignment: WrapAlignment.end,
      crossAxisAlignment: WrapCrossAlignment.center,
      spacing: 8,
      runSpacing: 8,
      children: [?tabs, ...stats, ?createAction],
    );
    return OfficePageHeader(
      title: 'Team',
      // At a narrow, accessibility-scaled width the full sentence wraps into
      // several lines and pushes the first roster card below the viewport.
      // Keep the context, but let the roster remain immediately reachable.
      description: compact && managementEnabled
          ? 'Agent roster and role-backed workers.'
          : 'Manage role-backed agents and inspect their runtime state.',
      actions: controls,
    );
  }

  Future<void> _createRole(BuildContext context) async {
    await Navigator.of(context).push<void>(
      PageRouteBuilder<void>(
        pageBuilder: (_, _, _) => _TeamFormPage(
          form: _RoleDraftDialog(
            models: models,
            onSave: (draft) async {
              await onCreateRole!(draft);
            },
          ),
        ),
        transitionDuration: Duration.zero,
        reverseTransitionDuration: Duration.zero,
      ),
    );
  }

  Future<void> _createAgent(BuildContext context) async {
    if (roles.isEmpty) {
      // A member cannot operate without a role. Route the owner to role
      // creation first instead of presenting a form that can only fail.
      await _createRole(context);
      return;
    }
    await Navigator.of(context).push<void>(
      PageRouteBuilder<void>(
        pageBuilder: (_, _, _) => _TeamFormPage(
          form: _AgentDraftDialog(
            roles: roles,
            onSave: (draft) async {
              await onCreateAgent!(draft);
            },
          ),
        ),
        transitionDuration: Duration.zero,
        reverseTransitionDuration: Duration.zero,
      ),
    );
  }
}

class _TeamStat extends StatelessWidget {
  const _TeamStat({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 8),
      decoration: BoxDecoration(
        color: FrankColors.panelRaised.withValues(alpha: 0.76),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: FrankColors.border),
      ),
      child: RichText(
        text: TextSpan(
          children: [
            TextSpan(
              text: '$value ',
              style: const TextStyle(
                color: FrankColors.ink,
                fontSize: 12,
                fontWeight: FontWeight.w600,
              ),
            ),
            TextSpan(
              text: label,
              style: const TextStyle(color: FrankColors.muted, fontSize: 12),
            ),
          ],
        ),
      ),
    );
  }
}

class _EmptyTeamState extends StatelessWidget {
  const _EmptyTeamState({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(28),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: const Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(FrankIcons.groupsOutlined, color: FrankColors.aubergineAccent),
          SizedBox(height: 12),
          Text(
            'No agents yet',
            style: TextStyle(color: FrankColors.ink, fontSize: 18),
          ),
          SizedBox(height: 6),
          Text(
            'Team profiles will appear here once the agency has an agent roster.',
            style: TextStyle(color: FrankColors.muted, height: 1.45),
          ),
        ],
      ),
    );
  }
}
