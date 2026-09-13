import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/layout/office_surface_frame.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/taskboard/bloc/taskboard_bloc.dart';
import 'package:frank_desktop/features/taskboard/presentation/taskboard_surface.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../support/fake_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  Future<void> pumpSurface(
    WidgetTester tester, {
    required FakeGateway gateway,
    ui.Size size = const ui.Size(1280, 800),
  }) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = size;
    addTearDown(tester.view.reset);
    final workspace = await tester.runAsync(
      () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
    );
    final loadedWorkspace = workspace!;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildFrankTheme(Brightness.dark),
        home: BlocProvider(
          create: (_) => TaskboardBloc(gateway: gateway),
          child: TaskboardSurface(workspace: loadedWorkspace),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
  }

  testWidgets('renders board, opens detail, approves, and closes with Escape', (
    tester,
  ) async {
    await pumpSurface(tester, gateway: FakeGateway());

    expect(find.text('Taskboard'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('taskboard-board-scroll')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('taskboard-task-NS-03')), findsOneWidget);
    expect(find.text('Needs attention  2'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('taskboard-task-NS-03')));
    await tester.pump();
    expect(find.text('Your decision'), findsOneWidget);
    expect(
      find.byKey(const ValueKey('taskboard-close-detail')),
      findsOneWidget,
    );

    await tester.tap(find.byKey(const ValueKey('taskboard-decision-NS-03')));
    await tester.pumpAndSettle();
    expect(
      find.text('Scope approved · Preparing specification'),
      findsNWidgets(2),
    );
    expect(find.text('Needs attention  1'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.byKey(const ValueKey('taskboard-close-detail')), findsNothing);
    expect(
      tester.binding.focusManager.primaryFocus?.debugLabel,
      'taskboard-task-NS-03',
    );
  });

  testWidgets(
    'desktop inspector overlays the frame and exposed cards stay interactive',
    (tester) async {
      await pumpSurface(tester, gateway: FakeGateway());

      final boardScrollBefore = tester.getRect(
        find.byKey(const ValueKey('taskboard-board-scroll')),
      );
      final boardContentBefore = tester.getSize(
        find.byKey(const ValueKey('taskboard-board-content')),
      );
      await tester.tap(find.byKey(const ValueKey('taskboard-task-NS-03')));
      await tester.pump();
      final rail = tester.getRect(
        find.byKey(const ValueKey('taskboard-inspector-rail')),
      );
      expect(rail.left, 920);
      expect(rail.top, 0);
      expect(rail.bottom, 800);
      expect(rail.width, OfficeInspectorDrawerOverlay.defaultPanelWidth);
      expect(
        tester.getRect(find.byKey(const ValueKey('taskboard-board-scroll'))),
        boardScrollBefore,
      );
      final boardContent = tester.getSize(
        find.byKey(const ValueKey('taskboard-board-content')),
      );
      expect(boardContent, boardContentBefore);
      expect(boardContent.width, greaterThanOrEqualTo(910));
      expect(find.text('Done'), findsOneWidget);

      // NS-02 is in the exposed working lane to the left of the pane. Selecting
      // it proves the board remains interactive.
      await tester.tap(find.byKey(const ValueKey('taskboard-task-NS-02')));
      await tester.pump();
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('taskboard-inspector-rail')),
          matching: find.text('Map receiving exceptions'),
        ),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('taskboard-inspector-rail')),
        findsOneWidget,
      );
    },
  );

  testWidgets('uses list at compact width and keeps long input valid', (
    tester,
  ) async {
    await pumpSurface(
      tester,
      gateway: FakeGateway(),
      size: const ui.Size(640, 800),
    );

    expect(
      find.byKey(const ValueKey('taskboard-list-scroll'), skipOffstage: false),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('taskboard-board-scroll')), findsNothing);
    expect(find.text('Map receiving exceptions'), findsOneWidget);
  });

  testWidgets('selected task replaces the body at compact width', (
    tester,
  ) async {
    await pumpSurface(
      tester,
      gateway: FakeGateway(),
      size: const ui.Size(640, 800),
    );

    await tester.tap(find.byKey(const ValueKey('taskboard-list-task-NS-03')));
    await tester.pump();
    final inspector = tester.getRect(
      find.byKey(const ValueKey('taskboard-inspector-rail')),
    );
    expect(inspector.left, 0);
    expect(inspector.width, 640);
    expect(inspector.top, 0);
    expect(inspector.bottom, 800);
    expect(
      find.byKey(const ValueKey('taskboard-list-scroll'), skipOffstage: false),
      findsOneWidget,
    );

    await tester.tap(find.byKey(const ValueKey('taskboard-close-detail')));
    await tester.pump();
    expect(
      find.byKey(const ValueKey('taskboard-inspector-rail')),
      findsNothing,
    );
  });

  testWidgets('shows load failure and retry action', (tester) async {
    final gateway = FakeGateway(taskboardLoadError: StateError('offline'));
    await pumpSurface(tester, gateway: gateway);
    expect(find.text('Taskboard unavailable'), findsOneWidget);
    expect(find.text('Retry'), findsOneWidget);

    gateway.taskboardLoadError = null;
    await tester.tap(find.text('Retry'));
    await tester.pump();
    await tester.pump();
    expect(find.text('Taskboard'), findsOneWidget);
    expect(find.byKey(const ValueKey('taskboard-task-NS-03')), findsOneWidget);
  });

  testWidgets(
    'validates a positive threshold and preserves it after gateway failure',
    (tester) async {
      final gateway = FakeGateway();
      await pumpSurface(tester, gateway: gateway);
      await tester.tap(find.byKey(const ValueKey('taskboard-task-MF-02')));
      await tester.pump();
      await tester.enterText(
        find.byKey(const ValueKey('taskboard-input-MF-02')),
        '0',
      );
      await tester.tap(find.byKey(const ValueKey('taskboard-decision-MF-02')));
      await tester.pumpAndSettle();
      expect(find.text('Enter a positive whole number.'), findsOneWidget);

      await tester.enterText(
        find.byKey(const ValueKey('taskboard-input-MF-02')),
        '5000',
      );
      gateway.taskboardDecisionError = StateError('offline');
      await tester.tap(find.byKey(const ValueKey('taskboard-decision-MF-02')));
      await tester.pumpAndSettle();
      expect(find.textContaining('offline'), findsOneWidget);
      final input = tester.widget<TextField>(
        find.byKey(const ValueKey('taskboard-input-MF-02')),
      );
      expect(input.controller?.text, '5000');
    },
  );

  testWidgets(
    'keeps taskboard view, filters, and selection across navigation',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const ui.Size(1280, 800);
      addTearDown(tester.view.reset);
      final gateway = FakeGateway();
      final workspace = await tester.runAsync(
        () => FixtureFrankGateway(latency: Duration.zero).loadWorkspace(),
      );
      final bloc = TaskboardBloc(gateway: gateway);
      addTearDown(bloc.close);

      Widget host(Widget child) => MaterialApp(
        theme: buildFrankTheme(Brightness.dark),
        home: BlocProvider.value(value: bloc, child: child),
      );

      await tester.pumpWidget(host(TaskboardSurface(workspace: workspace!)));
      await tester.pump();
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('taskboard-view-list')));
      await tester.pump();
      await tester.tap(
        find.byKey(const ValueKey('taskboard-attention-filter')),
      );
      await tester.pump();
      expect(
        find.byKey(const ValueKey('taskboard-list-scroll')),
        findsOneWidget,
      );
      expect(find.text('Needs attention  2'), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('taskboard-list-task-NS-03')));
      await tester.pump();
      expect(
        find.byKey(const ValueKey('taskboard-close-detail')),
        findsOneWidget,
      );

      await tester.pumpWidget(host(const SizedBox.shrink()));
      await tester.pump();
      await tester.pumpWidget(host(TaskboardSurface(workspace: workspace)));
      await tester.pump();
      await tester.pump();

      expect(
        find.byKey(const ValueKey('taskboard-list-scroll')),
        findsOneWidget,
      );
      expect(find.text('Needs attention  2'), findsOneWidget);
      expect(
        find.byKey(const ValueKey('taskboard-close-detail')),
        findsOneWidget,
      );
    },
  );
}
