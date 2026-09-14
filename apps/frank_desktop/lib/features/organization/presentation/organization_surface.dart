import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:vyuh_node_flow/vyuh_node_flow.dart';

import '../../../app/icons.dart';
import '../../../app/layout/office_surface_frame.dart';
import '../../../app/office_ui.dart';
import '../../../app/theme.dart';
import '../../../core/models/organization_models.dart';
import '../../../core/models/team_models.dart';
import '../../../core/models/workspace_models.dart';
import '../../../core/models/workflow_models.dart';
import '../bloc/organization_bloc.dart';
import '../organization_catalog.dart';
import 'organization_flow_adapter.dart';
import 'organization_flow_theme.dart';
import '../organization_validator.dart';

part 'organization_add_palette.dart';
part 'organization_canvas.dart';
part 'organization_editor.dart';
part 'organization_cards.dart';
part 'organization_toolbar.dart';
part 'organization_inspector.dart';
part 'organization_validation.dart';

class OrganizationSurface extends StatefulWidget {
  const OrganizationSurface({
    required this.workspace,
    this.profiles,
    this.isFixture = true,
    this.roles = const [],
    this.workflowProjection = const WorkflowProjection(),
    this.viewMode,
    this.onViewModeChanged,
    this.canMutate = true,
    this.mutationDisabledReason,
    super.key,
  });

  final OfficeWorkspace workspace;
  final List<TeamAgentProfile>? profiles;
  final bool isFixture;
  final List<TeamRoleSummary> roles;
  final WorkflowProjection workflowProjection;
  final OrganizationViewMode? viewMode;
  final ValueChanged<OrganizationViewMode>? onViewModeChanged;
  final bool canMutate;
  final String? mutationDisabledReason;

  @override
  State<OrganizationSurface> createState() => _OrganizationSurfaceState();
}
