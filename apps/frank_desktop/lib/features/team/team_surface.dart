import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../app/theme.dart';
import '../../core/fixtures/fixture_team.dart';
import '../../core/models/team_models.dart';
import '../../core/models/workspace_models.dart';

/// The read-only Team mockup for the Office surface.
///
/// Team presentation data is fixture-backed for this milestone. Keeping the
/// selection and tab state here makes the roster/profile interaction real while
/// leaving the gateway and remote contracts untouched.
class TeamSurface extends StatefulWidget {
  const TeamSurface({required this.workspace, this.profiles, super.key});

  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;

  @override
  State<TeamSurface> createState() => _TeamSurfaceState();
}

class _TeamSurfaceState extends State<TeamSurface> {
  final FocusNode _surfaceFocusNode = FocusNode(debugLabel: 'team-surface');
  TeamAgentProfile? _selectedProfile;
  TeamProfileTab _selectedTab = TeamProfileTab.overview;
  bool _reducedMotion = false;

  List<TeamAgentProfile> get _profiles =>
      widget.profiles ?? fixtureTeamProfiles(widget.workspace);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _reducedMotion = MediaQuery.maybeOf(context)?.disableAnimations ?? false;
  }

  @override
  void didUpdateWidget(covariant TeamSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_selectedProfile == null) return;
    final selectedId = _selectedProfile!.employeeId;
    final stillPresent = _profiles.any(
      (profile) => profile.employeeId == selectedId,
    );
    if (!stillPresent) {
      _selectedProfile = null;
      _selectedTab = TeamProfileTab.overview;
    }
  }

  @override
  void dispose() {
    _surfaceFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final selected = _selectedProfile;
    final content = Semantics(
      container: true,
      explicitChildNodes: true,
      label: selected == null
          ? 'Team roster'
          : 'Character profile for ${selected.name}',
      child: selected == null
          ? _TeamRoster(
              profiles: _profiles,
              reducedMotion: _reducedMotion,
              onSelect: _openProfile,
            )
          : _TeamProfile(
              profile: selected,
              selectedTab: _selectedTab,
              reducedMotion: _reducedMotion,
              onBack: _closeProfile,
              onTabSelected: (tab) => setState(() => _selectedTab = tab),
            ),
    );
    return Focus(
      focusNode: _surfaceFocusNode,
      autofocus: true,
      onKeyEvent: (_, event) {
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.escape &&
            selected != null) {
          _closeProfile();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: LayoutBuilder(
        builder: (context, constraints) => constraints.maxHeight.isFinite
            ? SingleChildScrollView(primary: false, child: content)
            : content,
      ),
    );
  }

  void _openProfile(TeamAgentProfile profile) {
    setState(() {
      _selectedProfile = profile;
      _selectedTab = TeamProfileTab.overview;
    });
    _surfaceFocusNode.requestFocus();
  }

  void _closeProfile() {
    if (_selectedProfile == null) return;
    setState(() {
      _selectedProfile = null;
      _selectedTab = TeamProfileTab.overview;
    });
    _surfaceFocusNode.requestFocus();
  }
}

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
    return LayoutBuilder(
      builder: (context, constraints) {
        final width = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : MediaQuery.sizeOf(context).width;
        final columns = width >= 1120
            ? 4
            : width >= 720
            ? 2
            : 1;
        final compact = width < 720;
        final workingCount = profiles
            .where(
              (profile) =>
                  profile.status == TeamAgentStatus.working ||
                  profile.status == TeamAgentStatus.reviewing,
            )
            .length;
        final availableCount = profiles
            .where((profile) => profile.status == TeamAgentStatus.available)
            .length;

        return Column(
          key: const ValueKey('team-roster'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _TeamHeader(
              agentCount: profiles.length,
              workingCount: workingCount,
              availableCount: availableCount,
              compact: compact,
            ),
            const SizedBox(height: 28),
            if (profiles.isEmpty)
              const _EmptyTeamState()
            else
              GridView.builder(
                key: const ValueKey('team-agent-grid'),
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                itemCount: profiles.length,
                gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                  crossAxisCount: columns,
                  crossAxisSpacing: 16,
                  mainAxisSpacing: 16,
                  childAspectRatio: 0.94,
                ),
                itemBuilder: (context, index) => _TeamAgentCard(
                  profile: profiles[index],
                  reducedMotion: reducedMotion,
                  onSelected: () => onSelect(profiles[index]),
                ),
              ),
          ],
        );
      },
    );
  }
}

