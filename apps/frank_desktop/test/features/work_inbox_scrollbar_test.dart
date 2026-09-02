import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/shell/main_sidebar.dart';

import '../support/fake_gateway.dart';

void main() {
  testWidgets('mission shelves use one thin overlay scrollbar', (tester) async {
    await _pumpInbox(tester, missionCount: 32);

    final scrollbar = _scrollbar(tester);
    final listFinder = find.byKey(const ValueKey('mission-shelf-scroll-view'));
    final list = tester.widget<ListView>(listFinder);
    final controller = list.controller!;

    expect(scrollbar.controller, same(controller));
    expect(scrollbar.thumbVisibility, isFalse);
    expect(scrollbar.trackVisibility, isFalse);
    expect(scrollbar.thickness, 3);
    expect(scrollbar.radius, const Radius.circular(999));
    expect(scrollbar.thumbColor, FrankColors.muted.withValues(alpha: 0.42));
    expect(scrollbar.minThumbLength, 32);
    expect(scrollbar.fadeDuration, const Duration(milliseconds: 150));
    expect(scrollbar.timeToFade, const Duration(milliseconds: 600));
    expect(scrollbar.mainAxisMargin, 4);
    expect(scrollbar.crossAxisMargin, 2);
    expect(scrollbar.interactive, isTrue);
    expect(controller.position.maxScrollExtent, greaterThan(0));
    final edgeMask = tester.widget<ShaderMask>(
      find.byKey(const ValueKey('work-inbox-edge-fade-mask')),
    );
    expect(edgeMask.blendMode, BlendMode.dstIn);
    expect(find.byKey(const ValueKey('work-inbox-top-fade')), findsNothing);
    expect(find.byKey(const ValueKey('work-inbox-bottom-fade')), findsNothing);

    final sidebarRect = tester.getRect(find.byType(MainSidebarContent));
    final scrollbarRect = tester.getRect(find.byType(RawScrollbar));
    expect(scrollbarRect.right, closeTo(sidebarRect.right, 1.5));

    await tester.drag(listFinder, const Offset(0, -280));
    await tester.pump();
    expect(controller.offset, greaterThan(0));

    controller.jumpTo(controller.position.maxScrollExtent);
    await tester.pump();
    expect(
      find.byKey(const ValueKey('work-inbox-edge-fade-mask')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('search results reuse the overlay scrollbar and reset to top', (
    tester,
  ) async {
    await _pumpInbox(tester, missionCount: 32);

    final shelfList = find.byKey(const ValueKey('mission-shelf-scroll-view'));
    final shelfController = tester.widget<ListView>(shelfList).controller!;
    await tester.drag(shelfList, const Offset(0, -280));
    await tester.pump();
    expect(shelfController.offset, greaterThan(0));

    await tester.enterText(_searchField(), 'Mission');
    await tester.pump();
    // The replacement list reports its scroll metrics after this frame; let
    // the edge-fade state render before inspecting the mask.
    await tester.pump();

    final searchList = find.byKey(const ValueKey('search-results-scroll-view'));
    final searchController = tester.widget<ListView>(searchList).controller!;
    expect(searchController, isNot(same(shelfController)));
    expect(searchController.position.maxScrollExtent, greaterThan(0));
    expect(find.byType(RawScrollbar), findsOneWidget);
    expect(
      find.byKey(const ValueKey('work-inbox-edge-fade-mask')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('work-inbox-top-fade')), findsNothing);
    expect(find.byKey(const ValueKey('work-inbox-bottom-fade')), findsNothing);

    await tester.drag(searchList, const Offset(0, -280));
    await tester.pump();
    expect(searchController.offset, greaterThan(0));

    await tester.enterText(_searchField(), 'Mission 3');
    await tester.pump();
    expect(searchController.offset, 0);

    await tester.tap(find.byTooltip('Clear the workspace search'));
    await tester.pump();
    expect(
      tester
          .widget<ListView>(
            find.byKey(const ValueKey('mission-shelf-scroll-view')),
          )
          .controller!
          .offset,
      greaterThan(0),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('short mission shelves have no scroll extent', (tester) async {
    await _pumpInbox(tester, missionCount: 1);

    final list = find.byKey(const ValueKey('mission-shelf-scroll-view'));
    final controller = tester.widget<ListView>(list).controller!;
    expect(find.byType(RawScrollbar), findsOneWidget);
    expect(controller.position.maxScrollExtent, 0);
    expect(
      find.byKey(const ValueKey('work-inbox-edge-fade-mask')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('work-inbox-top-fade')), findsNothing);
    expect(find.byKey(const ValueKey('work-inbox-bottom-fade')), findsNothing);

    await tester.drag(list, const Offset(0, -280));
    await tester.pump();
    expect(controller.offset, 0);
    expect(tester.takeException(), isNull);
  });

  testWidgets('mission titles use two-line fade with a one-line metadata fade', (
    tester,
  ) async {
    const title =
        'A mission title that is long enough to wrap across two lines in the sidebar';
    await _pumpInbox(tester, missionCount: 1, firstMissionTitle: title);

    final titleText = tester.widget<Text>(
      find.descendant(
        of: find.byKey(const ValueKey('mission-shelf-scroll-view')),
        matching: find.text(title),
      ),
    );
    expect(titleText.maxLines, 2);
    expect(titleText.softWrap, isTrue);
    expect(titleText.overflow, TextOverflow.fade);

    final metadataText = tester.widget<Text>(
      find.text('Scroll Project · Draft'),
    );
    expect(metadataText.maxLines, 1);
    expect(metadataText.overflow, TextOverflow.fade);
  });
}

RawScrollbar _scrollbar(WidgetTester tester) =>
    tester.widget<RawScrollbar>(find.byType(RawScrollbar));

Finder _searchField() => find.byWidgetPredicate(
  (widget) =>
      widget is TextField && widget.decoration?.hintText == 'Search workspace',
);

Future<void> _pumpInbox(
  WidgetTester tester, {
  required int missionCount,
  String? firstMissionTitle,
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const ui.Size(880, 640);
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    FrankApp(
      gateway: FakeGateway(
        workspace: _workspace(
          missionCount,
          firstMissionTitle: firstMissionTitle,
        ),
      ),
    ),
  );
  await tester.pump(const Duration(milliseconds: 500));
  await tester.tap(find.bySemanticsLabel('Projects view'));
  await tester.pump(const Duration(milliseconds: 220));
  // The first metrics notification arrives after layout and schedules the
  // directional fade state for the following frame.
  await tester.pump();
}

OfficeWorkspace _workspace(int missionCount, {String? firstMissionTitle}) {
  const employee = OfficeEmployee(
    id: 'ae',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
  );
  final missions = List<OfficeMission>.generate(
    missionCount,
    (index) => OfficeMission(
      id: 'mission-$index',
      title: index == 0 && firstMissionTitle != null
          ? firstMissionTitle
          : 'Mission ${index + 1}',
      status: MissionStatus.draft,
      updatedAt: DateTime.utc(2026, 1, 1).add(Duration(days: index)),
      messages: const [],
    ),
  );
  final project = OfficeProject(
    id: 'scroll-project',
    name: 'Scroll Project',
    client: 'Scroll Client',
    status: ProjectStatus.active,
    progress: 0.4,
    team: const ['Maya Chen'],
    summary: 'A workspace used to exercise the mission viewport.',
    messages: const [],
    missions: missions,
  );
  return OfficeWorkspace(
    name: 'Scroll Agency',
    projects: [project],
    employees: const [employee],
    accountExecutive: employee,
  );
}
