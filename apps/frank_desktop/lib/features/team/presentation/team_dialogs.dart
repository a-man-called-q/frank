part of 'team_surface.dart';

class _AgentEditDialog extends StatefulWidget {
  const _AgentEditDialog({required this.profile, required this.roles});

  final TeamAgentProfile profile;
  final List<TeamRoleSummary> roles;

  @override
  State<_AgentEditDialog> createState() => _AgentEditDialogState();
}

class _AgentEditDialogState extends State<_AgentEditDialog> {
  late final TextEditingController _name;
  late final TextEditingController _modelOverride;
  late final TextEditingController _palette;
  late final TextEditingController _seed;
  String? _roleId;

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(text: widget.profile.name);
    _modelOverride = TextEditingController(
      text: widget.profile.modelOverride ?? '',
    );
    _palette = TextEditingController(text: 'frank');
    _seed = TextEditingController(text: '1');
    _roleId = widget.profile.roleId ?? widget.roles.firstOrNull?.id;
  }

  @override
  void dispose() {
    _name.dispose();
    _modelOverride.dispose();
    _palette.dispose();
    _seed.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text('Edit ${widget.profile.name}'),
    content: SizedBox(
      width: 440,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _name,
              decoration: const InputDecoration(labelText: 'Display name'),
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<String>(
              initialValue: _roleId,
              decoration: const InputDecoration(labelText: 'Role'),
              items: [
                for (final role in widget.roles.where((role) => !role.archived))
                  DropdownMenuItem(value: role.id, child: Text(role.name)),
              ],
              onChanged: (value) => setState(() => _roleId = value),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _modelOverride,
              decoration: const InputDecoration(
                labelText: 'Agent model override',
                hintText: 'Leave empty to inherit role',
              ),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _palette,
                    decoration: const InputDecoration(
                      labelText: 'Avatar palette',
                    ),
                  ),
                ),
                const SizedBox(width: 10),
                SizedBox(
                  width: 100,
                  child: TextField(
                    controller: _seed,
                    keyboardType: TextInputType.number,
                    decoration: const InputDecoration(labelText: 'Seed'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () {
          final name = _name.text.trim();
          if (name.isEmpty || _roleId == null) return;
          final override = _modelOverride.text.trim();
          Navigator.pop(
            context,
            TeamAgentPatch(
              displayName: TeamPatchField<String>.set(name),
              roleId: TeamPatchField<String>.set(_roleId!),
              modelOverride: override.isEmpty
                  ? const TeamPatchField<String>.clear()
                  : TeamPatchField<String>.set(override),
              avatar: TeamPatchField<TeamAvatarSpec>.set(
                TeamAvatarSpec(
                  palette: _palette.text.trim().isEmpty
                      ? 'frank'
                      : _palette.text.trim(),
                  seed: int.tryParse(_seed.text.trim()) ?? 1,
                ),
              ),
            ),
          );
        },
        child: const Text('Save member'),
      ),
    ],
  );
}

class _RoleDraftDialog extends StatefulWidget {
  const _RoleDraftDialog();

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
  final _avatarPalette = TextEditingController(text: 'frank');
  final _avatarSeed = TextEditingController(text: '1');
  String _template = 'generalist';
  String _filesystem = 'workspace-write';
  String _shell = 'ask';
  String _network = 'ask';
  String _approval = 'ask';

