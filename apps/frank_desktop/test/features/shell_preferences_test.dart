import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/shell/bloc/shell_preferences.dart';
import 'package:frank_desktop/features/shell/sidebar_layout.dart';

void main() {
  test('memory preferences round-trip visibility and inbox settings', () async {
    final preferences = MemoryShellPreferences();
    await preferences.setSidebarVisible(false);
    await preferences.setSidebarWidth(401);
    await preferences.setProjectScope('project-1');
    await preferences.setPinnedMissionIds(['mission-2', 'mission-1']);

    final snapshot = await preferences.load();
    expect(snapshot.sidebarVisible, isFalse);
    expect(snapshot.sidebarWidth, 401);
    expect(snapshot.projectScope, 'project-1');
    expect(snapshot.pinnedMissionIds, ['mission-2', 'mission-1']);
  });

  test(
    'sidebar width defaults and normalizes invalid or out-of-range values',
    () async {
      final preferences = MemoryShellPreferences();
      expect(
        (await preferences.load()).sidebarWidth,
        SidebarLayout.defaultWidth,
      );

      await preferences.setSidebarWidth(120);
      expect(preferences.snapshot.sidebarWidth, SidebarLayout.minWidth);

      await preferences.setSidebarWidth(999);
      expect(preferences.snapshot.sidebarWidth, SidebarLayout.maxWidth);

      await preferences.setSidebarWidth(double.nan);
      expect(preferences.snapshot.sidebarWidth, SidebarLayout.defaultWidth);

      final restored = MemoryShellPreferences(
        initial: const ShellPreferenceSnapshot(sidebarWidth: 120),
      );
      expect((await restored.load()).sidebarWidth, SidebarLayout.minWidth);
    },
  );

  test('memory preferences can model a storage failure', () async {
    final preferences = MemoryShellPreferences()
      ..loadError = StateError('offline');
    expect(preferences.load, throwsStateError);
  });
}
