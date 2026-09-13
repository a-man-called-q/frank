part of 'organization_surface.dart';

sealed class _AddChoice {
  const _AddChoice();
}

final class _StaffChoice extends _AddChoice {
  const _StaffChoice(this.employee);

  final OfficeEmployee employee;
}

final class _CapabilityChoice extends _AddChoice {
  const _CapabilityChoice(this.capability);

  final OrganizationCapabilityKind capability;
}

final class _ApprovalChoice extends _AddChoice {
  const _ApprovalChoice();
}

final class _RoleChoice extends _AddChoice {
  const _RoleChoice(this.role);

  final TeamRoleSummary role;
}

final class _TaskboardChoice extends _AddChoice {
  const _TaskboardChoice(this.board);

  final WorkflowTaskboard board;
}

final class _ChildWorkflowChoice extends _AddChoice {
  const _ChildWorkflowChoice(this.workflowId, this.label);

  final String workflowId;
  final String label;
}

final class _GroupChoice extends _AddChoice {
  const _GroupChoice(this.label);

  final String label;
}

/// Existing-group rename remains a regular dialog. Add-group uses the inline
/// step in [_AddPalette] so it does not stack a second dialog route.
class _GroupNameDialog extends StatefulWidget {
  const _GroupNameDialog({this.initialValue});

  final String? initialValue;

  @override
  State<_GroupNameDialog> createState() => _GroupNameDialogState();
}

class _GroupNameDialogState extends State<_GroupNameDialog> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.initialValue,
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      backgroundColor: FrankColors.panel,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        side: const BorderSide(
          color: FrankColors.border,
          width: FrankUiTokens.borderWidth,
        ),
      ),
      title: const Text('Rename group'),
      content: TextField(
        key: const ValueKey('organization-group-name'),
        controller: _controller,
        autofocus: true,
        maxLength: 48,
        textCapitalization: TextCapitalization.words,
        decoration: _organizationInputDecoration(hintText: 'e.g. Research'),
        onSubmitted: (_) => _submit(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          style: _compactTextButtonStyle(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('organization-group-create'),
          onPressed: _submit,
          style: _publishButtonStyle(),
          child: const Text('Save group'),
        ),
      ],
    );
  }

  void _submit() {
    final label = _controller.text.trim();
    if (label.isEmpty) return;
    Navigator.pop(context, label);
  }
}

enum _AddPaletteMode { browse, group }

enum _PaletteEntryKind {
  staff,
  capability,
  approval,
  role,
  taskboard,
  childWorkflow,
  group,
}

final class _PaletteEntry {
  const _PaletteEntry({
    required this.id,
    required this.label,
    required this.description,
    required this.badge,
    required this.icon,
    required this.color,
    required this.kind,
    required this.choice,
    required this.metadata,
  });

  final String id;
  final String label;
  final String description;
  final String badge;
  final IconData icon;
  final Color color;
  final _PaletteEntryKind kind;
  final _AddChoice choice;
  final String metadata;
}

final class _RankedPaletteEntry {
  const _RankedPaletteEntry({
    required this.entry,
    required this.rank,
    required this.sourceIndex,
  });

  final _PaletteEntry entry;
  final int rank;
  final int sourceIndex;
}

class _AddPalette extends StatefulWidget {
  const _AddPalette({
    required this.employees,
    required this.usedEmployeeIds,
    this.roles = const [],
    this.taskboards = const [],
  });

  final List<OfficeEmployee> employees;
  final Set<String> usedEmployeeIds;
  final List<TeamRoleSummary> roles;
  final List<WorkflowTaskboard> taskboards;

  @override
  State<_AddPalette> createState() => _AddPaletteState();
}

class _AddPaletteState extends State<_AddPalette> {
  static const _panelRadius = 14.0;
  static const _panelMaxWidth = 440.0;
  static const _panelMaxHeight = 520.0;
  static const _rowHeight = 58.0;

