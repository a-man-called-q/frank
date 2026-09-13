part of 'work_inbox.dart';

class _ShelfList extends StatelessWidget {
  const _ShelfList({
    required this.controller,
    required this.employees,
    required this.groups,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.onSelectMission,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    required this.onShowMoreCompleted,
    required this.onRenameMission,
    required this.onArchiveMission,
  });

  final ScrollController controller;
  final List<OfficeEmployee> employees;
  final List<SidebarShelfGroup> groups;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final void Function(String projectId, String missionId) onSelectMission;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;
  final VoidCallback onShowMoreCompleted;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;

  @override
  Widget build(BuildContext context) {
    if (groups.isEmpty) {
      return const Center(
        child: Text(
          'No tasks yet',
          style: TextStyle(color: FrankColors.muted, fontSize: 12),
        ),
      );
    }
    return _WorkInboxScrollRegion(
      controller: controller,
      child: ListView(
        key: const ValueKey('mission-shelf-scroll-view'),
        controller: controller,
        primary: false,
        padding: const EdgeInsets.fromLTRB(12, 0, 12, 6),
        children: [
          for (var index = 0; index < groups.length; index++)
            _ShelfSection(
              employees: employees,
              group: groups[index],
              isLast: index == groups.length - 1,
              selectedProjectId: selectedProjectId,
              selectedMissionId: selectedMissionId,
              onSelectMission: onSelectMission,
              onTogglePinnedMission: onTogglePinnedMission,
              onReorderPinnedMissions: onReorderPinnedMissions,
              onShowMoreCompleted: onShowMoreCompleted,
              onRenameMission: onRenameMission,
              onArchiveMission: onArchiveMission,
            ),
        ],
      ),
    );
  }
}
