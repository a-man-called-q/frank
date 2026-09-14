import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/features/organization/presentation/organization_flow_adapter.dart';

void main() {
  test('organization graph serializes without Vyuh types', () {
    final graph = fixtureOrganizationGraph();
    final json = graph.toJson();
    final restored = OrganizationGraph.fromJson(json);

    expect(restored.toJson(), graph.toJson());
    expect(json.toString(), isNot(contains('Offset')));
    expect(json.toString(), isNot(contains('Node<')));
    expect(json.toString(), isNot(contains('Connection<')));
  });

  test('domain graph projects into controller without becoming canonical', () {
    final graph = fixtureOrganizationGraph();
    final projection = OrganizationFlowAdapter.project(
      graph,
      reducedMotion: true,
    );
    addTearDown(projection.controller.dispose);

    expect(projection.controller.nodes, hasLength(graph.nodes.length + 3));
    expect(
      projection.controller.connections,
      hasLength(graph.relations.length),
    );
    expect(
      projection.nodesById.keys,
      containsAll(graph.nodes.map((node) => node.id)),
    );
    expect(
      projection.controller.connections.map(
        (connection) => connection.data!.relation.id,
      ),
      containsAll(graph.relations.map((relation) => relation.id)),
    );
    expect(
      projection.controller.connections
          .where(
            (connection) =>
                connection.data!.relation.kind ==
                OrganizationRelationKind.toolAccess,
          )
          .every(
            (connection) => connection.style?.id == 'frank-dashed-smoothstep',
          ),
      isTrue,
    );
    expect(
      projection.controller.connections.every(
        (connection) => !connection.animated,
      ),
      isTrue,
    );
  });

  test(
    'custom groups round-trip while legacy node group JSON remains readable',
    () {
      const custom = OrganizationGroup(
        id: 'research',
        label: 'Research',
        position: OrganizationPoint(80, 100),
        size: OrganizationSize(520, 360),
        tone: OrganizationGroupTone.neutral,
      );
      final graph = fixtureOrganizationGraph().copyWith(
        groups: [...fixtureOrganizationGraph().groups, custom],
        nodes: [
          fixtureOrganizationGraph().nodes.first.copyWith(groupId: custom.id),
        ],
      );
      final restored = OrganizationGraph.fromJson(graph.toJson());
      expect(restored.groupById(custom.id), custom);
      expect(restored.nodes.single.groupId, custom.id);

      final legacyNode = Map<String, Object?>.from(graph.nodes.single.toJson())
        ..remove('groupId')
        ..['group'] = OrganizationGroup.clientServices.id;
      final legacyRestored = OrganizationNode.fromJson(legacyNode);
      expect(legacyRestored.groupId, OrganizationGroup.clientServices.id);
    },
  );

  test('taskboard wire shape never carries connector/profile fields', () {
    const node = OrganizationNode(
      id: 'board-node',
      kind: OrganizationNodeKind.taskboard,
      label: 'Inbox',
      position: OrganizationPoint(0, 0),
      taskboardId: 'taskboard-inbox',
      connectorProfileId: 'legacy-profile',
      connectorProfileLabel: 'Legacy provider',
      integrationRef: 'legacy-integration',
      profileRef: 'legacy-profile-ref',
      capability: OrganizationCapabilityKind.email,
    );
    final json = node.toJson();
    expect(json, {
      'id': 'board-node',
      'kind': 'taskboard',
      'label': 'Inbox',
      'position': {'x': 0, 'y': 0},
      'taskboard_id': 'taskboard-inbox',
    });

    final restored = OrganizationNode.fromJson({
      ...json,
      'connector_profile_id': 'stale-profile',
      'capability': 'email',
    });
    expect(restored.taskboardId, 'taskboard-inbox');
    expect(restored.connectorProfileId, isNull);
    expect(restored.capability, isNull);
  });

  test('retired approval nodes are not projected into the authoring graph', () {
    final graph = OrganizationGraph.fromJson({
      'id': 'remote',
      'draft_revision': 3,
      'published_revision': 2,
      'nodes': [
        {
          'id': 'approval',
          'kind': 'approval',
          'label': 'Old approval desk',
          'position': {'x': 0, 'y': 0},
        },
        {
          'id': 'role',
          'kind': 'role',
          'label': 'Worker',
          'role_id': 'role-worker',
          'position': {'x': 200, 'y': 0},
        },
      ],
      'relations': [
        {
          'id': 'retired-edge',
          'kind': 'review',
          'source_node_id': 'role',
          'target_node_id': 'approval',
        },
      ],
    });

    expect(graph.nodes.map((node) => node.id), ['role']);
    expect(graph.relations, isEmpty);
  });
}
