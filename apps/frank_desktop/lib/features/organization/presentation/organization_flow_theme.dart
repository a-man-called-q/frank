import 'package:flutter/widgets.dart';
import 'package:vyuh_node_flow/vyuh_node_flow.dart';

import '../../../app/theme.dart';

/// The minimap uses the same quiet surfaces as the sidebar. Vyuh's dark
/// preset is intentionally blue; keeping this local prevents that default
/// from becoming a second visual language in Organization.
MinimapTheme frankOrganizationMinimapTheme() => const MinimapTheme(
  backgroundColor: FrankColors.panel,
  nodeColor: FrankColors.muted,
  viewportColor: FrankColors.aubergineAccent,
  viewportFillOpacity: .14,
  viewportBorderOpacity: .8,
  borderColor: FrankColors.border,
  borderWidth: 1,
  borderRadius: FrankUiTokens.panelRadius,
  padding: EdgeInsets.all(6),
  nodeBorderRadius: 3,
);

NodeFlowTheme frankOrganizationFlowTheme() => NodeFlowTheme.dark.copyWith(
  backgroundColor: const Color(0x00000000),
  nodeTheme: NodeTheme.dark.copyWith(
    backgroundColor: FrankColors.panel,
    selectedBackgroundColor: FrankColors.panelRaised,
    highlightBackgroundColor: FrankColors.panelRaised,
    borderColor: FrankColors.border,
    selectedBorderColor: FrankColors.aubergineAccent,
    highlightBorderColor: FrankColors.aubergineAccent,
    borderWidth: FrankUiTokens.borderWidth,
    selectedBorderWidth: FrankUiTokens.borderWidth,
    borderRadius: const BorderRadius.all(
      Radius.circular(FrankUiTokens.panelRadius),
    ),
    titleStyle: const TextStyle(
      color: FrankColors.ink,
      fontSize: FrankUiTokens.textSize,
      fontWeight: FontWeight.w600,
    ),
    contentStyle: const TextStyle(color: FrankColors.muted, fontSize: 11),
  ),
  connectionTheme: ConnectionTheme.dark.copyWith(
    style: ConnectionStyles.smoothstep,
    color: FrankColors.muted,
    selectedColor: FrankColors.aubergineAccent,
    highlightColor: FrankColors.aubergineAccent,
    strokeWidth: 2,
    selectedStrokeWidth: 3,
    dashPattern: null,
    endPoint: ConnectionEndPoint.triangle,
    portExtension: 18,
  ),
  temporaryConnectionTheme: ConnectionTheme.dark.copyWith(
    style: ConnectionStyles.smoothstep,
    color: FrankColors.aubergineAccent,
    selectedColor: FrankColors.aubergineAccent,
    dashPattern: const [5, 5],
    endPoint: ConnectionEndPoint.triangle,
  ),
  gridTheme: GridTheme.dark.copyWith(
    color: FrankColors.border.withValues(alpha: .6),
    size: 20,
    thickness: .85,
    style: GridStyles.dots,
  ),
  portTheme: PortTheme.dark.copyWith(
    color: FrankColors.muted,
    connectedColor: FrankColors.aubergineAccent,
    highlightColor: FrankColors.aubergineAccent,
    size: const Size.square(8),
  ),
  selectionTheme: SelectionTheme.dark.copyWith(
    color: const Color(0x1AB27A9D),
    borderColor: FrankColors.aubergineAccent,
  ),
);
