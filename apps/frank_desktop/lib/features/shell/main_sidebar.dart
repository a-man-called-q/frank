import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:forui/forui.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';
import '../projects/bloc/projects_bloc.dart';
import '../projects/presentation/project_dialogs.dart';
import 'bloc/shell_bloc.dart';
import 'presentation/frank_desktop_menu.dart';
import 'presentation/work_inbox.dart';
import 'sidebar_layout.dart';

part 'presentation/sidebar_content.dart';

const _treeMissionInkOpacity = 0.76;
const _treeSelectedInkOpacity = 0.92;

const _frankNoFocusOutline = FFocusedOutlineStyle(
  color: Colors.transparent,
  borderRadius: BorderRadius.zero,
  spacing: 0,
);

final _frankMenuTileStyle = FTileStyleDelta.delta(
  focusedOutlineStyle: () => _frankNoFocusOutline,
  padding: EdgeInsetsGeometryDelta.value(EdgeInsets.zero),
  contentStyle: FItemContentStyleDelta.delta(
    suffixedPadding: EdgeInsetsGeometryDelta.value(
      EdgeInsets.symmetric(horizontal: 12, vertical: 6),
    ),
    unsuffixedPadding: EdgeInsetsGeometryDelta.value(
      EdgeInsets.symmetric(horizontal: 12, vertical: 6),
    ),
    prefixIconSpacing: 10,
    suffixIconSpacing: 10,
    titleTextStyle: FVariantsDelta.delta([
      FVariantOperation.all(
        TextStyleDelta.delta(
          fontFamily: FrankTypography.uiFontFamily,
          fontFamilyFallback: FrankTypography.uiFontFallback,
          fontSize: FrankUiTokens.textSize + 2,
          height: 20 / 14,
        ),
      ),
    ]),
  ),
);

/// One menu style is shared by the ellipsis popovers and secondary-click menus.
/// Keeping the dimensions here prevents the two entry points from drifting apart.
final _frankMenuStyle = FPopoverMenuStyleDelta.delta(
  minWidth: 248,
  maxWidth: 248,
  popoverPadding: EdgeInsetsGeometryDelta.value(
    EdgeInsets.symmetric(vertical: 4),
  ),
  tileGroupStyle: FTileGroupStyleDelta.delta(
    childPadding: EdgeInsetsGeometryDelta.value(EdgeInsets.zero),
    tileStyles: FVariantsDelta.delta([
      FVariantOperation.all(_frankMenuTileStyle),
    ]),
  ),
);

// Office navigation uses the same quiet, compact visual language as the
// project inbox. Keeping this style scoped to the Office group prevents
// unrelated menu surfaces from inheriting its compact treatment.
final _frankOfficeNavigationStyle = FSidebarGroupStyleDelta.delta(
  padding: const EdgeInsetsDelta.value(EdgeInsets.symmetric(horizontal: 12)),
  headerPadding: const EdgeInsetsGeometryDelta.value(
    EdgeInsets.fromLTRB(4, 0, 4, 2),
  ),
  labelStyle: const TextStyleDelta.value(
    TextStyle(
      fontFamily: FrankTypography.uiFontFamily,
      fontFamilyFallback: FrankTypography.uiFontFallback,
      color: FrankColors.muted,
      fontSize: FrankUiTokens.textSize,
      fontWeight: FontWeight.w400,
      height: 16 / 12,
    ),
  ),
  childrenSpacing: 4,
  itemStyle: FSidebarItemStyleDelta.delta(
    textStyle: FVariantsDelta.delta([
      FVariantOperation.all(
        TextStyleDelta.delta(
          fontFamily: FrankTypography.uiFontFamily,
          fontFamilyFallback: FrankTypography.uiFontFallback,
          color: FrankColors.muted,
          fontSize: FrankUiTokens.textSize,
          fontWeight: FontWeight.w400,
          height: 16 / 12,
        ),
      ),
      FVariantOperation.exact({
        FTappableVariant.selected,
      }, TextStyleDelta.delta(color: FrankColors.ink)),
    ]),
    iconSpacing: 8,
    iconStyle: FVariantsDelta.delta([
      FVariantOperation.all(
        IconThemeDataDelta.delta(
          color: FrankColors.muted,
          size: FrankUiTokens.iconSize,
        ),
      ),
      FVariantOperation.exact({
        FTappableVariant.selected,
      }, IconThemeDataDelta.delta(color: FrankColors.ink)),
    ]),
    padding: const EdgeInsetsGeometryDelta.value(
      EdgeInsets.symmetric(horizontal: 6, vertical: 7),
    ),
    borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
    backgroundColor: FVariantsValueDelta.delta([
      FVariantValueDeltaOperation.all(Colors.transparent),
      FVariantValueDeltaOperation.exact({
        FTappableVariant.hovered,
      }, FrankColors.ink.withValues(alpha: FrankUiTokens.hoverInkOpacity)),
      FVariantValueDeltaOperation.exact({
        FTappableVariant.selected,
      }, FrankColors.ink.withValues(alpha: FrankUiTokens.selectedInkOpacity)),
      FVariantValueDeltaOperation.exact({
        FTappableVariant.pressed,
      }, FrankColors.ink.withValues(alpha: FrankUiTokens.selectedInkOpacity)),
    ]),
    focusedOutlineStyle: FFocusedOutlineStyleDelta.delta(
      color: Colors.transparent,
      spacing: 0,
    ),
  ),
);

