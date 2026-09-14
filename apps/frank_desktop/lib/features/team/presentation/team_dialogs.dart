part of 'team_surface.dart';

Widget _teamDialog({
  required Widget title,
  required Widget content,
  required List<Widget> actions,
  Widget? feedback,
}) => Padding(
  // showFrankDialog supplies the single route-level FDialog. Keeping this
  // helper as content-only avoids the nested FDialog that made member/role
  // editors render as a large empty panel around a second card.
  padding: const EdgeInsets.all(20),
  child: Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      DefaultTextStyle(
        style: const TextStyle(
          color: FrankColors.ink,
          fontSize: 18,
          fontWeight: FontWeight.w600,
        ),
        child: title,
      ),
      const SizedBox(height: 16),
      Flexible(child: content),
      if (feedback != null) ...[const SizedBox(height: 12), feedback],
      const SizedBox(height: 16),
      Row(mainAxisAlignment: MainAxisAlignment.end, children: actions),
    ],
  ),
);

Widget _teamTextField(
  TextEditingController controller, {
  required String label,
  String? hint,
  int? maxLines = 1,
  TextInputType? keyboardType,
  bool autofocus = false,
}) => FTextField(
  control: FTextFieldControl.managed(controller: controller),
  label: Text(label),
  hint: hint,
  maxLines: maxLines,
  keyboardType: keyboardType,
  autofocus: autofocus,
);

Future<bool> _confirmTeamDiscard(BuildContext context) async {
  final discard = await showFrankDialog<bool>(
    context: context,
    builder: (dialogContext) => Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text(
          'Discard unsaved changes?',
          style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 10),
        const Text('Your local edits will be lost.'),
        const SizedBox(height: 20),
        Row(
          mainAxisAlignment: MainAxisAlignment.end,
          children: [
            FButton(
              onPress: () => Navigator.pop(dialogContext, false),
              variant: FButtonVariant.ghost,
              child: const Text('Keep editing'),
            ),
            const SizedBox(width: 8),
            FButton(
              onPress: () => Navigator.pop(dialogContext, true),
              variant: FButtonVariant.destructive,
              child: const Text('Discard changes'),
            ),
          ],
        ),
      ],
    ),
  );
  return discard ?? false;
}

Widget _teamDirtyGuard({
  required BuildContext context,
  required Widget child,
  required bool dirty,
  required bool busy,
  required Future<bool> Function() confirmDiscard,
}) => PopScope<void>(
  canPop: !dirty && !busy,
  onPopInvokedWithResult: (didPop, _) {
    if (didPop || busy || !dirty) return;
    confirmDiscard().then((discard) {
      if (discard && context.mounted) Navigator.pop(context);
    });
  },
  child: child,
);

Widget _teamSelect({
  required String label,
  required String? value,
  required List<(String, String)> options,
  required ValueChanged<String?> onChanged,
}) => FSelect<String>.rich(
  format: (value) => options
      .firstWhere((option) => option.$1 == value, orElse: () => (value, value))
      .$2,
  control: FSelectControl<String>.lifted(value: value, onChange: onChanged),
  label: Text(label),
  hint: 'Select $label',
  children: [
    for (final option in options)
      FSelectItem<String>.item(value: option.$1, title: Text(option.$2)),
  ],
);

Widget? _teamFeedback(BuildContext context, String? error) {
  if (error == null) return null;
  final normalized = error.toLowerCase();
  final isRevisionConflict =
      normalized.contains('revision') ||
      normalized.contains('stale') ||
      normalized.contains('conflict');
  return FrankActionFeedback(
    message: error,
    tone: FrankStatusTone.failure,
    action: isRevisionConflict
        ? FButton(
            onPress: () => Navigator.of(context).pop(),
            variant: FButtonVariant.ghost,
            child: const Text('Reload latest'),
          )
        : null,
  );
}

