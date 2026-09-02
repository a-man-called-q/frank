import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:forui/forui.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/features/shell/sidebar_layout.dart';
import 'package:frank_desktop/features/shell/presentation/shell_context_bar.dart';

void main() {
  testWidgets('without an effect builder the sidebar stays solid', (
    tester,
  ) async {
    _setWindow(tester);
    await tester.pumpWidget(const FrankApp());
    await tester.pump(const Duration(milliseconds: 500));

    final sidebar = tester.widget<FSidebarData>(
      find.byType(FSidebarData).first,
    );
    final decoration = sidebar.style.decoration as BoxDecoration;
    final scaffold = tester.widget<Scaffold>(find.byType(Scaffold).first);

    expect(scaffold.backgroundColor, FrankColors.canvas);
    expect(sidebar.style.backgroundFilter, isNull);
    expect(decoration.color, FrankColors.sidebarSolid);
    expect(find.byKey(const ValueKey('fake-sidebar-effect')), findsNothing);
  });

  testWidgets(
    'effect wrapper follows open, animated, closed, and drag widths',
    (tester) async {
      _setWindow(tester);
      await tester.pumpWidget(
        FrankApp(sidebarEffectBuilder: _fakeSidebarEffect),
      );
      await tester.pump(const Duration(milliseconds: 500));

      final wrapper = find.byKey(const ValueKey('fake-sidebar-effect'));
      final slot = find.byKey(const ValueKey('sidebar-slot'));
      expect(tester.getSize(wrapper).width, SidebarLayout.defaultWidth);
      expect(tester.getSize(wrapper).width, tester.getSize(slot).width);

      await tester.tap(find.byTooltip('Hide the workspace sidebar'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 140));
      final midAnimationWidth = tester.getSize(wrapper).width;
      expect(midAnimationWidth, greaterThan(0));
      expect(midAnimationWidth, lessThan(SidebarLayout.defaultWidth));
      expect(midAnimationWidth, tester.getSize(slot).width);

      await tester.pump(const Duration(milliseconds: 220));
      expect(tester.getSize(wrapper).width, 0);
      expect(tester.getSize(slot).width, 0);

      await tester.tap(find.byTooltip('Show the workspace sidebar'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 320));
      final handle = find.bySemanticsLabel('Resize sidebar');
      final gesture = await tester.startGesture(tester.getCenter(handle));
      await gesture.moveBy(const Offset(48, 0));
      await gesture.moveBy(Offset.zero);
      await tester.pump();
      expect(tester.getSize(wrapper).width, closeTo(312, 0.1));
      expect(tester.getSize(wrapper).width, tester.getSize(slot).width);
      await gesture.up();
    },
  );

  testWidgets(
    'native effect leaves the context strip and main surface opaque',
    (tester) async {
      _setWindow(tester);
      await tester.pumpWidget(
        FrankApp(sidebarEffectBuilder: _fakeSidebarEffect),
      );
      await tester.pump(const Duration(milliseconds: 500));

      final scaffold = tester.widget<Scaffold>(find.byType(Scaffold).first);
      final contextStrip = tester.widget<SizedBox>(
        find.byKey(const ValueKey('main-context-strip')),
      );
      final contextPaint = tester.widget<ColoredBox>(
        find.descendant(
          of: find.byKey(const ValueKey('main-context-strip')),
          matching: find.byType(ColoredBox),
        ),
      );
      final mainPaint = tester.widget<ColoredBox>(
        find.byKey(const ValueKey('main-surface-background')),
      );
      final mainRect = tester.getRect(
        find.byKey(const ValueKey('main-surface-background')),
      );

      expect(scaffold.backgroundColor, Colors.transparent);
      expect(contextStrip.height, ShellContextBar.height);
      expect(contextPaint.color, FrankColors.canvas);
      expect(mainPaint.color, FrankColors.canvas);
      expect(mainRect.left, closeTo(SidebarLayout.defaultWidth, 0.1));
      expect(mainRect.width, greaterThan(0));
    },
  );
}

Widget _fakeSidebarEffect(Widget child) =>
    KeyedSubtree(key: const ValueKey('fake-sidebar-effect'), child: child);

void _setWindow(WidgetTester tester) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(1600, 1000);
  addTearDown(tester.view.reset);
}