  @override
  void dispose() {
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

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Create role'),
    content: SizedBox(
      width: 500,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _name,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Role name'),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _description,
              maxLines: 2,
              decoration: const InputDecoration(labelText: 'Description'),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<String>(
              initialValue: _template,
              decoration: const InputDecoration(labelText: 'Template'),
              items: const [
                DropdownMenuItem(
                  value: 'generalist',
                  child: Text('Generalist'),
                ),
                DropdownMenuItem(
                  value: 'researcher',
                  child: Text('Researcher'),
                ),
                DropdownMenuItem(value: 'builder', child: Text('Builder')),
                DropdownMenuItem(value: 'reviewer', child: Text('Reviewer')),
              ],
              onChanged: (value) {
                if (value != null) setState(() => _template = value);
              },
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _model,
              decoration: const InputDecoration(labelText: 'OpenRouter model'),
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _packId,
                    decoration: const InputDecoration(labelText: 'Prompt pack'),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: TextField(
                    controller: _packLevel,
                    decoration: const InputDecoration(labelText: 'Level'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _instructions,
              maxLines: 3,
              decoration: const InputDecoration(labelText: 'Instructions'),
            ),
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
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () {
          final name = _name.text.trim();
          if (name.isEmpty) return;
          Navigator.pop(
            context,
            TeamRoleDraft(
              name: name,
              description: _description.text.trim(),
              template: _template,
              defaultModel: _model.text.trim().isEmpty
                  ? null
                  : _model.text.trim(),
              packId: _packId.text.trim().isEmpty ? null : _packId.text.trim(),
              packLevel: _packLevel.text.trim().isEmpty
                  ? null
                  : _packLevel.text.trim(),
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
                  ? 'frank'
                  : _avatarPalette.text.trim(),
              avatarSeed: int.tryParse(_avatarSeed.text.trim()) ?? 1,
            ),
          );
        },
        child: const Text('Create role'),
      ),
    ],
  );
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

  DropdownButtonFormField<String> _field({
    required String label,
    required String value,
    required List<DropdownMenuItem<String>> items,
    required ValueChanged<String?> onChanged,
  }) => DropdownButtonFormField<String>(
    initialValue: value,
    decoration: InputDecoration(labelText: label),
    items: items,
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
          DropdownMenuItem(value: 'read-only', child: Text('Read only')),
          DropdownMenuItem(
            value: 'workspace-write',
            child: Text('Workspace write'),
          ),
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
                DropdownMenuItem(value: 'deny', child: Text('Deny')),
                DropdownMenuItem(value: 'ask', child: Text('Ask')),
                DropdownMenuItem(value: 'allow', child: Text('Allow')),
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
                DropdownMenuItem(value: 'deny', child: Text('Deny')),
                DropdownMenuItem(value: 'ask', child: Text('Ask')),
                DropdownMenuItem(value: 'allow', child: Text('Allow')),
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
              items: const [
                DropdownMenuItem(value: 'never', child: Text('Never')),
                DropdownMenuItem(value: 'ask', child: Text('Ask')),
              ],
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
            child: TextField(
              controller: timeSeconds,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Time seconds'),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: TextField(
              controller: turns,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Turns'),
            ),
          ),
        ],
      ),
      const SizedBox(height: 8),
      Row(
        children: [
          Expanded(
            child: TextField(
              controller: measuredTokens,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Measured tokens'),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: TextField(
              controller: costMicros,
              keyboardType: TextInputType.number,
              decoration: const InputDecoration(labelText: 'Cost micros'),
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
      Expanded(
        child: TextField(
          controller: palette,
          decoration: const InputDecoration(labelText: 'Avatar palette'),
        ),
      ),
      const SizedBox(width: 10),
      SizedBox(
        width: 110,
        child: TextField(
          controller: seed,
          keyboardType: TextInputType.number,
          decoration: const InputDecoration(labelText: 'Avatar seed'),
        ),
      ),
    ],
  );
}

class _RoleEditDialog extends StatefulWidget {
  const _RoleEditDialog({required this.role});

  final TeamRoleSummary role;

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
  }

  @override
  void dispose() {
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

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text('Edit ${widget.role.name}'),
    content: SizedBox(
      width: 480,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _name,
              decoration: const InputDecoration(labelText: 'Role name'),
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _description,
              maxLines: 2,
              decoration: const InputDecoration(labelText: 'Description'),
            ),
            const SizedBox(height: 10),
            DropdownButtonFormField<String>(
              initialValue: _template,
              decoration: const InputDecoration(labelText: 'Template'),
              items: const [
                DropdownMenuItem(
                  value: 'generalist',
                  child: Text('Generalist'),
                ),
                DropdownMenuItem(
                  value: 'researcher',
                  child: Text('Researcher'),
                ),
                DropdownMenuItem(value: 'builder', child: Text('Builder')),
                DropdownMenuItem(value: 'reviewer', child: Text('Reviewer')),
              ],
              onChanged: (value) {
                if (value != null) setState(() => _template = value);
              },
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _model,
              decoration: const InputDecoration(
                labelText: 'OpenRouter model',
                hintText: 'openai/gpt-4o-mini',
              ),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _packId,
                    decoration: const InputDecoration(labelText: 'Prompt pack'),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: TextField(
                    controller: _packLevel,
                    decoration: const InputDecoration(labelText: 'Level'),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 10),
            TextField(
              controller: _instructions,
              maxLines: 4,
              decoration: const InputDecoration(labelText: 'Instructions'),
            ),
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
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () {
          final name = _name.text.trim();
          if (name.isEmpty) return;
          Navigator.pop(
            context,
            TeamRolePatch(
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
                      ? 'frank'
                      : _avatarPalette.text.trim(),
                  seed: int.tryParse(_avatarSeed.text.trim()) ?? 1,
                ),
              ),
            ),
          );
        },
        child: const Text('Save role'),
      ),
    ],
  );
}

class _AgentDraftDialog extends StatefulWidget {
  const _AgentDraftDialog({required this.roles});

  final List<TeamRoleSummary> roles;

  @override
  State<_AgentDraftDialog> createState() => _AgentDraftDialogState();
}

class _AgentDraftDialogState extends State<_AgentDraftDialog> {
  final _name = TextEditingController();
  String? _roleId;

  @override
  void initState() {
    super.initState();
    _roleId = widget.roles.firstOrNull?.id;
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Create team member'),
    content: SizedBox(
      width: 420,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            controller: _name,
            autofocus: true,
            decoration: const InputDecoration(labelText: 'Display name'),
          ),
          const SizedBox(height: 12),
          DropdownButtonFormField<String>(
            initialValue: _roleId,
            decoration: const InputDecoration(labelText: 'Role'),
            items: [
              for (final role in widget.roles)
                DropdownMenuItem(value: role.id, child: Text(role.name)),
            ],
            onChanged: (value) => setState(() => _roleId = value),
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () {
          final name = _name.text.trim();
          final roleId = _roleId;
          if (name.isEmpty || roleId == null) return;
          Navigator.pop(
            context,
            TeamAgentDraft(displayName: name, roleId: roleId),
          );
        },
        child: const Text('Create member'),
      ),
    ],
  );
}