class _AgentEditDialog extends StatefulWidget {
  const _AgentEditDialog({
    required this.profile,
    required this.roles,
    required this.onSave,
  });

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;
  final Future<void> Function(TeamAgentPatch patch) onSave;

  @override
  State<_AgentEditDialog> createState() => _AgentEditDialogState();
}

class _AgentEditDialogState extends State<_AgentEditDialog> {
  late final TextEditingController _name;
  late final TextEditingController _modelOverride;
  late final int _initialFingerprint;
  String? _roleId;
  bool _saving = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(text: widget.profile.name);
    _modelOverride = TextEditingController(
      text: widget.profile.modelOverride ?? '',
    );
    // A missing role is meaningful remote state. Do not silently assign the
    // first catalog role (usually Generalist) while opening an editor.
    _roleId = widget.profile.roleId;
    _name.addListener(_refreshDirty);
    _modelOverride.addListener(_refreshDirty);
    _initialFingerprint = _fingerprint;
  }

  @override
  void dispose() {
    _name.removeListener(_refreshDirty);
    _modelOverride.removeListener(_refreshDirty);
    _name.dispose();
    _modelOverride.dispose();
    super.dispose();
  }

  int get _fingerprint =>
      Object.hash(_name.text.trim(), _modelOverride.text.trim(), _roleId);

  bool get _dirty => _fingerprint != _initialFingerprint;

  void _refreshDirty() {
    if (mounted) setState(() {});
  }

  Future<void> _dismiss() async {
    if (_saving) return;
    if (!_dirty) {
      if (mounted) Navigator.pop(context);
      return;
    }
    if (await _confirmTeamDiscard(context) && mounted) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => _teamDirtyGuard(
    context: context,
    dirty: _dirty,
    busy: _saving,
    confirmDiscard: () => _confirmTeamDiscard(context),
    child: _teamDialog(
      title: Text('Edit ${widget.profile.name}'),
      content: SizedBox(
        width: 440,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              _teamTextField(_name, label: 'Display name'),
              const SizedBox(height: 10),
              _teamSelect(
                label: 'Role',
                value: _roleId,
                options: [
                  for (final role in widget.roles.where(
                    (role) => !role.archived,
                  ))
                    (role.id, role.name),
                ],
                onChanged: (value) {
                  setState(() => _roleId = value);
                },
              ),
              const SizedBox(height: 10),
              _teamTextField(
                _modelOverride,
                label: 'Agent model override',
                hint: 'Leave empty to inherit role',
              ),
            ],
          ),
        ),
      ),
      feedback: _teamFeedback(context, _error),
      actions: [
        FButton(
          onPress: _saving ? null : _dismiss,
          variant: FButtonVariant.ghost,
          child: const Text('Cancel'),
        ),
        FButton(
          onPress: _saving ? null : _submit,
          child: _saving
              ? const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [FProgress(), SizedBox(width: 8), Text('Saving…')],
                )
              : const Text('Save member'),
        ),
      ],
    ),
  );

  Future<void> _submit() async {
    final name = _name.text.trim();
    if (name.isEmpty || _roleId == null) {
      setState(() => _error = 'Display name and role are required.');
      return;
    }
    final override = _modelOverride.text.trim();
    final patch = TeamAgentPatch(
      displayName: TeamPatchField<String>.set(name),
      roleId: TeamPatchField<String>.set(_roleId!),
      modelOverride: override.isEmpty
          ? const TeamPatchField<String>.clear()
          : TeamPatchField<String>.set(override),
    );
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.onSave(patch);
      if (mounted) Navigator.pop(context);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _error =
            'Could not save member: ${frankFriendlyError(error, fallback: 'Try again.')}';
      });
    }
  }
}

class _RoleDraftDialog extends StatefulWidget {
  const _RoleDraftDialog({required this.onSave, this.models = const []});

  final Future<void> Function(TeamRoleDraft draft) onSave;
  final List<OpenRouterModel> models;

  @override
  State<_RoleDraftDialog> createState() => _RoleDraftDialogState();
}

