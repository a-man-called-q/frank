part of 'package:frank_desktop/features/shell/main_sidebar.dart';

enum _AccountAction { changePassword, logoutAll, logout }

class MainSidebarContent extends StatelessWidget {
  static const fixedWidth = SidebarLayout.defaultWidth;

  const MainSidebarContent({
    this.width = SidebarLayout.defaultWidth,
    this.nativeSidebarEffect = false,
    this.showDemoBanner = false,
    required this.isFullscreen,
    required this.workspace,
    required this.workspaceName,
    required this.activeView,
    required this.settingsSection,
    required this.projects,
    required this.selectedProjectId,
    required this.selectedMissionId,
    required this.expandedProjectIds,
    required this.projectScope,
    required this.pinnedMissionIds,
    this.authRepository,
    this.onLogout,
    this.onLogoutAll,
    this.onChangePassword,
    required this.searchFocusNode,
    this.onToggleSidebar,
    this.onDoubleTap,
    required this.onSelectView,
    required this.onSelectSettingsSection,
    required this.onToggleProject,
    required this.onSelectMission,
    required this.onCreateMission,
    this.onAddProject,
    this.canMutate = true,
    this.mutationDisabledReason,
    this.mutationStatus = ProjectsMutationStatus.idle,
    this.mutationError,
    this.activeOperation,
    this.onRetryMutation,
    required this.onPinProject,
    required this.onRenameProject,
    required this.onArchiveProject,
    required this.onRemoveProject,
    required this.onPinMission,
    required this.onRenameMission,
    required this.onArchiveMission,
    required this.onSelectProjectScope,
    required this.onTogglePinnedMission,
    required this.onReorderPinnedMissions,
    super.key,
  });

  final double width;
  final bool nativeSidebarEffect;
  final bool showDemoBanner;
  final OfficeWorkspace workspace;
  final bool isFullscreen;
  final String workspaceName;
  final WorkspaceView activeView;
  final SettingsSection settingsSection;
  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final String? selectedMissionId;
  final Set<String> expandedProjectIds;
  final String? projectScope;
  final List<String> pinnedMissionIds;
  final AuthRepository? authRepository;
  final Future<void> Function()? onLogout;
  final Future<void> Function()? onLogoutAll;
  final Future<void> Function(String currentPassword, String newPassword)?
  onChangePassword;
  final FocusNode searchFocusNode;
  final VoidCallback? onToggleSidebar;
  final VoidCallback? onDoubleTap;
  final ValueChanged<WorkspaceView> onSelectView;
  final ValueChanged<SettingsSection> onSelectSettingsSection;
  final ValueChanged<String> onToggleProject;
  final void Function(String projectId, String missionId) onSelectMission;
  final ValueChanged<String> onCreateMission;
  final VoidCallback? onAddProject;
  final bool canMutate;
  final String? mutationDisabledReason;
  final ProjectsMutationStatus mutationStatus;
  final String? mutationError;
  final ProjectOperation? activeOperation;
  final VoidCallback? onRetryMutation;
  final ValueChanged<String> onPinProject;
  final ValueChanged<String> onRenameProject;
  final ValueChanged<String> onArchiveProject;
  final ValueChanged<String> onRemoveProject;
  final void Function(String projectId, String missionId) onPinMission;
  final void Function(String projectId, String missionId) onRenameMission;
  final void Function(String projectId, String missionId) onArchiveMission;
  final ValueChanged<String?> onSelectProjectScope;
  final ValueChanged<String> onTogglePinnedMission;
  final ValueChanged<List<String>> onReorderPinnedMissions;

  @override
  Widget build(BuildContext context) {
    final sidebarView =
        activeView == WorkspaceView.office ||
            settingsSection == SettingsSection.taskboard ||
            settingsSection == SettingsSection.journal
        ? WorkspaceView.office
        : WorkspaceView.settings;
    final navigation = _GlobalNavigation(
      view: sidebarView,
      activeView: activeView,
      settingsSection: settingsSection,
      onSelectView: onSelectView,
      onSelectSettingsSection: onSelectSettingsSection,
    );

    return LayoutBuilder(
      builder: (context, constraints) {
        final shortHeight =
            constraints.maxHeight.isFinite && constraints.maxHeight < 360;
        // There is no useful room for the inbox below the traffic-light/header
        // chrome. Hiding its scrollable body keeps tiny windows free of
        // a RenderFlex overflow while the shell remains navigable.
        final inbox =
            activeView == WorkspaceView.office &&
                !shortHeight &&
                constraints.maxHeight >= 600
            ? WorkInboxPane(
                workspace: workspace,
                selectedProjectId: selectedProjectId,
                selectedMissionId: selectedMissionId,
                projectScope: projectScope,
                pinnedMissionIds: pinnedMissionIds,
                searchFocusNode: searchFocusNode,
                onProjectScopeChanged: onSelectProjectScope,
                onSelectProject: onSelectProjectScope,
                onSelectMission: onSelectMission,
                onSelectAgent: () =>
                    onSelectSettingsSection(SettingsSection.team),
                onTogglePinnedMission: onTogglePinnedMission,
                onReorderPinnedMissions: onReorderPinnedMissions,
                onCreateMission: onCreateMission,
                onAddProject: onAddProject,
                canMutate: canMutate,
                mutationDisabledReason: mutationDisabledReason,
                mutationStatus: mutationStatus,
                mutationError: mutationError,
                activeOperation: activeOperation,
                onRetryMutation: onRetryMutation,
                onRenameMission: onRenameMission,
                onArchiveMission: onArchiveMission,
              )
            : null;
        final sidebar = inbox == null
            ? FSidebar(
                style: _frankSidebarStyle(
                  width,
                  nativeSidebarEffect: nativeSidebarEffect,
                ),
                header: _SidebarHeader(
                  dense: shortHeight,
                  isFullscreen: isFullscreen,
                  workspaceName: workspaceName,
                  activeView: sidebarView,
                  showDemoBanner: showDemoBanner,
                  connectedServer: authRepository?.serverUrl.isNotEmpty == true
                      ? authRepository!.serverUrl
                      : null,
                  onSelectView: onSelectView,
                ),
                children: [navigation],
                footer: _SidebarFooter(
                  dense: shortHeight,
                  authRepository: authRepository,
                  onLogout: onLogout,
                  onLogoutAll: onLogoutAll,
                  onChangePassword: onChangePassword,
                ),
              )
            : FSidebar.raw(
                style: _frankSidebarStyle(
                  width,
                  nativeSidebarEffect: nativeSidebarEffect,
                ),
                header: _SidebarHeader(
                  dense: shortHeight,
                  isFullscreen: isFullscreen,
                  workspaceName: workspaceName,
                  activeView: sidebarView,
                  showDemoBanner: showDemoBanner,
                  connectedServer: authRepository?.serverUrl.isNotEmpty == true
                      ? authRepository!.serverUrl
                      : null,
                  onSelectView: onSelectView,
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxHeight: constraints.maxHeight < 760 ? 220 : 320,
                      ),
                      child: SingleChildScrollView(child: navigation),
                    ),
                    const SizedBox(height: 8),
                    Expanded(child: inbox),
                  ],
                ),
                footer: _SidebarFooter(
                  dense: shortHeight,
                  authRepository: authRepository,
                  onLogout: onLogout,
                  onLogoutAll: onLogoutAll,
                  onChangePassword: onChangePassword,
                ),
              );

        return SizedBox(width: width, child: sidebar);
      },
    );
  }
}
