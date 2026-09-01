import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/shell/presentation/frank_desktop_menu.dart';

void main() {
  testWidgets('context menu opens to the right without covering its anchor', (
    tester,
  ) async {
    _setWindow(tester, const Size(400, 300));
    final triggerKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopMenu(
          openOnTap: true,
          groups: const [
            FrankMenuGroup([FrankMenuItem(label: 'Action', onPressed: _noop)]),
          ],
          child: SizedBox(
            key: triggerKey,
            width: 32,
            height: 32,
            child: const ColoredBox(color: Colors.blue),
          ),
        ),
        left: 20,
        top: 80,
      ),
    );
    await tester.tap(find.byKey(triggerKey));
    await tester.pump();

    final trigger = tester.getRect(find.byKey(triggerKey));
    final menuItem = tester.getRect(find.text('Action'));
    expect(menuItem.left, greaterThan(trigger.right));
    expect(find.text('Action'), findsOneWidget);
  });

  testWidgets('context menu flips to the left near the right edge', (
    tester,
  ) async {
    _setWindow(tester, const Size(400, 300));
    final triggerKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopMenu(
          openOnTap: true,
          groups: const [
            FrankMenuGroup([FrankMenuItem(label: 'Action', onPressed: _noop)]),
          ],
          child: SizedBox(
            key: triggerKey,
            width: 32,
            height: 32,
            child: const ColoredBox(color: Colors.blue),
          ),
        ),
        right: 20,
        top: 80,
      ),
    );
    await tester.tap(find.byKey(triggerKey));
    await tester.pump();

    final trigger = tester.getRect(find.byKey(triggerKey));
    final menuItem = tester.getRect(find.text('Action'));
    expect(menuItem.right, lessThan(trigger.left));
  });

  testWidgets('select opens below and flips above at the viewport edge', (
    tester,
  ) async {
    _setWindow(tester, const Size(400, 300));
    final belowKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopSelect<String>(
          value: 'one',
          options: const [
            FrankDesktopSelectOption(value: 'one', label: 'One'),
            FrankDesktopSelectOption(value: 'two', label: 'Two'),
          ],
          onChanged: _select,
          child: SizedBox(
            key: belowKey,
            width: 140,
            height: 32,
            child: const ColoredBox(color: Colors.blue),
          ),
        ),
        left: 20,
        top: 20,
      ),
    );

    await tester.tap(find.byKey(belowKey));
    await tester.pump();
    final belowTrigger = tester.getRect(find.byKey(belowKey));
    final belowItem = tester.getRect(find.text('Two'));
    expect(belowItem.top, greaterThanOrEqualTo(belowTrigger.bottom));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.text('Two'), findsNothing);

    final aboveKey = GlobalKey();
    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopSelect<String>(
          value: 'one',
          options: const [
            FrankDesktopSelectOption(value: 'one', label: 'One'),
            FrankDesktopSelectOption(value: 'two', label: 'Two'),
          ],
          onChanged: _select,
          child: SizedBox(
            key: aboveKey,
            width: 140,
            height: 32,
            child: const ColoredBox(color: Colors.blue),
          ),
        ),
        left: 20,
        top: 250,
      ),
    );
    await tester.tap(find.byKey(aboveKey));
    await tester.pump();
    final aboveTrigger = tester.getRect(find.byKey(aboveKey));
    final aboveItem = tester.getRect(find.text('Two'));
    expect(aboveItem.bottom, lessThanOrEqualTo(aboveTrigger.top));
  });

  testWidgets('outside tap dismisses and restores trigger focus', (
    tester,
  ) async {
    _setWindow(tester, const Size(400, 300));
    final focusNode = FocusNode(debugLabel: 'desktop-menu-trigger');
    addTearDown(focusNode.dispose);
    final triggerKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopMenu(
          openOnTap: true,
          returnFocusNode: focusNode,
          groups: const [
            FrankMenuGroup([FrankMenuItem(label: 'Action', onPressed: _noop)]),
          ],
          child: Focus(
            focusNode: focusNode,
            child: SizedBox(
              key: triggerKey,
              width: 32,
              height: 32,
              child: const ColoredBox(color: Colors.blue),
            ),
          ),
        ),
        left: 20,
        top: 80,
      ),
    );
    await tester.tap(find.byKey(triggerKey));
    await tester.pump();
    expect(find.text('Action'), findsOneWidget);
    await tester.tapAt(const Offset(380, 280));
    await tester.pump();
    expect(find.text('Action'), findsNothing);
    expect(focusNode.hasFocus, isTrue);
  });
}

Widget _menuHost({
  required Widget child,
  double? left,
  double? right,
  double? top,
}) {
  return MaterialApp(
    theme: ThemeData.dark(),
    home: Scaffold(
      body: Stack(
        children: [
          Positioned(left: left, right: right, top: top, child: child),
        ],
      ),
    ),
  );
}

void _setWindow(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}

void _noop() {}

void _select(String value) {}
