part of 'team_surface.dart';

enum _ModelSourceChoice { role, agent }

class _SetupPanel extends StatefulWidget {
  const _SetupPanel({
    required this.profile,
    required this.catalog,
    required this.catalogError,
    required this.catalogLoading,
    required this.onSaveAgentModel,
    required this.onSaveRoleModel,
  });

  final TeamAgentProfile profile;
  final OpenRouterCatalog? catalog;
  final Object? catalogError;
  final bool catalogLoading;
  final TeamModelChange? onSaveAgentModel;
  final TeamRoleModelChange? onSaveRoleModel;

  @override
  State<_SetupPanel> createState() => _SetupPanelState();
}

class _SetupPanelState extends State<_SetupPanel> {
  late _ModelSourceChoice _source;
  String? _agentModel;
  String? _roleModel;
  bool _savingAgent = false;
  bool _savingRole = false;
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
        (!_savingAgent && !_savingRole)) {
      _syncFromProfile();
    }
  }

  void _syncFromProfile() {
    _source = profile.modelOverride == null
        ? _ModelSourceChoice.role
        : _ModelSourceChoice.agent;
    _agentModel = profile.modelOverride ?? profile.model;
    _roleModel =
        profile.roleDefaultModel ??
        (profile.modelSource == 'role' ? profile.model : null);
    _error = null;
  }

  @override
  Widget build(BuildContext context) {
    final editable = widget.onSaveAgentModel != null;
    final roleEditable =
        widget.onSaveRoleModel != null && profile.roleId != null;
    return _PanelCard(
      key: const ValueKey('team-profile-setup'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const _PanelHeading(
            eyebrow: 'RUNTIME SETUP',
            title: 'Configured for the work at hand.',
            detail:
                'OpenRouter is the only runtime. Effective changes are applied at an idle boundary.',
          ),
          const SizedBox(height: 24),
          _SetupRow(
            icon: Icons.cloud_outlined,
            label: 'Runtime',
            value: 'OpenRouter',
          ),
          _SetupRow(
            icon: Icons.memory_outlined,
            label: 'Effective model',
            value: profile.model.isEmpty ? 'Unconfigured' : profile.model,
          ),
          _SetupRow(
            icon: Icons.tune_outlined,
            label: 'Model source',
            value: profile.modelSource == 'agent'
                ? 'Agent override'
                : 'Role default',
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
            const LinearProgressIndicator()
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
                ChoiceChip(
                  key: const ValueKey('team-use-role-default'),
                  label: const Text('Use role default'),
                  selected: _source == _ModelSourceChoice.role,
                  onSelected: editable
                      ? (_) => setState(() => _source = _ModelSourceChoice.role)
                      : null,
                ),
                ChoiceChip(
                  key: const ValueKey('team-use-agent-override'),
                  label: const Text('Override for this agent'),
                  selected: _source == _ModelSourceChoice.agent,
                  onSelected: editable
                      ? (_) =>
                            setState(() => _source = _ModelSourceChoice.agent)
                      : null,
                ),
              ],
            ),
            const SizedBox(height: 12),
            if (_source == _ModelSourceChoice.agent)
              _SearchableModelMenu(
                key: const ValueKey('team-agent-model-picker'),
                label: 'Agent override model',
                value: _agentModel,
                models: widget.catalog!.models,
                enabled: editable && !_savingAgent,
                onChanged: (value) => setState(() => _agentModel = value),
              )
            else
              _SetupRow(
                icon: Icons.account_tree_outlined,
                label: 'Role default in use',
                value: _roleModel ?? 'Unconfigured',
              ),
            const SizedBox(height: 10),
            if (editable)
              FilledButton.icon(
                key: const ValueKey('team-save-agent-model'),
                onPressed: _savingAgent ? null : _saveAgentModel,
                icon: const Icon(Icons.save, size: FrankUiTokens.iconSize),
                label: Text(
                  _source == _ModelSourceChoice.role
                      ? 'Use role default'
                      : 'Save agent override',
                ),
              ),
            if (roleEditable) ...[
              const SizedBox(height: 20),
              const Text(
                'Role default model',
                style: TextStyle(
                  color: FrankColors.ink,
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 4),
              const Text(
                'Changing this affects every member without an agent override.',
                style: TextStyle(color: FrankColors.warningAmber, fontSize: 11),
              ),
              const SizedBox(height: 10),
              _SearchableModelMenu(
                key: const ValueKey('team-role-model-picker'),
                label: 'Role default canonical model slug',
                value: _roleModel,
                models: widget.catalog!.models,
                enabled: !_savingRole,
                onChanged: (value) => setState(() => _roleModel = value),
              ),
              const SizedBox(height: 10),
              OutlinedButton(
                key: const ValueKey('team-save-role-model'),
                onPressed: _savingRole ? null : _saveRoleModel,
                child: const Text('Save role default'),
              ),
            ],
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
            icon: Icons.auto_awesome_outlined,
            label: 'Prompt pack',
            value: profile.promptPack,
          ),
          _SetupRow(
            icon: Icons.tune_outlined,
            label: 'Level',
            value: profile.level,
          ),
          _SetupRow(
            icon: Icons.history_toggle_off_outlined,
            label: 'Role revision',
            value: profile.roleRevision == 0
                ? 'Legacy member'
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

  Future<void> _saveRoleModel() async {
    final callback = widget.onSaveRoleModel;
    final roleId = profile.roleId;
    if (callback == null || roleId == null) return;
    if (_roleModel == null || _roleModel!.trim().isEmpty) {
      setState(() => _error = 'Choose a role default model before saving.');
      return;
    }
    setState(() {
      _savingRole = true;
      _error = null;
    });
    try {
      await callback(roleId, _roleModel);
    } catch (error) {
      if (mounted) setState(() => _error = _teamError(error));
    } finally {
      if (mounted) setState(() => _savingRole = false);
    }
  }
}

class _SearchableModelMenu extends StatelessWidget {
  const _SearchableModelMenu({
    required this.label,
    required this.value,
    required this.models,
    required this.enabled,
    required this.onChanged,
    super.key,
  });

  final String label;
  final String? value;
  final List<OpenRouterModel> models;
  final bool enabled;
  final ValueChanged<String?> onChanged;

  @override
  Widget build(BuildContext context) {
    final slugs = {for (final model in models) model.canonicalSlug};
    return FrankDesktopSelectField<String?>(
      key: ValueKey('$label-menu-${value ?? ''}'),
      fieldKey: ValueKey('$label-menu-trigger-${value ?? ''}'),
      value: slugs.contains(value) ? value : null,
      label: label,
      hint: 'Choose a model',
      enabled: enabled,
      searchable: true,
      searchHint: 'Search models',
      options: [
        for (final model in models)
          FrankDesktopSelectOption<String?>(
            value: model.canonicalSlug,
            label: '${model.name} · ${model.canonicalSlug}',
          ),
      ],
      onChanged: onChanged,
    );
  }
}

class _PendingIdleBadge extends StatelessWidget {
  const _PendingIdleBadge();

  @override
  Widget build(BuildContext context) => const Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Icon(
        Icons.schedule,
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
        error ? Icons.error_outline : Icons.list_alt_outlined,
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