class _TeamHeader extends StatelessWidget {
  const _TeamHeader({
    required this.agentCount,
    required this.workingCount,
    required this.availableCount,
    required this.compact,
  });

  final int agentCount;
  final int workingCount;
  final int availableCount;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final copy = Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text(
          'Team',
          style: TextStyle(
            color: FrankColors.ink,
            fontSize: 30,
            fontWeight: FontWeight.w500,
            letterSpacing: -0.7,
          ),
        ),
        const SizedBox(height: 7),
        Text(
          'The people-shaped part of Frank. Meet the agents moving work forward.',
          style: TextStyle(
            color: FrankColors.muted,
            fontSize: 14,
            height: 20 / 14,
            letterSpacing: 0.05,
          ),
        ),
      ],
    );
    final stats = Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        _TeamStat(label: 'Agents', value: '$agentCount'),
        _TeamStat(label: 'Active', value: '$workingCount'),
        _TeamStat(label: 'Available', value: '$availableCount'),
      ],
    );

    if (compact) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [copy, const SizedBox(height: 18), stats],
      );
    }
    return Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        Expanded(child: copy),
        const SizedBox(width: 24),
        stats,
      ],
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
  const _EmptyTeamState();

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
                Expanded(child: _PortraitPanel(profile: profile)),
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
              _ProviderBadge(provider: profile.provider),
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

class _ProviderBadge extends StatelessWidget {
  const _ProviderBadge({required this.provider});

  final String provider;

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
        provider,
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
    final color = status.color;
    return Container(
      padding: EdgeInsets.symmetric(
        horizontal: compact ? 7 : 9,
        vertical: compact ? 4 : 6,
      ),
      decoration: BoxDecoration(
        color: FrankColors.canvas.withValues(alpha: compact ? 0.82 : 0.65),
        borderRadius: BorderRadius.circular(7),
        border: Border.all(color: color.withValues(alpha: 0.65)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(status.icon, size: compact ? 12 : 14, color: color),
          const SizedBox(width: 5),
          Text(
            status.label,
            style: TextStyle(
              color: color,
              fontSize: compact ? 10 : 12,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}

class _TeamProfile extends StatelessWidget {
  const _TeamProfile({
    required this.profile,
    required this.selectedTab,
    required this.reducedMotion,
    required this.onBack,
    required this.onTabSelected,
  });

  final TeamAgentProfile profile;
  final TeamProfileTab selectedTab;
  final bool reducedMotion;
  final VoidCallback onBack;
  final ValueChanged<TeamProfileTab> onTabSelected;

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('team-character-profile'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ProfileBreadcrumb(profile: profile, onBack: onBack),
        const SizedBox(height: 22),
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
            child: _ProfileTabBody(profile: profile, tab: selectedTab),
          ),
        ),
      ],
    );
  }
}

class _ProfileBreadcrumb extends StatelessWidget {
  const _ProfileBreadcrumb({required this.profile, required this.onBack});

  final TeamAgentProfile profile;
  final VoidCallback onBack;

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
            icon: const Icon(Icons.arrow_back, size: 18),
            color: FrankColors.ink,
            style: IconButton.styleFrom(
              backgroundColor: FrankColors.panelRaised,
              side: const BorderSide(color: FrankColors.border),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(9),
              ),
            ),
          ),
        ),
        const SizedBox(width: 12),
        const Text(
          'Team',
          style: TextStyle(color: FrankColors.muted, fontSize: 12),
        ),
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 8),
          child: Icon(Icons.chevron_right, size: 15, color: FrankColors.border),
        ),
        Text(
          profile.name,
          style: const TextStyle(color: FrankColors.ink, fontSize: 12),
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
        Row(
          children: [
            const Icon(
              Icons.memory_outlined,
              size: 15,
              color: FrankColors.muted,
            ),
            const SizedBox(width: 7),
            Text(
              profile.providerSummary,
              style: const TextStyle(color: FrankColors.muted, fontSize: 12),
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
              style: const TextStyle(color: FrankColors.muted, fontSize: 12),
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
        borderRadius: BorderRadius.circular(11),
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
          borderRadius: BorderRadius.circular(8),
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
            size: 15,
            color: selected ? FrankColors.ink : FrankColors.muted,
          ),
          label: Text(tab.label),
          style: TextButton.styleFrom(
            foregroundColor: selected ? FrankColors.ink : FrankColors.muted,
            padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 9),
            textStyle: const TextStyle(fontSize: 12),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(8),
            ),
          ),
        ),
      ),
    );
  }
}

