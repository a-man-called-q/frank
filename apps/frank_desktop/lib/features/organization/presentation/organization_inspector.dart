part of 'organization_surface.dart';

class _OrganizationInspector extends StatelessWidget {
  const _OrganizationInspector({
    required this.workspace,
    this.profiles,
    this.connectorProfiles = const [],
    required this.lookupIndex,
    this.docked = true,
    required this.onClose,
    required this.onDelete,
    required this.onDuplicate,
    this.canMutate = true,
    this.mutationDisabledReason,
  });

  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;
  final List<ConnectorProfile> connectorProfiles;
  final OrganizationLookupIndex lookupIndex;
  final bool docked;
  final VoidCallback onClose;
  final VoidCallback onDelete;
  final VoidCallback onDuplicate;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  Widget build(BuildContext context) {
    return BlocBuilder<OrganizationBloc, OrganizationState>(
      builder: (context, state) {
        final graph = state.graph;
        if (graph == null) return const SizedBox.shrink();
        final node = lookupIndex.nodeById[state.selectedNodeId];
        final relation = graph.relations
            .where((relation) => relation.id == state.selectedRelationId)
            .firstOrNull;
        final group = graph.groupById(state.selectedGroupId);
        if (node == null && relation == null && group == null) {
          // Keep a stale/missing selection explicit. Returning a zero-sized
          // child here still makes the overlay paint an empty inspector slab,
          // which looks like a broken editor after a remote refresh.
          return ColoredBox(
            color: FrankColors.panel,
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 360),
                child: _OrganizationPanel(
                  padding: const EdgeInsets.all(20),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Text(
                        'Selection unavailable',
                        style: TextStyle(
                          color: FrankColors.ink,
                          fontSize: 15,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      const SizedBox(height: 8),
                      const Text(
                        'The selected organization element is no longer in the current server snapshot.',
                        style: TextStyle(color: FrankColors.muted, height: 1.4),
                      ),
                      const SizedBox(height: 14),
                      FButton(
                        onPress: onClose,
                        size: FButtonSizeVariant.sm,
                        child: const Text('Close inspector'),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          );
        }
        return Semantics(
          container: true,
          label: 'Organization inspector',
          child: _OrganizationPanel(
            padding: EdgeInsets.zero,
            radius: 0,
            borderRadius: BorderRadius.zero,
            border: const Border(
              left: BorderSide(
                color: FrankColors.border,
                width: FrankUiTokens.borderWidth,
              ),
            ),
            child: Column(
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(18, 14, 10, 10),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          group != null
                              ? 'Group'
                              : node?.kind == OrganizationNodeKind.staff
                              ? 'Staff'
                              : node?.kind == OrganizationNodeKind.capability
                              ? 'Capability'
                              : node?.kind == OrganizationNodeKind.approval
                              ? 'Control'
                              : node?.kind == OrganizationNodeKind.role
                              ? 'Role worker'
                              : node?.kind == OrganizationNodeKind.taskboard
                              ? 'Taskboard'
                              : node?.kind == OrganizationNodeKind.childWorkflow
                              ? 'Child workflow'
                              : relation!.kind.label,
                          style: const TextStyle(
                            color: FrankColors.ink,
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      FButton.icon(
                        onPress: onClose,
                        semanticsLabel: docked
                            ? 'Close inspector'
                            : 'Back to organization',
                        semanticsTooltip: docked
                            ? 'Close inspector'
                            : 'Back to organization',
                        size: FButtonSizeVariant.sm,
                        child: Icon(
                          docked ? FrankIcons.close : FrankIcons.back,
                          size: 17,
                        ),
                      ),
                    ],
                  ),
                ),
                const FDivider(),
                Expanded(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(18),
                    child: group != null
                        ? _GroupInspector(group: group)
                        : node != null
                        ? _NodeInspector(
                            node: node,
                            workspace: workspace,
                            employee: node.employeeId == null
                                ? null
                                : lookupIndex.employeeById[node.employeeId],
                            profile: node.employeeId == null
                                ? null
                                : lookupIndex.profileByEmployeeId[node
                                      .employeeId],
                            connectorProfiles: connectorProfiles,
                          )
                        : _RelationInspector(relation: relation!, graph: graph),
                  ),
                ),
                const FDivider(),
                Padding(
                  padding: const EdgeInsets.all(12),
                  child: Wrap(
                    alignment: WrapAlignment.end,
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      if (node != null &&
                          node.kind != OrganizationNodeKind.staff)
                        FButton(
                          onPress: canMutate ? onDuplicate : null,
                          semanticsTooltip: canMutate
                              ? 'Duplicate organization element'
                              : mutationDisabledReason ??
                                    'Reconnect before changing the organization',
                          variant: FButtonVariant.ghost,
                          size: FButtonSizeVariant.sm,
                          prefix: const Icon(FrankIcons.plus, size: 15),
                          child: const Text('Duplicate'),
                        ),
                      if (group != null && !group.isBuiltIn)
                        FButton(
                          onPress: canMutate ? onDelete : null,
                          semanticsTooltip: canMutate
                              ? 'Delete group'
                              : mutationDisabledReason ??
                                    'Reconnect before changing the organization',
                          variant: FButtonVariant.destructive,
                          size: FButtonSizeVariant.sm,
                          prefix: const Icon(FrankIcons.archive, size: 15),
                          child: const Text('Delete'),
                        ),
                      if (group == null)
                        FButton(
                          onPress: canMutate ? onDelete : null,
                          semanticsTooltip: canMutate
                              ? 'Delete organization element'
                              : mutationDisabledReason ??
                                    'Reconnect before changing the organization',
                          variant: FButtonVariant.destructive,
                          size: FButtonSizeVariant.sm,
                          prefix: const Icon(FrankIcons.archive, size: 15),
                          child: const Text('Delete'),
                        ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _OrganizationTextField extends StatefulWidget {
  const _OrganizationTextField({
    required this.initialValue,
    required this.onSubmit,
    required this.semanticsLabel,
    this.enabled = true,
    this.minLines,
    this.maxLines = 1,
    super.key,
  });

  final String initialValue;
  final ValueChanged<String> onSubmit;
  final String semanticsLabel;
  final bool enabled;
  final int? minLines;
  final int? maxLines;

  @override
  State<_OrganizationTextField> createState() => _OrganizationTextFieldState();
}

class _OrganizationTextFieldState extends State<_OrganizationTextField> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.initialValue,
  );

  @override
  void didUpdateWidget(covariant _OrganizationTextField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.initialValue != oldWidget.initialValue &&
        widget.initialValue != _controller.text) {
      _controller.text = widget.initialValue;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Semantics(
    textField: true,
    label: widget.semanticsLabel,
    child: ExcludeSemantics(
      // ForUI 0.25 uses MergeSemantics inside FTextField. Organization
      // inspector fields are mounted/unmounted as the selected graph element
      // changes, and keeping that merged node in the surrounding flow tree
      // trips Flutter's stale-node assertion. The editable control remains
      // focusable and interactive while this stable parent provides the field
      // semantics.
      child: FTextField(
        key: widget.key,
        control: FTextFieldControl.managed(controller: _controller),
        enabled: widget.enabled,
        minLines: widget.minLines,
        maxLines: widget.maxLines,
        onSubmit: widget.onSubmit,
      ),
    ),
  );
}

class _GroupInspector extends StatelessWidget {
  const _GroupInspector({required this.group});

  final OrganizationGroup group;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _InspectorLabel('Name'),
        _OrganizationTextField(
          key: ValueKey('group-label-${group.id}'),
          initialValue: group.label,
          semanticsLabel: 'Group name',
          enabled: !group.isBuiltIn,
          onSubmit: (value) {
            final label = value.trim();
            if (label.isEmpty) return;
            context.read<OrganizationBloc>().add(
              OrganizationGroupUpdated(group.copyWith(label: label)),
            );
          },
        ),
        const SizedBox(height: 20),
        _InspectorLabel('Position'),
        Text(
          '${group.position.x.round()} × ${group.position.y.round()}',
          style: const TextStyle(color: FrankColors.ink),
        ),
        const SizedBox(height: 16),
        _InspectorLabel('Container size'),
        Text(
          '${group.size.width.round()} × ${group.size.height.round()}',
          style: const TextStyle(color: FrankColors.ink),
        ),
        const SizedBox(height: 20),
        _Notice(
          icon: group.isBuiltIn ? FrankIcons.circleDashed : FrankIcons.workflow,
          text: group.isBuiltIn
              ? 'Built-in container. Its name, position, and size stay fixed.'
              : 'Drag or resize this square container to organize the board.',
        ),
      ],
    );
  }
}

class _ReferenceValue extends StatelessWidget {
  const _ReferenceValue({required this.value});

  final String? value;

  @override
  Widget build(BuildContext context) => Container(
    width: double.infinity,
    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
    decoration: BoxDecoration(
      color: FrankColors.panelRaised,
      borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
      border: const Border.fromBorderSide(
        BorderSide(color: FrankColors.border),
      ),
    ),
    child: Text(
      value == null || value!.isEmpty ? 'Reference not selected' : value!,
      maxLines: 2,
      overflow: TextOverflow.ellipsis,
      style: const TextStyle(color: FrankColors.ink, fontSize: 11),
    ),
  );
}

class _NodeInspector extends StatelessWidget {
  const _NodeInspector({
    required this.node,
    required this.workspace,
    this.employee,
    this.profile,
    this.connectorProfiles = const [],
  });

  final OrganizationNode node;
  final OfficeWorkspace workspace;
  final OfficeEmployee? employee;
  final TeamAgentProfile? profile;
  final List<ConnectorProfile> connectorProfiles;

  @override
  Widget build(BuildContext context) {
    final capability = node.capability;
    final employeeValue = employee;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _InspectorLabel('Name'),
        _OrganizationTextField(
          key: ValueKey('node-label-${node.id}'),
          initialValue: node.label,
          semanticsLabel: 'Node name',
          onSubmit: (value) => context.read<OrganizationBloc>().add(
            OrganizationNodeUpdated(node.copyWith(label: value.trim())),
          ),
        ),
        const SizedBox(height: 20),
        if (employeeValue != null) ...[
          _InspectorLabel('Role'),
          Text(
            employeeValue.role,
            style: const TextStyle(color: FrankColors.ink),
          ),
          const SizedBox(height: 16),
          _InspectorLabel('Effective model'),
          Text(
            profile?.modelSummary ?? 'Unconfigured',
            style: const TextStyle(color: FrankColors.ink),
          ),
        ],
        if (capability != null) ...[
          _InspectorLabel('Connection profile'),
          _ConnectorProfileSelector(node: node, profiles: connectorProfiles),
          const SizedBox(height: 18),
          _InspectorLabel('Available permissions'),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final permission in capability.permissions)
                FBadge(child: Text(permission)),
            ],
          ),
          if (node.approvalRequired) ...[
            const SizedBox(height: 18),
            const _Notice(
              icon: FrankIcons.approval,
              text: 'Approval required by default for sensitive access.',
            ),
          ],
        ],
        if (node.kind == OrganizationNodeKind.role) ...[
          const SizedBox(height: 2),
          _InspectorLabel('Team role reference'),
          _ReferenceValue(value: node.roleId),
          const SizedBox(height: 16),
          const _Notice(
            icon: FrankIcons.user,
            text:
                'Role nodes are reusable worker templates. Agents are selected by the taskboard broker at claim time.',
          ),
        ],
        if (node.kind == OrganizationNodeKind.taskboard) ...[
          const SizedBox(height: 2),
          _InspectorLabel('Shared taskboard reference'),
          _ReferenceValue(value: node.taskboardId),
          const SizedBox(height: 16),
          const _Notice(
            icon: FrankIcons.dashboard,
            text:
                'Cards keep one stable id while they move between taskboards; direct agent-to-agent handoff is not used.',
          ),
        ],
        if (node.kind == OrganizationNodeKind.childWorkflow) ...[
          const SizedBox(height: 2),
          _InspectorLabel('Child workflow reference'),
          _ReferenceValue(value: node.childWorkflowId),
          const SizedBox(height: 16),
          const _Notice(
            icon: FrankIcons.workflow,
            text:
                'Child workflows are one-level composition/navigation. They do not create a second runtime or mailbox.',
          ),
        ],
        if (node.kind == OrganizationNodeKind.role ||
            node.kind == OrganizationNodeKind.taskboard ||
            node.kind == OrganizationNodeKind.childWorkflow) ...[
          if (node.inputPort != null || node.outputPort != null) ...[
            const SizedBox(height: 16),
            _InspectorLabel('Ports'),
            Text(
              '${node.inputPort ?? 'in'}  →  ${node.outputPort ?? 'out'}',
              style: const TextStyle(color: FrankColors.ink),
            ),
          ],
          if (node.reworkLimit != null) ...[
            const SizedBox(height: 16),
            _InspectorLabel('Rework limit'),
            Text(
              '${node.reworkLimit}',
              style: const TextStyle(color: FrankColors.ink),
            ),
          ],
        ],
        if (node.kind == OrganizationNodeKind.approval)
          const _Notice(
            icon: FrankIcons.approval,
            text:
                'Pauses the flow until a person reviews and approves the handoff.',
          ),
        const SizedBox(height: 22),
        _Notice(
          icon: FrankIcons.circleAlert,
          text: capability != null
              ? 'Only a connector profile reference is stored. Secrets, tokens, passwords, and connection strings never enter this graph.'
              : node.kind == OrganizationNodeKind.taskboard
              ? 'This is an internal routing board. It has no provider or credential profile.'
              : 'Runtime references are stored without secrets, tokens, passwords, or connection strings.',
        ),
      ],
    );
  }
}

class _ConnectorProfileSelector extends StatelessWidget {
  const _ConnectorProfileSelector({required this.node, required this.profiles});

