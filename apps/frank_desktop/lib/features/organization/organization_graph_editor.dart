import '../../core/models/organization_models.dart';

/// Pure organization graph geometry editor.
///
/// This class intentionally has no bloc, persistence, timers, or widget
/// dependencies. It centralizes the board's grid and containment rules while
/// preserving the value-object API used by the existing UI.
class OrganizationGraphEditor {
  const OrganizationGraphEditor({this.gridSize = 20});

  final double gridSize;

  OrganizationGraph moveNode(
    OrganizationGraph graph,
    String nodeId,
    OrganizationPoint position,
  ) => moveNodes(graph, {nodeId: position});

  OrganizationGraph moveNodes(
    OrganizationGraph graph,
    Map<String, OrganizationPoint> positions,
  ) {
    if (positions.isEmpty) return graph;
    final nextNodes = [
      for (final node in graph.nodes)
        if (positions[node.id] case final position?)
          _withMembership(graph, node, _snapPoint(position))
        else
          node,
    ];
    return graph.copyWith(nodes: nextNodes);
  }

  OrganizationGraph moveGroup(
    OrganizationGraph graph,
    String groupId,
    OrganizationPoint position, {
    Map<String, OrganizationPoint> nodePositions = const {},
  }) {
    final group = graph.groupById(groupId);
    if (group == null || group.isBuiltIn) return graph;
    final snapped = _snapPoint(position);
    final dx = snapped.x - group.position.x;
    final dy = snapped.y - group.position.y;
    final nextNodes = [
      for (final node in graph.nodes)
        if (node.groupId == group.id)
          node.copyWith(
            position: _snapPoint(
              nodePositions[node.id] ??
                  OrganizationPoint(node.position.x + dx, node.position.y + dy),
            ),
          )
        else
          node,
    ];
    return graph.copyWith(
      groups: _replaceGroup(graph.groups, group.copyWith(position: snapped)),
      nodes: nextNodes,
    );
  }

  OrganizationGraph resizeGroup(
    OrganizationGraph graph,
    String groupId,
    OrganizationPoint position,
    OrganizationSize size,
  ) {
    final group = graph.groupById(groupId);
    if (group == null || group.isBuiltIn) return graph;
    final resized = group.copyWith(
      position: _snapPoint(position),
      size: OrganizationSize(
        _snap(size.width.clamp(100, 2400).toDouble()),
        _snap(size.height.clamp(60, 1600).toDouble()),
      ),
    );
    final resizedGraph = graph.copyWith(
      groups: _replaceGroup(graph.groups, resized),
    );
    return resizedGraph.copyWith(
      nodes: [
        for (final node in graph.nodes)
          node.groupId == group.id
              ? node.copyWith(
                  groupId: _contains(resized, node)
                      ? group.id
                      : _smallestContainingGroup(
                          resizedGraph,
                          node,
                          excluding: group.id,
                        )?.id,
                )
              : node,
      ],
    );
  }

  OrganizationGraph setViewport(
    OrganizationGraph graph,
    OrganizationViewport viewport,
  ) => graph.copyWith(viewport: viewport);

  OrganizationPoint _snapPoint(OrganizationPoint point) =>
      OrganizationPoint(_snap(point.x), _snap(point.y));

  double _snap(double value) =>
      value.isFinite ? (value / gridSize).round() * gridSize : 0;

  OrganizationNode _withMembership(
    OrganizationGraph graph,
    OrganizationNode node,
    OrganizationPoint position,
  ) {
    final moved = node.copyWith(position: position);
    return moved.copyWith(groupId: _smallestContainingGroup(graph, moved)?.id);
  }

  List<OrganizationGroup> _replaceGroup(
    List<OrganizationGroup> groups,
    OrganizationGroup replacement,
  ) => [
    for (final group in groups)
      if (group.id == replacement.id) replacement else group,
  ];

  OrganizationGroup? _smallestContainingGroup(
    OrganizationGraph graph,
    OrganizationNode node, {
    String? excluding,
  }) {
    final left = node.position.x;
    final top = node.position.y;
    final right = left + _nodeWidth(node);
    final bottom = top + _nodeHeight(node);
    final containing = graph.groups.where((group) {
      if (group.id == excluding) return false;
      final groupRight = group.position.x + group.size.width;
      final groupBottom = group.position.y + group.size.height;
      return left >= group.position.x &&
          top >= group.position.y &&
          right <= groupRight &&
          bottom <= groupBottom;
    }).toList();
    if (containing.isEmpty) return null;
    containing.sort(
      (a, b) => (a.size.width * a.size.height).compareTo(
        b.size.width * b.size.height,
      ),
    );
    return containing.first;
  }

  bool _contains(OrganizationGroup group, OrganizationNode node) {
    final right = node.position.x + _nodeWidth(node);
    final bottom = node.position.y + _nodeHeight(node);
    return node.position.x >= group.position.x &&
        node.position.y >= group.position.y &&
        right <= group.position.x + group.size.width &&
        bottom <= group.position.y + group.size.height;
  }

  double _nodeWidth(OrganizationNode node) => switch (node.kind) {
    OrganizationNodeKind.staff => 220,
    OrganizationNodeKind.capability || OrganizationNodeKind.approval => 190,
    OrganizationNodeKind.role => 220,
    OrganizationNodeKind.taskboard => 210,
    OrganizationNodeKind.childWorkflow => 230,
  };

  double _nodeHeight(OrganizationNode node) => switch (node.kind) {
    OrganizationNodeKind.staff || OrganizationNodeKind.role => 126,
    OrganizationNodeKind.capability || OrganizationNodeKind.approval => 112,
    OrganizationNodeKind.taskboard => 118,
    OrganizationNodeKind.childWorkflow => 126,
  };
}
