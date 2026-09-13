import 'package:bloc_test/bloc_test.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/projects/bloc/projects_bloc.dart';

void main() {
  late final workspace = FixtureFrankGateway().loadWorkspace();

  blocTest<ProjectsBloc, ProjectsState>(
    'initializes the first project, mission, and expansion',
    build: ProjectsBloc.new,
    act: (bloc) async => bloc.add(ProjectsInitialized(await workspace)),
    expect: () => [
      predicate<ProjectsState>(
        (state) =>
            state.isReady &&
            state.selectedProjectId == 'northstar-inventory' &&
            state.selectedMissionId == 'northstar-discovery' &&
            state.expandedProjectIds.contains('northstar-inventory'),
      ),
    ],
  );

  blocTest<ProjectsBloc, ProjectsState>(
    'parent toggle changes visibility without changing active mission',
    build: ProjectsBloc.new,
    act: (bloc) async {
      bloc.add(ProjectsInitialized(await workspace));
      await Future<void>.delayed(const Duration(milliseconds: 1));
      bloc.add(const ProjectToggled('northstar-inventory'));
    },
    expect: () => [
      predicate<ProjectsState>((state) => state.isReady),
      predicate<ProjectsState>(
        (state) =>
            !state.expandedProjectIds.contains('northstar-inventory') &&
            state.selectedMissionId == 'northstar-discovery',
      ),
    ],
  );

  blocTest<ProjectsBloc, ProjectsState>(
    'mission selection restores the parent and remembers the mission',
    build: ProjectsBloc.new,
    act: (bloc) async {
      bloc.add(ProjectsInitialized(await workspace));
      await Future<void>.delayed(const Duration(milliseconds: 1));
      bloc.add(
        const MissionSelected(
          projectId: 'meridian-finance',
          missionId: 'meridian-controls',
        ),
      );
      await Future<void>.delayed(const Duration(milliseconds: 1));
      bloc.add(const OfficeViewEntered());
    },
    expect: () => [
      predicate<ProjectsState>((state) => state.isReady),
      predicate<ProjectsState>(
        (state) =>
            state.selectedProjectId == 'meridian-finance' &&
            state.selectedMissionId == 'meridian-controls' &&
            state.expandedProjectIds.contains('meridian-finance'),
      ),
      predicate<ProjectsState>(
        (state) =>
            state.selectedProjectId == 'meridian-finance' &&
            state.selectedMissionId == 'meridian-controls',
      ),
    ],
  );

  blocTest<ProjectsBloc, ProjectsState>(
    'action events emit notices without mutating fixture data',
    build: ProjectsBloc.new,
    act: (bloc) async {
      bloc.add(ProjectsInitialized(await workspace));
      await Future<void>.delayed(const Duration(milliseconds: 1));
      bloc.add(
        const ProjectRenameConfirmed(
          projectId: 'northstar-inventory',
          name: 'Renamed fixture',
        ),
      );
    },
    expect: () => [
      predicate<ProjectsState>((state) => state.isReady),
      predicate<ProjectsState>(
        (state) =>
            state.notice?.message.contains('Renamed fixture') == true &&
            state.projectById('northstar-inventory')?.name ==
                'Northstar Inventory',
      ),
    ],
  );

  test('empty workspace initializes without a selection', () {
    final bloc = ProjectsBloc();
    bloc.add(
      const ProjectsInitialized(
        OfficeWorkspace(
          name: 'Empty',
          projects: [],
          employees: [],
          accountExecutive: OfficeEmployee(
            id: 'ae',
            name: 'Maya',
            role: 'Account Executive',
            status: 'Available',
            initials: 'M',
            color: 0,
          ),
        ),
      ),
    );
    addTearDown(bloc.close);
    expectLater(
      bloc.stream,
      emits(
        predicate<ProjectsState>(
          (state) => state.isReady && state.selectedProjectId == null,
        ),
      ),
    );
  });
}
