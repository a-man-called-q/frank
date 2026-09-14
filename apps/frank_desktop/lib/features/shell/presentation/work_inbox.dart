import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:forui/forui.dart';

import '../../../app/icons.dart';
import '../../../app/office_ui.dart';
import '../../../app/theme.dart';
import '../../../core/models/project_models.dart';
import '../../../core/models/workspace_models.dart';
import '../../projects/bloc/projects_bloc.dart';
import '../sidebar_projection.dart';

part 'work_inbox_search.dart';
part 'work_inbox_scope.dart';
part 'work_inbox_shelves.dart';
part 'work_inbox_rows.dart';
part 'work_inbox_actions.dart';

class WorkInboxPane extends StatefulWidget {
  const WorkInboxPane({
    required this.workspace,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.projectScope,
    required this.pinnedMissionIds,
    required this.searchFocusNode,
    required this.onProjectScopeChanged,
    required this.onSelectProject,
    required this.onSelectMission,
    required this.onSelectAgent,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    required this.onCreateMission,
    this.onAddProject,
    this.canMutate = true,
    this.mutationDisabledReason,
    this.mutationStatus = ProjectsMutationStatus.idle,
    this.mutationError,
    this.activeOperation,
    this.onRetryMutation,
    required this.onRenameMission,
    required this.onArchiveMission,
    super.key,
  });

  final OfficeWorkspace workspace;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final String? projectScope;
  final List<String> pinnedMissionIds;
  final FocusNode searchFocusNode;
  final ValueChanged<String?> onProjectScopeChanged;
  final ValueChanged<String> onSelectProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final VoidCallback onSelectAgent;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;
  final ValueChanged<String> onCreateMission;
  final VoidCallback? onAddProject;
  final bool canMutate;
  final String? mutationDisabledReason;
  final ProjectsMutationStatus mutationStatus;
  final String? mutationError;
  final ProjectOperation? activeOperation;
  final VoidCallback? onRetryMutation;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;

  @override
  State<WorkInboxPane> createState() => _WorkInboxPaneState();
}

class _WorkInboxPaneState extends State<WorkInboxPane> {
  late final TextEditingController _searchController;
  late final ScrollController _missionScrollController;
  late final ScrollController _searchScrollController;
  bool _showAllCompleted = false;
  double _missionScrollOffset = 0;
  bool _missionScrollRestoreScheduled = false;

  @override
  void initState() {
    super.initState();
    _searchController = TextEditingController();
    _missionScrollController = ScrollController(
      debugLabel: 'Frank mission shelves',
      keepScrollOffset: false,
    );
    _searchScrollController = ScrollController(
      debugLabel: 'Frank work inbox search',
      keepScrollOffset: false,
    );
    _missionScrollController.addListener(_rememberMissionScrollOffset);
  }

