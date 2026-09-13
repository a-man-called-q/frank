part of 'office_shell.dart';

class _MainSurface extends StatelessWidget {
  const _MainSurface({
    required this.workspace,
    required this.project,
    required this.mission,
    required this.conversation,
    required this.messages,
    required this.generating,
    required this.sceneController,
    super.key,
  });

  final OfficeWorkspace workspace;
  final OfficeProject? project;
  final OfficeMission? mission;
  final ConversationContext? conversation;
  final List<OfficeMessage> messages;
  final bool generating;
  final OfficeSceneController sceneController;

  @override
  Widget build(BuildContext context) {
    final shell = context.watch<ShellBloc>().state;
    if (shell.destination case SettingsDestination(:final section)) {
      return _SettingsSectionSurface(section: section, workspace: workspace);
    }
    if (project == null || conversation == null) {
      return const NoProjectSurface();
    }

    return AccountExecutiveChat(
      executive: workspace.accountExecutive,
      project: project!,
      mission: mission,
      showNoMissionsNotice: true,
      messages: messages,
      generating: generating,
      // The shell owns one retained floor for every destination. Keeping the
      // chat rail floor-free prevents a project switch from replacing the
      // scene/controller that carries the camera state.
      renderFloor: false,
      // Reused so the floor-reset button (rendered beside the composer) acts
      // on the same camera the shell's shared floor is displaying.
      sceneController: sceneController,
      onSend: (text) => context.read<ChatBloc>().add(
        ChatMessageSubmitted(context: conversation!, text: text),
      ),
      onStop: () =>
          context.read<ChatBloc>().add(const ChatMessageStopRequested()),
    );
  }
}

class NoProjectSurface extends StatelessWidget {
  const NoProjectSurface({super.key});

  @override
  Widget build(BuildContext context) {
    return const ColoredBox(
      color: FrankColors.canvas,
      child: FrankEmptyState(
        showLogo: true,
        title: 'Your office is quiet',
        message: 'No projects yet',
        description: 'Projects will appear here after you create one.',
      ),
    );
  }
}

class _SettingsSectionSurface extends StatelessWidget {
  const _SettingsSectionSurface({
    required this.section,
    required this.workspace,
  });

  final SettingsSection section;
  final OfficeWorkspace workspace;

  @override
  Widget build(BuildContext context) {
    final motionDisabled =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    final surface = switch (section) {
      SettingsSection.models => OpenRouterSurface(
        gateway: context.read<FrankGateway>(),
      ),
      SettingsSection.organization => _GatewayOrganizationSurface(
        workspace: workspace,
        data: context.read<OfficeSessionCache>(),
      ),
      SettingsSection.team => _GatewayTeamSurface(
        workspace: workspace,
        data: context.read<OfficeSessionCache>(),
      ),
      SettingsSection.ledger => _GatewayLedgerSurface(
        workspace: workspace,
        data: context.read<OfficeSessionCache>(),
      ),
      SettingsSection.taskboard => _GatewayTaskboardSurface(
        workspace: workspace,
        data: context.read<OfficeSessionCache>(),
      ),
      SettingsSection.journal => const JournalSurface(),
    };

    return Semantics(
      container: true,
      label: '${section.label} settings section',
      child: KeyedSubtree(
        key: ValueKey('settings-section-surface-${section.name}'),
        child: ColoredBox(
          key: const ValueKey('settings-section-content-host'),
          color: Colors.transparent,
          child: AnimatedSwitcher(
            key: const ValueKey('settings-section-content-switcher'),
            duration: motionDisabled
                ? Duration.zero
                : const Duration(milliseconds: 180),
            reverseDuration: motionDisabled
                ? Duration.zero
                : const Duration(milliseconds: 180),
            switchInCurve: Curves.easeOutCubic,
            switchOutCurve: Curves.easeInCubic,
            transitionBuilder: (child, animation) => AnimatedBuilder(
              animation: animation,
              child: child,
              builder: (context, child) {
                final exiting =
                    animation.status == AnimationStatus.reverse ||
                    animation.status == AnimationStatus.dismissed;
                return IgnorePointer(
                  ignoring: exiting,
                  child: ExcludeFocus(
                    excluding: exiting,
                    child: FadeTransition(opacity: animation, child: child),
                  ),
                );
              },
            ),
            child: KeyedSubtree(
              key: ValueKey('settings-section-content-${section.name}'),
              child: surface,
            ),
          ),
        ),
      ),
    );
  }
}

class _GatewayTaskboardSurface extends StatefulWidget {
  const _GatewayTaskboardSurface({required this.workspace, required this.data});

  final OfficeWorkspace workspace;
  final OfficeSessionCache data;

