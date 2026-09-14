part of 'team_surface.dart';

enum _ModelSourceChoice { role, agent }

class _SetupPanel extends StatefulWidget {
  const _SetupPanel({
    required this.profile,
    required this.catalog,
    required this.catalogError,
    required this.catalogLoading,
    this.providerConfigured,
    this.providerError,
    required this.onSaveAgentModel,
    required this.onSaveRoleModel,
    this.onEditRole,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final TeamAgentProfile profile;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final bool? providerConfigured;
  final Object? providerError;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;
  final VoidCallback? onEditRole;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  State<_SetupPanel> createState() => _SetupPanelState();
}

class _SetupPanelState extends State<_SetupPanel> {
  late _ModelSourceChoice _source;
  String? _agentModel;
  String? _roleModel;
  bool _savingAgent = false;
  String? _error;

  TeamAgentProfile get profile => widget.profile;

  @override
  void initState() {
    super.initState();
    _syncFromProfile();
  }

  @override
  void didUpdateWidget(covariant _SetupPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.profile.employeeId != widget.profile.employeeId ||
        !_savingAgent) {
      _syncFromProfile();
    }
  }

  void _syncFromProfile() {
    _source = profile.modelOverride == null
        ? _ModelSourceChoice.role
        : _ModelSourceChoice.agent;
    _agentModel = profile.modelOverride;
    _roleModel =
        profile.roleDefaultModel ??
        (profile.modelSource == 'role' ? profile.model : null);
    _error = null;
  }

  @override
  Widget build(BuildContext context) {
    final editable = widget.canMutate && widget.onSaveAgentModel != null;
    final modelUnavailable =
        profile.model.trim().isEmpty ||
        profile.model.trim().toLowerCase() == 'unconfigured';
    // A stale or unavailable catalog must not erase a valid model already
    // persisted on the agent/role. Configuration is incomplete only when the
    // effective worker model is actually empty.
    final configurationIncomplete = modelUnavailable;
    final sourceLabel = switch (profile.modelSource.trim().toLowerCase()) {
      'agent' || 'member' => 'Agent override',
      'role' => 'Role default',
      'system' => 'System default',
      'unavailable' || '' => 'Unavailable',
      _ => 'Not reported',
    };
    final effectiveModel = modelUnavailable ? 'Not reported' : profile.model;
    final providerLabel = widget.providerError != null
        ? 'Error'
        : widget.providerConfigured == false
        ? 'Not configured'
        : widget.catalogError != null
        ? 'Error'
        : widget.catalogLoading
        ? 'Checking'
        : 'Connected';
    final catalogLabel = widget.catalogError != null
        ? 'Error'
        : widget.catalogLoading
        ? 'Loading'
        : widget.catalog?.stale == true
        ? 'Stale'
        : 'Ready';
    final promptPack = profile.promptPack.trim().isEmpty
        ? 'Not reported'
        : profile.promptPack;
    final level = profile.level.trim().isEmpty ? 'Not reported' : profile.level;
    return _PanelCard(
      key: const ValueKey('team-profile-setup'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _PanelHeading(
            eyebrow: 'RUNTIME SETUP',
            title: configurationIncomplete
                ? 'Configuration incomplete.'
                : 'Runtime configuration.',
            detail: configurationIncomplete
                ? 'The daemon has not reported a complete model/catalog configuration yet.'
                : 'Effective changes are applied at an idle boundary.',
          ),
          const SizedBox(height: 24),
          _SetupRow(
            icon: FrankIcons.cloudOutlined,
            label: 'Runtime',
            value: 'OpenRouter',
          ),
          _SetupRow(
            icon: FrankIcons.memoryOutlined,
            label: 'Effective model',
            value: effectiveModel,
          ),
          _SetupRow(
            icon: FrankIcons.tuneOutlined,
            label: 'Model source',
            value: sourceLabel,
          ),
          _SetupRow(
            icon: FrankIcons.personOutline,
            label: 'Agent override',
            value: profile.modelOverride ?? 'None',
          ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: _SetupRow(
                  icon: FrankIcons.accountTreeOutlined,
                  label: 'Role default',
                  value: profile.roleDefaultModel ?? 'Not set',
                ),
              ),
              if (widget.onEditRole != null)
                FButton(
                  key: const ValueKey('team-edit-role-from-setup'),
                  onPress: widget.canMutate ? widget.onEditRole : null,
                  semanticsTooltip: widget.canMutate
                      ? 'Edit role'
                      : widget.mutationDisabledReason ??
                            'Reconnect before editing the role',
                  variant: FButtonVariant.outline,
                  size: FButtonSizeVariant.sm,
                  child: const Text('Edit role'),
                ),
            ],
          ),
          _SetupRow(
            icon: FrankIcons.cloudOutlined,
            label: 'Provider',
            value: providerLabel,
          ),
          _SetupRow(
            icon: FrankIcons.listAltOutlined,
            label: 'Catalog',
            value: widget.catalog?.refreshedAt == null
                ? catalogLabel
                : '$catalogLabel · ${_formatCatalogDate(widget.catalog!.refreshedAt!)}',
          ),
          if (profile.pendingModelChange) ...[
            const SizedBox(height: 10),
            const _PendingIdleBadge(),
          ],
          const SizedBox(height: 18),
          if (widget.catalogError != null)
            const _TeamCatalogState(
              message:
                  'Model catalog unavailable. No model fallback is applied.',
              error: true,
            )
          else if (widget.catalogLoading)
            const FProgress()
          else if (widget.catalog?.models.isEmpty ?? true)
            const _TeamCatalogState(
              message: 'No tool-capable OpenRouter models are available.',
            )
          else ...[
            const Text(
              'Model source',
              style: TextStyle(
                color: FrankColors.ink,
                fontSize: 12,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                FButton(
                  key: const ValueKey('team-use-role-default'),
                  child: const Text('Use role default'),
                  variant: _source == _ModelSourceChoice.role
                      ? FButtonVariant.secondary
                      : FButtonVariant.ghost,
                  onPress: editable
                      ? () => setState(() => _source = _ModelSourceChoice.role)
                      : null,
                ),
                FButton(
                  key: const ValueKey('team-use-agent-override'),
                  child: const Text('Override for this agent'),
                  variant: _source == _ModelSourceChoice.agent
                      ? FButtonVariant.secondary
                      : FButtonVariant.ghost,
                  onPress: editable
                      ? () => setState(() => _source = _ModelSourceChoice.agent)
                      : null,
                ),
              ],
            ),
            const SizedBox(height: 12),
            if (_source == _ModelSourceChoice.agent)
              FrankOpenRouterModelPicker(
                key: const ValueKey('team-agent-model-picker'),
                label: 'Agent override model',
                value: _agentModel,
                models: widget.catalog!.models,
                enabled: editable && !_savingAgent,
                onChanged: (value) => setState(() => _agentModel = value),
              )
            else
              _SetupRow(
                icon: FrankIcons.accountTreeOutlined,
                label: 'Role default in use',
                value: _roleModel ?? 'Not reported',
              ),
            if (_source == _ModelSourceChoice.agent &&
                _agentModel != null &&
                !widget.catalog!.models.any(
                  (model) => model.canonicalSlug == _agentModel,
                ))
              const _TeamCatalogState(
                message: 'Saved agent override is missing from this catalog.',
              ),
            if (_source == _ModelSourceChoice.role &&
                _roleModel != null &&
                !widget.catalog!.models.any(
                  (model) => model.canonicalSlug == _roleModel,
                ))
              const _TeamCatalogState(
                message: 'Saved role default is missing from this catalog.',
              ),
            const SizedBox(height: 10),
            if (editable)
              FButton(
                key: const ValueKey('team-save-agent-model'),
                onPress: _savingAgent ? null : _saveAgentModel,
                prefix: const Icon(
                  FrankIcons.save,
                  size: FrankUiTokens.iconSize,
                ),
                child: Text(
                  _source == _ModelSourceChoice.role
                      ? 'Use role default'
                      : 'Save agent override',
                ),
              ),
          ],
          if (_error case final error?) ...[
            const SizedBox(height: 10),
            Text(
              error,
              key: const ValueKey('team-model-error'),
              style: const TextStyle(color: FrankColors.failure),
            ),
          ],
          const SizedBox(height: 16),
          _SetupRow(
            icon: FrankIcons.autoAwesomeOutlined,
            label: 'Prompt pack',
            value: promptPack,
          ),
          _SetupRow(
            icon: FrankIcons.tuneOutlined,
            label: 'Level',
            value: level,
          ),
          _SetupRow(
            icon: FrankIcons.historyToggleOffOutlined,
            label: 'Role revision',
            value: profile.roleRevision == 0
                ? 'Not reported'
                : 'Revision ${profile.roleRevision}',
          ),
        ],
      ),
    );
  }

  Future<void> _saveAgentModel() async {
    final callback = widget.onSaveAgentModel;
    if (callback == null) return;
    final model = _source == _ModelSourceChoice.role ? null : _agentModel;
    if (_source == _ModelSourceChoice.agent &&
        (model == null || model.trim().isEmpty)) {
      setState(
        () => _error = 'Choose an agent model before saving the override.',
      );
      return;
    }
    setState(() {
      _savingAgent = true;
      _error = null;
    });
    try {
      await callback(profile.employeeId, model);
    } catch (error) {
      if (mounted) setState(() => _error = _teamError(error));
    } finally {
      if (mounted) setState(() => _savingAgent = false);
    }
  }
}

