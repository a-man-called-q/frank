part of 'team_surface.dart';

enum TeamRosterTab { members, roles }

enum TeamProfileTab { overview, identity, setup, capabilities, activity }

extension TeamProfileTabMetadata on TeamProfileTab {
  String get label => switch (this) {
    TeamProfileTab.overview => 'Overview',
    TeamProfileTab.identity => 'Identity',
    TeamProfileTab.setup => 'Setup',
    TeamProfileTab.capabilities => 'Capabilities',
    TeamProfileTab.activity => 'Activity',
  };

  IconData get icon => switch (this) {
    TeamProfileTab.overview => Icons.person_outline,
    TeamProfileTab.identity => Icons.badge_outlined,
    TeamProfileTab.setup => Icons.tune_outlined,
    TeamProfileTab.capabilities => Icons.extension_outlined,
    TeamProfileTab.activity => Icons.timeline_outlined,
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
        final columns = width >= 1120
            ? 3
            : width >= 720
            ? 2
            : 1;
        final textScale = MediaQuery.maybeOf(context)?.textScaler.scale(1) ?? 1;

        if (profiles.isEmpty) {
          return const SliverToBoxAdapter(
            child: _EmptyTeamState(key: ValueKey('team-roster')),
          );
        }
        // A one-column roster should grow with its copy at accessibility text
        // sizes. A fixed-ratio grid cannot negotiate the extra metadata lines
        // and would clip the card vertically on a narrow surface.
        if (columns == 1 || textScale > 1.25) {
          return SliverList(
            key: const ValueKey('team-agent-list'),
            delegate: SliverChildBuilderDelegate(
              (context, index) => Padding(
                padding: EdgeInsets.only(
                  top: index == 0 && width >= 600 ? 16 : 0,
                  bottom: index == profiles.length - 1 ? 0 : 16,
                ),
                child: Align(
                  alignment: Alignment.topCenter,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 560),
                    child: _TeamAgentCard(
                      profile: profiles[index],
                      reducedMotion: reducedMotion,
                      onSelected: () => onSelect(profiles[index]),
                    ),
                  ),
                ),
              ),
              childCount: profiles.length,
            ),
          );
        }
        return SliverGrid(
          key: const ValueKey('team-agent-grid'),
          delegate: SliverChildBuilderDelegate(
            (context, index) => _TeamAgentCard(
              profile: profiles[index],
              reducedMotion: reducedMotion,
              onSelected: () => onSelect(profiles[index]),
            ),
            childCount: profiles.length,
          ),
          gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
            crossAxisCount: columns,
            crossAxisSpacing: 16,
            mainAxisSpacing: 16,
            childAspectRatio: 0.94,
          ),
        );
      },
    );
  }
}

class _TeamRoles extends StatelessWidget {
  const _TeamRoles({
    required this.roles,
    required this.profiles,
    this.onUpdateRole,
    this.onArchiveRole,
  });