class _RoleDraftDialogState extends State<_RoleDraftDialog> {
  final _name = TextEditingController();
  final _description = TextEditingController();
  final _model = TextEditingController();
  final _packId = TextEditingController(text: 'caveman');
  final _packLevel = TextEditingController(text: 'full');
  final _instructions = TextEditingController();
  final _timeSeconds = TextEditingController();
  final _turns = TextEditingController();
  final _measuredTokens = TextEditingController();
  final _costMicros = TextEditingController();
  final _avatarPalette = TextEditingController(text: 'default');
  final _avatarSeed = TextEditingController(text: '1');
  String _template = 'generalist';
  String _filesystem = 'workspace-write';
  String _shell = 'ask';
  String _network = 'ask';
  String _approval = 'ask';
  bool _saving = false;
  String? _error;

  List<TextEditingController> get _controllers => [
    _name,
    _description,
    _model,
    _packId,
    _packLevel,
    _instructions,
    _timeSeconds,
    _turns,
    _measuredTokens,
    _costMicros,
    _avatarPalette,
    _avatarSeed,
  ];

  bool get _dirty =>
      _name.text.trim().isNotEmpty ||
      _description.text.trim().isNotEmpty ||
      _model.text.trim().isNotEmpty ||
      _packId.text.trim() != 'caveman' ||
      _packLevel.text.trim() != 'full' ||
      _instructions.text.isNotEmpty ||
      _timeSeconds.text.trim().isNotEmpty ||
      _turns.text.trim().isNotEmpty ||
      _measuredTokens.text.trim().isNotEmpty ||
      _costMicros.text.trim().isNotEmpty ||
      _avatarPalette.text.trim() != 'default' ||
      (int.tryParse(_avatarSeed.text.trim()) ?? 1) != 1 ||
      _template != 'generalist' ||
      _filesystem != 'workspace-write' ||
      _shell != 'ask' ||
      _network != 'ask' ||
      _approval != 'ask';

  @override
  void dispose() {
    for (final controller in _controllers) {
      controller.removeListener(_refreshDirty);
    }
    _name.dispose();
    _description.dispose();
    _model.dispose();
    _packId.dispose();
    _packLevel.dispose();
    _instructions.dispose();
    _timeSeconds.dispose();
    _turns.dispose();
    _measuredTokens.dispose();
    _costMicros.dispose();
    _avatarPalette.dispose();
    _avatarSeed.dispose();
    super.dispose();
  }

  void _refreshDirty() {
    if (mounted) setState(() {});
  }

