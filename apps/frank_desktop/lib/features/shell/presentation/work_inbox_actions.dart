part of 'work_inbox.dart';

class _MissionActions extends StatelessWidget {
  const _MissionActions({
    required this.entry,
    required this.onTogglePinned,
    required this.onOpenMenu,
    super.key,
  });

  final SidebarMissionEntry entry;
  final VoidCallback onTogglePinned;
  final VoidCallback onOpenMenu;

  @override
  Widget build(BuildContext context) {
    final mission = entry.mission;
    return Align(
      alignment: Alignment.centerRight,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 28,
            height: 28,
            child: IconButton(
              onPressed: onTogglePinned,
              tooltip: entry.pinned
                  ? 'Unpin task ${mission.title} from the pinned list'
                  : 'Pin task ${mission.title} to the pinned list',
              icon: Icon(
                FrankIcons.pin,
                size: 14,
                color: entry.pinned
                    ? FrankColors.aubergineAccent
                    : FrankColors.muted.withValues(alpha: 0.6),
              ),
              padding: EdgeInsets.zero,
              visualDensity: VisualDensity.compact,
              constraints: const BoxConstraints.tightFor(width: 28, height: 28),
            ),
          ),
          SizedBox(
            width: 28,
            height: 28,
            child: Semantics(
              button: true,
              label: 'Task actions for ${mission.title}',
              child: IconButton(
                onPressed: onOpenMenu,
                tooltip: 'Open actions for ${mission.title}',
                icon: const Icon(FrankIcons.more, size: 15),
                padding: EdgeInsets.zero,
                constraints: const BoxConstraints.tightFor(
                  width: 28,
                  height: 28,
                ),
                visualDensity: VisualDensity.compact,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _PassiveMissionSignals extends StatelessWidget {
  const _PassiveMissionSignals({required this.entry, super.key});

  final SidebarMissionEntry entry;

  @override
  Widget build(BuildContext context) {
    final signals = <Widget>[];
    final mission = entry.mission;
    if (mission.pendingApprovalCount > 0) {
      signals.add(
        Semantics(
          container: true,
          label: '${mission.pendingApprovalCount} approvals pending',
          excludeSemantics: true,
          child: Container(
            constraints: const BoxConstraints(minWidth: 20, minHeight: 20),
            padding: const EdgeInsets.symmetric(horizontal: 5),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: FrankColors.warningAmberSoft,
              borderRadius: BorderRadius.circular(6),
            ),
            child: Text(
              '${mission.pendingApprovalCount}',
              style: const TextStyle(
                color: FrankColors.warningAmber,
                fontSize: 10,
              ),
            ),
          ),
        ),
      );
    }
    if (entry.pinned) {
      if (signals.isNotEmpty) signals.add(const SizedBox(width: 4));
      signals.add(
        Semantics(
          container: true,
          label: 'Pinned position ${entry.pinnedPosition ?? 'unknown'}',
          excludeSemantics: true,
          child: const Icon(
            FrankIcons.pin,
            size: 14,
            color: FrankColors.aubergineAccent,
          ),
        ),
      );
    }
    if (signals.isEmpty) return const SizedBox.shrink();
    return Align(
      alignment: Alignment.centerRight,
      child: Row(mainAxisSize: MainAxisSize.min, children: signals),
    );
  }
}

class _SearchResults extends StatelessWidget {
  const _SearchResults({
    required this.controller,
    required this.results,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.onSelectProject,
    required this.onSelectMission,
    required this.onSelectAgent,
  });

  final ScrollController controller;
  final List<SidebarSearchItem> results;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final ValueChanged<String> onSelectProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final VoidCallback onSelectAgent;

  @override
  Widget build(BuildContext context) {
    if (results.isEmpty) {
      return const Center(
        child: Text(
          'No matches found.',
          style: TextStyle(color: FrankColors.muted, fontSize: 12),
        ),
      );
    }
    return _WorkInboxScrollRegion(
      controller: controller,
      child: ListView.separated(
        key: const ValueKey('search-results-scroll-view'),
        controller: controller,
        primary: false,
        padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
        itemCount: results.length,
        separatorBuilder: (_, _) =>
            const Divider(height: 1, color: FrankColors.border),
        itemBuilder: (context, index) {
          final result = results[index];
          final icon = switch (result.kind) {
            SidebarSearchKind.project => FrankIcons.folder,
            SidebarSearchKind.mission => FrankIcons.briefcase,
            SidebarSearchKind.agent => FrankIcons.user,
            SidebarSearchKind.message => FrankIcons.message,
          };
          return Material(
            color: Colors.transparent,
            child: ListTile(
              dense: true,
              contentPadding: const EdgeInsets.symmetric(horizontal: 4),
              leading: Icon(icon, size: 16, color: FrankColors.muted),
              title: Text(
                result.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.ink, fontSize: 12),
              ),
              subtitle: Text(
                result.subtitle,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: FrankColors.muted, fontSize: 10),
              ),
              selected:
                  result.projectId == selectedProjectId &&
                  result.missionId == selectedMissionId,
              onTap: () {
                if (result.missionId case final missionId?) {
                  onSelectMission(result.projectId!, missionId);
                } else if (result.projectId case final projectId?) {
                  onSelectProject(projectId);
                } else if (result.kind == SidebarSearchKind.agent) {
                  onSelectAgent();
                }
              },
            ),
          );
        },
      ),
    );
  }
}
