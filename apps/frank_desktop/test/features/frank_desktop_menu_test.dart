import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/controls/frank_desktop_menu.dart';

void main() {
  testWidgets('canonical select field matches its trigger width', (
    tester,
  ) async {
    _setWindow(tester, const Size(600, 400));
    final triggerKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: SizedBox(
          width: 220,
          child: FrankDesktopSelectField<String>(
            value: 'one',
            options: const [
              FrankDesktopSelectOption(value: 'one', label: 'One'),
              FrankDesktopSelectOption(value: 'two', label: 'Two'),
            ],
            onChanged: _select,
            fieldKey: triggerKey,
            semanticsLabel: 'Project scope',
          ),
        ),
        left: 20,
        top: 20,
      ),
    );
    await tester.tap(find.byKey(triggerKey));
    await tester.pump();

    final trigger = tester.getRect(find.byKey(triggerKey));
    final menu = tester.getRect(
      find.byWidgetPredicate(
        (widget) => widget.runtimeType.toString() == '_FrankDesktopMenuSurface',
      ),
    );
    expect(menu.width, closeTo(trigger.width, 0.01));
  });

  testWidgets('searchable select filters options inside the shared menu', (
    tester,
  ) async {
    _setWindow(tester, const Size(600, 400));
    final triggerKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopSelectField<String>(
          value: 'alpha',
          options: const [
            FrankDesktopSelectOption(value: 'alpha', label: 'Alpha'),
            FrankDesktopSelectOption(value: 'beta', label: 'Beta'),
          ],
          onChanged: _select,
          fieldKey: triggerKey,
          searchable: true,
          semanticsLabel: 'Model',
        ),
        left: 20,
        top: 20,
      ),
    );
    await tester.tap(find.byKey(triggerKey));
    await tester.pump();
    await tester.enterText(find.byType(TextField), 'beta');
    await tester.pump();

    final menu = find.byWidgetPredicate(
      (widget) => widget.runtimeType.toString() == '_FrankDesktopMenuSurface',
    );
    expect(
      find.descendant(of: menu, matching: find.text('Beta')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: menu, matching: find.text('Alpha')),
      findsNothing,
    );
  });

  testWidgets('pointer-open context menus use the fixed action width', (
    tester,
  ) async {
    _setWindow(tester, const Size(600, 400));
    final controller = FrankDesktopMenuController();
    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopMenu(
          controller: controller,
          groups: const [
            FrankMenuGroup([FrankMenuItem(label: 'Delete', onPressed: _noop)]),
          ],
          child: const SizedBox(width: 1, height: 1),
        ),
        left: 0,
        top: 0,
      ),
    );
    controller.openAt(const Offset(100, 100));
    await tester.pump();

    final menu = tester.getRect(
      find.byWidgetPredicate(
        (widget) => widget.runtimeType.toString() == '_FrankDesktopMenuSurface',
      ),
    );
    expect(menu.width, 248);
  });

  testWidgets('menus clamp their width inside a narrow viewport', (
    tester,
  ) async {
    _setWindow(tester, const Size(220, 300));
    final controller = FrankDesktopMenuController();
    await tester.pumpWidget(
      _menuHost(
        child: FrankDesktopMenu(
          controller: controller,
          groups: const [
            FrankMenuGroup([FrankMenuItem(label: 'Action', onPressed: _noop)]),
          ],
          child: const SizedBox(width: 1, height: 1),
        ),
        left: 0,
        top: 0,
      ),
    );
    controller.openAt(const Offset(100, 100));
    await tester.pump();

    final menu = tester.getRect(
      find.byWidgetPredicate(
        (widget) => widget.runtimeType.toString() == '_FrankDesktopMenuSurface',
      ),
    );
    expect(menu.width, lessThanOrEqualTo(204));
    expect(menu.left, greaterThanOrEqualTo(8));
    expect(menu.right, lessThanOrEqualTo(212));
  });

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

  testWidgets('clicking another select switches menus in one click', (
    tester,
  ) async {
    _setWindow(tester, const Size(600, 400));
    final firstKey = GlobalKey();
    final secondKey = GlobalKey();

    await tester.pumpWidget(
      _menuHost(
        child: Stack(
          children: [
            Positioned(
              left: 20,
              top: 20,
              child: FrankDesktopSelect<String>(
                value: 'first',
                options: const [
                  FrankDesktopSelectOption(
                    value: 'first',
                    label: 'First option',
                  ),
                  FrankDesktopSelectOption(
                    value: 'first-other',
                    label: 'First other option',
                  ),
                ],
                onChanged: _select,
                child: SizedBox(
                  key: firstKey,
                  width: 120,
                  height: 32,
                  child: const ColoredBox(color: Colors.blue),
                ),
              ),
            ),
            Positioned(
              left: 320,
              top: 20,
              child: FrankDesktopSelect<String>(
                value: 'second',
                options: const [
                  FrankDesktopSelectOption(
                    value: 'second',
                    label: 'Second option',
                  ),
                  FrankDesktopSelectOption(
                    value: 'second-other',
                    label: 'Second other option',
                  ),
                ],
                onChanged: _select,
                child: SizedBox(
                  key: secondKey,
                  width: 120,
                  height: 32,
                  child: const ColoredBox(color: Colors.green),
                ),
              ),
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.byKey(firstKey));
    await tester.pump();
    expect(find.text('First other option'), findsOneWidget);

    await tester.tap(find.byKey(secondKey));
    await tester.pump();
    expect(find.text('First other option'), findsNothing);
    expect(find.text('Second other option'), findsOneWidget);
  });

  testWidgets('outside tap dismisses while activating the underlying trigger', (
    tester,
  ) async {
    _setWindow(tester, const Size(600, 400));
    final selectKey = GlobalKey();
    var activated = false;

    await tester.pumpWidget(
      _menuHost(
        child: Stack(
          children: [
            Positioned(
              left: 20,
              top: 20,
              child: FrankDesktopSelect<String>(
                value: 'value',
                options: const [
                  FrankDesktopSelectOption(value: 'value', label: 'Value'),
                  FrankDesktopSelectOption(
                    value: 'other',
                    label: 'Other option',
                  ),
                ],
                onChanged: _select,
                child: SizedBox(
                  key: selectKey,
                  width: 120,
                  height: 32,
                  child: const ColoredBox(color: Colors.blue),
                ),
              ),
            ),
            Positioned(
              left: 20,
              top: 120,
              child: ElevatedButton(
                onPressed: () => activated = true,
                child: const Text('Underlying action'),
              ),
            ),
          ],
        ),
      ),
    );

    await tester.tap(find.byKey(selectKey));
    await tester.pump();
    expect(find.text('Other option'), findsOneWidget);

    await tester.tap(find.text('Underlying action'));
    await tester.pump();
    expect(activated, isTrue);
    expect(find.text('Other option'), findsNothing);
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

  testWidgets('scroll dismissal closes menus without restoring trigger focus', (
    tester,
  ) async {
    _setWindow(tester, const Size(400, 300));
    final focusNode = FocusNode(debugLabel: 'desktop-menu-trigger');
    final scrollController = ScrollController();
    addTearDown(focusNode.dispose);
    addTearDown(scrollController.dispose);
    final triggerKey = GlobalKey();

    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData.dark(),
        home: Scaffold(
          body: FrankDesktopMenuDismissScope(
            child: SingleChildScrollView(
              controller: scrollController,
              child: Column(
                children: [
                  const SizedBox(height: 24),
                  FrankDesktopMenu(
                    openOnTap: true,
                    returnFocusNode: focusNode,
                    groups: const [
                      FrankMenuGroup([
                        FrankMenuItem(label: 'Action', onPressed: _noop),
                      ]),
                    ],
                    child: Focus(
                      focusNode: focusNode,
                      child: SizedBox(
                        key: triggerKey,
                        width: 120,
                        height: 32,
                        child: const ColoredBox(color: Colors.blue),
                      ),
                    ),
                  ),
                  const SizedBox(height: 600),
                ],
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.byKey(triggerKey));
    await tester.pump();
    expect(find.text('Action'), findsOneWidget);

    scrollController.jumpTo(100);
    await tester.pump();
    expect(find.text('Action'), findsNothing);
    expect(focusNode.hasFocus, isFalse);
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
