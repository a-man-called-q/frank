import 'package:flutter/widgets.dart';
import 'package:flutter/foundation.dart';
import 'package:forui/forui.dart';
import '../support/frank_test_app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/presentation/shell_context_bar.dart';

const _employee = OfficeEmployee(
  id: 'ae',
  name: 'Maya Chen',
  role: 'Account Executive',
  status: 'Available',
  initials: 'MC',
  color: 0xFF9A68A5,
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

    debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
    await tester.pumpWidget(
      FrankTestApp(
        home: SizedBox.expand(
          child: Align(
            alignment: Alignment.topLeft,
            child: ShellContextBar(
              workspace: _workspace,
              project: null,
              mission: null,
              sidebarVisible: false,
              isFullscreen: false,
              onToggleSidebar: () {},
            ),
          ),
        ),
      ),
    );
    debugDefaultTargetPlatformOverride = null;

    expect(
      tester.getCenter(find.bySemanticsLabel('Show the workspace sidebar')).dy,
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
      FrankTestApp(
        home: FScaffold(
          child: ShellContextBar(
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

  testWidgets(
    'toggle button stays in fixed position and context title slides when sidebarVisible changes',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(900, 200);
      addTearDown(tester.view.reset);

      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      await tester.pumpWidget(
        FrankTestApp(
          home: SizedBox.expand(
            child: Align(
              alignment: Alignment.topLeft,
              child: ShellContextBar(
                workspace: _workspace,
                project: null,
                mission: null,
                sidebarVisible: false,
                isFullscreen: false,
                onToggleSidebar: () {},
              ),
            ),
          ),
        ),
      );
      debugDefaultTargetPlatformOverride = null;

      final closedTogglePos = tester.getTopLeft(
        find.bySemanticsLabel('Show the workspace sidebar'),
      );
      expect(closedTogglePos.dx, 76.0);
      expect(closedTogglePos.dy, 0.0);
      expect(find.bySemanticsLabel('Hide the workspace sidebar'), findsNothing);
      expect(tester.getTopLeft(find.text('Frank Agency')).dx, 135.0);

      final toggleGlyph = tester.getRect(find.byIcon(FrankIcons.panelOpen));
      final logo = tester.getRect(find.bySemanticsLabel('Frank'));
      final title = tester.getRect(find.text('Frank Agency'));
      expect(logo.left - toggleGlyph.right, closeTo(8.0, 1.0));
      expect(title.left - logo.right, closeTo(8.0, 1.0));

      debugDefaultTargetPlatformOverride = TargetPlatform.macOS;
      await tester.pumpWidget(
        FrankTestApp(
          home: SizedBox.expand(
            child: Align(
              alignment: Alignment.topLeft,
              child: ShellContextBar(
                workspace: _workspace,
                project: null,
                mission: null,
                sidebarVisible: true,
                sidebarWidth: 264.0,
                isFullscreen: false,
                onToggleSidebar: () {},
              ),
            ),
          ),
        ),
      );
      debugDefaultTargetPlatformOverride = null;
      await tester.pumpAndSettle();

      final openTogglePos = tester.getTopLeft(
        find.bySemanticsLabel('Hide the workspace sidebar'),
      );
      expect(openTogglePos.dx, 76.0);
      expect(openTogglePos.dy, 0.0);
      expect(find.bySemanticsLabel('Show the workspace sidebar'), findsNothing);
      expect(tester.getTopLeft(find.text('Frank Agency')).dx, 278.0);
      expect(
        tester.getCenter(find.text('Frank Agency')).dy,
        closeTo(16.0, 1.0),
      );
    },
  );
}
