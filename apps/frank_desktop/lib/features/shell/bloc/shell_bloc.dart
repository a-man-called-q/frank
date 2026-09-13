import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/workspace_models.dart';
import '../sidebar_layout.dart';
import 'shell_preferences.dart';

enum ShellLoadStatus { loading, ready, failure }

sealed class ShellDestination {
  const ShellDestination();
}

/// The project and mission inbox, chat, and retained office floor.
final class OfficeDestination extends ShellDestination {
  const OfficeDestination();
}

/// Configuration and operational surfaces grouped under Settings.
final class SettingsDestination extends ShellDestination {
  const SettingsDestination([this.section = SettingsSection.organization]);

  final SettingsSection section;
}

class ShellState {
  const ShellState({
    this.status = ShellLoadStatus.loading,
    this.workspace,
    this.destination = const OfficeDestination(),
    this.sidebarVisible = true,
    this.sidebarWidth = SidebarLayout.defaultWidth,
    this.preferencesStatus = ShellPreferencesStatus.loading,
    this.projectScope,
    this.pinnedMissionIds = const [],
    this.error,
  });

  final ShellLoadStatus status;
  final OfficeWorkspace? workspace;
  final ShellDestination destination;
  final bool sidebarVisible;
  final double sidebarWidth;
  final ShellPreferencesStatus preferencesStatus;
  final String? projectScope;
  final List<String> pinnedMissionIds;
  final String? error;

  /// Compatibility getter for callers that still use the old terminology.
  bool get sidebarCollapsed => !sidebarVisible;

  WorkspaceView get activeView => switch (destination) {
    OfficeDestination() => WorkspaceView.office,
    SettingsDestination() => WorkspaceView.settings,
  };

  SettingsSection? get settingsSection => switch (destination) {
    SettingsDestination(:final section) => section,
    _ => null,
  };

  bool get isLoading => status == ShellLoadStatus.loading;
  bool get hasError => status == ShellLoadStatus.failure;
  bool get isReady => status == ShellLoadStatus.ready && workspace != null;

  ShellState copyWith({
    ShellLoadStatus? status,
    OfficeWorkspace? workspace,
    ShellDestination? destination,
    bool? sidebarVisible,
    bool? sidebarCollapsed,
    double? sidebarWidth,
    ShellPreferencesStatus? preferencesStatus,
    Object? projectScope = _unset,
    List<String>? pinnedMissionIds,
    String? error,
    bool clearError = false,
  }) {
    return ShellState(
      status: status ?? this.status,
      workspace: workspace ?? this.workspace,
      destination: destination ?? this.destination,
      sidebarVisible:
          sidebarVisible ??
          (sidebarCollapsed == null ? this.sidebarVisible : !sidebarCollapsed),
      sidebarWidth: sidebarWidth ?? this.sidebarWidth,
      preferencesStatus: preferencesStatus ?? this.preferencesStatus,
      projectScope: identical(projectScope, _unset)
          ? this.projectScope
          : projectScope as String?,
      pinnedMissionIds: pinnedMissionIds ?? this.pinnedMissionIds,
      error: clearError ? null : error ?? this.error,
    );
  }

  static const _unset = Object();
}

sealed class ShellEvent {
  const ShellEvent();
}

final class ShellStarted extends ShellEvent {
  const ShellStarted();
}

final class ShellRetryRequested extends ShellEvent {
  const ShellRetryRequested();
}

final class ShellViewSelected extends ShellEvent {
  const ShellViewSelected(this.view);

  final WorkspaceView view;
}

final class ShellSettingsSectionSelected extends ShellEvent {
  const ShellSettingsSectionSelected(this.section);

  final SettingsSection section;
}

final class ShellSidebarToggled extends ShellEvent {
  const ShellSidebarToggled();
}

final class ShellSidebarResizeEnded extends ShellEvent {
  const ShellSidebarResizeEnded(this.rawWidth);

  final double rawWidth;
}

final class ShellProjectScopeChanged extends ShellEvent {
  const ShellProjectScopeChanged(this.projectId);

  final String? projectId;
}

final class ShellPinnedMissionOrderChanged extends ShellEvent {
  const ShellPinnedMissionOrderChanged(this.missionIds);

  final List<String> missionIds;
}

class ShellBloc extends Bloc<ShellEvent, ShellState> {
  ShellBloc({required WorkspaceGateway gateway, ShellPreferences? preferences})
    : _gateway = gateway,
      _preferences = preferences ?? SharedShellPreferences(),
      super(const ShellState()) {
    on<ShellStarted>((_, emit) => _loadWorkspace(emit));
    on<ShellRetryRequested>((_, emit) => _loadWorkspace(emit));
    on<ShellViewSelected>(_selectView);
    on<ShellSettingsSectionSelected>(_selectSettingsSection);
    on<ShellSidebarToggled>(_toggleSidebar);
    on<ShellSidebarResizeEnded>(_resizeSidebar);
    on<ShellProjectScopeChanged>(_changeProjectScope);
    on<ShellPinnedMissionOrderChanged>(_changePinnedMissionOrder);
  }