  final OrganizationNode node;
  final List<ConnectorProfile> profiles;

  @override
  Widget build(BuildContext context) {
    if (profiles.isEmpty) {
      return const _Notice(
        icon: FrankIcons.cloudOffOutlined,
        text: 'No compatible connection profiles are available.',
      );
    }
    final compatible = profiles
        .where((profile) => _profileSupports(node.capability, profile.kind))
        .toList(growable: false);
    final selected =
        compatible.any((profile) => profile.id == node.connectorProfileId)
        ? node.connectorProfileId
        : null;
    return FSelect<String?>.rich(
      key: ValueKey('node-connection-profile-${node.id}'),
      control: FSelectControl<String?>.lifted(
        value: selected,
        onChange: (profileId) {
          final profile = compatible
              .where((candidate) => candidate.id == profileId)
              .firstOrNull;
          context.read<OrganizationBloc>().add(
            OrganizationNodeUpdated(
              node.copyWith(
                connectorProfileId: profileId,
                connectorProfileLabel: profile?.name,
                profileRef: profile?.id,
                configured:
                    profile != null &&
                    profile.health != ConnectorHealth.unhealthy,
              ),
            ),
          );
        },
      ),
      format: (value) => value == null
          ? 'Not selected'
          : compatible.firstWhere((profile) => profile.id == value).name,
      enabled: compatible.isNotEmpty,
      hint: compatible.isEmpty ? 'No compatible profile' : 'Choose a profile',
      children: [
        FSelectItem<String?>.item(
          value: null,
          title: const Text('Not selected'),
        ),
        for (final profile in compatible)
          FSelectItem<String?>.item(
            value: profile.id,
            title: Text(profile.name),
          ),
      ],
    );
  }
}

bool _profileSupports(
  OrganizationCapabilityKind? capability,
  ConnectorKind kind,
) => switch (capability) {
  OrganizationCapabilityKind.email ||
  OrganizationCapabilityKind.calendar ||
  OrganizationCapabilityKind.drive => kind == ConnectorKind.googleWorkspace,
  OrganizationCapabilityKind.browser => kind == ConnectorKind.browser,
  OrganizationCapabilityKind.terminal => kind == ConnectorKind.terminal,
  OrganizationCapabilityKind.database =>
    kind == ConnectorKind.postgres || kind == ConnectorKind.sqlite,
  null => false,
};

class _RelationInspector extends StatelessWidget {
  const _RelationInspector({required this.relation, required this.graph});

