import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../../app/controls/frank_desktop_menu.dart';
import '../../../app/layout/office_surface_frame.dart';
import '../../../app/office_ui.dart';
import '../../../app/theme.dart';
import '../../../core/models/openrouter_models.dart';
import '../../../core/models/team_models.dart';
import '../../../core/models/workspace_models.dart';

part 'team_roster.dart';
part 'team_profile.dart';
part 'team_setup.dart';
part 'team_dialogs.dart';

typedef TeamModelChange =
    Future<List<TeamAgentProfile>> Function(String agentId, String? model);

typedef TeamRoleModelChange =
    Future<List<TeamAgentProfile>> Function(String roleId, String? model);
typedef TeamRoleCreate =
    Future<List<TeamAgentProfile>> Function(TeamRoleDraft draft);
typedef TeamAgentCreate =
    Future<List<TeamAgentProfile>> Function(TeamAgentDraft draft);
typedef TeamRoleUpdate =
    Future<void> Function(String roleId, TeamRolePatch patch);
typedef TeamRoleArchive = Future<void> Function(String roleId);
typedef TeamAgentUpdate =
    Future<List<TeamAgentProfile>> Function(
      String agentId,
      TeamAgentPatch patch,
    );
typedef TeamAgentArchive =
    Future<List<TeamAgentProfile>> Function(String agentId);

/// The gateway-backed Team surface. Profiles are supplied by the shell
/// composition root so this widget never silently replaces a failed or missing
/// load with demo data. Runtime mutations stay behind the gateway while the
/// profile keeps the local selection/tab state needed for keyboard navigation.
class TeamSurface extends StatefulWidget {
  const TeamSurface({
    required this.workspace,
    this.profiles,
    this.isFixture = true,
    this.catalog,
    this.catalogError,
    this.catalogLoading = false,
    this.onSaveAgentModel,
    this.onSaveRoleModel,
    this.roles = const [],
    this.onCreateRole,
    this.onCreateAgent,
    this.onUpdateRole,
    this.onArchiveRole,
    this.onUpdateAgent,
    this.onArchiveAgent,
    super.key,
  });

  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;
  final bool isFixture;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;
  final List<TeamRoleSummary> roles;
  final TeamRoleCreate? onCreateRole;
  final TeamAgentCreate? onCreateAgent;
  final TeamRoleUpdate? onUpdateRole;
  final TeamRoleArchive? onArchiveRole;
  final TeamAgentUpdate? onUpdateAgent;
  final TeamAgentArchive? onArchiveAgent;

  @override
  State<TeamSurface> createState() => _TeamSurfaceState();
}

class _TeamSurfaceState extends State<TeamSurface> {
  final FocusNode _surfaceFocusNode = FocusNode(debugLabel: 'team-surface');
  late final ScrollController _rosterScrollController = ScrollController();
  late final ScrollController _profileScrollController = ScrollController();
  TeamAgentProfile? _selectedProfile;
  TeamProfileTab _selectedTab = TeamProfileTab.overview;
  TeamRosterTab _selectedRosterTab = TeamRosterTab.members;
  bool _reducedMotion = false;
  double _rosterScrollOffset = 0;
  bool _restoreRosterPosition = false;

  @override
  void initState() {
    super.initState();
    _rosterScrollController.addListener(_recordRosterScrollOffset);
  }