  Future<void> _dismiss() async {
    if (_saving) return;
    if (!_dirty) {
      if (mounted) Navigator.pop(context);
      return;
    }
    if (await _confirmTeamDiscard(context) && mounted) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => _teamDirtyGuard(
    context: context,
    dirty: _dirty,
    busy: _saving,
    confirmDiscard: () => _confirmTeamDiscard(context),
    child: _teamDialog(
      title: const Text('Create role'),
      content: SizedBox(
        width: 500,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              _teamTextField(_name, label: 'Role name', autofocus: true),
              const SizedBox(height: 12),
              _teamTextField(_description, label: 'Description', maxLines: 2),
              const SizedBox(height: 12),
              _teamSelect(
                label: 'Template',
                value: _template,
                options: const [
                  ('generalist', 'Generalist'),
                  ('researcher', 'Researcher'),
                  ('builder', 'Builder'),
                  ('reviewer', 'Reviewer'),
                ],
                onChanged: (value) {
                  if (value != null) setState(() => _template = value);
                },
              ),
              const SizedBox(height: 12),
              if (widget.models.isEmpty)
                _teamTextField(_model, label: 'OpenRouter model')
              else
                FrankOpenRouterModelPicker(
                  label: 'Role default model',
                  value: _model.text.trim().isEmpty ? null : _model.text.trim(),
                  models: widget.models,
                  enabled: !_saving,
                  onChanged: (value) => _model.text = value ?? '',
                ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: _teamTextField(_packId, label: 'Prompt pack'),
                  ),
                  const SizedBox(width: 10),
                  Expanded(child: _teamTextField(_packLevel, label: 'Level')),
                ],
              ),
              const SizedBox(height: 12),
              _teamTextField(_instructions, label: 'Instructions', maxLines: 3),
              const SizedBox(height: 12),
              _RolePolicyFields(
                filesystem: _filesystem,
                shell: _shell,
                network: _network,
                approval: _approval,
                onFilesystemChanged: (value) =>
                    setState(() => _filesystem = value),
                onShellChanged: (value) => setState(() => _shell = value),
                onNetworkChanged: (value) => setState(() => _network = value),
                onApprovalChanged: (value) => setState(() => _approval = value),
              ),
              const SizedBox(height: 12),
              _RoleBudgetFields(
                timeSeconds: _timeSeconds,
                turns: _turns,
                measuredTokens: _measuredTokens,
                costMicros: _costMicros,
              ),
              const SizedBox(height: 12),
              _RoleAvatarFields(palette: _avatarPalette, seed: _avatarSeed),
            ],
          ),
        ),
      ),
      feedback: _teamFeedback(context, _error),
      actions: [
        FButton(
          onPress: _saving ? null : _dismiss,
          variant: FButtonVariant.ghost,
          child: const Text('Cancel'),
        ),
        FButton(
          onPress: _saving ? null : _submit,
          child: _saving
              ? const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [FProgress(), SizedBox(width: 8), Text('Saving…')],
                )
              : const Text('Create role'),
        ),
      ],
    ),
  );

  @override
  void initState() {
    super.initState();
    for (final controller in _controllers) {
      controller.addListener(_refreshDirty);
    }
  }

  Future<void> _submit() async {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Role name is required.');
      return;
    }
    final draft = TeamRoleDraft(
      name: name,
      description: _description.text.trim(),
      template: _template,
      defaultModel: _model.text.trim().isEmpty ? null : _model.text.trim(),
      packId: _packId.text.trim().isEmpty ? null : _packId.text.trim(),
      packLevel: _packLevel.text.trim().isEmpty ? null : _packLevel.text.trim(),
      instructions: _instructions.text,
      policy: _rolePolicy(
        filesystem: _filesystem,
        shell: _shell,
        network: _network,
        approval: _approval,
      ),
      budget: _roleBudget(
        timeSeconds: _timeSeconds.text,
        turns: _turns.text,
        measuredTokens: _measuredTokens.text,
        costMicros: _costMicros.text,
      ),
      avatarPalette: _avatarPalette.text.trim().isEmpty
          ? 'default'
          : _avatarPalette.text.trim(),
      avatarSeed: int.tryParse(_avatarSeed.text.trim()) ?? 1,
    );
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.onSave(draft);
      if (mounted) Navigator.pop(context);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _error =
            'Could not create role: ${frankFriendlyError(error, fallback: 'Try again.')}';
      });
    }
  }
}

Map<String, Object?> _rolePolicy({
  required String filesystem,
  required String shell,
  required String network,
  required String approval,
}) => {
  'filesystem': filesystem,
  'shell': shell,
  'network': network,
  'approval': approval,
};

Map<String, Object?> _roleBudget({
  required String timeSeconds,
  required String turns,
  required String measuredTokens,
  required String costMicros,
}) => {
  'time_seconds': int.tryParse(timeSeconds.trim()),
  'turns': int.tryParse(turns.trim()),
  'measured_tokens': int.tryParse(measuredTokens.trim()),
  'cost_micros': int.tryParse(costMicros.trim()),
};

String _policyString(
  Map<String, Object?> policy,
  String key,
  String fallback,
) => policy[key] is String ? policy[key]! as String : fallback;

