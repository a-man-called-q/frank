import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/presentation/shell_context_bar.dart';

const _employee = OfficeEmployee(
  id: 'ae',
  name: 'Maya Chen',
  role: 'Account Executive',
  status: 'Available',
  initials: 'MC',
  color: 0xFFE2A84B,
);

const _workspace = OfficeWorkspace(
  name: 'Frank Agency',
  projects: [],
  employees: [_employee],
  accountExecutive: _employee,
);

void main() {
  testWidgets('windowed macOS context content shares the traffic-light line', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(900, 200);
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildFrankTheme(
          Brightness.dark,
        ).copyWith(platform: TargetPlatform.macOS),
        home: Scaffold(
          body: ShellContextBar(
            workspace: _workspace,
            project: null,
            mission: null,
            sidebarVisible: false,
            isFullscreen: false,
            onToggleSidebar: () {},
          ),
        ),
      ),
    );

    expect(
      tester.getCenter(find.byTooltip('Show sidebar')).dy,
      closeTo(16, 0.5),
    );
    expect(tester.getTopLeft(find.text('Connected')).dy, lessThan(16));
    expect(tester.getSize(find.byType(ShellContextBar)).height, 32);
  });

  testWidgets('double-clicking the header invokes the window zoom action', (
    tester,
  ) async {
    var doubleTapped = 0;

    await tester.pumpWidget(
      MaterialApp(
        theme: buildFrankTheme(
          Brightness.dark,
        ).copyWith(platform: TargetPlatform.macOS),
        home: Scaffold(
          body: ShellContextBar(
            workspace: _workspace,
            project: null,
            mission: null,
            sidebarVisible: false,
            isFullscreen: false,
            onToggleSidebar: () {},
            onDoubleTap: () => doubleTapped++,
          ),
        ),
      ),
    );

    final header = tester.getRect(find.byType(ShellContextBar));
    final firstTap = await tester.startGesture(header.center);
    await firstTap.up();
    await tester.pump(const Duration(milliseconds: 60));
    final secondTap = await tester.startGesture(header.center);
    await secondTap.up();
    await tester.pump(const Duration(milliseconds: 60));
    await tester.pump();

    expect(doubleTapped, 1);
  });
}
