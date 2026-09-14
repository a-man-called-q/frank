import 'package:flutter/widgets.dart';
import 'package:flutter_mobx/flutter_mobx.dart';
import 'package:vyuh_node_flow/vyuh_node_flow.dart';
// PathSegment is part of ConnectionStyle's public method signature, but
// vyuh_node_flow 0.31.0 does not re-export the type from its package barrel.
// Keep this narrow import in the adapter boundary; no Vyuh type escapes it.
// ignore: implementation_imports
import 'package:vyuh_node_flow/src/connections/styles/path_segments.dart';

import '../../../app/theme.dart';
import '../../../core/models/organization_models.dart';
import 'organization_flow_theme.dart';

const organizationHandoffColor = FrankColors.aubergineAccent;
const organizationToolColor = Color(0xFF6F91AD);
const organizationReviewColor = Color(0xFFB18A55);
const organizationRoleColor = Color(0xFF8B72B5);
const organizationTaskboardColor = Color(0xFF5F9F91);
const organizationChildWorkflowColor = Color(0xFF9B7A55);

String organizationGroupHeading(String label) {
  final trimmed = label.trim();
  if (trimmed.isEmpty || trimmed != trimmed.toUpperCase()) return label;
  return trimmed
      .split(' ')
      .map(
        (word) => word.isEmpty
            ? word
            : '${word[0]}${word.substring(1).toLowerCase()}',
      )
      .join(' ');
}

class OrganizationFlowNodeData {
  const OrganizationFlowNodeData.node(this.node) : group = null;
  const OrganizationFlowNodeData.group(this.group) : node = null;

  final OrganizationNode? node;
  final OrganizationGroup? group;
}

class OrganizationFlowRelationData {
  const OrganizationFlowRelationData(this.relation);

  final OrganizationRelation relation;
}

class OrganizationFlowProjection {
  OrganizationFlowProjection({
    required this.controller,
    required this.nodesById,
    required this.groupsById,
  });

  final NodeFlowController<
    OrganizationFlowNodeData,
    OrganizationFlowRelationData
  >
  controller;
  final Map<String, OrganizationNode> nodesById;
  final Map<String, OrganizationGroup> groupsById;
}

abstract final class OrganizationFlowAdapter {
  static OrganizationFlowProjection project(
    OrganizationGraph graph, {
    bool reducedMotion = false,
    bool wideLayout = true,
    double textScaleFactor = 1,
    void Function(String id, Offset position, Size size)? onGroupResized,
  }) {
    final domainNodes = {for (final node in graph.nodes) node.id: node};
    final domainGroups = {for (final group in graph.groups) group.id: group};
    final nodes = <Node<OrganizationFlowNodeData>>[
      for (final group in graph.groups) _groupNode(group),
      for (final node in graph.nodes)
        _flowNode(node, textScaleFactor: textScaleFactor),
    ];
    final connections = [
      for (final relation in graph.relations)
        if (domainNodes.containsKey(relation.sourceNodeId) &&
            domainNodes.containsKey(relation.targetNodeId))
          _flowConnection(relation, reducedMotion: reducedMotion),
    ];
    final controller =
        NodeFlowController<
          OrganizationFlowNodeData,
          OrganizationFlowRelationData
        >(
          initialViewport: GraphViewport(
            x: graph.viewport.x,
            y: graph.viewport.y,
            zoom: graph.viewport.zoom,
          ),
          config: NodeFlowConfig(
            showAttribution: false,
            minZoom: .35,
            maxZoom: 1.8,
            plugins: [
              AutoPanPlugin(),
              LodPlugin(enabled: true),
              MinimapPlugin(
                // A dense graph benefits from navigation on desktop, but the
                // minimap should not consume the compact sheet/canvas.
                visible: wideLayout && graph.nodes.length > 8,
                interactive: true,
                theme: frankOrganizationMinimapTheme(),
              ),
              SnapPlugin([GridSnapDelegate(gridSize: 20)], enabled: true),
              StatsPlugin(),
              if (onGroupResized != null)
                OrganizationGeometryPlugin(onResizeEnded: onGroupResized),
            ],
          ),
          nodes: nodes,
          connections: connections,
        );
    return OrganizationFlowProjection(
      controller: controller,
      nodesById: domainNodes,
      groupsById: domainGroups,
    );
  }

  static GroupNode<OrganizationFlowNodeData> _groupNode(
    OrganizationGroup group,
  ) => GroupNode<OrganizationFlowNodeData>(
    id: 'group-${group.id}',
    position: Offset(group.position.x, group.position.y),
    size: Size(group.size.width, group.size.height),
    title: group.label,
    data: OrganizationFlowNodeData.group(group),
    color: _groupToneColor(group.tone),
    behavior: GroupBehavior.bounds,
    locked: group.locked,
    preserveWhenEmpty: true,
    widgetBuilder: (context, node) => OrganizationGroupVisual(
      node: node as GroupNode<OrganizationFlowNodeData>,
    ),
  );

