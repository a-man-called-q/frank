import 'dart:ui' show Tristate;

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/app/layout/office_surface_frame.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/organization/bloc/organization_bloc.dart';
import 'package:frank_desktop/features/organization/presentation/organization_surface.dart';

import '../support/fake_gateway.dart';

void main() {
  test('validation summary pluralizes errors and warnings independently', () {
    const singular = OrganizationValidation([
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.error,
        message: 'One error',
      ),
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.warning,
        message: 'One warning',
      ),
    ]);
    const plural = OrganizationValidation([
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.error,
        message: 'First error',
      ),
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.error,
        message: 'Second error',
      ),
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.warning,
        message: 'First warning',
      ),
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.warning,
        message: 'Second warning',
      ),
    ]);

    expect(organizationValidationSummary(singular), '1 error · 1 warning');
    expect(organizationValidationSummary(plural), '2 errors · 2 warnings');
  });

  testWidgets(
    'fixture renders agency catalog, groups, toolbar, and semantics',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1600, 1000);
      addTearDown(tester.view.reset);
      final workspace = _workspace();
      final gateway = FakeGateway(organization: fixtureOrganizationGraph());
      final bloc = OrganizationBloc(gateway: gateway);
      addTearDown(bloc.close);

      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: MaterialApp(
            theme: buildFrankTheme(Brightness.dark),
            home: BlocProvider.value(
              value: bloc,
              child: OrganizationSurface(workspace: workspace),
            ),
          ),
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 30));

      expect(find.bySemanticsLabel('Organization flow editor'), findsOneWidget);
      expect(
        find.bySemanticsLabel('Organization editor toolbar'),
        findsOneWidget,
      );
      expect(find.text('Maya Chen'), findsOneWidget);
      expect(find.text('Email'), findsOneWidget);
      expect(find.text('Approval Desk'), findsOneWidget);
      expect(find.text('Client Services'), findsOneWidget);
      expect(find.text('Delivery'), findsOneWidget);
      expect(find.text('Operations & Review'), findsOneWidget);
      expect(find.text('0 errors · 1 warning'), findsOneWidget);
      final validationIcon = tester.widget<Icon>(
        find.byKey(const ValueKey('organization-validation-status-icon')),
      );
      expect(validationIcon.icon, FrankIcons.circleAlert);
      expect(validationIcon.color, FrankColors.warningAmber);
      expect(
        find.descendant(
          of: find.byType(OrganizationSurface),
          matching: find.byType(BackdropFilter),
        ),
        findsNothing,
      );
      expect(
        find.byKey(const ValueKey('organization-publish')),
        findsOneWidget,
      );
      for (final key in [
        'organization-undo',
        'organization-redo',
        'organization-validate',
        'organization-zoom-out',
        'organization-zoom',
        'organization-zoom-in',
        'organization-zoom-100',
        'organization-fit',
        'organization-minimap',
      ]) {
        expect(find.byKey(ValueKey(key)), findsOneWidget);
      }
      expect(find.text('Undo'), findsNothing);
      expect(find.text('Redo'), findsNothing);
      expect(find.text('Fit view'), findsNothing);
      expect(find.text('Minimap'), findsNothing);
      expect(find.byTooltip('Undo'), findsOneWidget);
      expect(find.byTooltip('Redo'), findsOneWidget);
      expect(find.byTooltip('Fit view'), findsOneWidget);
      expect(find.byTooltip('Minimap'), findsOneWidget);
      final minimapSemantics = tester.getSemantics(
        find.bySemanticsLabel('Minimap'),
      );
      expect(minimapSemantics.flagsCollection.isButton, isTrue);
      expect(minimapSemantics.flagsCollection.isToggled, Tristate.isTrue);
      expect(
        tester
            .widget<FilledButton>(
              find.byKey(const ValueKey('organization-publish')),
            )
            .onPressed,
        isNotNull,
      );
      expect(
        tester
            .widget<IconButton>(find.byKey(const ValueKey('organization-undo')))
            .onPressed,
        isNull,
      );

      await tester.tap(find.byKey(const ValueKey('organization-minimap')));
      await tester.pump();
      expect(
        tester
            .getSemantics(find.bySemanticsLabel('Minimap'))
            .flagsCollection
            .isToggled,
        Tristate.isFalse,
      );

      bloc.add(
        const OrganizationNodeMoved('staff-maya', OrganizationPoint(42, 42)),
      );
      await tester.pump(const Duration(milliseconds: 10));
      expect(
        tester
            .widget<IconButton>(find.byKey(const ValueKey('organization-undo')))
            .onPressed,
        isNotNull,
      );
      expect(
        tester
            .widget<FilledButton>(
              find.byKey(const ValueKey('organization-publish')),
            )
            .onPressed,
        isNull,
      );
      expect(find.bySemanticsLabel('Maya Chen output port'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pump(const Duration(milliseconds: 600));
    },
  );

  testWidgets(
    'desktop inspector overlays the frame and canvas remains interactive',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1200, 800);
      addTearDown(tester.view.reset);
      final bloc = OrganizationBloc(
        gateway: FakeGateway(organization: fixtureOrganizationGraph()),
      );
      addTearDown(bloc.close);

      await tester.pumpWidget(
        MaterialApp(
          theme: buildFrankTheme(Brightness.dark),
          home: BlocProvider.value(
            value: bloc,
            child: OrganizationSurface(workspace: _workspace()),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 30));
      final frameBefore = tester.getRect(
        find.byKey(const ValueKey('office-surface-frame-canvas')),
      );
      final publishBefore = tester.getRect(
        find.byKey(const ValueKey('organization-publish')),
      );
      bloc.add(const OrganizationSelectionChanged(nodeId: 'staff-maya'));
      await tester.pump();

      final rail = tester.getRect(
        find.byKey(const ValueKey('organization-inspector-rail')),
      );
      expect(rail.left, 840);
      expect(rail.top, 0);
      expect(rail.bottom, 800);
      expect(rail.width, OfficeInspectorDrawerOverlay.defaultPanelWidth);
      expect(
        tester.getRect(
          find.byKey(const ValueKey('office-surface-frame-canvas')),
        ),
        frameBefore,
      );
      expect(
        tester.getRect(find.byKey(const ValueKey('organization-publish'))),
        publishBefore,
      );

      // Email is in the exposed client-services group. Selecting it proves the
      // drawer overlays only the right side and leaves the canvas interactive.
      await tester.tap(find.bySemanticsLabel(RegExp(r'^Email capability')));
      await tester.pump();
      expect(bloc.state.selectedNodeId, 'email-primary');
      expect(
        find.byKey(const ValueKey('organization-inspector-rail')),
        findsOneWidget,
      );

      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      expect(bloc.state.selectedNodeId, isNull);
      await tester.pump(const Duration(milliseconds: 400));
    },
  );

  testWidgets('add palette shows curated picks and searches all metadata', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 800);
    addTearDown(tester.view.reset);
    final workspace = _workspace();
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
    );
    addTearDown(bloc.close);

    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: MaterialApp(
          theme: buildFrankTheme(Brightness.dark),
          home: BlocProvider.value(
            value: bloc,
            child: OrganizationSurface(workspace: workspace),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 30));
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();

    expect(find.bySemanticsLabel('Add to office'), findsOneWidget);
    expect(find.text('Add to office'), findsOneWidget);
    expect(find.text('QUICK ADD'), findsOneWidget);
    expect(find.textContaining('available'), findsOneWidget);
    expect(
      find.byKey(
        const ValueKey('organization-add-result-capability-taskboard'),
      ),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('organization-add-result-capability-drive')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('add-group')), findsOneWidget);
    expect(find.text('Terminal'), findsOneWidget);
    expect(find.text('Database'), findsOneWidget);

    expect(find.text('Execution'), findsNothing);
    await tester.enterText(
      find.byKey(const ValueKey('organization-add-search')),
      'database',
    );
    await tester.pump();
    expect(find.text('RESULTS'), findsOneWidget);
    expect(find.text('Terminal'), findsOneWidget);
    expect(find.text('Database'), findsNWidgets(2));
    expect(find.text('Email'), findsOneWidget);
    expect(find.text('inspect · read · write'), findsOneWidget);
  });

  testWidgets('add palette is keyboard navigable and closes with Escape', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 800);
    addTearDown(tester.view.reset);
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
    );
    addTearDown(bloc.close);

    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: MaterialApp(
          theme: buildFrankTheme(Brightness.dark),
          home: BlocProvider.value(
            value: bloc,
            child: OrganizationSurface(workspace: _workspace()),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 30));
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();

    final taskboard = find.byKey(
      const ValueKey('organization-add-result-capability-taskboard'),
    );
    final drive = find.byKey(
      const ValueKey('organization-add-result-capability-drive'),
    );
    final group = find.byKey(const ValueKey('add-group'));
    expect(
      tester.getSemantics(taskboard).flagsCollection.isSelected,
      Tristate.isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(
      tester.getSemantics(drive).flagsCollection.isSelected,
      Tristate.isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(
      tester.getSemantics(taskboard).flagsCollection.isSelected,
      Tristate.isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.end);
    await tester.pump();
    expect(
      tester.getSemantics(group).flagsCollection.isSelected,
      Tristate.isTrue,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.home);
    await tester.pump();
    expect(
      tester.getSemantics(taskboard).flagsCollection.isSelected,
      Tristate.isTrue,
    );
    final nodeCountBeforeEnter = bloc.state.graph!.nodes.length;
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(bloc.state.graph!.nodes, hasLength(nodeCountBeforeEnter + 1));
    expect(find.bySemanticsLabel('Add to office'), findsNothing);
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('organization-add-search')));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.bySemanticsLabel('Add to office'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('add palette supports no-results and outside dismissal', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 800);
    addTearDown(tester.view.reset);
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
    );
    addTearDown(bloc.close);

    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: MaterialApp(
          theme: buildFrankTheme(Brightness.dark),
          home: BlocProvider.value(
            value: bloc,
            child: OrganizationSurface(workspace: _workspace()),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 30));
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('organization-add-search')),
      'does-not-exist',
    );
    await tester.pump();
    expect(find.text('No matching office element.'), findsOneWidget);
    expect(find.bySemanticsLabel('Add to office results'), findsOneWidget);
    await tester.tapAt(const Offset(12, 12));
    await tester.pumpAndSettle();
    expect(find.bySemanticsLabel('Add to office'), findsNothing);
  });

  testWidgets('group step returns to browse and preserves its query', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 800);
    addTearDown(tester.view.reset);
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
    );
    addTearDown(bloc.close);

    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: MaterialApp(
          theme: buildFrankTheme(Brightness.dark),
          home: BlocProvider.value(
            value: bloc,
            child: OrganizationSurface(workspace: _workspace()),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 30));
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('organization-add-search')),
      'group',
    );
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-group')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('organization-group-name')),
      'x' * 60,
    );
    await tester.pump();
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('organization-group-name')),
          )
          .controller!
          .text
          .length,
      48,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.bySemanticsLabel('New group'), findsNothing);
    expect(find.text('RESULTS'), findsOneWidget);
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('organization-add-search')),
          )
          .controller!
          .text,
      'group',
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.bySemanticsLabel('Add to office'), findsNothing);
  });

  testWidgets('empty organization offers onboarding and adds staff', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 800);
    addTearDown(tester.view.reset);
    final bloc = OrganizationBloc(
      gateway: FakeGateway(
        organization: const OrganizationGraph(
          id: 'empty-agency',
          draftRevision: 0,
          publishedRevision: 0,
          nodes: [],
          relations: [],
        ),
      ),
      autosaveDelay: const Duration(milliseconds: 5),
    );
    addTearDown(bloc.close);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildFrankTheme(Brightness.dark),
        home: BlocProvider.value(
          value: bloc,
          child: OrganizationSurface(workspace: _workspace()),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 30));

    expect(find.text('Build your agency flow'), findsOneWidget);
    await tester.tap(find.text('Add office element'));
    await tester.pumpAndSettle();
    expect(find.text('Maya Chen'), findsOneWidget);
    await tester.tap(find.text('Maya Chen'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 10));

    expect(bloc.state.graph!.nodes, hasLength(1));
    expect(bloc.state.graph!.nodes.single.employeeId, 'ae-maya');
    expect(bloc.state.selectedNodeId, bloc.state.graph!.nodes.single.id);
  });

  testWidgets('add palette creates a square custom group', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 800);
    addTearDown(tester.view.reset);
    final workspace = _workspace();
    final bloc = OrganizationBloc(
      gateway: FakeGateway(organization: fixtureOrganizationGraph()),
      autosaveDelay: const Duration(milliseconds: 5),
    );
    addTearDown(bloc.close);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildFrankTheme(Brightness.dark),
        home: BlocProvider.value(
          value: bloc,
          child: OrganizationSurface(workspace: workspace),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 30));
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('organization-add-search')),
      'group',
    );
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-group')));
    await tester.pumpAndSettle();
    expect(find.bySemanticsLabel('New group'), findsOneWidget);
    expect(
      tester
          .widget<FilledButton>(
            find.byKey(const ValueKey('organization-group-create')),
          )
          .onPressed,
      isNull,
    );
    await tester.enterText(
      find.byKey(const ValueKey('organization-group-name')),
      '  Research  ',
    );
    await tester.pump();
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('organization-group-name')),
          )
          .controller!
          .text,
      '  Research  ',
    );
    expect(
      tester
          .widget<FilledButton>(
            find.byKey(const ValueKey('organization-group-create')),
          )
          .onPressed,
      isNotNull,
    );
    await tester.tap(find.byKey(const ValueKey('organization-group-create')));
    await tester.pumpAndSettle();
    expect(find.bySemanticsLabel('New group'), findsNothing);

    expect(
      bloc.state.graph!.groups,
      contains(
        predicate<OrganizationGroup>(
          (group) =>
              group.label == 'Research' &&
              !group.locked &&
              group.tone == OrganizationGroupTone.aubergine,
        ),
      ),
    );
    expect(
      find.byWidgetPredicate(
        (widget) =>
            widget is Text &&
            widget.key is ValueKey<String> &&
            (widget.key! as ValueKey<String>).value.startsWith(
              'organization-group-heading-custom-group-',
            ),
      ),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });
}

OfficeWorkspace _workspace() => const OfficeWorkspace(
  name: 'Frank Agency',
  projects: [],
  employees: [
    OfficeEmployee(
      id: 'ae-maya',
      name: 'Maya Chen',
      role: 'Account Executive',
      status: 'Available',
      initials: 'MC',
      color: 0xFF9A68A5,
    ),
    OfficeEmployee(
      id: 'analyst-budi',
      name: 'Budi Santoso',
      role: 'System Analyst',
      status: 'Working',
      initials: 'BS',
      color: 0xFF82B7E8,
    ),
    OfficeEmployee(
      id: 'programmer-nia',
      name: 'Nia Alvarez',
      role: 'Junior Programmer',
      status: 'Idle',
      initials: 'NA',
      color: 0xFF77C69B,
    ),
    OfficeEmployee(
      id: 'accountant-dimas',
      name: 'Dimas Pratama',
      role: 'Accountant',
      status: 'Reviewing',
      initials: 'DP',
      color: 0xFFBE9DEB,
    ),
  ],
  accountExecutive: OfficeEmployee(
    id: 'ae-maya',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
  ),
);