  final List<TeamRoleSummary> roles;
  final List<TeamAgentProfile> profiles;
  final TeamRoleUpdate? onUpdateRole;
  final TeamRoleArchive? onArchiveRole;

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
            ),
          );
        },
      ),
    );
  }

  Future<void> _edit(BuildContext context, TeamRoleSummary role) async {
    final patch = await showDialog<TeamRolePatch>(
      context: context,
      builder: (_) => _RoleEditDialog(role: role),
    );
    if (patch == null || onUpdateRole == null || !context.mounted) return;
    try {
      await onUpdateRole!(role.id, patch);
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Role could not be updated: $error')),
        );
      }
    }
  }

  Future<void> _archive(BuildContext context, TeamRoleSummary role) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('Archive ${role.name}?'),
        content: const Text(
          'Agents and tasks still using this role must be moved first.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
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
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Role could not be archived: $error')),
        );
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
    super.key,
  });

  final TeamRoleSummary role;
  final int memberCount;
  final VoidCallback? onEdit;
  final VoidCallback? onArchive;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(18),
    decoration: BoxDecoration(
      color: FrankColors.panel,
      borderRadius: BorderRadius.circular(14),
      border: Border.all(color: FrankColors.border),
    ),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        CircleAvatar(
          backgroundColor: FrankColors.aubergineAccent.withValues(alpha: 0.16),
          foregroundColor: FrankColors.aubergineAccent,
          child: const Icon(Icons.account_tree_outlined, size: 19),
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
              const SizedBox(height: 4),
              Text(
                role.description.isEmpty
                    ? 'No description yet.'
                    : role.description,
                style: const TextStyle(color: FrankColors.muted, fontSize: 12),
              ),
              const SizedBox(height: 10),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                children: [
                  _RolePill(label: role.template),
                  _RolePill(label: '$memberCount members'),
                  if (role.revision > 0)
                    _RolePill(label: 'Revision ${role.revision}'),
                  if (role.defaultModel != null)
                    _RolePill(label: role.defaultModel!),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(width: 8),
        if (onEdit != null)
          IconButton(
            key: ValueKey('team-role-edit-${role.id}'),
            tooltip: 'Edit role',
            onPressed: onEdit,
            icon: const Icon(Icons.edit_outlined, size: 18),
          ),
        if (onArchive != null)
          IconButton(
            key: ValueKey('team-role-archive-${role.id}'),
            tooltip: 'Archive role',
            onPressed: onArchive,
            icon: const Icon(Icons.archive_outlined, size: 18),
          ),
      ],
    ),
  );
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
    required this.selectedTab,
    required this.onTabSelected,
    this.onCreateRole,
    this.onCreateAgent,
  });

  final int agentCount;
  final bool isFixture;
  final int workingCount;
  final int availableCount;
  final List<TeamRoleSummary> roles;
  final TeamRosterTab selectedTab;
  final ValueChanged<TeamRosterTab> onTabSelected;
  final TeamRoleCreate? onCreateRole;
  final TeamAgentCreate? onCreateAgent;

  @override
  Widget build(BuildContext context) {
    final compact =
        OfficeLayoutMetricsScope.maybeOf(context)?.isCompact ?? false;
    final stats = Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        _TeamStat(label: 'Agents', value: '$agentCount'),
        if (!compact) ...[
          _TeamStat(label: 'Active', value: '$workingCount'),
          _TeamStat(label: 'Available', value: '$availableCount'),
        ],
        if (isFixture) const FrankSampleDataBadge(),
        if (onCreateRole != null)
          OutlinedButton.icon(
            key: const ValueKey('team-create-role'),
            onPressed: () => _createRole(context),
            icon: const Icon(Icons.account_tree_outlined, size: 16),
            label: const Text('New role'),
          ),
        if (onCreateAgent != null)
          FilledButton.icon(
            key: const ValueKey('team-create-agent'),
            onPressed: roles.isEmpty ? null : () => _createAgent(context),
            icon: const Icon(Icons.person_add_alt_1, size: 16),
            label: const Text('New member'),
          ),
      ],
    );
    final showRoleTab = roles.isNotEmpty || onCreateRole != null;
    final managementEnabled =
        roles.isNotEmpty || onCreateRole != null || onCreateAgent != null;
    final tabs = showRoleTab
        ? SegmentedButton<TeamRosterTab>(
            key: const ValueKey('team-roster-tabs'),
            segments: const [
              ButtonSegment(
                value: TeamRosterTab.members,
                label: Text('Members'),
                icon: Icon(Icons.people_outline, size: 15),
              ),
              ButtonSegment(
                value: TeamRosterTab.roles,
                label: Text('Roles'),
                icon: Icon(Icons.account_tree_outlined, size: 15),
              ),
            ],
            selected: {selectedTab},
            onSelectionChanged: (selection) => onTabSelected(selection.first),
          )
        : null;
    final createMenu = onCreateRole == null && onCreateAgent == null
        ? null
        : PopupMenuButton<String>(
            key: const ValueKey('team-create-menu'),
            tooltip: 'Create team item',
            icon: const Icon(Icons.add, size: 18),
            onSelected: (value) {
              if (value == 'role') {
                _createRole(context);
              } else {
                _createAgent(context);
              }
            },
            itemBuilder: (context) => [
              if (onCreateRole != null)
                const PopupMenuItem<String>(
                  value: 'role',
                  child: Text('New role'),
                ),
              if (onCreateAgent != null)
                PopupMenuItem<String>(
                  value: 'agent',
                  enabled: roles.isNotEmpty,
                  child: const Text('New member'),
                ),
            ],
          );
    return OfficePageHeader(
      title: 'Team',
      // At a narrow, accessibility-scaled width the full sentence wraps into
      // several lines and pushes the first roster card below the viewport.
      // Keep the context, but let the roster remain immediately reachable.
      description: compact && managementEnabled
          ? 'Agent roster and role templates.'
          : 'The people-shaped part of Frank. Meet the agents moving work forward.',
      actions: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          if (compact && managementEnabled)
            Row(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                if (tabs != null) Expanded(child: tabs),
                if (tabs != null && createMenu != null)
                  const SizedBox(width: 8),
                ?createMenu,
              ],
            )
          else ...[
            ?tabs,
            if (tabs != null) const SizedBox(height: 8),
            stats,
          ],
        ],
      ),
    );
  }

  Future<void> _createRole(BuildContext context) async {
    final draft = await showDialog<TeamRoleDraft>(
      context: context,
      builder: (_) => const _RoleDraftDialog(),
    );
    if (draft == null || onCreateRole == null || !context.mounted) return;
    try {
      await onCreateRole!(draft);
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Role could not be created: $error')),
        );
      }
    }
  }

  Future<void> _createAgent(BuildContext context) async {
    final draft = await showDialog<TeamAgentDraft>(
      context: context,
      builder: (_) => _AgentDraftDialog(roles: roles),
    );
    if (draft == null || onCreateAgent == null || !context.mounted) return;
    try {
      await onCreateAgent!(draft);
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Member could not be created: $error')),
        );
      }
    }
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
          Icon(Icons.groups_outlined, color: FrankColors.aubergineAccent),
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