  static Node<OrganizationFlowNodeData> _flowNode(
    OrganizationNode node, {
    required double textScaleFactor,
  }) {
    final baseSize = switch (node.kind) {
      OrganizationNodeKind.staff => const Size(220, 126),
      OrganizationNodeKind.capability => const Size(190, 112),
      OrganizationNodeKind.approval => const Size(190, 112),
      OrganizationNodeKind.role => const Size(220, 126),
      OrganizationNodeKind.taskboard => const Size(210, 118),
      OrganizationNodeKind.childWorkflow => const Size(230, 126),
    };
    // Flow nodes have a deliberate fixed geometry so dragging and connecting
    // remain stable. Grow their vertical budget for accessibility text
    // scaling instead of allowing the compact cards to paint outside their
    // node bounds. At the default scale this is exactly the historical size.
    final scale = textScaleFactor.isFinite && textScaleFactor > 1
        ? textScaleFactor
        : 1.0;
    // Keep a little extra vertical breathing room once accessibility text is
    // larger than the card's historical fixed geometry. The canvas cards stay
    // unchanged at the default scale, while scaled labels remain inside their
    // flow-node bounds instead of painting into connections or neighboring
    // cards.
    final size = Size(
      baseSize.width,
      baseSize.height * scale + (scale > 1 ? 24 : 0),
    );
    final accent = switch (node.kind) {
      OrganizationNodeKind.staff => FrankColors.aubergineAccent,
      OrganizationNodeKind.capability => organizationToolColor,
      OrganizationNodeKind.approval => organizationReviewColor,
      OrganizationNodeKind.role => organizationRoleColor,
      OrganizationNodeKind.taskboard => organizationTaskboardColor,
      OrganizationNodeKind.childWorkflow => organizationChildWorkflowColor,
    };
    final isLegacyNode =
        node.kind == OrganizationNodeKind.staff ||
        node.kind == OrganizationNodeKind.capability;
    final theme = NodeTheme.dark.copyWith(
      backgroundColor: node.kind == OrganizationNodeKind.approval
          ? const Color(0xFF2B251B)
          : node.kind == OrganizationNodeKind.taskboard
          ? const Color(0xFF172A29)
          : FrankColors.panel,
      selectedBackgroundColor: node.kind == OrganizationNodeKind.approval
          ? const Color(0xFF352A1D)
          : node.kind == OrganizationNodeKind.taskboard
          ? const Color(0xFF1D3734)
          : const Color(0xFF222025),
      // Keep the established legacy editor treatment pixel-stable.  Workflow
      // nodes use their semantic accent so the v2 graph is scannable without
      // changing existing staff/capability/approval fixtures.
      borderColor: isLegacyNode
          ? FrankColors.border
          : accent.withValues(alpha: .62),
      selectedBorderColor: isLegacyNode ? FrankColors.aubergineAccent : accent,
      highlightBorderColor: FrankColors.aubergineAccent,
      borderWidth: FrankUiTokens.borderWidth,
      selectedBorderWidth: FrankUiTokens.borderWidth,
      borderRadius: const BorderRadius.all(
        Radius.circular(FrankUiTokens.panelRadius),
      ),
    );
    return Node<OrganizationFlowNodeData>(
      id: node.id,
      type: node.kind.name,
      position: Offset(node.position.x, node.position.y),
      size: size,
      data: OrganizationFlowNodeData.node(node),
      theme: theme,
      ports: [
        Port(
          id: 'in',
          name: 'Input',
          position: PortPosition.left,
          offset: Offset(0, size.height / 2),
          multiConnections: true,
          tooltip: 'Incoming work',
        ),
        Port(
          id: 'out',
          name: 'Output',
          position: PortPosition.right,
          offset: Offset(0, size.height / 2),
          multiConnections: true,
          tooltip: 'Outgoing work',
        ),
      ],
    );
  }

  static Color _groupToneColor(OrganizationGroupTone tone) => switch (tone) {
    // Group colors are deliberately reduced to one quiet accent. Status and
    // approval colors belong to content state, not large containers.
    OrganizationGroupTone.aubergine => FrankColors.aubergineAccent,
    OrganizationGroupTone.blue ||
    OrganizationGroupTone.amber => FrankColors.muted,
    OrganizationGroupTone.neutral => FrankColors.muted,
  };