  late final TextEditingController _searchController = TextEditingController();
  late final TextEditingController _groupController = TextEditingController();
  late final FocusNode _searchFocusNode = FocusNode(
    debugLabel: 'Add to office search',
  );
  late final FocusNode _groupFocusNode = FocusNode(
    debugLabel: 'New group name',
  );
  late final ScrollController _resultsScrollController = ScrollController();

  _AddPaletteMode _mode = _AddPaletteMode.browse;
  String _query = '';
  int _selectedIndex = 0;

  @override
  void initState() {
    super.initState();
    _requestSearchFocus();
  }

  @override
  void dispose() {
    _searchController.dispose();
    _groupController.dispose();
    _searchFocusNode.dispose();
    _groupFocusNode.dispose();
    _resultsScrollController.dispose();
    super.dispose();
  }

  List<OfficeEmployee> get _unusedEmployees => widget.employees
      .where((employee) => !widget.usedEmployeeIds.contains(employee.id))
      .toList(growable: false);

  List<_PaletteEntry> get _allEntries {
    final entries = <_PaletteEntry>[];
    for (final employee in _unusedEmployees) {
      entries.add(
        _PaletteEntry(
          id: 'staff-${employee.id}',
          label: employee.name,
          description: employee.role,
          badge: 'Person',
          icon: FrankIcons.user,
          color: Color(employee.color),
          kind: _PaletteEntryKind.staff,
          choice: _StaffChoice(employee),
          metadata:
              '${employee.role} ${employee.status} ${employee.initials} '
              'staff person people team',
        ),
      );
    }
    for (final role in widget.roles.where((role) => !role.archived)) {
      entries.add(
        _PaletteEntry(
          id: 'role-${role.id}',
          label: role.name,
          description: role.description.isEmpty
              ? 'Reusable worker template'
              : role.description,
          badge: 'Role',
          icon: FrankIcons.user,
          color: organizationRoleColor,
          kind: _PaletteEntryKind.role,
          choice: _RoleChoice(role),
          metadata:
              '${role.name} ${role.description} ${role.template} role worker agent template',
        ),
      );
    }
    final seenWorkflows = <String>{};
    for (final board in widget.taskboards.where((board) => !board.archived)) {
      entries.add(
        _PaletteEntry(
          id: 'taskboard-${board.id}',
          label: board.name,
          description: 'Shared ${board.dispatchMode.name} hand-off surface',
          badge: 'Taskboard',
          icon: FrankIcons.dashboard,
          color: organizationTaskboardColor,
          kind: _PaletteEntryKind.taskboard,
          choice: _TaskboardChoice(board),
          metadata:
              '${board.name} board kanban tasks work handoff ${board.dispatchMode.name}',
        ),
      );
      final workflowId = board.workflowId;
      if (workflowId != null && seenWorkflows.add(workflowId)) {
        entries.add(
          _PaletteEntry(
            id: 'child-workflow-$workflowId',
            label:
                'Workflow ${workflowId.length > 8 ? workflowId.substring(0, 8) : workflowId}',
            description: 'One-level composed child workflow',
            badge: 'Child workflow',
            icon: FrankIcons.workflow,
            color: organizationChildWorkflowColor,
            kind: _PaletteEntryKind.childWorkflow,
            choice: _ChildWorkflowChoice(workflowId, 'Child workflow'),
            metadata: '$workflowId workflow child subflow composition',
          ),
        );
      }
    }
    for (final capability in OrganizationCapabilityKind.values) {
      entries.add(
        _PaletteEntry(
          id: 'capability-${capability.name}',
          label: capability.label,
          description:
              '${capability.category} · ${capability.permissions.join(' · ')}',
          badge: 'Tool',
          icon: capability.icon,
          color: FrankColors.blue,
          kind: _PaletteEntryKind.capability,
          choice: _CapabilityChoice(capability),
          metadata: _capabilitySearchMetadata(capability),
        ),
      );
    }
    entries.add(
      const _PaletteEntry(
        id: 'approval-desk',
        label: 'Approval Desk',
        description: 'Human checkpoint',
        badge: 'Control',
        icon: FrankIcons.approval,
        color: FrankColors.warningAmber,
        kind: _PaletteEntryKind.approval,
        choice: _ApprovalChoice(),
        metadata:
            'approval control human checkpoint review reviewer gate authorize',
      ),
    );
    entries.add(
      const _PaletteEntry(
        id: 'group',
        label: 'Group',
        description: 'Container for office elements',
        badge: 'Structure',
        icon: FrankIcons.workflow,
        color: FrankColors.aubergineAccent,
        kind: _PaletteEntryKind.group,
        choice: _GroupChoice(''),
        metadata:
            'group structure container office elements organization workspace '
            'section',
      ),
    );
    return entries;
  }

