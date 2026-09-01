import '../../core/models/workspace_models.dart';

enum SidebarShelf { attention, pinned, draft, active, completed }

class SidebarMissionEntry {
  const SidebarMissionEntry({
    required this.project,
    required this.mission,
    required this.shelf,
    required this.pinned,
    this.pinnedPosition,
  });

  final OfficeProject project;
  final OfficeMission mission;
  final SidebarShelf shelf;
  final bool pinned;
  final int? pinnedPosition;
}

class SidebarShelfGroup {
  const SidebarShelfGroup({
    required this.shelf,
    required this.entries,
    required this.totalCount,
    required this.hasMore,
  });

  final SidebarShelf shelf;
  final List<SidebarMissionEntry> entries;
  final int totalCount;
  final bool hasMore;
}

enum SidebarSearchKind { project, mission, agent, message }

class SidebarSearchItem {
  const SidebarSearchItem({
    required this.kind,
    required this.title,
    required this.subtitle,
    this.projectId,
    this.missionId,
  });

  final SidebarSearchKind kind;
  final String title;
  final String subtitle;
  final String? projectId;
  final String? missionId;
}

abstract final class SidebarProjection {
  static List<SidebarShelfGroup> shelves({
    required List<OfficeProject> projects,
    String? projectId,
    List<String> pinnedMissionIds = const [],
    bool showAllCompleted = false,
  }) {
    final allowedProjects = projectId == null
        ? projects
        : projects.where((project) => project.id == projectId);
    final pinnedOrder = {
      for (final (index, id) in pinnedMissionIds.indexed) id: index,
    };
    final entries = <SidebarMissionEntry>[];

    for (final project in allowedProjects) {
      for (final mission in project.missions) {
        final shelf = _shelfFor(mission, pinnedOrder.containsKey(mission.id));
        if (shelf == null) continue;
        entries.add(
          SidebarMissionEntry(
            project: project,
            mission: mission,
            shelf: shelf,
            pinned: pinnedOrder.containsKey(mission.id),
            pinnedPosition: pinnedOrder[mission.id] == null
                ? null
                : pinnedOrder[mission.id]! + 1,
          ),
        );
      }
    }

    final groups = <SidebarShelfGroup>[];
    for (final shelf in SidebarShelf.values) {
      final group = entries.where((entry) => entry.shelf == shelf).toList();
      if (group.isEmpty) continue;
      group.sort((a, b) {
        if (shelf == SidebarShelf.pinned) {
          return (pinnedOrder[a.mission.id] ?? 1 << 30).compareTo(
            pinnedOrder[b.mission.id] ?? 1 << 30,
          );
        }
        return _updatedAt(b.mission).compareTo(_updatedAt(a.mission));
      });
      final totalCount = group.length;
      final visible = shelf == SidebarShelf.completed && !showAllCompleted
          ? group.take(5).toList(growable: false)
          : group;
      groups.add(
        SidebarShelfGroup(
          shelf: shelf,
          entries: List.unmodifiable(visible),
          totalCount: totalCount,
          hasMore:
              shelf == SidebarShelf.completed && totalCount > visible.length,
        ),
      );
    }
    return List.unmodifiable(groups);
  }

  static List<SidebarSearchItem> search({
    required OfficeWorkspace workspace,
    String? projectId,
    required String query,
  }) {
    final needle = query.trim().toLowerCase();
    if (needle.isEmpty) return const [];
    final results = <SidebarSearchItem>[];
    final projects = projectId == null
        ? workspace.projects
        : workspace.projects.where((project) => project.id == projectId);

    bool matches(String value) => value.toLowerCase().contains(needle);

    for (final project in projects) {
      if (matches(project.name) || matches(project.client)) {
        results.add(
          SidebarSearchItem(
            kind: SidebarSearchKind.project,
            title: project.name,
            subtitle: '${project.client} · ${project.statusLabel}',
            projectId: project.id,
          ),
        );
      }
      for (final message in project.messages) {
        if (matches(message.text)) {
          results.add(
            SidebarSearchItem(
              kind: SidebarSearchKind.message,
              title: message.text,
              subtitle: '${project.name} · Project conversation',
              projectId: project.id,
            ),
          );
        }
      }
      for (final mission in project.missions) {
        if (matches(mission.title) || matches(mission.statusLabel)) {
          results.add(
            SidebarSearchItem(
              kind: SidebarSearchKind.mission,
              title: mission.title,
              subtitle: '${project.name} · ${mission.statusLabel}',
              projectId: project.id,
              missionId: mission.id,
            ),
          );
        }
        for (final message in mission.messages) {
          if (matches(message.text)) {
            results.add(
              SidebarSearchItem(
                kind: SidebarSearchKind.message,
                title: message.text,
                subtitle: '${project.name} · ${mission.title}',
                projectId: project.id,
                missionId: mission.id,
              ),
            );
          }
        }
      }
    }
    for (final employee in workspace.employees) {
      if (matches(employee.name) || matches(employee.role)) {
        results.add(
          SidebarSearchItem(
            kind: SidebarSearchKind.agent,
            title: employee.name,
            subtitle: '${employee.role} · ${employee.status}',
          ),
        );
      }
    }
    return List.unmodifiable(results);
  }

  static SidebarShelf? _shelfFor(OfficeMission mission, bool pinned) {
    if (mission.needsAttention) return SidebarShelf.attention;
    if (pinned) return SidebarShelf.pinned;
    if (mission.isDraft) return SidebarShelf.draft;
    if (mission.isActive) return SidebarShelf.active;
    if (mission.isCompleted || mission.status == MissionStatus.cancelled) {
      return SidebarShelf.completed;
    }
    return null;
  }

  static DateTime _updatedAt(OfficeMission mission) =>
      mission.updatedAt ?? DateTime.fromMillisecondsSinceEpoch(0);
}
