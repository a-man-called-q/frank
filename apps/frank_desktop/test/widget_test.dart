import 'package:flame/game.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/floor/empty_office_floor.dart';
import 'package:frank_desktop/features/shell/main_sidebar.dart';

void main() {
  test('fixture gateway exposes operational mission metadata', () async {
    final workspace = await FixtureFrankGateway().loadWorkspace();

    expect(workspace.name, 'Frank Agency');
    expect(workspace.accountExecutive.name, 'Maya Chen');
    expect(workspace.employees, hasLength(4));
    expect(workspace.projects, hasLength(3));
    expect(workspace.projects.first.name, 'Northstar Inventory');
    expect(workspace.projects.first.missions.first.pendingApprovalCount, 1);
  });

  testWidgets('office shell renders the inbox, context bar, and floor', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    expect(find.bySemanticsLabel('Office view'), findsOneWidget);
    expect(find.bySemanticsLabel('Projects view'), findsOneWidget);
    expect(find.byTooltip('Hide sidebar'), findsOneWidget);
    expect(
      tester.getSize(find.bySemanticsLabel('Workspace view')).width,
      greaterThan(180),
    );
    expect(find.text('Office'), findsOneWidget);
    expect(find.text('Projects'), findsOneWidget);
    expect(find.text('Needs attention'), findsWidgets);
    expect(find.text('Draft'), findsOneWidget);
    expect(find.text('OFFICE FLOOR · EMPTY FOR NOW'), findsOneWidget);
    expect(find.bySemanticsLabel('Frank'), findsOneWidget);
    expect(find.bySemanticsLabel('Message Maya'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('desktop collapse gives the main surface the full width', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final floor = find.byType(EmptyOfficeFloor);
    final gameWidget = find.byWidgetPredicate(
      (widget) => widget is GameWidget<EmptyOfficeGame>,
    );
    final gameBefore = tester
        .widget<GameWidget<EmptyOfficeGame>>(gameWidget)
        .game;
    final openRect = tester.getRect(find.byKey(const ValueKey('main-surface')));

    await tester.tap(find.byTooltip('Hide sidebar'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 220));

    final closedRect = tester.getRect(
      find.byKey(const ValueKey('main-surface')),
    );
    expect(find.byTooltip('Show sidebar'), findsOneWidget);
    expect(closedRect.width, greaterThan(openRect.width + 200));
    expect(tester.widget<EmptyOfficeFloor>(floor), isNotNull);
    final gameAfter = tester
        .widget<GameWidget<EmptyOfficeGame>>(gameWidget)
        .game;
    expect(identical(gameBefore, gameAfter), isTrue);
    expect(tester.takeException(), isNull);
  });

  testWidgets('minimum desktop width keeps the normal sidebar layout', (
    tester,
  ) async {
    _setWindow(tester, const Size(880, 640));
    await _pumpApp(tester);

    expect(find.byTooltip('Hide sidebar'), findsOneWidget);
    expect(find.bySemanticsLabel('Resize sidebar'), findsOneWidget);
    expect(find.bySemanticsLabel('Close sidebar'), findsNothing);
    expect(
      tester.getRect(find.byType(MainSidebarContent)).width,
      MainSidebarContent.fixedWidth,
    );
    expect(
      tester.getSize(find.bySemanticsLabel('Workspace view')).width,
      greaterThan(180),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('keyboard shortcut toggles the desktop sidebar', (tester) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyB);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.byTooltip('Show sidebar'), findsOneWidget);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyB);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.byTooltip('Hide sidebar'), findsOneWidget);
  });

  testWidgets('Cmd/Ctrl+K reopens the sidebar and focuses inline search', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.tap(find.byTooltip('Hide sidebar'));
    await tester.pump(const Duration(milliseconds: 220));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));

    expect(find.byTooltip('Hide sidebar'), findsOneWidget);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'workspace-search');
  });

  testWidgets('inline search clears with its button and Escape', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.enterText(_searchField(), 'warehouse');
    await tester.pump();
    expect(find.byTooltip('Clear search'), findsOneWidget);

    await tester.tap(find.byTooltip('Clear search'));
    await tester.pump();
    expect(_searchController(tester).text, isEmpty);
    expect(find.byTooltip('Clear search'), findsNothing);

    await tester.enterText(_searchField(), 'warehouse');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(_searchController(tester).text, isEmpty);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'workspace-search');

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(
      FocusManager.instance.primaryFocus?.debugLabel,
      isNot('workspace-search'),
    );
  });

  testWidgets('Cmd/Ctrl+K from Settings returns to inline search', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.tap(find.byTooltip('Settings'));
    await tester.pump(const Duration(milliseconds: 220));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));

    expect(find.bySemanticsLabel('Projects view'), findsOneWidget);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'workspace-search');
  });

  testWidgets('desktop sidebar uses a fixed width and exposes actions', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    expect(find.bySemanticsLabel('Resize sidebar'), findsOneWidget);
    expect(
      tester.getRect(find.byType(MainSidebarContent)).width,
      MainSidebarContent.fixedWidth,
    );

    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();
    await tester.tap(
      find.byTooltip('Actions for Design replenishment dashboard'),
    );
    await tester.pump();
    expect(find.text('Pin'), findsOneWidget);
    expect(find.text('Rename'), findsOneWidget);
    expect(find.text('Archive'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
  });

  testWidgets('mission rows keep passive signals and reveal actions on focus', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    expect(find.byTooltip('Pin Design replenishment dashboard'), findsNothing);
    expect(
      find.byTooltip('Actions for Design replenishment dashboard'),
      findsNothing,
    );

    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();

    expect(
      find.byTooltip('Pin Design replenishment dashboard'),
      findsOneWidget,
    );
    expect(
      find.byTooltip('Actions for Design replenishment dashboard'),
      findsOneWidget,
    );
    expect(
      find.bySemanticsLabel(RegExp(r'1 approvals pending')),
      findsOneWidget,
    );
    expect(
      find.byTooltip(
        'Design replenishment dashboard\n'
        'Project: Northstar Inventory\n'
        'Status: Draft\n'
        'Agent assignment: Unassigned\n'
        'Approval count: 0',
      ),
      findsOneWidget,
    );
  });

  testWidgets('inline search finds a mission and opens its conversation', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.enterText(_searchField(), 'warehouse');
    await tester.pump();
    expect(find.text('Map warehouse intake'), findsWidgets);

    await tester.tap(find.text('Map warehouse intake').first);
    await tester.pump(const Duration(milliseconds: 220));

    expect(find.bySemanticsLabel('Projects view'), findsOneWidget);
    expect(find.text('Map warehouse intake'), findsWidgets);
    expect(
      find.bySemanticsLabel(
        'Conversation with Maya Chen for Map warehouse intake',
      ),
      findsOneWidget,
    );
    expect(_searchController(tester).text, isEmpty);
    expect(find.byTooltip('Clear search'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('scope, pinning, agent search, and settings navigation work', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    expect(find.byTooltip('Pin Design replenishment dashboard'), findsNothing);
    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();
    final pin = find.byTooltip('Pin Design replenishment dashboard');
    expect(pin, findsOneWidget);
    await tester.tap(pin);
    await tester.pump();
    expect(
      find.byTooltip('Unpin Design replenishment dashboard'),
      findsOneWidget,
    );

    await tester.tap(find.text('All projects'));
    await tester.pump();
    await tester.tap(find.text('Meridian Finance').last);
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.text('Review approval controls'), findsOneWidget);

    await tester.enterText(_searchField(), 'maya');
    await tester.pump();
    await tester.tap(find.text('Maya Chen'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(
      find.text('Manage the agents available to Frank Agency.'),
      findsOneWidget,
    );

    await tester.tap(find.byTooltip('Settings'));
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.text('Settings'), findsOneWidget);
    expect(find.text('Activity'), findsOneWidget);
    expect(find.text('Ledger'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('clicking composer padding focuses its input', (tester) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final composer = find.bySemanticsLabel('Message Maya');
    expect(composer, findsOneWidget);
    final rect = tester.getRect(composer);
    await tester.tapAt(Offset(rect.left + 24, rect.top + 20));
    await tester.pump();

    expect(FocusManager.instance.primaryFocus?.debugLabel, 'Frank composer');
    expect(tester.takeException(), isNull);
  });
}

void _setWindow(WidgetTester tester, [Size size = const Size(1600, 1000)]) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}

Future<void> _pumpApp(WidgetTester tester) async {
  await tester.pumpWidget(const FrankApp());
  await tester.pump(const Duration(milliseconds: 500));
}

Finder _searchField() => find.byWidgetPredicate(
  (widget) =>
      widget is TextField && widget.decoration?.hintText == 'Search workspace',
);

TextEditingController _searchController(WidgetTester tester) =>
    tester.widget<TextField>(_searchField()).controller!;