  final WorkspaceGateway _gateway;
  final ShellPreferences _preferences;
  int _loadGeneration = 0;

  Future<void> _loadWorkspace(Emitter<ShellState> emit) async {
    final generation = ++_loadGeneration;
    emit(
      state.copyWith(
        status: ShellLoadStatus.loading,
        preferencesStatus: ShellPreferencesStatus.loading,
        clearError: true,
      ),
    );
    final preferencesFuture = _readPreferences();
    try {
      final workspace = await _gateway.loadWorkspace();
      if (isClosed || generation != _loadGeneration) return;
      final (preferences, preferencesStatus) = await preferencesFuture;
      final scope =
          preferences.projectScope != null &&
              workspace.projects.any(
                (project) => project.id == preferences.projectScope,
              )
          ? preferences.projectScope
          : null;
      emit(
        state.copyWith(
          status: ShellLoadStatus.ready,
          workspace: workspace,
          sidebarVisible: preferences.sidebarVisible,
          sidebarWidth: preferences.sidebarWidth,
          preferencesStatus: preferencesStatus,
          projectScope: scope,
          pinnedMissionIds: preferences.pinnedMissionIds,
          clearError: true,
        ),
      );
    } on Object catch (error) {
      if (isClosed || generation != _loadGeneration) return;
      await preferencesFuture;
      emit(
        state.copyWith(
          status: ShellLoadStatus.failure,
          preferencesStatus: ShellPreferencesStatus.fallback,
          error: error.toString(),
        ),
      );
    }
  }

  Future<(ShellPreferenceSnapshot, ShellPreferencesStatus)>
  _readPreferences() async {
    try {
      final preferences = await _preferences.load();
      return (
        ShellPreferenceSnapshot(
          sidebarVisible: preferences.sidebarVisible,
          sidebarWidth: SidebarLayout.normalizeWidth(preferences.sidebarWidth),
          projectScope: preferences.projectScope,
          pinnedMissionIds: List.unmodifiable(preferences.pinnedMissionIds),
        ),
        ShellPreferencesStatus.ready,
      );
    } on Object {
      return (const ShellPreferenceSnapshot(), ShellPreferencesStatus.fallback);
    }
  }

  void _selectView(ShellViewSelected event, Emitter<ShellState> emit) {
    final destination = switch (event.view) {
      WorkspaceView.office => const OfficeDestination(),
      WorkspaceView.settings => const SettingsDestination(),
    };
    emit(state.copyWith(destination: destination, clearError: true));
  }

  void _selectSettingsSection(
    ShellSettingsSectionSelected event,
    Emitter<ShellState> emit,
  ) {
    emit(
      state.copyWith(
        destination: SettingsDestination(event.section),
        clearError: true,
      ),
    );
  }

  void _toggleSidebar(ShellSidebarToggled event, Emitter<ShellState> emit) {
    final visible = !state.sidebarVisible;
    emit(state.copyWith(sidebarVisible: visible));
    unawaited(_persist(() => _preferences.setSidebarVisible(visible)));
  }

  void _resizeSidebar(ShellSidebarResizeEnded event, Emitter<ShellState> emit) {
    if (SidebarLayout.shouldCollapse(event.rawWidth)) {
      emit(state.copyWith(sidebarVisible: false));
      unawaited(_persist(() => _preferences.setSidebarVisible(false)));
      return;
    }

    final width = SidebarLayout.normalizeWidth(event.rawWidth);
    emit(state.copyWith(sidebarVisible: true, sidebarWidth: width));
    unawaited(_persist(() => _preferences.setSidebarWidth(width)));
  }

  void _changeProjectScope(
    ShellProjectScopeChanged event,
    Emitter<ShellState> emit,
  ) {
    emit(state.copyWith(projectScope: event.projectId));
    unawaited(_persist(() => _preferences.setProjectScope(event.projectId)));
  }

  void _changePinnedMissionOrder(
    ShellPinnedMissionOrderChanged event,
    Emitter<ShellState> emit,
  ) {
    final ids = List<String>.unmodifiable(event.missionIds);
    emit(state.copyWith(pinnedMissionIds: ids));
    unawaited(_persist(() => _preferences.setPinnedMissionIds(ids)));
  }

  Future<void> _persist(Future<void> Function() operation) async {
    try {
      await operation();
    } on Object {
      // Layout preferences are best-effort and must never interrupt the UI.
    }
  }
}
