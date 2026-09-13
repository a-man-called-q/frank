import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/features/organization/organization_graph_editor.dart';
import 'package:frank_desktop/features/organization/organization_history.dart';

OrganizationGraph _graph() => const OrganizationGraph(
  id: 'org',
  draftRevision: 1,
  publishedRevision: 1,
  nodes: [
    OrganizationNode(
      id: 'node',
      kind: OrganizationNodeKind.capability,
      label: 'Node',
      position: OrganizationPoint(0, 0),
    ),
  ],
  relations: [],
  groups: [
    OrganizationGroup(
      id: 'custom',
      label: 'Custom',
      position: OrganizationPoint(0, 0),
      size: OrganizationSize(500, 300),
    ),
  ],
);

void main() {
  test('graph editor snaps positions and recomputes membership', () {
    final next = const OrganizationGraphEditor().moveNode(
      _graph(),
      'node',
      const OrganizationPoint(21, 39),
    );

    expect(next.nodes.single.position, const OrganizationPoint(20, 40));
    expect(next.nodes.single.groupId, 'custom');
  });

  test('history keeps redo deterministic and bounds undo depth', () {
    final history = OrganizationHistory(maxDepth: 2);
    final first = _graph();
    final second = first.copyWith(draftRevision: 2);
    final third = first.copyWith(draftRevision: 3);
    history.record(first);
    history.record(second);
    history.record(third);

    final current = first.copyWith(draftRevision: 4);
    final undo = history.undo(current);
    expect(undo?.draftRevision, 3);
    final redo = history.redo(undo!);
    expect(redo?.draftRevision, 4);
    expect(history.canUndo, isTrue);
  });
}