class _RolePolicyFields extends StatelessWidget {
  const _RolePolicyFields({
    required this.filesystem,
    required this.shell,
    required this.network,
    required this.approval,
    required this.onFilesystemChanged,
    required this.onShellChanged,
    required this.onNetworkChanged,
    required this.onApprovalChanged,
  });

  final String filesystem;
  final String shell;
  final String network;
  final String approval;
  final ValueChanged<String> onFilesystemChanged;
  final ValueChanged<String> onShellChanged;
  final ValueChanged<String> onNetworkChanged;
  final ValueChanged<String> onApprovalChanged;

  Widget _field({
    required String label,
    required String value,
    required List<(String, String)> items,
    required ValueChanged<String?> onChanged,
  }) => _teamSelect(
    label: label,
    value: value,
    options: items,
    onChanged: onChanged,
  );

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      const Text(
        'Role policy',
        style: TextStyle(fontWeight: FontWeight.w600, fontSize: 13),
      ),
      const SizedBox(height: 8),
      _field(
        label: 'Filesystem',
        value: filesystem,
        items: const [
          ('read-only', 'Read only'),
          ('workspace-write', 'Workspace write'),
        ],
        onChanged: (value) {
          if (value != null) onFilesystemChanged(value);
        },
      ),
      const SizedBox(height: 8),
      Row(
        children: [
          Expanded(
            child: _field(
              label: 'Shell',
              value: shell,
              items: const [
                ('deny', 'Deny'),
                ('ask', 'Ask'),
                ('allow', 'Allow'),
              ],
              onChanged: (value) {
                if (value != null) onShellChanged(value);
              },
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: _field(
              label: 'Network',
              value: network,
              items: const [
                ('deny', 'Deny'),
                ('ask', 'Ask'),
                ('allow', 'Allow'),
              ],
              onChanged: (value) {
                if (value != null) onNetworkChanged(value);
              },
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: _field(
              label: 'Approval',
              value: approval,
              items: const [('never', 'Never'), ('ask', 'Ask')],
              onChanged: (value) {
                if (value != null) onApprovalChanged(value);
              },
            ),
          ),
        ],
      ),
    ],
  );
}

class _RoleBudgetFields extends StatelessWidget {
  const _RoleBudgetFields({
    required this.timeSeconds,
    required this.turns,
    required this.measuredTokens,
    required this.costMicros,
  });

  final TextEditingController timeSeconds;
  final TextEditingController turns;
  final TextEditingController measuredTokens;
  final TextEditingController costMicros;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      const Text(
        'Budget limits (optional)',
        style: TextStyle(fontWeight: FontWeight.w600, fontSize: 13),
      ),
      const SizedBox(height: 8),
      Row(
        children: [
          Expanded(
            child: _teamTextField(
              timeSeconds,
              label: 'Time seconds',
              keyboardType: TextInputType.number,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: _teamTextField(
              turns,
              label: 'Turns',
              keyboardType: TextInputType.number,
            ),
          ),
        ],
      ),
      const SizedBox(height: 8),
      Row(
        children: [
          Expanded(
            child: _teamTextField(
              measuredTokens,
              label: 'Measured tokens',
              keyboardType: TextInputType.number,
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: _teamTextField(
              costMicros,
              label: 'Cost micros',
              keyboardType: TextInputType.number,
            ),
          ),
        ],
      ),
    ],
  );
}

class _RoleAvatarFields extends StatelessWidget {
  const _RoleAvatarFields({required this.palette, required this.seed});

  final TextEditingController palette;
  final TextEditingController seed;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Expanded(child: _teamTextField(palette, label: 'Avatar palette')),
      const SizedBox(width: 10),
      SizedBox(
        width: 110,
        child: _teamTextField(
          seed,
          label: 'Avatar seed',
          keyboardType: TextInputType.number,
        ),
      ),
    ],
  );
}

class _RoleEditDialog extends StatefulWidget {
  const _RoleEditDialog({
    required this.role,
    required this.onSave,
    this.models = const [],
  });

