import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/sidebar_projection.dart';

void main() {
  late OfficeWorkspace workspace;

  setUp(() async {
    workspace = await FixtureFrankGateway().loadWorkspace();
  });

  test('classifies attention before pinned and keeps each mission once', () {
    final groups = SidebarProjection.shelves(
      projects: workspace.projects,
      pinnedMissionIds: const ['northstar-discovery', 'northstar-dashboard'],
    );

    final discovery = groups
        .firstWhere((group) => group.shelf == SidebarShelf.attention)
        .entries
        .singleWhere((entry) => entry.mission.id == 'northstar-discovery');
    expect(discovery.mission.pendingApprovalCount, 1);
    expect(
      groups
          .expand((group) => group.entries)
          .where((entry) => entry.mission.id == 'northstar-discovery'),
      hasLength(1),
    );
    expect(
      groups
          .firstWhere((group) => group.shelf == SidebarShelf.pinned)
          .entries
          .map((entry) => entry.mission.id),
      ['northstar-dashboard'],
    );
  });

  test('filters all shelves by project scope and sorts by updated time', () {
    final groups = SidebarProjection.shelves(
      projects: workspace.projects,
      projectId: 'meridian-finance',
    );
    expect(
      groups
          .expand((group) => group.entries)
          .every((entry) => entry.project.id == 'meridian-finance'),
      isTrue,
    );
    expect(groups.first.shelf, SidebarShelf.attention);
    expect(
      groups.first.entries.single.mission.title,
      'Review approval controls',
    );
  });

  test('search finds projects, missions, messages, and agents', () {
    expect(
      SidebarProjection.search(
        workspace: workspace,
        query: 'northstar',
      ).any((result) => result.kind == SidebarSearchKind.project),
      isTrue,
    );
    expect(
      SidebarProjection.search(
        workspace: workspace,
        query: 'warehouse',
      ).any((result) => result.kind == SidebarSearchKind.mission),
      isTrue,
    );
    expect(
      SidebarProjection.search(
        workspace: workspace,
        query: 'maya',
      ).any((result) => result.kind == SidebarSearchKind.agent),
      isTrue,
    );
  });

  test('completed shelf paginates to five entries', () {
    final missions = List<OfficeMission>.generate(
      6,
      (index) => OfficeMission(
        id: 'done-$index',
        title: 'Done $index',
        status: MissionStatus.completed,
        updatedAt: DateTime.utc(2026, 1, index + 1),
        messages: const [],
      ),
    );
    final project = OfficeProject(
      id: 'project',
      name: 'Project',
      client: 'Client',
      status: ProjectStatus.delivered,
      progress: 1,
      team: const [],
      summary: 'Summary',
      messages: const [],
      missions: missions,
    );
    final groups = SidebarProjection.shelves(projects: [project]);
    expect(groups.single.entries, hasLength(5));
    expect(groups.single.hasMore, isTrue);
    expect(
      SidebarProjection.shelves(
        projects: [project],
        showAllCompleted: true,
      ).single.entries,
      hasLength(6),
    );
  });
}