  @override
  void dispose() {
    _searchController.dispose();
    _missionScrollController.dispose();
    _searchScrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = _searchController.text.trim();
    final searchResults = SidebarProjection.search(
      workspace: widget.workspace,
      projectId: widget.projectScope,
      query: query,
    );
    final groups = SidebarProjection.shelves(
      projects: widget.workspace.projects,
      projectId: widget.projectScope,
      pinnedMissionIds: widget.pinnedMissionIds,
      showAllCompleted: _showAllCompleted,
    );
    final scopeProject = _scopeProject;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _SearchField(
                controller: _searchController,
                focusNode: widget.searchFocusNode,
                onChanged: _onSearchChanged,
                onClear: _clearSearch,
              ),
              const SizedBox(height: 10),
              Row(
                children: [
                  Expanded(
                    child: _ScopeSelector(
                      projects: widget.workspace.projects,
                      selectedProjectId: widget.projectScope,
                      onChanged: widget.onProjectScopeChanged,
                    ),
                  ),
                  const SizedBox(width: 6),
                  FButton(
                    onPress: scopeProject == null || !widget.canMutate
                        ? null
                        : () => widget.onCreateMission(scopeProject.id),
                    semanticsLabel: !widget.canMutate
                        ? widget.mutationDisabledReason ??
                              'New mission unavailable while the server is offline'
                        : scopeProject == null
                        ? 'Select a project to enable New mission'
                        : 'New mission',
                    semanticsTooltip: !widget.canMutate
                        ? widget.mutationDisabledReason ??
                              'Reconnect before creating a mission'
                        : scopeProject == null
                        ? 'Select a project to enable New mission'
                        : 'New mission',
                    size: FButtonSizeVariant.sm,
                    prefix: const Icon(FrankIcons.plus, size: 15),
                    child: const Text('New mission'),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              FButton(
                key: const ValueKey('sidebar-add-project'),
                onPress: widget.canMutate ? widget.onAddProject : null,
                semanticsLabel: widget.canMutate
                    ? 'Add project'
                    : widget.mutationDisabledReason ??
                          'Add project unavailable',
                semanticsTooltip: widget.canMutate
                    ? 'Add project'
                    : widget.mutationDisabledReason ??
                          'Reconnect before adding a project',
                variant: FButtonVariant.outline,
                prefix: const Icon(FrankIcons.plus, size: 15),
                child: const Text('Add project'),
              ),
              if (widget.mutationStatus != ProjectsMutationStatus.idle) ...[
                const SizedBox(height: 10),
                FrankActionFeedback(
                  message: _mutationMessage,
                  tone: widget.mutationStatus == ProjectsMutationStatus.failure
                      ? FrankStatusTone.failure
                      : FrankStatusTone.working,
                  action:
                      widget.mutationStatus == ProjectsMutationStatus.failure &&
                          widget.onRetryMutation != null
                      ? FButton(
                          onPress: widget.onRetryMutation,
                          variant: FButtonVariant.ghost,
                          size: FButtonSizeVariant.sm,
                          child: const Text('Retry'),
                        )
                      : null,
                ),
              ],
            ],
          ),
        ),
        const SizedBox(height: 12),
        Expanded(
          child: query.isNotEmpty
              ? _SearchResults(
                  controller: _searchScrollController,
                  results: searchResults,
                  selectedProjectId: widget.selectedProjectId,
                  selectedMissionId: widget.selectedMissionId,
                  onSelectProject: (projectId) {
                    _clearSearch();
                    widget.onSelectProject(projectId);
                  },
                  onSelectMission: (projectId, missionId) {
                    _clearSearch();
                    widget.onSelectMission(projectId, missionId);
                  },
                  onSelectAgent: () {
                    _clearSearch();
                    widget.onSelectAgent();
                  },
                )
              : _ShelfList(
                  controller: _missionScrollController,
                  employees: widget.workspace.employees,
                  groups: groups,
                  selectedProjectId: widget.selectedProjectId,
                  selectedMissionId: widget.selectedMissionId,
                  onSelectMission: widget.onSelectMission,
                  onTogglePinnedMission: widget.onTogglePinnedMission,
                  onReorderPinnedMissions: widget.onReorderPinnedMissions,
                  onShowMoreCompleted: () =>
                      setState(() => _showAllCompleted = true),
                  onRenameMission: widget.onRenameMission,
                  onArchiveMission: widget.onArchiveMission,
                ),
        ),
      ],
    );
  }

  OfficeProject? get _scopeProject {
    for (final project in widget.workspace.projects) {
      if (project.id == widget.projectScope) return project;
    }
    return null;
  }

  String get _mutationMessage {
    final operation = widget.activeOperation;
    return switch (widget.mutationStatus) {
      ProjectsMutationStatus.registering => 'Registering project…',
      ProjectsMutationStatus.cloning =>
        operation == null
            ? 'Clone queued…'
            : 'Clone ${operation.status.name}: ${operation.phase}',
      ProjectsMutationStatus.creatingMission => 'Creating mission…',
      ProjectsMutationStatus.failure => _friendlyMutationFailure(),
      ProjectsMutationStatus.idle => '',
    };
  }

  String _friendlyMutationFailure() {
    final message = frankFriendlyError(
      widget.mutationError,
      fallback: 'Project change failed.',
    );
    final raw = widget.mutationError?.toString().toLowerCase() ?? '';
    if (raw.contains('supervisor') && raw.contains('model')) {
      return '$message Open Models & OpenRouter to choose a supervisor model.';
    }
    return message;
  }

  void _clearSearch() {
    if (_searchController.text.isEmpty) return;
    _searchController.clear();
    _resetSearchScroll();
    setState(() {});
    _restoreMissionScrollPosition();
  }

  void _onSearchChanged(String value) {
    _resetSearchScroll();
    setState(() {});
    if (value.trim().isEmpty) {
      _restoreMissionScrollPosition();
    }
  }

  void _resetSearchScroll() {
    if (_searchScrollController.hasClients &&
        _searchScrollController.positions.length == 1 &&
        _searchScrollController.offset != 0) {
      _searchScrollController.jumpTo(0);
    }
  }

  void _rememberMissionScrollOffset() {
    if (_missionScrollController.hasClients &&
        _missionScrollController.positions.length == 1) {
      _missionScrollOffset = _missionScrollController.offset;
    }
  }

  void _restoreMissionScrollPosition() {
    if (_missionScrollOffset <= 0 || _missionScrollRestoreScheduled) return;
    _missionScrollRestoreScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _missionScrollRestoreScheduled = false;
      if (!mounted ||
          !_missionScrollController.hasClients ||
          _missionScrollController.positions.length != 1) {
        return;
      }
      final position = _missionScrollController.position;
      final offset = _missionScrollOffset.clamp(0.0, position.maxScrollExtent);
      if (position.pixels != offset) {
        _missionScrollController.jumpTo(offset);
      }
    });
  }
}