  final TeamRoleSummary role;
  final Future<void> Function(TeamRolePatch patch) onSave;
  final List<OpenRouterModel> models;

  @override
  State<_RoleEditDialog> createState() => _RoleEditDialogState();
}

class _RoleEditDialogState extends State<_RoleEditDialog> {
  late final TextEditingController _name;
  late final TextEditingController _description;
  late final TextEditingController _model;
  late final TextEditingController _packId;
  late final TextEditingController _packLevel;
  late final TextEditingController _instructions;
  late final TextEditingController _timeSeconds;
  late final TextEditingController _turns;
  late final TextEditingController _measuredTokens;
  late final TextEditingController _costMicros;
  late final TextEditingController _avatarPalette;
  late final TextEditingController _avatarSeed;
  late String _template;
  late String _filesystem;
  late String _shell;
  late String _network;
  late String _approval;
  bool _saving = false;
  String? _error;
  late final int _initialFingerprint;

  List<TextEditingController> get _controllers => [
    _name,
    _description,
    _model,
    _packId,
    _packLevel,
    _instructions,
    _timeSeconds,
    _turns,
    _measuredTokens,
    _costMicros,
    _avatarPalette,
    _avatarSeed,
  ];

  int get _fingerprint => Object.hash(
    _name.text.trim(),
    _description.text.trim(),
    _model.text.trim(),
    _packId.text.trim(),
    _packLevel.text.trim(),
    _instructions.text,
    _timeSeconds.text.trim(),
    _turns.text.trim(),
    _measuredTokens.text.trim(),
    _costMicros.text.trim(),
    _avatarPalette.text.trim(),
    _avatarSeed.text.trim(),
    _template,
    _filesystem,
    _shell,
    _network,
    _approval,
  );

  bool get _dirty => _fingerprint != _initialFingerprint;

  @override
  void initState() {
    super.initState();
    final role = widget.role;
    _name = TextEditingController(text: role.name);
    _description = TextEditingController(text: role.description);
    _model = TextEditingController(text: role.defaultModel ?? '');
    _packId = TextEditingController(text: role.packId ?? 'caveman');
    _packLevel = TextEditingController(text: role.packLevel ?? 'full');
    _instructions = TextEditingController(text: role.instructions);
    _timeSeconds = TextEditingController(
      text: role.budget['time_seconds']?.toString() ?? '',
    );
    _turns = TextEditingController(
      text: role.budget['turns']?.toString() ?? '',
    );
    _measuredTokens = TextEditingController(
      text: role.budget['measured_tokens']?.toString() ?? '',
    );
    _costMicros = TextEditingController(
      text: role.budget['cost_micros']?.toString() ?? '',
    );
    _avatarPalette = TextEditingController(text: role.avatarPalette);
    _avatarSeed = TextEditingController(text: role.avatarSeed.toString());
    _template = role.template;
    _filesystem = _policyString(role.policy, 'filesystem', 'workspace-write');
    _shell = _policyString(role.policy, 'shell', 'ask');
    _network = _policyString(role.policy, 'network', 'ask');
    _approval = _policyString(role.policy, 'approval', 'ask');
    for (final controller in _controllers) {
      controller.addListener(_refreshDirty);
    }
    _initialFingerprint = _fingerprint;
  }

  @override
  void dispose() {
    for (final controller in _controllers) {
      controller.removeListener(_refreshDirty);
    }
    _name.dispose();
    _description.dispose();
    _model.dispose();
    _packId.dispose();
    _packLevel.dispose();
    _instructions.dispose();
    _timeSeconds.dispose();
    _turns.dispose();
    _measuredTokens.dispose();
    _costMicros.dispose();
    _avatarPalette.dispose();
    _avatarSeed.dispose();
    super.dispose();
  }

  void _refreshDirty() {
    if (mounted) setState(() {});
  }