FSidebarStyleDelta _frankSidebarStyle(
  double width, {
  required bool nativeSidebarEffect,
}) => FSidebarStyleDelta.delta(
  constraints: BoxConstraints.tightFor(width: width),
  decoration: DecorationDelta.value(
    BoxDecoration(
      color: nativeSidebarEffect
          ? FrankColors.sidebarGlass
          : FrankColors.sidebarSolid,
      border: Border(right: BorderSide(color: FrankColors.border)),
    ),
  ),
  groupStyle: FSidebarGroupStyleDelta.delta(
    focusedOutlineStyle: FFocusedOutlineStyleDelta.delta(
      color: Colors.transparent,
      spacing: 0,
    ),
    itemStyle: FSidebarItemStyleDelta.delta(
      focusedOutlineStyle: FFocusedOutlineStyleDelta.delta(
        color: Colors.transparent,
        spacing: 0,
      ),
    ),
  ),
);

const _frankMenuTitleStyle = TextStyle(
  fontFamily: FrankTypography.uiFontFamily,
  fontFamilyFallback: FrankTypography.uiFontFallback,
  fontSize: 14,
  height: 20 / 14,
);

Widget _frankMenuTitle(String title) => SizedBox(
  height: 20,
  child: Align(
    alignment: Alignment.centerLeft,
    child: Text(title, style: _frankMenuTitleStyle),
  ),
);

/// Tracks the last input modality explicitly instead of relying on
/// FocusManager.highlightMode, which can remain in traditional mode after a
/// pointer click on macOS.
class _TreeInputModality extends ChangeNotifier {
  bool _keyboard = false;

  bool get keyboard => _keyboard;

  void pointerDown() {
    if (!_keyboard) return;
    _keyboard = false;
    notifyListeners();
  }

  void keyDown() {
    if (_keyboard) return;
    _keyboard = true;
    notifyListeners();
  }
}

/// Connected sidebar boundary. It translates user gestures into feature
/// events and leaves the large widget tree below as a presentational view.
class MainSidebar extends StatelessWidget {
  const MainSidebar({
    required this.searchFocusNode,
    required this.isFullscreen,
    this.width,
    this.nativeSidebarEffect = false,
    super.key,
  });

  final FocusNode searchFocusNode;
  final bool isFullscreen;
  final double? width;
  final bool nativeSidebarEffect;

  @override
  Widget build(BuildContext context) {
    final shell = context.watch<ShellBloc>().state;
    final projectsState = context.watch<ProjectsBloc>().state;
    final workspace = shell.workspace;
    if (workspace == null) return const SizedBox.shrink();
    final sidebarWidth = SidebarLayout.contentWidthForPreview(
      width ?? shell.sidebarWidth,
    );

    return BlocListener<ProjectsBloc, ProjectsState>(
      listenWhen: (previous, current) =>
          previous.notice?.id != current.notice?.id && current.notice != null,
      listener: (context, state) {
        final notice = state.notice;
        if (notice == null) return;
        ScaffoldMessenger.of(context)
          ..hideCurrentSnackBar()
          ..showSnackBar(
            SnackBar(
              content: Text(notice.message),
              behavior: SnackBarBehavior.floating,
              width: 360,
              duration: const Duration(milliseconds: 2200),
            ),
          );
        context.read<ProjectsBloc>().add(ProjectsNoticeConsumed(notice.id));
      },
      child: LayoutBuilder(
        builder: (context, _) {
          return MainSidebarContent(
            width: sidebarWidth,
            nativeSidebarEffect: nativeSidebarEffect,
            isFullscreen: isFullscreen,
            workspace: workspace,
            workspaceName: workspace.name,
            activeView: shell.activeView,
            officeSection: shell.officeSection ?? OfficeSection.organization,
            projects: projectsState.projects,
            selectedProjectId: projectsState.selectedProjectId,
            selectedMissionId: projectsState.selectedMissionId,
            expandedProjectIds: projectsState.expandedProjectIds,
            projectScope: shell.projectScope,
            pinnedMissionIds: shell.pinnedMissionIds,
            searchFocusNode: searchFocusNode,
            onSelectView: (view) {
              context.read<ShellBloc>().add(ShellViewSelected(view));
              if (view == WorkspaceView.projects) {
                context.read<ProjectsBloc>().add(const ProjectsViewEntered());
              }
            },
            onSelectOfficeSection: (section) => context.read<ShellBloc>().add(
              ShellOfficeSectionSelected(section),
            ),
            onToggleProject: (projectId) =>
                context.read<ProjectsBloc>().add(ProjectToggled(projectId)),
            onSelectMission: (projectId, missionId) {
              context.read<ShellBloc>().add(
                const ShellViewSelected(WorkspaceView.projects),
              );
              context.read<ProjectsBloc>().add(
                MissionSelected(projectId: projectId, missionId: missionId),
              );
            },
            onCreateMission: (projectId) => context.read<ProjectsBloc>().add(
              MissionCreationRequested(projectId),
            ),
            onPinProject: (projectId) => context.read<ProjectsBloc>().add(
              ProjectPinRequested(projectId),
            ),
            onRenameProject: (projectId) =>
                _renameProject(context, projectsState, projectId),
            onArchiveProject: (projectId) =>
                _archiveProject(context, projectsState, projectId),
            onRemoveProject: (projectId) =>
                _removeProject(context, projectsState, projectId),
            onPinMission: (projectId, missionId) =>
                context.read<ProjectsBloc>().add(
                  MissionPinRequested(
                    projectId: projectId,
                    missionId: missionId,
                  ),
                ),
            onRenameMission: (projectId, missionId) =>
                _renameMission(context, projectsState, projectId, missionId),
            onArchiveMission: (projectId, missionId) =>
                _archiveMission(context, projectsState, projectId, missionId),
            onSelectProjectScope: (projectId) {
              context.read<ShellBloc>().add(
                ShellProjectScopeChanged(projectId),
              );
              if (projectId case final id?) {
                context.read<ProjectsBloc>().add(ProjectActivated(id));
              }
            },
            onTogglePinnedMission: (missionId) {
              final ids = [...shell.pinnedMissionIds];
              if (!ids.remove(missionId)) ids.insert(0, missionId);
              context.read<ShellBloc>().add(
                ShellPinnedMissionOrderChanged(ids),
              );
            },
            onReorderPinnedMissions: (ids) {
              final scopedIds = ids.toSet();
              final mergedIds = [
                ...ids,
                ...shell.pinnedMissionIds.where(
                  (id) => !scopedIds.contains(id),
                ),
              ];
              context.read<ShellBloc>().add(
                ShellPinnedMissionOrderChanged(mergedIds),
              );
            },
          );
        },
      ),
    );
  }