  @override
  State<_GatewayTaskboardSurface> createState() =>
      _GatewayTaskboardSurfaceState();
}

class _GatewayTaskboardSurfaceState extends State<_GatewayTaskboardSurface> {
  late Future<List<TeamAgentProfile>> _profiles;

  @override
  void initState() {
    super.initState();
    _profiles = widget.data.loadTeamProfiles();
  }

  @override
  void didUpdateWidget(covariant _GatewayTaskboardSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(widget.data, oldWidget.data) ||
        widget.workspace != oldWidget.workspace) {
      _profiles = widget.data.loadTeamProfiles();
    }
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<List<TeamAgentProfile>>(
    future: _profiles,
    initialData: widget.data.gateway.cachedTeamProfiles,
    builder: (context, snapshot) =>
        TaskboardSurface(workspace: widget.workspace, profiles: snapshot.data),
  );
}

class _GatewayTeamSurface extends StatefulWidget {
  const _GatewayTeamSurface({required this.workspace, required this.data});

  final OfficeWorkspace workspace;
  final OfficeSessionCache data;

  @override
  State<_GatewayTeamSurface> createState() => _GatewayTeamSurfaceState();
}

class _GatewayTeamSurfaceState extends State<_GatewayTeamSurface> {
  late Future<List<TeamAgentProfile>> _profiles;
  late Future<List<TeamRoleSummary>> _roles;
  late Future<OpenRouterCatalog> _catalog;

  @override
  void initState() {
    super.initState();
    _profiles = widget.data.loadTeamProfiles();
    _roles = widget.data.gateway.loadTeamRoles();
    _catalog = widget.data.loadOpenRouterCatalog();
  }

  @override
  void didUpdateWidget(covariant _GatewayTeamSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(widget.data, oldWidget.data) ||
        widget.workspace != oldWidget.workspace) {
      _profiles = widget.data.loadTeamProfiles();
      _roles = widget.data.gateway.loadTeamRoles();
      _catalog = widget.data.loadOpenRouterCatalog();
    }
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<List<TeamAgentProfile>>(
    future: _profiles,
    initialData: widget.data.gateway.cachedTeamProfiles,
    builder: (context, snapshot) {
      if (snapshot.hasError) {
        return _GatewaySurfaceStateView(
          title: 'Team',
          description:
              'The people-shaped part of Frank. Meet the agents moving work forward.',
          message: 'Team profiles unavailable. Try again.',
          error: true,
          onRetry: _retryProfiles,
          fullWidth: true,
        );
      }
      final profiles = snapshot.data;
      if (profiles == null) {
        return _GatewaySurfaceStateView(
          title: 'Team',
          description:
              'The people-shaped part of Frank. Meet the agents moving work forward.',
          message: snapshot.connectionState == ConnectionState.done
              ? 'No team profiles are configured.'
              : 'Loading team profiles…',
          semanticsLabel: 'Team roster',
          fullWidth: true,
        );
      }
      return FutureBuilder<List<TeamRoleSummary>>(
        future: _roles,
        builder: (context, roleSnapshot) {
          if (roleSnapshot.hasError &&
              frankIsUnsupportedError(roleSnapshot.error)) {
            return const _GatewaySurfaceStateView(
              title: 'Team',
              description:
                  'Create role-backed agents and manage their runtime settings.',
              message: 'Team management isn’t available on this server build.',
              semanticsLabel: 'Team unavailable',
              fullWidth: true,
            );
          }
          return FutureBuilder<OpenRouterCatalog>(
            future: _catalog,
            builder: (context, catalogSnapshot) => TeamSurface(
              workspace: widget.workspace,
              profiles: profiles,
              roles: roleSnapshot.data ?? const [],
              isFixture: widget.data.gateway.isFixture,
              catalog: catalogSnapshot.data,
              catalogError: catalogSnapshot.error,
              catalogLoading:
                  catalogSnapshot.connectionState != ConnectionState.done,
              onSaveAgentModel: _saveAgentModel,
              onSaveRoleModel: _saveRoleModel,
              onCreateRole: _createRole,
              onCreateAgent: _createAgent,
              onUpdateRole: _updateRole,
              onArchiveRole: _archiveRole,
              onUpdateAgent: _updateAgent,
              onArchiveAgent: _archiveAgent,
            ),
          );
        },
      );
    },
  );

  void _retryProfiles() {
    widget.data.invalidateTeamProfiles();
    setState(() {
      _profiles = widget.data.loadTeamProfiles(refresh: true);
    });
  }

  void _reloadProfilesAfterRevisionConflict(Object error) {
    if (!error.toString().toLowerCase().contains('revision')) return;
    widget.data.invalidateTeamProfiles();
    final freshProfiles = widget.data.loadTeamProfiles(refresh: true);
    if (mounted) setState(() => _profiles = freshProfiles);
  }

