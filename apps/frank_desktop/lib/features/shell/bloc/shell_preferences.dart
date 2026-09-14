import 'package:shared_preferences/shared_preferences.dart';

import '../../../core/models/organization_models.dart';
import '../sidebar_layout.dart';

enum ShellPreferencesStatus { loading, ready, fallback }

class ShellPreferenceSnapshot {
  const ShellPreferenceSnapshot({
    this.sidebarVisible = true,
    this.sidebarWidth = SidebarLayout.defaultWidth,
    this.projectScope,
    this.pinnedMissionIds = const [],
    this.organizationViewMode = OrganizationViewMode.canvas,
  });

  final bool sidebarVisible;
  final double sidebarWidth;
  final String? projectScope;
  final List<String> pinnedMissionIds;
  final OrganizationViewMode organizationViewMode;
}

abstract interface class ShellPreferences {
  Future<ShellPreferenceSnapshot> load();

  Future<void> setSidebarVisible(bool visible);

  Future<void> setSidebarWidth(double width);

  Future<void> setProjectScope(String? projectId);

  Future<void> setPinnedMissionIds(List<String> missionIds);

  Future<void> setOrganizationViewMode(OrganizationViewMode mode);
}

/// Persists non-critical desktop layout preferences in the platform's small
/// key/value store. Workspace and mission data never lives here.
class SharedShellPreferences implements ShellPreferences {
  SharedShellPreferences({SharedPreferencesAsync? preferences})
    : _preferences = preferences;

  static const _visibleKey = 'frank.shell.sidebar.visible.v1';
  static const _widthKey = 'frank.shell.sidebar.width.v1';
  static const _scopeKey = 'frank.shell.scope.v1';
  static const _pinnedKey = 'frank.shell.pinned-missions.v1';
  static const _organizationViewKey = 'frank.shell.organization-view.v1';

  SharedPreferencesAsync? _preferences;

  SharedPreferencesAsync get _store =>
      _preferences ??= SharedPreferencesAsync();

  @override
  Future<ShellPreferenceSnapshot> load() async {
    final visible = await _store.getBool(_visibleKey);
    final width = await _store.getDouble(_widthKey);
    final scope = await _store.getString(_scopeKey);
    final pinned = await _store.getStringList(_pinnedKey);
    final organizationView = await _store.getString(_organizationViewKey);
    return ShellPreferenceSnapshot(
      sidebarVisible: visible ?? true,
      sidebarWidth: SidebarLayout.normalizeWidth(width),
      projectScope: scope,
      pinnedMissionIds: pinned ?? const [],
      organizationViewMode: organizationView == 'outline'
          ? OrganizationViewMode.outline
          : OrganizationViewMode.canvas,
    );
  }

  @override
  Future<void> setSidebarVisible(bool visible) =>
      _store.setBool(_visibleKey, visible);

  @override
  Future<void> setSidebarWidth(double width) =>
      _store.setDouble(_widthKey, SidebarLayout.normalizeWidth(width));

  @override
  Future<void> setProjectScope(String? projectId) async {
    if (projectId == null) {
      await _store.remove(_scopeKey);
    } else {
      await _store.setString(_scopeKey, projectId);
    }
  }

  @override
  Future<void> setPinnedMissionIds(List<String> missionIds) =>
      _store.setStringList(_pinnedKey, missionIds);

  @override
  Future<void> setOrganizationViewMode(OrganizationViewMode mode) =>
      _store.setString(_organizationViewKey, mode.name);
}

/// Deterministic storage for widget and BLoC tests.
class MemoryShellPreferences implements ShellPreferences {
  MemoryShellPreferences({
    ShellPreferenceSnapshot initial = const ShellPreferenceSnapshot(),
  }) : _snapshot = ShellPreferenceSnapshot(
         sidebarVisible: initial.sidebarVisible,
         sidebarWidth: SidebarLayout.normalizeWidth(initial.sidebarWidth),
         projectScope: initial.projectScope,
         pinnedMissionIds: List.unmodifiable(initial.pinnedMissionIds),
         organizationViewMode: initial.organizationViewMode,
       );

  ShellPreferenceSnapshot _snapshot;

  ShellPreferenceSnapshot get snapshot => _snapshot;

  Object? loadError;
  Object? writeError;

  @override
  Future<ShellPreferenceSnapshot> load() async {
    if (loadError != null) throw loadError!;
    return _snapshot;
  }

  @override
  Future<void> setSidebarVisible(bool visible) => _write(
    ShellPreferenceSnapshot(
      sidebarVisible: visible,
      sidebarWidth: _snapshot.sidebarWidth,
      projectScope: _snapshot.projectScope,
      pinnedMissionIds: _snapshot.pinnedMissionIds,
      organizationViewMode: _snapshot.organizationViewMode,
    ),
  );

  @override
  Future<void> setProjectScope(String? projectId) => _write(
    ShellPreferenceSnapshot(
      sidebarVisible: _snapshot.sidebarVisible,
      sidebarWidth: _snapshot.sidebarWidth,
      projectScope: projectId,
      pinnedMissionIds: _snapshot.pinnedMissionIds,
      organizationViewMode: _snapshot.organizationViewMode,
    ),
  );

  @override
  Future<void> setPinnedMissionIds(List<String> missionIds) => _write(
    ShellPreferenceSnapshot(
      sidebarVisible: _snapshot.sidebarVisible,
      sidebarWidth: _snapshot.sidebarWidth,
      projectScope: _snapshot.projectScope,
      pinnedMissionIds: List.unmodifiable(missionIds),
      organizationViewMode: _snapshot.organizationViewMode,
    ),
  );

  @override
  Future<void> setSidebarWidth(double width) => _write(
    ShellPreferenceSnapshot(
      sidebarVisible: _snapshot.sidebarVisible,
      sidebarWidth: SidebarLayout.normalizeWidth(width),
      projectScope: _snapshot.projectScope,
      pinnedMissionIds: _snapshot.pinnedMissionIds,
      organizationViewMode: _snapshot.organizationViewMode,
    ),
  );

  @override
  Future<void> setOrganizationViewMode(OrganizationViewMode mode) => _write(
    ShellPreferenceSnapshot(
      sidebarVisible: _snapshot.sidebarVisible,
      sidebarWidth: _snapshot.sidebarWidth,
      projectScope: _snapshot.projectScope,
      pinnedMissionIds: _snapshot.pinnedMissionIds,
      organizationViewMode: mode,
    ),
  );

  Future<void> _write(ShellPreferenceSnapshot next) async {
    if (writeError != null) throw writeError!;
    _snapshot = next;
  }
}
