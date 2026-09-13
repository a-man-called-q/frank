import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/features/shell/main_sidebar.dart';
import 'package:frank_desktop/features/shell/sidebar_layout.dart';

void main() {
  testWidgets('drag reflows the sidebar immediately and snaps partial widths', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final slot = find.byKey(const ValueKey('sidebar-slot'));
    final handle = find.bySemanticsLabel('Resize sidebar');
    expect(tester.getSize(slot).width, SidebarLayout.defaultWidth);
    expect(tester.getSize(handle).width, SidebarLayout.resizeHandleWidth);
    expect(
      tester
          .widget<MouseRegion>(
            find
                .descendant(of: handle, matching: find.byType(MouseRegion))
                .first,
          )
          .cursor,
      SystemMouseCursors.resizeColumn,
    );

    final gesture = await tester.startGesture(tester.getCenter(handle));
    await gesture.moveBy(const Offset(48, 0));
    await gesture.moveBy(Offset.zero);
    await tester.pump();
    expect(tester.getSize(slot).width, closeTo(312, 0.1));
    expect(
      tester.getRect(find.byType(MainSidebarContent)).width,
      closeTo(312, 0.1),
    );
    await gesture.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 320));
    expect(tester.getSize(slot).width, closeTo(312, 0.1));

    final partial = await tester.startGesture(
      tester.getCenter(find.bySemanticsLabel('Resize sidebar')),
    );
    await partial.moveBy(const Offset(-140, 0));
    await partial.moveBy(Offset.zero);
    await tester.pump();
    // 312 - 140 = 172: the content stays at its 240px layout minimum while
    // the viewport follows the pointer and remains visibly open.
    expect(tester.getSize(slot).width, closeTo(172, 0.1));
    expect(
      tester.getRect(find.byType(MainSidebarContent)).width,
      closeTo(SidebarLayout.minWidth, 0.1),
    );
    expect(find.byTooltip('Hide the workspace sidebar'), findsOneWidget);
    await partial.up();
    await tester.pump();
    expect(find.byTooltip('Show the workspace sidebar'), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 150));
    expect(tester.getSize(slot).width, greaterThan(0));
    expect(tester.getSize(slot).width, lessThan(240));
    await tester.pump(const Duration(milliseconds: 180));
    expect(tester.getSize(slot).width, 0);
  });

  testWidgets('collapse reopens at the last valid width', (tester) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final handle = find.bySemanticsLabel('Resize sidebar');
    final gesture = await tester.startGesture(tester.getCenter(handle));
    await gesture.moveBy(const Offset(-78, 0));
    await gesture.moveBy(Offset.zero);
    await tester.pump();
    // The collapse threshold is evaluated only on pointer release.
    expect(find.byTooltip('Hide the workspace sidebar'), findsOneWidget);
    expect(
      tester.getSize(find.byKey(const ValueKey('sidebar-slot'))).width,
      186,
    );
    await gesture.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 320));
    expect(find.byTooltip('Show the workspace sidebar'), findsOneWidget);
    expect(tester.getSize(find.byKey(const ValueKey('sidebar-slot'))).width, 0);

    await tester.tap(find.byTooltip('Show the workspace sidebar'));
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 150));
    final openingWidth = tester
        .getSize(find.byKey(const ValueKey('sidebar-slot')))
        .width;
    expect(openingWidth, greaterThan(0));
    expect(openingWidth, lessThan(SidebarLayout.defaultWidth));
    await tester.pump(const Duration(milliseconds: 180));
    expect(
      tester.getSize(find.byKey(const ValueKey('sidebar-slot'))).width,
      SidebarLayout.defaultWidth,
    );
  });

  testWidgets('resize handle exposes width semantics and keyboard adjustment', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final handle = find.bySemanticsLabel('Resize sidebar');
    final node = tester.getSemantics(handle);
    expect(node.label, 'Resize sidebar');
    expect(node.value, '264 pixels');
    expect(node.increasedValue, '280 pixels');
    expect(node.decreasedValue, '248 pixels');

    await tester.tap(handle);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 320));
    expect(
      tester.getSize(find.byKey(const ValueKey('sidebar-slot'))).width,
      280,
    );
    expect(
      FocusManager.instance.primaryFocus?.debugLabel,
      'sidebar-resize-handle',
    );
  });

  testWidgets('maximum width clamps during a drag without overflow', (
    tester,
  ) async {
    _setWindow(tester, const Size(880, 640));
    await _pumpApp(tester);

    final gesture = await tester.startGesture(
      tester.getCenter(find.bySemanticsLabel('Resize sidebar')),
    );
    await gesture.moveBy(const Offset(200, 0));
    await gesture.moveBy(Offset.zero);
    await tester.pump();
    expect(
      tester.getSize(find.byKey(const ValueKey('sidebar-slot'))).width,
      SidebarLayout.maxWidth,
    );
    await gesture.up();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 320));
    expect(tester.takeException(), isNull);
  });
}

void _setWindow(WidgetTester tester, [Size size = const Size(1600, 1000)]) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}

Future<void> _pumpApp(WidgetTester tester) async {
  await tester.pumpWidget(const FrankApp(showLogin: false));
  await tester.pump(const Duration(milliseconds: 500));
}