class _ProfileTabBody extends StatelessWidget {
  const _ProfileTabBody({required this.profile, required this.tab});

  final TeamAgentProfile profile;
  final TeamProfileTab tab;

  @override
  Widget build(BuildContext context) {
    return switch (tab) {
      TeamProfileTab.overview => _OverviewPanel(profile: profile),
      TeamProfileTab.identity => _IdentityPanel(profile: profile),
      TeamProfileTab.setup => _SetupPanel(profile: profile),
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
              value: profile.providerSummary,
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
  const _IdentityPanel({required this.profile});

  final TeamAgentProfile profile;

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
          _DetailRow(label: 'Role template', value: profile.role),
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
}

class _SetupPanel extends StatelessWidget {
  const _SetupPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      key: const ValueKey('team-profile-setup'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _PanelHeading(
            eyebrow: 'RUNTIME SETUP',
            title: 'Configured for the work at hand.',
            detail: 'These values are read-only in the Team mockup.',
          ),
          const SizedBox(height: 24),
          _SetupRow(
            icon: Icons.cloud_outlined,
            label: 'Provider',
            value: profile.provider,
          ),
          _SetupRow(
            icon: Icons.memory_outlined,
            label: 'Default model',
            value: profile.model,
          ),
          _SetupRow(
            icon: Icons.auto_awesome_outlined,
            label: 'Prompt pack',
            value: profile.promptPack,
          ),
          _SetupRow(
            icon: Icons.tune_outlined,
            label: 'Level',
            value: profile.level,
          ),
        ],
      ),
    );
  }
}

class _CapabilitiesPanel extends StatelessWidget {
  const _CapabilitiesPanel({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('team-profile-capabilities'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _PanelHeading(
          eyebrow: 'LOADOUT',
          title: 'Tools with a reason to be here.',
          detail: 'Access is shown as a presentation fixture for this mockup.',
        ),
        const SizedBox(height: 18),
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            for (final capability in profile.capabilities)
              _CapabilityCard(capability: capability, accent: profile.accent),
          ],
        ),
      ],
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
            eyebrow: 'RECENT ACTIVITY',
            title: 'A short trail of useful things.',
            detail:
                'Events are deterministic until the live gateway is connected.',
          ),
          const SizedBox(height: 20),
          for (var index = 0; index < profile.activity.length; index++) ...[
            _ActivityRow(
              event: profile.activity[index],
              accent: profile.accent,
            ),
            if (index < profile.activity.length - 1)
              const Divider(height: 24, color: FrankColors.border),
          ],
        ],
      ),
    );
  }
}

class _PanelHeading extends StatelessWidget {
  const _PanelHeading({
    required this.eyebrow,
    required this.title,
    required this.detail,
  });

  final String eyebrow;
  final String title;
  final String detail;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          eyebrow,
          style: const TextStyle(
            color: FrankColors.aubergineAccent,
            fontSize: 10,
            fontWeight: FontWeight.w600,
            letterSpacing: 1.15,
          ),
        ),
        const SizedBox(height: 7),
        Text(
          title,
          style: const TextStyle(
            color: FrankColors.ink,
            fontSize: 19,
            fontWeight: FontWeight.w500,
          ),
        ),
        const SizedBox(height: 5),
        Text(
          detail,
          style: const TextStyle(
            color: FrankColors.muted,
            fontSize: 12,
            height: 1.45,
          ),
        ),
      ],
    );
  }
}