  static Connection<OrganizationFlowRelationData> _flowConnection(
    OrganizationRelation relation, {
    required bool reducedMotion,
  }) {
    final color = switch (relation.kind) {
      OrganizationRelationKind.handoff => organizationHandoffColor,
      OrganizationRelationKind.toolAccess => organizationToolColor,
      OrganizationRelationKind.review => organizationReviewColor,
      OrganizationRelationKind.pickup => organizationTaskboardColor,
      OrganizationRelationKind.drop => organizationTaskboardColor,
      OrganizationRelationKind.rework => organizationChildWorkflowColor,
    };
    return Connection<OrganizationFlowRelationData>(
      id: relation.id,
      sourceNodeId: relation.sourceNodeId,
      sourcePortId: 'out',
      targetNodeId: relation.targetNodeId,
      targetPortId: 'in',
      data: OrganizationFlowRelationData(relation),
      style:
          relation.kind == OrganizationRelationKind.toolAccess ||
              relation.kind == OrganizationRelationKind.rework
          ? _dashedSmoothstepStyle
          : ConnectionStyles.smoothstep,
      color: color,
      selectedColor: color,
      strokeWidth:
          relation.kind == OrganizationRelationKind.review ||
              relation.kind == OrganizationRelationKind.rework
          ? 2.4
          : 2,
      selectedStrokeWidth: 3,
      endPoint: ConnectionEndPoint.triangle.copyWith(color: color),
      // The editor's animation controller is global: attaching an effect to
      // one edge would animate every edge continuously. Keep the baseline
      // graph still so reduced-motion users and the idle canvas remain calm.
      animated: false,
    );
  }
}

/// Vyuh exposes dash patterns on the shared connection theme. Organization
/// needs one relation type (tool access) to be dashed while handoffs and
/// reviews stay solid, so this adapter keeps the dash geometry local to that
/// connection style instead of leaking a graph-wide theme override.
final class _DashedSmoothstepConnectionStyle extends ConnectionStyle {
  const _DashedSmoothstepConnectionStyle();

  @override
  String get id => 'frank-dashed-smoothstep';

  @override
  String get displayName => 'Frank dashed smooth step';

  @override
  ({Offset start, List<PathSegment> segments}) createSegments(
    ConnectionPathParameters params,
  ) => ConnectionStyles.smoothstep.createSegments(params);

  @override
  Path buildPath(Offset start, List<PathSegment> segments) {
    final source = ConnectionStyles.smoothstep.buildPath(start, segments);
    final dashed = Path();
    const dashLength = 8.0;
    const gapLength = 5.0;
    for (final metric in source.computeMetrics()) {
      var distance = 0.0;
      var draw = true;
      while (distance < metric.length) {
        final length = draw ? dashLength : gapLength;
        final end = (distance + length).clamp(0.0, metric.length);
        if (draw && end > distance) {
          dashed.addPath(metric.extractPath(distance, end), Offset.zero);
        }
        distance = end;
        draw = !draw;
      }
    }
    return dashed;
  }

  @override
  double get defaultHitTolerance =>
      ConnectionStyles.smoothstep.defaultHitTolerance;
}

const _dashedSmoothstepStyle = _DashedSmoothstepConnectionStyle();

/// Bridges Vyuh's resize event back to the persisted organization graph.
///
/// The flow controller remains a projection. This plugin only reports a
/// completed group resize; the BLoC owns the durable geometry and history.
final class OrganizationGeometryPlugin extends NodeFlowPlugin {
  OrganizationGeometryPlugin({required this.onResizeEnded});

  final void Function(String id, Offset position, Size size) onResizeEnded;
  NodeFlowController? _controller;

  @override
  String get id => 'frank-organization-geometry';

  @override
  void attach(NodeFlowController controller) => _controller = controller;

  @override
  void detach() => _controller = null;

  @override
  void onEvent(GraphEvent event) {
    if (event case ResizeEnded(:final nodeId, :final finalSize)) {
      final node = _controller?.nodes[nodeId];
      if (node is GroupNode) {
        onResizeEnded(nodeId, node.visualPosition.value, finalSize);
      }
    }
  }
}

/// Neutral square container used for both built-in and custom groups.
///
/// Keep the color treatment in the thin header/border so a group gives the
/// board structure without becoming a large competing color field.
class OrganizationGroupVisual extends StatelessWidget {
  const OrganizationGroupVisual({super.key, required this.node});

  final GroupNode<OrganizationFlowNodeData> node;

  @override
  Widget build(BuildContext context) => Observer(
    builder: (_) {
      final group = node.data.group!;
      final accent = OrganizationFlowAdapter._groupToneColor(group.tone);
      final selected = node.isSelected;
      return DecoratedBox(
        decoration: BoxDecoration(
          color: FrankColors.panel.withValues(alpha: .74),
          border: Border.all(
            color: selected
                ? accent
                : FrankColors.border.withValues(alpha: .82),
            width: selected ? 2 : 1,
          ),
          borderRadius: BorderRadius.circular(FrankUiTokens.panelRadius),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              height: 28,
              padding: const EdgeInsets.symmetric(horizontal: 10),
              alignment: Alignment.centerLeft,
              color: FrankColors.panelRaised.withValues(alpha: .72),
              child: Row(
                children: [
                  Container(
                    width: 2,
                    height: 12,
                    color: accent.withValues(alpha: selected ? 1 : .78),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      key: ValueKey('organization-group-heading-${group.id}'),
                      organizationGroupHeading(group.label),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                        color: FrankColors.ink,
                        fontSize: 12,
                        fontWeight: FontWeight.w600,
                        letterSpacing: 0,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const Expanded(child: SizedBox()),
          ],
        ),
      );
    },
  );
}
