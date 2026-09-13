import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/layout/office_surface_frame.dart';
import 'package:frank_desktop/app/theme.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('OfficeLayoutMetrics', () {
    test('uses the documented breakpoint and gutter boundaries', () {
      final compact = OfficeLayoutMetrics.fromWidth(719);
      expect(compact.layoutClass, OfficeLayoutClass.compact);
      expect(compact.gutter, 16);

      final regular = OfficeLayoutMetrics.fromWidth(720);
      expect(regular.layoutClass, OfficeLayoutClass.regular);
      expect(regular.gutter, 24);

      final regularAtUpperBoundary = OfficeLayoutMetrics.fromWidth(1199);
      expect(regularAtUpperBoundary.layoutClass, OfficeLayoutClass.regular);
      expect(regularAtUpperBoundary.gutter, 24);

      final wide = OfficeLayoutMetrics.fromWidth(1200);
      expect(wide.layoutClass, OfficeLayoutClass.wide);
      expect(wide.gutter, 24);
      expect(OfficeLayoutMetrics.fromWidth(1800).contentWidth, 1240);
    });
  });

  testWidgets('desktop page scrollbar survives hover and controller changes', (
    tester,
  ) async {
    _setWindow(tester, const ui.Size(1000, 560));
    final suppliedController = ScrollController();
    addTearDown(suppliedController.dispose);
    final mouse = await tester.createGesture(kind: ui.PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(0, 0));
    addTearDown(mouse.removePointer);

    for (final controller in [null, suppliedController, null]) {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildFrankTheme(
            Brightness.dark,
          ).copyWith(platform: TargetPlatform.macOS),
          home: OfficeSurfaceFrame.page(
            header: const Text('Page'),
            scrollController: controller,
            slivers: const [SliverToBoxAdapter(child: SizedBox(height: 2000))],
          ),
        ),
      );
      await tester.pump();

      expect(find.byType(Scrollbar), findsOneWidget);
      final scrollbar = tester.widget<Scrollbar>(find.byType(Scrollbar));
      final scrollView = tester.widget<CustomScrollView>(
        find.byType(CustomScrollView),
      );
      expect(scrollbar.controller, isNotNull);
      expect(scrollbar.controller, same(scrollView.controller));
      expect(scrollbar.controller!.positions, hasLength(1));
      if (controller != null) {
        expect(scrollbar.controller, same(controller));
      }
      scrollbar.controller!.jumpTo(0);
      await tester.pump();

      final rect = tester.getRect(find.byType(Scrollbar));
      final thumbPoint = Offset(rect.right - 5, rect.top + 30);
      await mouse.moveTo(thumbPoint);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 250));
      expect(tester.takeException(), isNull);

      await mouse.down(thumbPoint);
      await tester.pump(const Duration(milliseconds: 100));
      await mouse.moveBy(const Offset(0, 100));
      await mouse.up();
      await tester.pumpAndSettle();
      expect(scrollbar.controller!.offset, greaterThan(0));
      expect(tester.takeException(), isNull);
      await mouse.moveTo(const Offset(0, 0));
    }

    await tester.pumpWidget(const SizedBox.shrink());
    expect(suppliedController.hasClients, isFalse);
    // The frame must not dispose a controller owned by its caller.
    suppliedController.addListener(() {});
  });

  testWidgets('page owns one vertical scroll and constrains content', (
    tester,
  ) async {
    _setWindow(tester, const ui.Size(1000, 560));
    await tester.pumpWidget(
      _app(
        OfficeSurfaceFrame.page(
          scrollKey: const ValueKey('test-office-page-scroll'),
          header: const OfficePageHeader(
            title: 'Test page',
            description: 'Shared page heading.',
          ),
          slivers: [
            SliverToBoxAdapter(
              child: Container(
                key: const ValueKey('test-office-page-body'),
                height: 1400,
                color: FrankColors.panelRaised,
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(CustomScrollView), findsOneWidget);
    expect(find.byType(Scrollable), findsOneWidget);
    expect(find.byType(SingleChildScrollView), findsNothing);
    expect(
      find.byKey(const ValueKey('test-office-page-scroll')),
      findsOneWidget,
    );
    final headerBefore = tester.getRect(find.text('Test page'));
    expect(headerBefore.top, greaterThanOrEqualTo(0));

    final bodyRect = tester.getRect(
      find.byKey(const ValueKey('test-office-page-body')),
    );
    expect(bodyRect.width, lessThanOrEqualTo(952));

    await tester.drag(
      find.byKey(const ValueKey('test-office-page-scroll')),
      const Offset(0, -220),
    );
    await tester.pump();
    final scrollable = tester.state<ScrollableState>(find.byType(Scrollable));
    expect(scrollable.position.pixels, greaterThan(0));
    // The frame owns the heading above the single body scroll, so it remains
    // available while the page content moves.
    expect(find.text('Test page'), findsOneWidget);
    expect(tester.getRect(find.text('Test page')).top, greaterThanOrEqualTo(0));
  });

  testWidgets('full-width page keeps only responsive gutters', (tester) async {
    _setWindow(tester, const ui.Size(1600, 560));
    await tester.pumpWidget(
      _app(
        OfficeSurfaceFrame.page(
          fullWidth: true,
          header: const OfficePageHeader(title: 'Full width page'),
          slivers: [
            SliverToBoxAdapter(
              child: Container(
                key: const ValueKey('test-full-width-body'),
                height: 120,
                color: FrankColors.panelRaised,
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();

    final bodyRect = tester.getRect(
      find.byKey(const ValueKey('test-full-width-body')),
    );
    expect(bodyRect.left, 24);
    expect(bodyRect.width, 1552);
  });

  testWidgets('header alignment uses the pre-gutter breakpoint', (
    tester,
  ) async {
    for (final width in [719.0, 720.0]) {
      _setWindow(tester, ui.Size(width, 320));
      await tester.pumpWidget(
        _app(
          OfficeSurfaceFrame.page(
            header: OfficePageHeader(
              title: 'Breakpoint',
              actions: SizedBox(
                key: const ValueKey('breakpoint-actions'),
                width: 40,
                height: 32,
              ),
            ),
            slivers: const [],
          ),
        ),
      );
      await tester.pump();
      final align = tester.widget<Align>(
        find
            .ancestor(
              of: find.byKey(const ValueKey('breakpoint-actions')),
              matching: find.byType(Align),
            )
            .first,
      );
      expect(
        align.alignment,
        width < 720 ? Alignment.centerLeft : Alignment.topRight,
      );
    }
  });

  testWidgets('canvas is full bleed and does not add a scroll owner', (
    tester,
  ) async {
    _setWindow(tester, const ui.Size(1000, 560));
    await tester.pumpWidget(
      _app(
        OfficeSurfaceFrame.canvas(
          child: const ColoredBox(
            key: ValueKey('test-office-canvas-body'),
            color: FrankColors.canvas,
          ),
        ),
      ),
    );
    await tester.pump();

    expect(
      tester.getSize(find.byKey(const ValueKey('test-office-canvas-body'))),
      const ui.Size(1000, 560),
    );
    expect(find.byType(CustomScrollView), findsNothing);
    expect(find.byType(SingleChildScrollView), findsNothing);
  });

  testWidgets('canvas overlay fills the frame without blocking exposed child', (
    tester,
  ) async {
    _setWindow(tester, const ui.Size(1000, 560));
    var tapped = false;
    await tester.pumpWidget(
      _app(
        OfficeSurfaceFrame.canvas(
          child: GestureDetector(
            key: const ValueKey('overlay-test-background'),
            onTap: () => tapped = true,
            child: const ColoredBox(color: FrankColors.canvas),
          ),
          overlay: Align(
            alignment: Alignment.centerRight,
            child: SizedBox(
              key: ValueKey('overlay-test-rail'),
              width: FrankUiTokens.inspectorRailWidth,
              height: double.infinity,
              child: const ColoredBox(color: FrankColors.panelRaised),
            ),
          ),
        ),
      ),
    );
    await tester.pump();

    final rail = tester.getRect(
      find.byKey(const ValueKey('overlay-test-rail')),
    );
    expect(rail, const Rect.fromLTWH(680, 0, 320, 560));

    await tester.tapAt(const Offset(120, 280));
    expect(tapped, isTrue);
  });

  testWidgets('drawer overlays the full frame and resizes without reflow', (
    tester,
  ) async {
    _setWindow(tester, const ui.Size(1280, 560));
    await tester.pumpWidget(
      _app(
        OfficeInspectorDrawerOverlay(
          child: OfficeSurfaceFrame.canvas(
            header: const SizedBox(
              key: ValueKey('inspector-test-header'),
              height: 48,
              child: Text('Header'),
            ),
            child: const ColoredBox(
              key: ValueKey('inspector-test-main'),
              color: FrankColors.canvas,
            ),
          ),
          inspector: const ColoredBox(
            key: ValueKey('inspector-test-inspector'),
            color: FrankColors.panelRaised,
          ),
          inspectorKey: const ValueKey('inspector-test-pane'),
          inspectorLabel: 'Test inspector',
        ),
      ),
    );
    await tester.pump();

    final headerTop = tester.getRect(
      find.byKey(const ValueKey('inspector-test-header')),
    );
    final inspector = tester.getRect(
      find.byKey(const ValueKey('inspector-test-pane')),
    );
    expect(inspector, const Rect.fromLTWH(920, 0, 360, 560));
    expect(
      tester.getRect(find.byKey(const ValueKey('inspector-test-main'))).right,
      1280,
    );

    await tester.drag(
      find.byKey(const ValueKey('office-inspector-resize-handle')),
      const Offset(-40, 0),
    );
    await tester.pump();
    final resized = tester.getRect(
      find.byKey(const ValueKey('inspector-test-pane')),
    );
    expect(resized.width, greaterThan(inspector.width));
    expect(resized.left, lessThan(inspector.left));
    expect(
      tester.getRect(find.byKey(const ValueKey('inspector-test-header'))),
      headerTop,
    );

    await tester.tap(
      find.byKey(const ValueKey('office-inspector-resize-handle')),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(
      tester.getRect(find.byKey(const ValueKey('inspector-test-pane'))).width,
      lessThan(resized.width),
    );

    await tester.drag(
      find.byKey(const ValueKey('office-inspector-resize-handle')),
      const Offset(1000, 0),
    );
    await tester.pump();
    expect(
      tester.getRect(find.byKey(const ValueKey('inspector-test-pane'))).width,
      OfficeInspectorDrawerOverlay.minPanelWidth,
    );
    await tester.drag(
      find.byKey(const ValueKey('office-inspector-resize-handle')),
      const Offset(-1000, 0),
    );
    await tester.pump();
    expect(
      tester.getRect(find.byKey(const ValueKey('inspector-test-pane'))).width,
      OfficeInspectorDrawerOverlay.maxPanelWidth,
    );
    expect(
      tester.getSemantics(find.bySemanticsLabel('Test inspector width')).value,
      '480 pixels',
    );
  });

  testWidgets('drawer covers the root from the top at every feature width', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    for (final width in [640.0, 1087.0, 1088.0, 1280.0, 1600.0]) {
      tester.view.physicalSize = ui.Size(width, 560);
      await tester.pumpWidget(
        _app(
          OfficeInspectorDrawerOverlay(
            child: OfficeSurfaceFrame.canvas(
              header: const SizedBox(
                key: ValueKey('breakpoint-inspector-header'),
                height: 40,
              ),
              child: const ColoredBox(
                key: ValueKey('breakpoint-inspector-main'),
                color: FrankColors.canvas,
              ),
            ),
            inspector: const ColoredBox(
              key: ValueKey('breakpoint-inspector-pane'),
              color: FrankColors.panelRaised,
            ),
            inspectorLabel: 'Breakpoint inspector',
          ),
        ),
      );
      await tester.pump();
      final inspector = tester.getRect(
        find.byKey(const ValueKey('breakpoint-inspector-pane')),
      );
      if (width < OfficeInspectorDrawerOverlay.desktopBreakpoint) {
        expect(inspector.left, 0);
        expect(inspector.width, width);
      } else {
        expect(inspector.width, OfficeInspectorDrawerOverlay.defaultPanelWidth);
        expect(inspector.left, width - inspector.width);
      }
      expect(inspector.top, 0);
      expect(inspector.bottom, 560);
      expect(
        tester
            .getRect(find.byKey(const ValueKey('breakpoint-inspector-header')))
            .top,
        width < 720 ? 16 : 24,
      );
      if (width == 640) {
        expect(
          tester.binding.focusManager.primaryFocus?.debugLabel,
          'office-inspector',
        );
      }
    }
  });

  testWidgets('compact drawer leaves an offset sidebar outside the feature', (
    tester,
  ) async {
    _setWindow(tester, const ui.Size(1088, 560));
    await tester.pumpWidget(
      _app(
        Row(
          children: [
            const SizedBox(
              key: ValueKey('drawer-test-sidebar'),
              width: 240,
              child: ColoredBox(color: FrankColors.sidebarSolid),
            ),
            Expanded(
              child: OfficeInspectorDrawerOverlay(
                child: OfficeSurfaceFrame.canvas(
                  child: const ColoredBox(
                    key: ValueKey('offset-drawer-main'),
                    color: FrankColors.canvas,
                  ),
                ),
                inspector: const ColoredBox(
                  key: ValueKey('offset-drawer-pane'),
                  color: FrankColors.panelRaised,
                ),
                inspectorLabel: 'Offset inspector',
              ),
            ),
          ],
        ),
      ),
    );
    await tester.pump();

    final drawer = tester.getRect(
      find.byKey(const ValueKey('offset-drawer-pane')),
    );
    expect(drawer, const Rect.fromLTWH(240, 0, 848, 560));
    expect(
      tester.getRect(find.byKey(const ValueKey('drawer-test-sidebar'))).right,
      drawer.left,
    );
    expect(
      tester.getRect(find.byKey(const ValueKey('offset-drawer-main'))).right,
      1088,
    );
  });
}

Widget _app(Widget child) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: buildFrankTheme(Brightness.dark),
  home: child,
);

void _setWindow(WidgetTester tester, ui.Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}