class _PanelCard extends StatelessWidget {
  const _PanelCard({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: child,
    );
  }
}

class _InfoCard extends StatelessWidget {
  const _InfoCard({
    required this.icon,
    required this.label,
    required this.value,
    required this.accent,
  });

  final IconData icon;
  final String label;
  final String value;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 220,
      padding: const EdgeInsets.all(15),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, size: 18, color: accent),
          const SizedBox(height: 13),
          Text(
            label,
            style: const TextStyle(color: FrankColors.muted, fontSize: 11),
          ),
          const SizedBox(height: 4),
          Text(
            value,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              color: FrankColors.ink,
              fontSize: 13,
              fontWeight: FontWeight.w500,
              height: 1.3,
            ),
          ),
        ],
      ),
    );
  }
}

class _TraitSection extends StatelessWidget {
  const _TraitSection({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return _PanelCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'WHAT THEY BRING',
            style: TextStyle(
              color: FrankColors.aubergineAccent,
              fontSize: 10,
              fontWeight: FontWeight.w600,
              letterSpacing: 1.1,
            ),
          ),
          const SizedBox(height: 10),
          _TraitChips(profile: profile),
        ],
      ),
    );
  }
}

class _TraitChips extends StatelessWidget {
  const _TraitChips({required this.profile});

  final TeamAgentProfile profile;

  @override
  Widget build(BuildContext context) {
    return Wrap(
      spacing: 7,
      runSpacing: 7,
      children: [
        for (final trait in profile.traits)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
            decoration: BoxDecoration(
              color: profile.accent.withValues(alpha: 0.1),
              borderRadius: BorderRadius.circular(7),
              border: Border.all(color: profile.accent.withValues(alpha: 0.34)),
            ),
            child: Text(
              trait,
              style: TextStyle(color: profile.accent, fontSize: 11),
            ),
          ),
      ],
    );
  }
}

class _DetailRow extends StatelessWidget {
  const _DetailRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: Text(
            label,
            style: const TextStyle(color: FrankColors.muted, fontSize: 12),
          ),
        ),
        Text(
          value,
          style: const TextStyle(
            color: FrankColors.ink,
            fontSize: 12,
            fontWeight: FontWeight.w500,
          ),
        ),
      ],
    );
  }
}

class _SetupRow extends StatelessWidget {
  const _SetupRow({
    required this.icon,
    required this.label,
    required this.value,
  });

  final IconData icon;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 11),
      child: Row(
        children: [
          Icon(icon, size: 17, color: FrankColors.aubergineAccent),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              label,
              style: const TextStyle(color: FrankColors.muted, fontSize: 12),
            ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
            decoration: BoxDecoration(
              color: FrankColors.panelRaised,
              borderRadius: BorderRadius.circular(7),
              border: Border.all(color: FrankColors.border),
            ),
            child: Text(
              value,
              style: const TextStyle(
                color: FrankColors.ink,
                fontSize: 12,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _CapabilityCard extends StatelessWidget {
  const _CapabilityCard({required this.capability, required this.accent});

  final TeamCapability capability;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 220,
      padding: const EdgeInsets.all(15),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        children: [
          Container(
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              color: accent.withValues(alpha: 0.11),
              borderRadius: BorderRadius.circular(8),
            ),
            child: Icon(capability.icon, size: 17, color: accent),
          ),
          const SizedBox(width: 11),
          Expanded(
            child: Text(
              capability.label,
              style: const TextStyle(
                color: FrankColors.ink,
                fontSize: 12,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ActivityRow extends StatelessWidget {
  const _ActivityRow({required this.event, required this.accent});

  final TeamActivityEvent event;
  final Color accent;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          padding: const EdgeInsets.all(8),
          decoration: BoxDecoration(
            color: accent.withValues(alpha: 0.11),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Icon(event.icon, size: 16, color: accent),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                event.label,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 12,
                  fontWeight: FontWeight.w500,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                event.detail,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: 11,
                  height: 1.35,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(width: 12),
        Text(
          event.timeLabel,
          style: const TextStyle(color: FrankColors.muted, fontSize: 10),
        ),
      ],
    );
  }
}