  Future<void> _renameProject(
    BuildContext context,
    ProjectsState state,
    String projectId,
  ) async {
    final project = state.projectById(projectId);
    if (project == null || !context.mounted) return;
    final name = await showDialog<String>(
      context: context,
      builder: (_) => RenameProjectDialog(projectName: project.name),
    );
    if (!context.mounted || name == null) return;
    context.read<ProjectsBloc>().add(
      ProjectRenameConfirmed(projectId: projectId, name: name),
    );
  }

  Future<void> _renameMission(
    BuildContext context,
    ProjectsState state,
    String projectId,
    String missionId,
  ) async {
    final mission = state.missionById(state.projectById(projectId), missionId);
    if (mission == null || !context.mounted) return;
    final name = await showDialog<String>(
      context: context,
      builder: (_) => RenameMissionDialog(missionTitle: mission.title),
    );
    if (!context.mounted || name == null) return;
    context.read<ProjectsBloc>().add(
      MissionRenameConfirmed(
        projectId: projectId,
        missionId: missionId,
        name: name,
      ),
    );
  }

  Future<void> _archiveProject(
    BuildContext context,
    ProjectsState state,
    String projectId,
  ) async {
    final project = state.projectById(projectId);
    if (project == null || !context.mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ProjectConfirmationDialog(
        title: 'Archive project?',
        message:
            '“${project.name}” will move to the archived project list when project actions are connected.',
        confirmLabel: 'Archive project',
      ),
    );
    if (!context.mounted || confirmed != true) return;
    context.read<ProjectsBloc>().add(ProjectArchiveConfirmed(projectId));
  }

  Future<void> _archiveMission(
    BuildContext context,
    ProjectsState state,
    String projectId,
    String missionId,
  ) async {
    final mission = state.missionById(state.projectById(projectId), missionId);
    if (mission == null || !context.mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ProjectConfirmationDialog(
        title: 'Archive task?',
        message:
            '“${mission.title}” will move to the archived task list when task actions are connected.',
        confirmLabel: 'Archive task',
      ),
    );
    if (!context.mounted || confirmed != true) return;
    context.read<ProjectsBloc>().add(
      MissionArchiveConfirmed(projectId: projectId, missionId: missionId),
    );
  }

  Future<void> _removeProject(
    BuildContext context,
    ProjectsState state,
    String projectId,
  ) async {
    final project = state.projectById(projectId);
    if (project == null || !context.mounted) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => ProjectConfirmationDialog(
        title: 'Remove project?',
        message:
            'This only removes “${project.name}” from Frank. It will not delete the repository, worktree, or any files.',
        confirmLabel: 'Remove project',
        destructive: true,
      ),
    );
    if (!context.mounted || confirmed != true) return;
    context.read<ProjectsBloc>().add(ProjectRemoveConfirmed(projectId));
  }
}