  List<TeamAgentProfile> get _profiles => widget.profiles ?? const [];

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
    final updatedProfile = _profiles
        .where((profile) => profile.employeeId == selectedId)
        .firstOrNull;
    if (updatedProfile == null) {
      _selectedProfile = null;
      _selectedTab = TeamProfileTab.overview;
    } else if (!identical(updatedProfile, _selectedProfile)) {
      // Keep an open profile projected from the latest gateway response. The
      // employee ID is the stable identity; provider, portrait, and model
      // metadata may change while the profile is open.
      _selectedProfile = updatedProfile;
    }
  }

  @override
  void dispose() {
    _rosterScrollController
      ..removeListener(_recordRosterScrollOffset)
      ..dispose();
    _profileScrollController.dispose();
    _surfaceFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final selected = _selectedProfile;
    final profileOpen = selected != null;
    final header = profileOpen
        ? _ProfileHeader(
            profile: selected,
            onBack: _closeProfile,
            onArchive: widget.onArchiveAgent == null
                ? null
                : () => _archiveSelected(context, selected),
          )
        : _TeamHeader(
            agentCount: _profiles.length,
            isFixture: widget.isFixture,
            roles: widget.roles,
            selectedTab: _selectedRosterTab,
            onTabSelected: (tab) => setState(() => _selectedRosterTab = tab),
            onCreateRole: widget.onCreateRole,
            onCreateAgent: widget.onCreateAgent,
            workingCount: _profiles
                .where(
                  (profile) =>
                      profile.status == TeamAgentStatus.working ||
                      profile.status == TeamAgentStatus.reviewing,
                )
                .length,
            availableCount: _profiles
                .where((profile) => profile.status == TeamAgentStatus.available)
                .length,
          );
    final rosterContent = _selectedRosterTab == TeamRosterTab.members
        ? _TeamRoster(
            profiles: _profiles,
            reducedMotion: _reducedMotion,
            onSelect: _openProfile,
          )
        : _TeamRoles(
            roles: widget.roles,
            profiles: _profiles,
            onUpdateRole: widget.onUpdateRole,
            onArchiveRole: widget.onArchiveRole,
          );
    final content = SliverSemantics(
      container: true,
      explicitChildNodes: true,
      label: !profileOpen
          ? (_selectedRosterTab == TeamRosterTab.members
                ? 'Team roster'
                : 'Team roles')
          : 'Character profile for ${selected.name}',
      sliver: !profileOpen
          ? rosterContent
          : SliverToBoxAdapter(
              child: _TeamProfile(
                profile: selected,
                roles: widget.roles,
                selectedTab: _selectedTab,
                reducedMotion: _reducedMotion,
                catalog: widget.catalog,
                catalogError: widget.catalogError,
                catalogLoading: widget.catalogLoading,
                onSaveAgentModel: widget.onSaveAgentModel,
                onSaveRoleModel: widget.onSaveRoleModel,
                onUpdateAgent: widget.onUpdateAgent,
                onTabSelected: (tab) => setState(() => _selectedTab = tab),
              ),
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
      child: OfficeSurfaceFrame.page(
        key: ValueKey(profileOpen ? 'team-profile-frame' : 'team-roster-frame'),
        fullWidth: true,
        scrollKey: ValueKey(
          profileOpen ? 'team-profile-scroll' : 'team-roster-scroll',
        ),
        scrollController: profileOpen
            ? _profileScrollController
            : _rosterScrollController,
        header: header,
        slivers: [content],
      ),
    );
  }

  void _openProfile(TeamAgentProfile profile) {
    setState(() {
      _selectedProfile = profile;
      _selectedTab = TeamProfileTab.overview;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _selectedProfile == null) return;
      if (_profileScrollController.hasClients) {
        _profileScrollController.jumpTo(0);
      }
    });
    _surfaceFocusNode.requestFocus();
  }

  void _closeProfile() {
    if (_selectedProfile == null) return;
    setState(() {
      _selectedProfile = null;
      _selectedTab = TeamProfileTab.overview;
    });
    _restoreRosterPosition = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_restoreRosterPosition) return;
      _restoreRosterPosition = false;
      if (_rosterScrollController.hasClients) {
        final position = _rosterScrollController.position;
        position.jumpTo(
          _rosterScrollOffset.clamp(0, position.maxScrollExtent).toDouble(),
        );
      }
    });
    _surfaceFocusNode.requestFocus();
  }

  Future<void> _archiveSelected(
    BuildContext context,
    TeamAgentProfile profile,
  ) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('Archive ${profile.name}?'),
        content: const Text(
          'Archiving keeps the agent and task history available without allowing new assignments.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('Archive member'),
          ),
        ],
      ),
    );
    if (confirmed != true || widget.onArchiveAgent == null || !mounted) return;
    try {
      await widget.onArchiveAgent!(profile.employeeId);
      if (mounted) _closeProfile();
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Member could not be archived: $error')),
        );
      }
    }
  }

  void _recordRosterScrollOffset() {
    if (_rosterScrollController.hasClients) {
      _rosterScrollOffset = _rosterScrollController.offset;
    }
  }
}