  Future<void> _dismiss() async {
    if (_saving) return;
    if (!_dirty) {
      if (mounted) Navigator.pop(context);
      return;
    }
    if (await _confirmTeamDiscard(context) && mounted) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => _teamDirtyGuard(
    context: context,
    dirty: _dirty,
    busy: _saving,
    confirmDiscard: () => _confirmTeamDiscard(context),
    child: _teamDialog(
      title: Text('Edit ${widget.role.name}'),
      content: SizedBox(
        width: 480,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              _teamTextField(_name, label: 'Role name'),
              const SizedBox(height: 10),
              _teamTextField(_description, label: 'Description', maxLines: 2),
              const SizedBox(height: 10),
              _teamSelect(
                label: 'Template',
                value: _template,
                options: const [
                  ('generalist', 'Generalist'),
                  ('researcher', 'Researcher'),
                  ('builder', 'Builder'),
                  ('reviewer', 'Reviewer'),
                ],
                onChanged: (value) {
                  if (value != null) setState(() => _template = value);
                },
              ),
              const SizedBox(height: 10),
              if (widget.models.isEmpty)
                _teamTextField(
                  _model,
                  label: 'OpenRouter model',
                  hint: 'openai/gpt-4o-mini',
                )
              else
                FrankOpenRouterModelPicker(
                  label: 'Role default model',
                  value: _model.text.trim().isEmpty ? null : _model.text.trim(),
                  models: widget.models,
                  enabled: !_saving,
                  onChanged: (value) => _model.text = value ?? '',
                ),
              const SizedBox(height: 10),
              Row(
                children: [
                  Expanded(
                    child: _teamTextField(_packId, label: 'Prompt pack'),
                  ),
                  const SizedBox(width: 10),
                  Expanded(child: _teamTextField(_packLevel, label: 'Level')),
                ],
              ),
              const SizedBox(height: 10),
              _teamTextField(_instructions, label: 'Instructions', maxLines: 4),
              const SizedBox(height: 12),
              _RolePolicyFields(
                filesystem: _filesystem,
                shell: _shell,
                network: _network,
                approval: _approval,
                onFilesystemChanged: (value) =>
                    setState(() => _filesystem = value),
                onShellChanged: (value) => setState(() => _shell = value),
                onNetworkChanged: (value) => setState(() => _network = value),
                onApprovalChanged: (value) => setState(() => _approval = value),
              ),
              const SizedBox(height: 12),
              _RoleBudgetFields(
                timeSeconds: _timeSeconds,
                turns: _turns,
                measuredTokens: _measuredTokens,
                costMicros: _costMicros,
              ),
              const SizedBox(height: 12),
              _RoleAvatarFields(palette: _avatarPalette, seed: _avatarSeed),
            ],
          ),
        ),
      ),
      feedback: _teamFeedback(context, _error),
      actions: [
        FButton(
          onPress: _saving ? null : _dismiss,
          variant: FButtonVariant.ghost,
          child: const Text('Cancel'),
        ),
        FButton(
          onPress: _saving ? null : _submit,
          child: _saving
              ? const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [FProgress(), SizedBox(width: 8), Text('Saving…')],
                )
              : const Text('Save role'),
        ),
      ],
    ),
  );

  Future<void> _submit() async {
    final name = _name.text.trim();
    if (name.isEmpty) {
      setState(() => _error = 'Role name is required.');
      return;
    }
    final patch = TeamRolePatch(
      name: TeamPatchField<String>.set(name),
      description: TeamPatchField<String>.set(_description.text.trim()),
      template: TeamPatchField<String>.set(_template),
      defaultModel: _model.text.trim().isEmpty
          ? const TeamPatchField<String>.clear()
          : TeamPatchField<String>.set(_model.text.trim()),
      packId: _packId.text.trim().isEmpty
          ? const TeamPatchField<String>.clear()
          : TeamPatchField<String>.set(_packId.text.trim()),
      packLevel: _packLevel.text.trim().isEmpty
          ? const TeamPatchField<String>.clear()
          : TeamPatchField<String>.set(_packLevel.text.trim()),
      instructions: TeamPatchField<String>.set(_instructions.text),
      policy: TeamPatchField<Map<String, Object?>>.set(
        _rolePolicy(
          filesystem: _filesystem,
          shell: _shell,
          network: _network,
          approval: _approval,
        ),
      ),
      budget: TeamPatchField<Map<String, Object?>>.set(
        _roleBudget(
          timeSeconds: _timeSeconds.text,
          turns: _turns.text,
          measuredTokens: _measuredTokens.text,
          costMicros: _costMicros.text,
        ),
      ),
      avatar: TeamPatchField<TeamAvatarSpec>.set(
        TeamAvatarSpec(
          palette: _avatarPalette.text.trim().isEmpty
              ? 'default'
              : _avatarPalette.text.trim(),
          seed: int.tryParse(_avatarSeed.text.trim()) ?? 1,
        ),
      ),
    );
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.onSave(patch);
      if (mounted) Navigator.pop(context);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _error =
            'Could not save role: ${frankFriendlyError(error, fallback: 'Try again.')}';
      });
    }
  }
}

