import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../app/layout/office_surface_frame.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';
import '../../core/auth/auth_repository.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/ledger_models.dart';
import '../../core/models/openrouter_models.dart';
import '../../core/models/team_models.dart';
import '../../core/models/workspace_models.dart';
import '../../core/models/workflow_models.dart';
import '../chat/account_chat.dart';
import '../chat/bloc/chat_bloc.dart';
import '../floor/office_scene_floor.dart';
import '../ledger/presentation/ledger_surface.dart';
import '../journal/journal_surface.dart';
import '../organization/bloc/organization_bloc.dart';
import '../organization/presentation/organization_surface.dart';
import '../openrouter/openrouter_surface.dart';
import '../projects/bloc/projects_bloc.dart';
import '../projects/presentation/project_dialogs.dart';
import '../team/presentation/team_surface.dart';
import '../taskboard/bloc/taskboard_bloc.dart';
import '../taskboard/presentation/taskboard_surface.dart';
import 'bloc/shell_bloc.dart';
import 'main_sidebar.dart';
import '../../app/controls/frank_desktop_menu.dart';
import 'presentation/shell_context_bar.dart';
import 'shortcut_registry.dart';
import 'sidebar_effect.dart';
import 'sidebar_layout.dart';
import 'window_chrome.dart';

part 'office_shell_coordinator.dart';
part 'office_shell_surfaces.dart';

/// Presentation mapping for Settings sections. Keeping this beside the shell
/// makes the scene preset and the surface constructor agree without putting
/// UI layout concerns into the domain model.
OfficeSurfaceMode settingsSurfaceModeForSection(SettingsSection section) =>
    switch (section) {
      SettingsSection.organization ||
      SettingsSection.taskboard => OfficeSurfaceMode.canvas,
      SettingsSection.team ||
      SettingsSection.ledger ||
      SettingsSection.models ||
      SettingsSection.journal => OfficeSurfaceMode.page,
    };

/// Composition root for the remote-only desktop shell.
///
/// The gateway is the only dependency that crosses into the application. The
/// three feature blocs are kept independent; this widget is the sole place
/// where their lifecycle and cross-feature context are coordinated.
class OfficeShell extends StatelessWidget {
  const OfficeShell({
    required this.gateway,
    this.showDemoBanner = false,
    this.authRepository,
    this.onLogout,
    this.onLogoutAll,
    this.onChangePassword,
    this.sidebarEffectBuilder,
    this.onStatusChanged,
    super.key,
  });

  final FrankGateway gateway;
  final bool showDemoBanner;
  final AuthRepository? authRepository;
  final Future<void> Function()? onLogout;
  final Future<void> Function()? onLogoutAll;
  final Future<void> Function(String currentPassword, String newPassword)?
  onChangePassword;
  final SidebarEffectBuilder? sidebarEffectBuilder;

  /// Reports shell bootstrap status to an enclosing surface such as login.
  ///
  /// The callback is optional so the regular shell composition remains
  /// unchanged for callers that do not coordinate a transition with it.
  final ValueChanged<ShellLoadStatus>? onStatusChanged;

  @override
  Widget build(BuildContext context) {
    return RepositoryProvider<OfficeSessionCache>(
      create: (_) => OfficeSessionCache(gateway),
      child: RepositoryProvider<FrankGateway>.value(
        value: gateway,
        child: MultiBlocProvider(
          providers: [
            BlocProvider(
              create: (_) =>
                  ShellBloc(gateway: gateway)..add(const ShellStarted()),
            ),
            BlocProvider(create: (_) => ProjectsBloc()),
            BlocProvider(create: (_) => ChatBloc(gateway: gateway)),
            BlocProvider(create: (_) => OrganizationBloc(gateway: gateway)),
            BlocProvider(create: (_) => TaskboardBloc(gateway: gateway)),
          ],
          child: _OfficeCoordinator(
            authRepository: authRepository,
            showDemoBanner: showDemoBanner,
            onLogout: onLogout,
            onLogoutAll: onLogoutAll,
            onChangePassword: onChangePassword,
            sidebarEffectBuilder: sidebarEffectBuilder,
            onStatusChanged: onStatusChanged,
          ),
        ),
      ),
    );
  }
}

/// Lazily caches feature projections at the shell composition root. Surfaces
/// can therefore share one profile request (and one identity projection)
/// while still keeping their loading and error states independent.
class OfficeSessionCache {
  OfficeSessionCache(this.gateway);

  final FrankGateway gateway;
  Future<List<TeamAgentProfile>>? _teamProfiles;
  Future<LedgerDashboardData>? _ledgerDashboard;
  Future<OpenRouterCatalog>? _openRouterCatalog;

  Future<List<TeamAgentProfile>> loadTeamProfiles({bool refresh = false}) {
    if (!refresh && _teamProfiles != null) return _teamProfiles!;
    return _cacheTeamProfiles(gateway.loadTeamProfiles());
  }

  Future<List<TeamAgentProfile>> _cacheTeamProfiles(
    Future<List<TeamAgentProfile>> request,
  ) {
    _teamProfiles = request;
    // A rejected request must not poison the shared cache. Keep a newer
    // refresh or mutation result intact if this request completes later.
    request.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {
        if (identical(_teamProfiles, request)) _teamProfiles = null;
      },
    );
    return request;
  }

  void invalidateTeamProfiles() => _teamProfiles = null;

  void updateTeamProfiles(List<TeamAgentProfile> profiles) {
    _teamProfiles = Future<List<TeamAgentProfile>>.value(profiles);
  }

  Future<LedgerDashboardData> loadLedgerDashboard() =>
      _ledgerDashboard ??= gateway.loadLedgerDashboard();

  Future<OpenRouterCatalog> loadOpenRouterCatalog({bool refresh = false}) {
    if (refresh) {
      return _openRouterCatalog = gateway.loadOpenRouterModels(refresh: true);
    }
    return _openRouterCatalog ??= gateway.loadOpenRouterModels();
  }
}