  List<_PaletteEntry> get _visibleEntries {
    final normalized = _query.trim().toLowerCase();
    final all = _allEntries;
    if (normalized.isEmpty) {
      final curated = <_PaletteEntry>[
        ...all.where((entry) => entry.kind == _PaletteEntryKind.staff).take(2),
      ];
      for (final id in const [
        'capability-taskboard',
        'capability-drive',
        'approval-desk',
        'group',
      ]) {
        final match = all.where((entry) => entry.id == id).firstOrNull;
        if (match != null) curated.add(match);
      }
      curated.addAll(all.where((entry) => entry.kind == _PaletteEntryKind.role).take(2));
      curated.addAll(
        all.where((entry) => entry.kind == _PaletteEntryKind.taskboard).take(2),
      );
      return curated;
    }

    final ranked = <_RankedPaletteEntry>[];
    for (var index = 0; index < all.length; index++) {
      final entry = all[index];
      final rank = _rankEntry(entry, normalized);
      if (rank != null) {
        ranked.add(
          _RankedPaletteEntry(entry: entry, rank: rank, sourceIndex: index),
        );
      }
    }
    ranked.sort((a, b) {
      final rankComparison = a.rank.compareTo(b.rank);
      return rankComparison == 0
          ? a.sourceIndex.compareTo(b.sourceIndex)
          : rankComparison;
    });
    return ranked.map((result) => result.entry).toList(growable: false);
  }

  int? _rankEntry(_PaletteEntry entry, String normalized) {
    final label = entry.label.toLowerCase();
    final metadata = entry.metadata.toLowerCase();
    if (label == normalized) return 0;
    if (label.startsWith(normalized)) return 1;
    if (label.contains(normalized)) return 2;
    if (metadata.contains(normalized)) return 3;
    return null;
  }

  String _capabilitySearchMetadata(OrganizationCapabilityKind capability) {
    final aliases = switch (capability) {
      OrganizationCapabilityKind.email => 'mail inbox gmail messages send',
      OrganizationCapabilityKind.calendar =>
        'schedule meetings events appointments google calendar',
      OrganizationCapabilityKind.taskboard =>
        'tasks kanban work projects linear assignments',
      OrganizationCapabilityKind.drive =>
        'files storage documents docs google drive sharing',
      OrganizationCapabilityKind.browser => 'web internet browsing chrome',
      OrganizationCapabilityKind.terminal =>
        'shell command line commands execute workspace',
      OrganizationCapabilityKind.database =>
        'db data sql records query inspect',
    };
    return '${capability.label} ${capability.name} ${capability.category} '
        '${capability.permissions.join(' ')} capability tool $aliases';
  }