class _AgentDraftDialog extends StatefulWidget {
  const _AgentDraftDialog({required this.roles, required this.onSave});

  final List<TeamRoleSummary> roles;
  final Future<void> Function(TeamAgentDraft draft) onSave;

  @override
  State<_AgentDraftDialog> createState() => _AgentDraftDialogState();
}

class _AgentDraftDialogState extends State<_AgentDraftDialog> {
  final _name = TextEditingController();
  String? _roleId;
  late String? _initialRoleId;
  bool _saving = false;
  String? _error;

  bool get _dirty => _name.text.trim().isNotEmpty || _roleId != _initialRoleId;

  @override
  void initState() {
    super.initState();
    _roleId = widget.roles.firstOrNull?.id;
    _initialRoleId = _roleId;
    _name.addListener(_refreshDirty);
  }

  @override
  void dispose() {
    _name.removeListener(_refreshDirty);
    _name.dispose();
    super.dispose();
  }

  void _refreshDirty() {
    if (mounted) setState(() {});
  }

  Future<void> _dismiss() async {
    if (_saving) return;
    if (!_dirty) {
      if (mounted) Navigator.pop(context);
      return;
    }
    if (await _confirmTeamDiscard(context) && mounted) Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => _teamDirtyGuard(
    context: context,
    dirty: _dirty,
    busy: _saving,
    confirmDiscard: () => _confirmTeamDiscard(context),
    child: _teamDialog(
      title: const Text('Create team member'),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _teamTextField(_name, label: 'Display name', autofocus: true),
            const SizedBox(height: 12),
            _teamSelect(
              label: 'Role',
              value: _roleId,
              options: [for (final role in widget.roles) (role.id, role.name)],
              onChanged: (value) => setState(() => _roleId = value),
            ),
          ],
        ),
      ),
      feedback: _teamFeedback(context, _error),
      actions: [
        FButton(
          onPress: _saving ? null : _dismiss,
          variant: FButtonVariant.ghost,
          child: const Text('Cancel'),
        ),
        FButton(
          onPress: _saving ? null : _submit,
          child: _saving
              ? const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [FProgress(), SizedBox(width: 8), Text('Saving…')],
                )
              : const Text('Create member'),
        ),
      ],
    ),
  );

  Future<void> _submit() async {
    final name = _name.text.trim();
    final roleId = _roleId;
    if (name.isEmpty || roleId == null) {
      setState(() => _error = 'Display name and role are required.');
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.onSave(TeamAgentDraft(displayName: name, roleId: roleId));
      if (mounted) Navigator.pop(context);
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _saving = false;
        _error =
            'Could not create member: ${frankFriendlyError(error, fallback: 'Try again.')}';
      });
    }
  }
}