  Future<List<TeamAgentProfile>> _saveAgentModel(
    String agentId,
    String? model,
  ) async {
    try {
      final profiles = await widget.data.gateway.updateAgentModelOverride(
        agentId: agentId,
        model: model,
        expectedRevision: widget.data.gateway.snapshotRevision,
      );
      widget.data.updateTeamProfiles(profiles);
      if (mounted) setState(() => _profiles = Future.value(profiles));
      return profiles;
    } catch (error) {
      // Reload the projection after a stale revision so the editor shows the
      // server value and the user can retry against the new revision.
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<List<TeamAgentProfile>> _saveRoleModel(
    String roleId,
    String? model,
  ) async {
    try {
      final profiles = await widget.data.gateway.updateRoleDefaultModel(
        roleId: roleId,
        model: model,
        expectedRevision: widget.data.gateway.snapshotRevision,
      );
      widget.data.updateTeamProfiles(profiles);
      if (mounted) setState(() => _profiles = Future.value(profiles));
      return profiles;
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<List<TeamAgentProfile>> _createRole(TeamRoleDraft draft) async {
    try {
      final profiles = await widget.data.gateway.createRole(draft);
      widget.data.updateTeamProfiles(profiles);
      if (mounted) {
        setState(() {
          _profiles = Future.value(profiles);
          _roles = widget.data.gateway.loadTeamRoles();
        });
      }
      return profiles;
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<List<TeamAgentProfile>> _createAgent(TeamAgentDraft draft) async {
    try {
      final profiles = await widget.data.gateway.createAgent(draft);
      widget.data.updateTeamProfiles(profiles);
      if (mounted) setState(() => _profiles = Future.value(profiles));
      return profiles;
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<void> _updateRole(String roleId, TeamRolePatch patch) async {
    try {
      final profiles = await widget.data.gateway.updateRolePatch(
        roleId: roleId,
        patch: patch,
        expectedRevision: widget.data.gateway.snapshotRevision,
      );
      widget.data.updateTeamProfiles(profiles);
      if (!mounted) return;
      setState(() {
        _roles = widget.data.gateway.loadTeamRoles();
        _profiles = Future.value(profiles);
      });
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<void> _archiveRole(String roleId) async {
    try {
      final profiles = await widget.data.gateway.archiveRole(
        roleId: roleId,
        expectedRevision: widget.data.gateway.snapshotRevision,
      );
      widget.data.updateTeamProfiles(profiles);
      if (!mounted) return;
      setState(() {
        _roles = widget.data.gateway.loadTeamRoles();
        _profiles = Future.value(profiles);
      });
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<List<TeamAgentProfile>> _updateAgent(
    String agentId,
    TeamAgentPatch patch,
  ) async {
    try {
      final profiles = await widget.data.gateway.updateAgentPatch(
        agentId: agentId,
        patch: patch,
        expectedRevision: widget.data.gateway.snapshotRevision,
      );
      widget.data.updateTeamProfiles(profiles);
      if (mounted) setState(() => _profiles = Future.value(profiles));
      return profiles;
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }

  Future<List<TeamAgentProfile>> _archiveAgent(String agentId) async {
    try {
      final profiles = await widget.data.gateway.archiveAgent(
        agentId: agentId,
        expectedRevision: widget.data.gateway.snapshotRevision,
      );
      widget.data.updateTeamProfiles(profiles);
      if (mounted) setState(() => _profiles = Future.value(profiles));
      return profiles;
    } catch (error) {
      _reloadProfilesAfterRevisionConflict(error);
      rethrow;
    }
  }
}

class _GatewayLedgerSurface extends StatefulWidget {
  const _GatewayLedgerSurface({required this.workspace, required this.data});

  final OfficeWorkspace workspace;
  final OfficeSessionCache data;

  @override
  State<_GatewayLedgerSurface> createState() => _GatewayLedgerSurfaceState();
}

class _GatewayLedgerSurfaceState extends State<_GatewayLedgerSurface> {
  late Future<LedgerDashboardData> _dashboard;

  @override
  void initState() {
    super.initState();
    _dashboard = widget.data.loadLedgerDashboard();
  }

  @override
  void didUpdateWidget(covariant _GatewayLedgerSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(widget.data, oldWidget.data) ||
        widget.workspace != oldWidget.workspace) {
      _dashboard = widget.data.loadLedgerDashboard();
    }
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<LedgerDashboardData>(
    future: _dashboard,
    initialData: widget.data.gateway.cachedLedgerDashboard,
    builder: (context, snapshot) {
      if (snapshot.hasError) {
        return _GatewaySurfaceStateView(
          title: 'Ledger',
          description:
              'What was measured, what was estimated, and what Frank refuses to guess.',
          message: 'Ledger unavailable. Try again from the shell.',
          error: true,
          fullWidth: true,
        );
      }
      final dashboard = snapshot.data;
      if (dashboard == null) {
        return _GatewaySurfaceStateView(
          title: 'Ledger',
          description:
              'What was measured, what was estimated, and what Frank refuses to guess.',
          message: snapshot.connectionState == ConnectionState.done
              ? 'No ledger data is available.'
              : 'Loading ledger…',
          fullWidth: true,
        );
      }
      return LedgerSurface(workspace: widget.workspace, data: dashboard);
    },
  );
}

class _GatewayOrganizationSurface extends StatefulWidget {
  const _GatewayOrganizationSurface({
    required this.workspace,
    required this.data,
  });

  final OfficeWorkspace workspace;
  final OfficeSessionCache data;

  @override
  State<_GatewayOrganizationSurface> createState() =>
      _GatewayOrganizationSurfaceState();
}

class _GatewayOrganizationSurfaceState
    extends State<_GatewayOrganizationSurface> {
  late Future<List<TeamAgentProfile>> _profiles;
  late Future<List<TeamRoleSummary>> _roles;
  late Future<WorkflowProjection> _workflowProjection;

  @override
  void initState() {
    super.initState();
    _profiles = widget.data.loadTeamProfiles();
    _roles = _loadRoles();
    _workflowProjection = _loadWorkflowProjection();
  }

  @override
  void didUpdateWidget(covariant _GatewayOrganizationSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(widget.data, oldWidget.data) ||
        widget.workspace != oldWidget.workspace) {
      _profiles = widget.data.loadTeamProfiles();
      _roles = _loadRoles();
      _workflowProjection = _loadWorkflowProjection();
    }
  }

  Future<List<TeamRoleSummary>> _loadRoles() async {
    try {
      return await widget.data.gateway.loadTeamRoles();
    } on Object {
      // Role metadata is additive to the Organization v1 surface. An older
      // daemon can still render and edit legacy staff nodes.
      return const <TeamRoleSummary>[];
    }
  }

  Future<WorkflowProjection> _loadWorkflowProjection() async {
    try {
      return await widget.data.gateway.loadWorkflowProjection();
    } on Object {
      // Keep Organization usable when the v2 feature is not negotiated yet.
      return const WorkflowProjection();
    }
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<List<TeamAgentProfile>>(
    future: _profiles,
    initialData: widget.data.gateway.cachedTeamProfiles,
    builder: (context, snapshot) {
      if (snapshot.hasError && frankIsUnsupportedError(snapshot.error)) {
        return const _GatewaySurfaceStateView(
          title: 'Organization',
          description:
              'Configure agents, connections, and taskboard assignments.',
          message: 'Organization editor isn’t available on this server build.',
          semanticsLabel: 'Organization unavailable',
          fullWidth: true,
        );
      }
      return FutureBuilder<List<TeamRoleSummary>>(
        future: _roles,
        initialData: const <TeamRoleSummary>[],
        builder: (context, rolesSnapshot) => FutureBuilder<WorkflowProjection>(
          future: _workflowProjection,
          initialData: const WorkflowProjection(),
          builder: (context, workflowSnapshot) => OrganizationSurface(
            workspace: widget.workspace,
            profiles: snapshot.data,
            roles: rolesSnapshot.data ?? const <TeamRoleSummary>[],
            workflowProjection:
                workflowSnapshot.data ?? const WorkflowProjection(),
          ),
        ),
      );
    },
  );
}

class _GatewaySurfaceStateView extends StatelessWidget {
  const _GatewaySurfaceStateView({
    required this.title,
    required this.description,
    required this.message,
    this.error = false,
    this.onRetry,
    this.semanticsLabel,
    this.fullWidth = false,
  });

  final String title;
  final String description;
  final String message;
  final bool error;
  final VoidCallback? onRetry;
  final String? semanticsLabel;
  final bool fullWidth;

  @override
  Widget build(BuildContext context) {
    final friendlyMessage = frankFriendlyError(message, fallback: message);
    return Semantics(
      container: true,
      label: semanticsLabel ?? title,
      child: OfficeSurfaceFrame.page(
        fullWidth: fullWidth,
        header: OfficePageHeader(title: title, description: description),
        slivers: [
          SliverToBoxAdapter(
            child: FrankUnavailableState(
              title: error ? '$title unavailable' : title,
              message: friendlyMessage,
              icon: error ? Icons.error_outline : Icons.hourglass_empty,
              onRetry: onRetry,
            ),
          ),
        ],
      ),
    );
  }
}