  void _requestSearchFocus() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _mode == _AddPaletteMode.browse) {
        _searchFocusNode.requestFocus();
      }
    });
  }

  void _requestGroupFocus() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _mode == _AddPaletteMode.group) {
        _groupFocusNode.requestFocus();
      }
    });
  }

  void _onQueryChanged(String value) {
    setState(() {
      _query = value;
      _selectedIndex = 0;
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_resultsScrollController.hasClients) {
        _resultsScrollController.jumpTo(0);
      }
    });
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    if (_mode == _AddPaletteMode.group) {
      if (event.logicalKey == LogicalKeyboardKey.escape) {
        _backToBrowse();
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.enter) {
        _submitGroup();
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }

    final entries = _visibleEntries;
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      Navigator.of(context).pop();
      return KeyEventResult.handled;
    }
    if (entries.isEmpty) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.arrowDown) {
      _moveSelection(1, entries.length);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.arrowUp) {
      _moveSelection(-1, entries.length);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.home) {
      _setSelection(0);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.end) {
      _setSelection(entries.length - 1);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter) {
      _activate(entries[_selectedIndex]);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  void _moveSelection(int delta, int length) {
    _setSelection((_selectedIndex + delta) % length);
  }

  void _setSelection(int index) {
    final entries = _visibleEntries;
    if (entries.isEmpty) return;
    final next = index.clamp(0, entries.length - 1).toInt();
    if (next == _selectedIndex) return;
    setState(() => _selectedIndex = next);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_resultsScrollController.hasClients) return;
      final position = _resultsScrollController.position;
      final top = next * _rowHeight;
      final bottom = top + _rowHeight;
      final visibleTop = position.pixels;
      final visibleBottom = visibleTop + position.viewportDimension;
      if (top < visibleTop) {
        _resultsScrollController.jumpTo(top);
      } else if (bottom > visibleBottom) {
        _resultsScrollController.jumpTo(
          (bottom - position.viewportDimension).clamp(
            position.minScrollExtent,
            position.maxScrollExtent,
          ),
        );
      }
    });
  }

  void _activate(_PaletteEntry entry) {
    if (entry.kind == _PaletteEntryKind.group) {
      setState(() {
        _mode = _AddPaletteMode.group;
        _groupController.clear();
      });
      _requestGroupFocus();
      return;
    }
    Navigator.of(context).pop(entry.choice);
  }

  void _backToBrowse() {
    setState(() {
      _mode = _AddPaletteMode.browse;
      _selectedIndex = 0;
    });
    _requestSearchFocus();
  }

  void _submitGroup() {
    final label = _groupController.text.trim();
    if (label.isEmpty) return;
    Navigator.of(context).pop(_GroupChoice(label));
  }

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      minimum: const EdgeInsets.all(24),
      child: LayoutBuilder(
        builder: (context, constraints) {
          final width = constraints.maxWidth
              .clamp(0.0, _panelMaxWidth)
              .toDouble();
          return Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: _panelMaxHeight),
              child: SizedBox(
                width: width,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    borderRadius: BorderRadius.circular(_panelRadius),
                    border: Border.all(
                      color: FrankColors.border.withValues(alpha: .9),
                    ),
                    boxShadow: const [
                      BoxShadow(
                        color: Color(0x66000000),
                        blurRadius: 28,
                        offset: Offset(0, 14),
                      ),
                      BoxShadow(
                        color: Color(0x33000000),
                        blurRadius: 4,
                        offset: Offset(0, 2),
                      ),
                    ],
                  ),
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(_panelRadius),
                    child: BackdropFilter(
                      filter: ui.ImageFilter.blur(sigmaX: 16, sigmaY: 16),
                      child: Material(
                        color: FrankColors.panelRaised.withValues(alpha: .88),
                        child: _mode == _AddPaletteMode.group
                            ? _buildGroupStep()
                            : _buildBrowseStep(),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }

  Widget _buildBrowseStep() {
    final entries = _visibleEntries;
    return Semantics(
      container: true,
      scopesRoute: true,
      namesRoute: true,
      explicitChildNodes: true,
      label: 'Add to office',
      child: Focus(
        onKeyEvent: _handleKeyEvent,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 16, 14, 12),
              child: Row(
                children: [
                  const Expanded(
                    child: Text(
                      'Add to office',
                      style: TextStyle(
                        color: FrankColors.ink,
                        fontSize: 16,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  const _PaletteShortcut(label: 'ESC'),
                  const SizedBox(width: 4),
                  IconButton(
                    key: const ValueKey('organization-add-close'),
                    tooltip: 'Close',
                    onPressed: () => Navigator.of(context).pop(),
                    style: IconButton.styleFrom(
                      minimumSize: const Size.square(28),
                      maximumSize: const Size.square(28),
                      padding: EdgeInsets.zero,
                      foregroundColor: FrankColors.muted,
                      hoverColor: FrankColors.ink.withValues(alpha: .08),
                    ),
                    icon: const Icon(FrankIcons.close, size: 15),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 0, 18, 14),
              child: Focus(
                onKeyEvent: _handleKeyEvent,
                child: Semantics(
                  textField: true,
                  label: 'Search office elements',
                  child: TextField(
                    key: const ValueKey('organization-add-search'),
                    controller: _searchController,
                    focusNode: _searchFocusNode,
                    autofocus: true,
                    textInputAction: TextInputAction.search,
                    decoration:
                        _organizationInputDecoration(
                          hintText: 'Search people, tools, or structure…',
                        ).copyWith(
                          prefixIcon: const Icon(
                            FrankIcons.search,
                            size: FrankUiTokens.iconSize,
                          ),
                        ),
                    onChanged: _onQueryChanged,
                    onSubmitted: (_) {
                      final visible = _visibleEntries;
                      if (visible.isNotEmpty) {
                        _activate(visible[_selectedIndex]);
                      }
                    },
                  ),
                ),
              ),
            ),
            const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 13, 18, 7),
              child: Row(
                children: [
                  Text(
                    _query.trim().isEmpty ? 'QUICK ADD' : 'RESULTS',
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 10,
                      letterSpacing: .8,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const Spacer(),
                  if (entries.isNotEmpty)
                    Text(
                      '${entries.length} available',
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 10,
                      ),
                    ),
                ],
              ),
            ),
            Flexible(
              child: Semantics(
                key: const ValueKey('organization-add-results-semantics'),
                container: true,
                explicitChildNodes: true,
                label: 'Add to office results',
                child: entries.isEmpty
                    ? const Padding(
                        padding: EdgeInsets.fromLTRB(18, 10, 18, 24),
                        child: SizedBox(
                          height: 50,
                          child: Center(
                            child: Text(
                              'No matching office element.',
                              textAlign: TextAlign.center,
                              style: TextStyle(color: FrankColors.muted),
                            ),
                          ),
                        ),
                      )
                    : ListView.builder(
                        key: const ValueKey('organization-add-results'),
                        controller: _resultsScrollController,
                        shrinkWrap: true,
                        padding: const EdgeInsets.fromLTRB(10, 0, 10, 12),
                        itemExtent: _rowHeight,
                        itemCount: entries.length,
                        itemBuilder: (context, index) =>
                            _buildRow(entries[index], index),
                      ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildRow(_PaletteEntry entry, int index) {
    final selected = index == _selectedIndex;
    final rowColor = selected
        ? entry.color.withValues(alpha: .14)
        : Colors.transparent;
    return MouseRegion(
      onEnter: (_) => _setSelection(index),
      child: Semantics(
        container: true,
        button: true,
        selected: selected,
        label: '${entry.label}, ${entry.description}, ${entry.badge}',
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            key: entry.kind == _PaletteEntryKind.group
                ? const ValueKey('add-group')
                : ValueKey('organization-add-result-${entry.id}'),
            onTap: () => _activate(entry),
            borderRadius: BorderRadius.circular(10),
            splashFactory: NoSplash.splashFactory,
            hoverColor: entry.color.withValues(alpha: .08),
            child: AnimatedContainer(
              duration: const Duration(milliseconds: 90),
              curve: Curves.easeOut,
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
              decoration: BoxDecoration(
                color: rowColor,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Row(
                children: [
                  _PaletteIconTile(icon: entry.icon, color: entry.color),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          entry.label,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            color: FrankColors.ink,
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                        const SizedBox(height: 2),
                        Text(
                          entry.description,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            color: FrankColors.muted,
                            fontSize: 10,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),
                  _PaletteBadge(label: entry.badge, color: entry.color),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildGroupStep() {
    return Semantics(
      container: true,
      scopesRoute: true,
      namesRoute: true,
      explicitChildNodes: true,
      label: 'New group',
      child: Focus(
        onKeyEvent: _handleKeyEvent,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(18, 14, 18, 18),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  IconButton(
                    key: const ValueKey('organization-group-back'),
                    tooltip: 'Back',
                    onPressed: _backToBrowse,
                    style: IconButton.styleFrom(
                      minimumSize: const Size.square(28),
                      maximumSize: const Size.square(28),
                      padding: EdgeInsets.zero,
                      foregroundColor: FrankColors.muted,
                    ),
                    icon: Transform.flip(
                      flipX: true,
                      child: const Icon(FrankIcons.chevronRight, size: 16),
                    ),
                  ),
                  const SizedBox(width: 6),
                  const Text(
                    'New group',
                    style: TextStyle(
                      color: FrankColors.ink,
                      fontSize: 16,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  const Spacer(),
                  const _PaletteShortcut(label: 'ESC'),
                ],
              ),
              const SizedBox(height: 18),
              const Text(
                'Create a container for related office elements.',
                style: TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
              const SizedBox(height: 10),
              Focus(
                onKeyEvent: _handleKeyEvent,
                child: TextField(
                  key: const ValueKey('organization-group-name'),
                  controller: _groupController,
                  focusNode: _groupFocusNode,
                  autofocus: true,
                  maxLength: 48,
                  textCapitalization: TextCapitalization.words,
                  decoration: _organizationInputDecoration(
                    hintText: 'e.g. Research',
                  ),
                  onChanged: (_) => setState(() {}),
                  onSubmitted: (_) => _submitGroup(),
                ),
              ),
              const SizedBox(height: 6),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: _backToBrowse,
                    style: _compactTextButtonStyle(),
                    child: const Text('Back'),
                  ),
                  const SizedBox(width: 6),
                  ValueListenableBuilder<TextEditingValue>(
                    valueListenable: _groupController,
                    builder: (context, value, child) {
                      final canCreate = value.text.trim().isNotEmpty;
                      return FilledButton(
                        key: const ValueKey('organization-group-create'),
                        onPressed: canCreate ? _submitGroup : null,
                        style: _publishButtonStyle(),
                        child: const Text('Create group'),
                      );
                    },
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _PaletteIconTile extends StatelessWidget {
  const _PaletteIconTile({required this.icon, required this.color});

  final IconData icon;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 34,
      height: 34,
      decoration: BoxDecoration(
        color: color.withValues(alpha: .16),
        borderRadius: BorderRadius.circular(9),
        border: Border.all(color: color.withValues(alpha: .28)),
      ),
      child: Icon(icon, size: 17, color: color),
    );
  }
}

class _PaletteBadge extends StatelessWidget {
  const _PaletteBadge({required this.label, required this.color});

  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 4),
      decoration: BoxDecoration(
        color: color.withValues(alpha: .1),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: color.withValues(alpha: .22)),
      ),
      child: Text(
        label,
        style: TextStyle(
          color: color,
          fontSize: 8,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}

class _PaletteShortcut extends StatelessWidget {
  const _PaletteShortcut({required this.label});

  final String label;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 3),
      decoration: BoxDecoration(
        color: FrankColors.ink.withValues(alpha: .06),
        borderRadius: BorderRadius.circular(5),
        border: Border.all(color: FrankColors.border),
      ),
      child: Text(
        label,
        style: const TextStyle(
          color: FrankColors.muted,
          fontSize: 8,
          fontWeight: FontWeight.w600,
          letterSpacing: .4,
        ),
      ),
    );
  }
}
