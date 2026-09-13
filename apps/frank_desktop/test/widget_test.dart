import 'dart:async';
import 'dart:ui' show Tristate;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:forui/forui.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/chat/presentation/focusable_composer.dart';
import 'package:frank_desktop/features/floor/office_scene_floor.dart';
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

  testWidgets('office shell defaults to the project inbox and floor', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    expect(find.byKey(const ValueKey('global-navigation')), findsOneWidget);
    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('global-navigation-workspace')),
        matching: find.text('Floor'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('global-navigation-workspace')),
        matching: find.text('Office'),
      ),
      findsOneWidget,
    );
    expect(FrankIcons.floor, isNot(FrankIcons.taskboard));
    expect(FrankIcons.taskboard, isNot(FrankIcons.journal));
    expect(FrankIcons.office, isNot(FrankIcons.floor));
    expect(find.bySemanticsLabel('Office view'), findsOneWidget);
    expect(find.bySemanticsLabel('Settings view'), findsOneWidget);
    expect(find.bySemanticsLabel('Workspace view'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('global-navigation-workspace')),
      findsOneWidget,
    );
    for (final group in ['agency', 'insights', 'system']) {
      expect(find.byKey(ValueKey('global-navigation-$group')), findsNothing);
    }
    expect(
      find.byKey(const ValueKey('settings-section-models')),
      findsNothing,
    );
    expect(find.byTooltip('Hide the workspace sidebar'), findsOneWidget);
    expect(
      tester.getSize(find.byKey(const ValueKey('global-navigation'))).width,
      greaterThan(180),
    );
    expect(
      tester.getSize(find.bySemanticsLabel('Workspace view')).width,
      greaterThan(180),
    );
    expect(
      find.byKey(const ValueKey('mission-shelf-scroll-view')),
      findsOneWidget,
    );
    expect(
      find.bySemanticsLabel(RegExp(r'Office floor (loading|unavailable)')),
      findsOneWidget,
    );
    expect(find.bySemanticsLabel('Frank'), findsOneWidget);
    expect(find.byType(FocusableComposer), findsOneWidget);
    expect(find.byKey(const ValueKey('settings-navigation')), findsNothing);
    expect(find.bySemanticsLabel('Organization flow editor'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('navigation groups follow the selected workspace mode', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    expect(
      find.byKey(const ValueKey('global-navigation-workspace')),
      findsOneWidget,
    );
    for (final group in ['agency', 'insights', 'system']) {
      expect(find.byKey(ValueKey('global-navigation-$group')), findsNothing);
    }

    await tester.tap(find.byKey(const ValueKey('settings-section-taskboard')));
    await tester.pump(const Duration(milliseconds: 120));
    expect(
      find.byKey(const ValueKey('settings-section-content-taskboard')),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('global-navigation-workspace')),
      findsOneWidget,
    );
    expect(
      tester
          .widget<FSidebarItem>(
            find.byKey(const ValueKey('settings-section-taskboard')),
          )
          .selected,
      isTrue,
    );
    expect(
      tester
          .widget<FSidebarItem>(find.byKey(const ValueKey('global-nav-office')))
          .selected,
      isFalse,
    );

    await tester.tap(find.byKey(const ValueKey('global-nav-office')));
    await tester.pump(const Duration(milliseconds: 120));
    expect(
      tester
          .widget<FSidebarItem>(find.byKey(const ValueKey('global-nav-office')))
          .selected,
      isTrue,
    );
    expect(
      tester
          .widget<FSidebarItem>(
            find.byKey(const ValueKey('settings-section-taskboard')),
          )
          .selected,
      isFalse,
    );

    await _openSettings(tester);
    expect(
      find.byKey(const ValueKey('global-navigation-workspace')),
      findsNothing,
    );
    for (final group in ['agency', 'insights', 'system']) {
      expect(find.byKey(ValueKey('global-navigation-$group')), findsOneWidget);
    }

    final team = find.byKey(const ValueKey('settings-section-team'));
    await tester.tap(team);
    await tester.pump(const Duration(milliseconds: 120));
    expect(find.bySemanticsLabel('Settings view'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('settings-section-content-team')),
      findsOneWidget,
    );

    final models = find.byKey(const ValueKey('settings-section-models'));
    await tester.tap(models);
    await tester.pump(const Duration(milliseconds: 120));
    expect(
      find.byKey(const ValueKey('settings-section-content-models')),
      findsOneWidget,
    );
    expect(tester.widget<FSidebarItem>(models).selected, isTrue);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Office and Settings toggle keeps the broad destinations reachable',
    (tester) async {
      _setWindow(tester);
      await _pumpApp(tester);

      final office = find.bySemanticsLabel('Office view');
      final settings = find.bySemanticsLabel('Settings view');

      await tester.tap(settings);
      await tester.pump(const Duration(milliseconds: 220));
      expect(
        find.byKey(const ValueKey('settings-section-content-organization')),
        findsOneWidget,
      );
      expect(find.byKey(const ValueKey('global-navigation')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('global-navigation-workspace')),
        findsNothing,
      );
      expect(
        find.byKey(const ValueKey('global-navigation-agency')),
        findsOneWidget,
      );

      await tester.tap(office);
      await tester.pump(const Duration(milliseconds: 220));
      expect(
        find.byKey(const ValueKey('mission-shelf-scroll-view')),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('global-navigation-workspace')),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('global-navigation-agency')),
        findsNothing,
      );

      await tester.tap(office);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.pump(const Duration(milliseconds: 220));
      expect(
        find.byKey(const ValueKey('settings-section-content-organization')),
        findsOneWidget,
      );
      expect(find.bySemanticsLabel('Settings view'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('sidebar footer exposes a disabled User profile control', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final userButton = find.byKey(const ValueKey('sidebar-user-button'));
    expect(userButton, findsOneWidget);
    expect(
      find.descendant(of: userButton, matching: find.text('User')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: userButton, matching: find.text('U')),
      findsOneWidget,
    );
    expect(find.byTooltip('User profile is not available yet'), findsOneWidget);

    final semantics = tester.getSemantics(userButton);
    expect(semantics.flagsCollection.isButton, isTrue);
    expect(semantics.flagsCollection.isEnabled, Tristate.isFalse);
    expect(semantics.label, 'User');
    expect(semantics.hint, 'User profile is not available yet');

    await tester.tap(userButton);
    await tester.pump();
    expect(
      find.byKey(const ValueKey('mission-shelf-scroll-view')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('compact sidebar keeps the User control within the viewport', (
    tester,
  ) async {
    _setWindow(tester, const Size(880, 340));
    await _pumpApp(tester);

    final userButton = find.byKey(const ValueKey('sidebar-user-button'));
    expect(userButton, findsOneWidget);
    expect(tester.getRect(userButton).bottom, lessThanOrEqualTo(340));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Settings sections navigate and Settings resets to Organization',
    (tester) async {
      _setWindow(tester);
      await _pumpApp(tester);

      await _openSettings(tester);

      for (final section in ['team', 'ledger', 'models']) {
        await tester.tap(find.byKey(ValueKey('settings-section-$section')));
        await tester.pump();
        expect(
          find.byKey(ValueKey('settings-section-surface-$section')),
          findsOneWidget,
        );
        expect(
          tester
              .widget<FSidebarItem>(
                find.byKey(ValueKey('settings-section-$section')),
              )
              .selected,
          isTrue,
        );
      }

      await _openOffice(tester);
      expect(
        find.byKey(const ValueKey('mission-shelf-scroll-view')),
        findsOneWidget,
      );
      for (final section in ['taskboard', 'journal']) {
        await tester.tap(find.byKey(ValueKey('settings-section-$section')));
        await tester.pump(const Duration(milliseconds: 120));
        expect(
          find.byKey(ValueKey('settings-section-surface-$section')),
          findsOneWidget,
        );
        expect(
          find.byKey(const ValueKey('global-navigation-workspace')),
          findsOneWidget,
        );
      }

      await _openSettings(tester);
      // Forui's tappable semantics finish their press lifecycle on a short
      // timer. Drain it before the test widget tree is disposed.
      await tester.pump(const Duration(milliseconds: 120));
      expect(
        find.byKey(const ValueKey('settings-section-surface-organization')),
        findsOneWidget,
      );
    },
  );

  testWidgets('Settings glass keeps the scene mounted between sections', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final stage = find.byKey(const ValueKey('office-scene-stage'));
    final stageElement = tester.element(stage);
    final retainedController = tester
        .widget<OfficeSceneFloor>(find.byType(OfficeSceneFloor))
        .controller!;
    retainedController.setReady(true);
    retainedController.panByPixels(const Offset(72, 24), const Size(900, 640));
    retainedController.rotateByPixels(
      const Offset(48, 0),
      const Size(900, 640),
    );
    retainedController.zoomByScroll(-120);
    final retainedTargetX = retainedController.target.x;
    final retainedTargetZ = retainedController.target.z;
    final retainedYaw = retainedController.yaw;
    final retainedZoom = retainedController.zoom;
    final filter = tester.widget<ImageFiltered>(
      find.byKey(const ValueKey('office-scene-image-filter')),
    );
    expect(filter.enabled, isFalse);
    await _openSettings(tester);
    expect(
      tester
          .widget<ImageFiltered>(
            find.byKey(const ValueKey('office-scene-image-filter')),
          )
          .enabled,
      isTrue,
    );
    expect(
      tester.getRect(
        find.byKey(const ValueKey('settings-section-content-host')),
      ),
      tester.getRect(find.byKey(const ValueKey('main-surface'))),
    );
    expect(
      find.byKey(const ValueKey('settings-section-content-organization')),
      findsOneWidget,
    );

    await tester.tap(find.byKey(const ValueKey('settings-section-team')));
    await tester.pump(const Duration(milliseconds: 40));
    expect(identical(stageElement, tester.element(stage)), isTrue);
    expect(
      find.byKey(const ValueKey('settings-section-content-team')),
      findsOneWidget,
    );

    await tester.tap(find.byKey(const ValueKey('settings-section-ledger')));
    await tester.pump(const Duration(milliseconds: 220));
    expect(identical(stageElement, tester.element(stage)), isTrue);
    expect(
      find.byKey(const ValueKey('settings-section-content-ledger')),
      findsOneWidget,
    );

    await _openOffice(tester);
    await tester.pump(const Duration(milliseconds: 220));
    expect(identical(stageElement, tester.element(stage)), isTrue);
    expect(
      find.byKey(const ValueKey('mission-shelf-scroll-view')),
      findsOneWidget,
    );

    await _openSettings(tester);
    expect(identical(stageElement, tester.element(stage)), isTrue);
    final officeController = tester
        .widget<OfficeSceneFloor>(find.byType(OfficeSceneFloor))
        .controller!;
    expect(identical(retainedController, officeController), isTrue);
    expect(officeController.target.x, closeTo(retainedTargetX, 1e-9));
    expect(officeController.target.z, closeTo(retainedTargetZ, 1e-9));
    expect(officeController.yaw, closeTo(retainedYaw, 1e-9));
    expect(officeController.zoom, closeTo(retainedZoom, 1e-9));
    expect(
      find.byKey(const ValueKey('settings-section-content-organization')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('reduced motion makes Settings section changes immediate', (
    tester,
  ) async {
    _setWindow(tester);
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: const FrankApp(showLogin: false),
      ),
    );
    await tester.pump(const Duration(milliseconds: 500));

    await _openSettings(tester);
    final switcher = tester.widget<AnimatedSwitcher>(
      find.byKey(const ValueKey('settings-section-content-switcher')),
    );
    expect(switcher.duration, Duration.zero);
    expect(switcher.reverseDuration, Duration.zero);

    await tester.tap(find.byKey(const ValueKey('settings-section-team')));
    // Forui's tappable semantics finish their press lifecycle on a short
    // timer. Drain it so the reduced-motion assertion does not leave a
    // pending callback when the widget tree is disposed.
    await tester.pump(const Duration(milliseconds: 120));
    expect(
      find.byKey(const ValueKey('settings-section-content-team')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('desktop collapse gives the main surface the full width', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    final floor = find.byType(OfficeSceneFloor);
    final floorElement = tester.element(floor);
    final openRect = tester.getRect(find.byKey(const ValueKey('main-surface')));

    await tester.tap(find.byTooltip('Hide the workspace sidebar'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 220));

    final closedRect = tester.getRect(
      find.byKey(const ValueKey('main-surface')),
    );
    expect(find.byTooltip('Show the workspace sidebar'), findsOneWidget);
    expect(closedRect.width, greaterThan(openRect.width + 200));
    expect(tester.widget<OfficeSceneFloor>(floor), isNotNull);
    expect(identical(floorElement, tester.element(floor)), isTrue);
    expect(tester.takeException(), isNull);
  });

  testWidgets('office floor keeps a deterministic loading placeholder', (
    tester,
  ) async {
    final resources = Completer<void>();
    await tester.pumpWidget(
      MaterialApp(
        home: OfficeSceneFloor(initializeResources: () => resources.future),
      ),
    );
    await tester.pump();

    expect(find.bySemanticsLabel('Stylized 3D office floor'), findsOneWidget);
    expect(find.bySemanticsLabel('Office floor loading'), findsOneWidget);
    expect(find.text('OFFICE SCENE · INITIALIZING'), findsOneWidget);
    expect(find.byKey(const ValueKey('office-scene-view')), findsNothing);
  });

  testWidgets('office floor exposes a retryable GPU error', (tester) async {
    var attempts = 0;

    Future<void> failToInitialize() async {
      attempts += 1;
      throw StateError('GPU unavailable in test');
    }

    await tester.pumpWidget(
      MaterialApp(
        home: OfficeSceneFloor(initializeResources: failToInitialize),
      ),
    );
    await tester.pump();
    await tester.pump();

    expect(find.bySemanticsLabel('Office floor unavailable'), findsOneWidget);
    expect(find.text('3D FLOOR UNAVAILABLE'), findsOneWidget);
    expect(find.text('Retry'), findsOneWidget);
    expect(attempts, 1);

    await tester.tap(find.text('Retry'));
    await tester.pump();
    await tester.pump();
    expect(attempts, 2);
    expect(find.bySemanticsLabel('Office floor unavailable'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('minimum desktop width keeps the normal sidebar layout', (
    tester,
  ) async {
    _setWindow(tester, const Size(880, 640));
    await _pumpApp(tester);

    expect(find.byTooltip('Hide the workspace sidebar'), findsOneWidget);
    expect(find.bySemanticsLabel('Resize sidebar'), findsOneWidget);
    expect(find.bySemanticsLabel('Close sidebar'), findsNothing);
    expect(
      tester.getRect(find.byType(MainSidebarContent)).width,
      MainSidebarContent.fixedWidth,
    );
    expect(
      tester.getSize(find.byKey(const ValueKey('global-navigation'))).width,
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
    expect(find.byTooltip('Show the workspace sidebar'), findsOneWidget);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyB);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));
    expect(find.byTooltip('Hide the workspace sidebar'), findsOneWidget);
  });

  testWidgets('Cmd/Ctrl+K reopens the sidebar and focuses inline search', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.tap(find.byTooltip('Hide the workspace sidebar'));
    await tester.pump(const Duration(milliseconds: 220));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));

    expect(find.byTooltip('Hide the workspace sidebar'), findsOneWidget);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'workspace-search');
  });

  testWidgets('inline search clears with its button and Escape', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    await tester.enterText(_searchField(), 'warehouse');
    await tester.pump();
    expect(find.byTooltip('Clear the workspace search'), findsOneWidget);

    await tester.tap(find.byTooltip('Clear the workspace search'));
    await tester.pump();
    expect(_searchController(tester).text, isEmpty);
    expect(find.byTooltip('Clear the workspace search'), findsNothing);

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

  testWidgets('Cmd/Ctrl+K from Office opens inline search', (tester) async {
    _setWindow(tester);
    await _pumpApp(tester);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump(const Duration(milliseconds: 220));

    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'workspace-search');
  });

  testWidgets('desktop sidebar uses a fixed width and exposes actions', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    expect(find.bySemanticsLabel('Resize sidebar'), findsOneWidget);
    expect(
      tester.getRect(find.byType(MainSidebarContent)).width,
      MainSidebarContent.fixedWidth,
    );

    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();
    await tester.tap(
      find.byTooltip('Open actions for Design replenishment dashboard'),
    );
    await tester.pump();
    expect(find.text('Pin'), findsOneWidget);
    expect(find.text('Rename'), findsOneWidget);
    expect(find.text('Archive'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
  });

  testWidgets('task rows keep passive signals and reveal actions on focus', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    expect(
      find.byTooltip(
        'Pin task Design replenishment dashboard to the pinned list',
      ),
      findsNothing,
    );
    expect(
      find.byTooltip('Open actions for Design replenishment dashboard'),
      findsNothing,
    );

    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();

    expect(
      find.byTooltip(
        'Pin task Design replenishment dashboard to the pinned list',
      ),
      findsOneWidget,
    );
    expect(
      find.byTooltip('Open actions for Design replenishment dashboard'),
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

  testWidgets('inline search finds a task and opens its conversation', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    await tester.enterText(_searchField(), 'warehouse');
    await tester.pump();
    expect(find.text('Map warehouse intake'), findsWidgets);

    await tester.tap(find.text('Map warehouse intake').first);
    await tester.pump(const Duration(milliseconds: 220));

    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(find.text('Map warehouse intake'), findsWidgets);
    expect(
      find.bySemanticsLabel(
        'Conversation with Maya Chen for Map warehouse intake',
      ),
      findsOneWidget,
    );
    expect(_searchController(tester).text, isEmpty);
    expect(find.byTooltip('Clear the workspace search'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('scope, pinning, agent search, and Office navigation work', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    expect(
      find.byTooltip(
        'Pin task Design replenishment dashboard to the pinned list',
      ),
      findsNothing,
    );
    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();
    final pin = find.byTooltip(
      'Pin task Design replenishment dashboard to the pinned list',
    );
    expect(pin, findsOneWidget);
    await tester.tap(pin);
    await tester.pump();
    expect(
      find.byTooltip(
        'Unpin task Design replenishment dashboard from the pinned list',
      ),
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
      find.byKey(const ValueKey('settings-section-surface-team')),
      findsOneWidget,
    );
    expect(find.bySemanticsLabel('Team roster'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Forui action popovers switch with selectors in one click', (
    tester,
  ) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    await tester.tap(find.text('Design replenishment dashboard'));
    await tester.pump();
    final actionTrigger = find.byTooltip(
      'Open actions for Design replenishment dashboard',
    );
    await tester.tap(actionTrigger);
    await tester.pump();
    expect(find.text('Pin'), findsOneWidget);

    await tester.tap(find.text('All projects'));
    await tester.pump();
    expect(find.text('Pin'), findsNothing);
    expect(find.text('Atlas Handoff'), findsWidgets);

    await tester.tap(actionTrigger);
    await tester.pump();
    expect(find.text('Atlas Handoff'), findsNothing);
    expect(find.text('Pin'), findsOneWidget);
  });

  testWidgets('clicking composer padding focuses its input', (tester) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    final composer = find.byType(FocusableComposer);
    expect(composer, findsOneWidget);
    final rect = tester.getRect(composer);
    await tester.tapAt(Offset(rect.left + 24, rect.top + 20));
    await tester.pump();

    expect(FocusManager.instance.primaryFocus?.debugLabel, 'Frank composer');
    expect(tester.takeException(), isNull);
  });

  testWidgets('floor reset stays outside the chat composer', (tester) async {
    _setWindow(tester);
    await _pumpApp(tester);
    await _openOffice(tester);

    final resetButton = find.byKey(const ValueKey('floor-reset-view-button'));
    expect(resetButton, findsOneWidget);
    expect(find.bySemanticsLabel('Reset floor view'), findsOneWidget);
    expect(
      tester
          .getRect(resetButton)
          .overlaps(tester.getRect(find.byType(FocusableComposer))),
      isFalse,
    );
    // The headless test host has no GPU scene, so the action stays visible but
    // disabled until the real scene reports readiness.
    expect(tester.widget<IconButton>(resetButton).onPressed, isNull);
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

Future<void> _openOffice(WidgetTester tester) async {
  await tester.tap(find.bySemanticsLabel('Office view'));
  await tester.pump(const Duration(milliseconds: 220));
}

Future<void> _openSettings(WidgetTester tester) async {
  await tester.tap(find.bySemanticsLabel('Settings view'));
  await tester.pump(const Duration(milliseconds: 220));
}

Finder _searchField() => find.byWidgetPredicate(
  (widget) =>
      widget is TextField && widget.decoration?.hintText == 'Search workspace',
);

TextEditingController _searchController(WidgetTester tester) =>
    tester.widget<TextField>(_searchField()).controller!;