  final OrganizationRelation relation;
  final OrganizationGraph graph;

  @override
  Widget build(BuildContext context) {
    final source = graph.nodes
        .where((node) => node.id == relation.sourceNodeId)
        .firstOrNull;
    final target = graph.nodes
        .where((node) => node.id == relation.targetNodeId)
        .firstOrNull;
    if (source == null || target == null) {
      return const _Notice(
        icon: FrankIcons.circleAlert,
        text: 'This relation points to a node that no longer exists.',
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _InspectorLabel('Flow'),
        Text(
          '${source.label}  →  ${target.label}',
          style: const TextStyle(color: FrankColors.ink, fontSize: 13),
        ),
        const SizedBox(height: 22),
        if (relation.kind == OrganizationRelationKind.handoff) ...[
          _InspectorLabel('Input / artifact received'),
          _OrganizationTextField(
            semanticsLabel: 'Input / artifact received',
            initialValue: relation.contract.inputSummary,
            minLines: 2,
            maxLines: 3,
            onSubmit: (value) => _updateContract(
              context,
              relation.contract.copyWith(inputSummary: value),
            ),
          ),
          const SizedBox(height: 18),
          _InspectorLabel('Expected output'),
          _OrganizationTextField(
            semanticsLabel: 'Expected output',
            initialValue: relation.contract.expectedOutput,
            minLines: 2,
            maxLines: 3,
            onSubmit: (value) => _updateContract(
              context,
              relation.contract.copyWith(expectedOutput: value),
            ),
          ),
          const SizedBox(height: 18),
          _InspectorLabel('Context policy'),
          FSelect<OrganizationContextPolicy>.rich(
            control: FSelectControl<OrganizationContextPolicy>.lifted(
              value: relation.contract.contextPolicy,
              onChange: (policy) {
                if (policy != null) {
                  _updateContract(
                    context,
                    relation.contract.copyWith(contextPolicy: policy),
                  );
                }
              },
            ),
            format: (policy) => policy.label,
            label: const Text('Context policy'),
            children: [
              for (final policy in OrganizationContextPolicy.values)
                FSelectItem<OrganizationContextPolicy>.item(
                  value: policy,
                  title: Text(policy.label),
                ),
            ],
          ),
        ],
        if (relation.kind == OrganizationRelationKind.toolAccess) ...[
          _InspectorLabel('Permissions'),
          for (final permission in target.capability?.permissions ?? const [])
            FCheckbox(
              value: relation.permissions.contains(permission),
              label: Text(permission),
              onChange: (checked) {
                final permissions = [...relation.permissions];
                if (checked) {
                  if (!permissions.contains(permission)) {
                    permissions.add(permission);
                  }
                } else {
                  permissions.remove(permission);
                }
                context.read<OrganizationBloc>().add(
                  OrganizationRelationUpdated(
                    relation.copyWith(permissions: permissions),
                  ),
                );
              },
            ),
          if (target.approvalRequired)
            const _Notice(
              icon: FrankIcons.approval,
              text: 'This capability requires approval before use.',
            ),
        ],
        if (relation.kind == OrganizationRelationKind.review)
          const _Notice(
            icon: FrankIcons.approval,
            text: 'Review relations route work through a human checkpoint.',
          ),
      ],
    );
  }

  void _updateContract(
    BuildContext context,
    OrganizationHandoffContract contract,
  ) {
    context.read<OrganizationBloc>().add(
      OrganizationRelationUpdated(relation.copyWith(contract: contract)),
    );
  }
}

String organizationValidationSummary(OrganizationValidation validation) {
  final errors = validation.issues
      .where((issue) => issue.severity == OrganizationIssueSeverity.error)
      .length;
  final errorLabel = errors == 1 ? 'error' : 'errors';
  final warningLabel = validation.warningCount == 1 ? 'warning' : 'warnings';
  return '$errors $errorLabel · ${validation.warningCount} $warningLabel';
}