String _formatCatalogDate(DateTime value) {
  final local = value.toLocal();
  String two(int number) => number.toString().padLeft(2, '0');
  return '${local.year}-${two(local.month)}-${two(local.day)} ${two(local.hour)}:${two(local.minute)}';
}

class _PendingIdleBadge extends StatelessWidget {
  const _PendingIdleBadge();

  @override
  Widget build(BuildContext context) => const Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Icon(
        FrankIcons.schedule,
        size: FrankUiTokens.iconSize,
        color: FrankColors.warningAmber,
      ),
      SizedBox(width: 7),
      Text(
        'Applies when idle',
        style: TextStyle(color: FrankColors.warningAmber, fontSize: 12),
      ),
    ],
  );
}

class _TeamCatalogState extends StatelessWidget {
  const _TeamCatalogState({required this.message, this.error = false});

  final String message;
  final bool error;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Icon(
        error ? FrankIcons.errorOutline : FrankIcons.listAltOutlined,
        size: FrankUiTokens.iconSize,
        color: error ? FrankColors.failure : FrankColors.warningAmber,
      ),
      const SizedBox(width: 8),
      Expanded(
        child: Text(message, style: const TextStyle(color: FrankColors.muted)),
      ),
    ],
  );
}

String _teamError(Object error) =>
    frankFriendlyError(error, fallback: 'The team update could not be saved.');

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
