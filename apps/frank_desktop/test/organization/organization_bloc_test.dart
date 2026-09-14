import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/features/organization/bloc/organization_bloc.dart';

import '../support/fake_gateway.dart';

void main() {
  test(
    'loads lazily, coalesces a move, undoes, redoes, and autosaves',
    () async {
      final gateway = FakeGateway(organization: fixtureOrganizationGraph());
      final bloc = OrganizationBloc(
        gateway: gateway,
        autosaveDelay: const Duration(milliseconds: 5),
      );
      addTearDown(bloc.close);

      expect(gateway.organizationLoadCalls, 0);
      bloc.add(const OrganizationStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == OrganizationLoadStatus.ready,
      );
      expect(gateway.organizationLoadCalls, 1);

      final original = bloc.state.graph!.nodes.first.position;
      bloc.add(
        const OrganizationNodeMoved('staff-maya', OrganizationPoint(80, 100)),
      );
      await bloc.stream.firstWhere((state) => state.canUndo);
      expect(bloc.state.graph!.nodes.first.position.x, 80);

      bloc.add(const OrganizationUndoRequested());
      await bloc.stream.firstWhere((state) => state.canRedo);
      expect(bloc.state.graph!.nodes.first.position.x, original.x);

      bloc.add(const OrganizationRedoRequested());
      await bloc.stream.firstWhere(
        (state) => state.graph!.nodes.first.position.x == 80,
      );
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus == OrganizationPersistenceStatus.clean,
      );
      expect(gateway.organizationSaves, isNotEmpty);
    },
  );

  test('canonical move state keeps the 20 px grid contract', () async {
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
    );
    addTearDown(bloc.close);

    bloc.add(const OrganizationStarted());
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == OrganizationLoadStatus.ready,
    );
    bloc.add(
      const OrganizationNodeMoved('staff-maya', OrganizationPoint(83, 107)),
    );
    await bloc.stream.firstWhere((state) => state.canUndo);

    final moved = bloc.state.graph!.nodes.firstWhere(
      (node) => node.id == 'staff-maya',
    );
    expect(moved.position.x, 80);
    expect(moved.position.y, 100);
  });

  test(
    'custom groups persist geometry, membership, and one-step undo',
    () async {
      final bloc = OrganizationBloc(
        gateway: FakeGateway(organization: fixtureOrganizationGraph()),
      );
      addTearDown(bloc.close);

      bloc.add(const OrganizationStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == OrganizationLoadStatus.ready,
      );
      const group = OrganizationGroup(
        id: 'research',
        label: 'Research',
        position: OrganizationPoint(40, 40),
        size: OrganizationSize(420, 300),
      );
      bloc.add(const OrganizationGroupAdded(group));
      await bloc.stream.firstWhere(
        (state) => state.graph!.groupById('research') != null,
      );
      expect(bloc.state.selectedGroupId, 'research');

      bloc.add(
        OrganizationGroupUpdated(group.copyWith(label: 'Applied Research')),
      );
      await bloc.stream.firstWhere(
        (state) =>
            state.graph!.groupById('research')?.label == 'Applied Research',
      );

      final maya = bloc.state.graph!.nodes.firstWhere(
        (node) => node.id == 'staff-maya',
      );
      bloc.add(OrganizationNodeUpdated(maya.copyWith(groupId: 'research')));
      await bloc.stream.firstWhere(
        (state) =>
            state.graph!.nodes
                .firstWhere((node) => node.id == maya.id)
                .groupId ==
            'research',
      );

      bloc.add(
        const OrganizationGroupMoved('research', OrganizationPoint(200, 100)),
      );
      await bloc.stream.firstWhere(
        (state) => state.graph!.groupById('research')!.position.x == 200,
      );
      expect(
        bloc.state.graph!.nodes
            .firstWhere((node) => node.id == maya.id)
            .position,
        const OrganizationPoint(200, 140),
      );

      bloc.add(
        const OrganizationGroupResized(
          'research',
          OrganizationPoint(200, 100),
          OrganizationSize(600, 400),
        ),
      );
      await bloc.stream.firstWhere(
        (state) => state.graph!.groupById('research')!.size.width == 600,
      );
      bloc.add(const OrganizationGroupsDeleted(['research']));
      await bloc.stream.firstWhere(
        (state) => state.graph!.groupById('research') == null,
      );
      expect(
        bloc.state.graph!.nodes
            .firstWhere((node) => node.id == maya.id)
            .groupId,
        isNull,
      );
      expect(bloc.state.canUndo, isTrue);
    },
  );

  test('built-in groups reject geometry and label edits', () async {
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
    );
    addTearDown(bloc.close);

    bloc.add(const OrganizationStarted());
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == OrganizationLoadStatus.ready,
    );
    final builtIn = bloc.state.graph!.groupById(
      OrganizationGroup.clientServices.id,
    )!;

    bloc.add(
      OrganizationGroupMoved(builtIn.id, const OrganizationPoint(200, 200)),
    );
    await bloc.stream.firstWhere(
      (state) => state.error == 'Built-in groups cannot be moved.',
    );
    expect(bloc.state.graph!.groupById(builtIn.id), builtIn);

    bloc.add(OrganizationGroupUpdated(builtIn.copyWith(label: 'Renamed')));
    await bloc.stream.firstWhere(
      (state) => state.error == 'Built-in groups cannot be edited.',
    );
    expect(bloc.state.graph!.groupById(builtIn.id), builtIn);
  });

  test('persists a multi-select drag as one undo snapshot', () async {
    final gateway = FakeGateway(organization: fixtureOrganizationGraph());
    final bloc = OrganizationBloc(
      gateway: gateway,
      autosaveDelay: const Duration(milliseconds: 5),
    );
    addTearDown(bloc.close);

    bloc.add(const OrganizationStarted());
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == OrganizationLoadStatus.ready,
    );
    final graph = bloc.state.graph!;
    final first = graph.nodes.firstWhere((node) => node.id == 'staff-maya');
    final second = graph.nodes.firstWhere((node) => node.id == 'email-primary');
    bloc.add(
      OrganizationNodesMoved({
        first.id: OrganizationPoint(100, 120),
        second.id: OrganizationPoint(100, 280),
      }),
    );
    await bloc.stream.firstWhere((state) => state.canUndo);

    bloc.add(const OrganizationUndoRequested());
    await bloc.stream.firstWhere((state) => state.canRedo);
    expect(
      bloc.state.graph!.nodes
          .firstWhere((node) => node.id == first.id)
          .position
          .x,
      first.position.x,
    );
    expect(
      bloc.state.graph!.nodes
          .firstWhere((node) => node.id == second.id)
          .position
          .x,
      second.position.x,
    );
  });

  test(
    'failed save keeps draft and retry publishes after a successful save',
    () async {
      final gateway = FakeGateway(organization: fixtureOrganizationGraph())
        ..organizationSaveError = StateError('disk busy');
      final bloc = OrganizationBloc(
        gateway: gateway,
        autosaveDelay: const Duration(milliseconds: 5),
      );
      addTearDown(bloc.close);

      bloc.add(const OrganizationStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == OrganizationLoadStatus.ready,
      );
      final node = bloc.state.graph!.nodes.first;
      bloc.add(OrganizationNodeUpdated(node.copyWith(label: 'Maya Agency')));
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus ==
            OrganizationPersistenceStatus.saveFailure,
      );
      expect(bloc.state.graph!.nodes.first.label, 'Maya Agency');
      expect(bloc.state.canUndo, isTrue);

      gateway.organizationSaveError = null;
      bloc.add(const OrganizationRetryRequested());
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus == OrganizationPersistenceStatus.clean,
      );
      bloc.add(const OrganizationPublishRequested());
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus == OrganizationPersistenceStatus.published,
      );
      expect(gateway.organizationPublishes, hasLength(1));
      expect(bloc.state.graph!.publishedRevision, 2);
    },
  );

  test('publish conflict preserves draft and history', () async {
    final gateway = FakeGateway(organization: fixtureOrganizationGraph());
    final bloc = OrganizationBloc(
      gateway: gateway,
      autosaveDelay: const Duration(milliseconds: 5),
    );
    addTearDown(bloc.close);
    bloc.add(const OrganizationStarted());
    await bloc.stream.firstWhere(
      (state) => state.loadStatus == OrganizationLoadStatus.ready,
    );
    final node = bloc.state.graph!.nodes.first;
    bloc.add(OrganizationNodeUpdated(node.copyWith(label: 'Conflict draft')));
    await bloc.stream.firstWhere(
      (state) => state.persistenceStatus == OrganizationPersistenceStatus.clean,
    );
    gateway.organizationPublishError = const OrganizationRevisionConflict(1, 2);
    bloc.add(const OrganizationPublishRequested());
    await bloc.stream.firstWhere(
      (state) =>
          state.persistenceStatus ==
          OrganizationPersistenceStatus.publishFailure,
    );

    expect(bloc.state.graph!.nodes.first.label, 'Conflict draft');
    expect(bloc.state.canUndo, isTrue);
    expect(bloc.state.error, contains('expected revision 1'));
  });

  test(
    'retry rebases a published revision conflict without losing the draft',
    () async {
      final gateway = FakeGateway(organization: fixtureOrganizationGraph())
        ..organizationPublishError = const OrganizationRevisionConflict(1, 2);
      final bloc = OrganizationBloc(
        gateway: gateway,
        autosaveDelay: const Duration(milliseconds: 5),
      );
      addTearDown(bloc.close);
      bloc.add(const OrganizationStarted());
      await bloc.stream.firstWhere(
        (state) => state.loadStatus == OrganizationLoadStatus.ready,
      );
      final node = bloc.state.graph!.nodes.first;
      bloc.add(OrganizationNodeUpdated(node.copyWith(label: 'Rebased draft')));
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus == OrganizationPersistenceStatus.clean,
      );

      bloc.add(const OrganizationPublishRequested());
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus ==
            OrganizationPersistenceStatus.publishFailure,
      );
      expect(bloc.state.graph!.publishedRevision, 2);
      gateway.organizationPublishError = null;
      gateway.publishedOrganization = bloc.state.graph!.copyWith(
        publishedRevision: 2,
      );
      bloc.add(const OrganizationRetryRequested());
      await bloc.stream.firstWhere(
        (state) =>
            state.persistenceStatus == OrganizationPersistenceStatus.published,
      );
      expect(bloc.state.graph!.nodes.first.label, 'Rebased draft');
      expect(bloc.state.graph!.publishedRevision, 3);
    },
  );
}
