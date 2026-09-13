import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../../app/icons.dart';
import '../../../app/controls/frank_desktop_menu.dart';
import '../../../app/theme.dart';
import '../../../core/models/workspace_models.dart';
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
                  Tooltip(
                    message: scopeProject == null
                        ? 'Select a project to enable task creation'
                        : 'Create a new task in ${scopeProject.name}',
                    child: IconButton(
                      onPressed: scopeProject == null
                          ? null
                          : () => widget.onCreateMission(scopeProject.id),
                      icon: const Icon(FrankIcons.plus, size: 17),
                      color: FrankColors.muted,
                      visualDensity: VisualDensity.compact,
                      constraints: const BoxConstraints.tightFor(
                        width: 30,
                        height: 30,
                      ),
                    ),
                  ),
                ],
              ),
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
        _searchScrollController.offset != 0) {
      _searchScrollController.jumpTo(0);
    }
  }

  void _rememberMissionScrollOffset() {
    if (_missionScrollController.hasClients) {
      _missionScrollOffset = _missionScrollController.offset;
    }
  }

  void _restoreMissionScrollPosition() {
    if (_missionScrollOffset <= 0 || _missionScrollRestoreScheduled) return;
    _missionScrollRestoreScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _missionScrollRestoreScheduled = false;
      if (!mounted || !_missionScrollController.hasClients) return;
      final position = _missionScrollController.position;
      final offset = _missionScrollOffset.clamp(0.0, position.maxScrollExtent);
      if (position.pixels != offset) {
        _missionScrollController.jumpTo(offset);
      }
    });
  }
}
