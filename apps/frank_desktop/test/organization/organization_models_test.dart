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
}
