import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/organization_models.dart';

void main() {
  test(
    'fixture gateway keeps draft and published snapshots separate',
    () async {
      final gateway = FixtureFrankGateway(latency: Duration.zero);
      final initial = await gateway.loadOrganization();
      final edited = initial.copyWith(
        nodes: [
          for (final node in initial.nodes)
            node.id == 'staff-maya'
                ? node.copyWith(label: 'Maya Agency')
                : node,
        ],
      );

      final saved = await gateway.saveOrganizationDraft(edited);
      expect(saved.draftRevision, initial.draftRevision + 1);
      expect(saved.publishedRevision, initial.publishedRevision);
      expect(
        (await gateway.loadOrganization()).nodes.first.label,
        'Maya Agency',
      );

      final published = await gateway.publishOrganization(
        saved,
        expectedPublishedRevision: initial.publishedRevision,
      );
      expect(published.publishedRevision, initial.publishedRevision + 1);
    },
  );

  test('fixture gateway rejects a stale published revision', () async {
    final gateway = FixtureFrankGateway(latency: Duration.zero);
    final graph = await gateway.loadOrganization();

    expect(
      () => gateway.publishOrganization(
        graph,
        expectedPublishedRevision: graph.publishedRevision + 1,
      ),
      throwsA(isA<OrganizationRevisionConflict>()),
    );
  });
}
