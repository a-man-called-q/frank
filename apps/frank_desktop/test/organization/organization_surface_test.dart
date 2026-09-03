import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/icons.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/organization/bloc/organization_bloc.dart';
import 'package:frank_desktop/features/organization/organization_surface.dart';

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
            .widget<TextButton>(find.widgetWithText(TextButton, 'Undo'))
            .onPressed,
        isNull,
      );

      bloc.add(
        const OrganizationNodeMoved('staff-maya', OrganizationPoint(42, 42)),
      );
      await tester.pump(const Duration(milliseconds: 10));
      expect(
        tester
            .widget<TextButton>(find.widgetWithText(TextButton, 'Undo'))
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

  testWidgets('add palette is searchable', (tester) async {
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

    expect(find.text('Execution'), findsNothing);
    expect(find.text('Terminal'), findsNWidgets(2));
    await tester.enterText(find.byType(TextField).last, 'database');
    await tester.pump();
    expect(find.text('Database'), findsNWidgets(2));
    expect(find.text('Email'), findsOneWidget);
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
    await tester.enterText(find.byType(TextField).last, 'group');
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-group')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('organization-group-name')),
      'Research',
    );
    await tester.tap(find.byKey(const ValueKey('organization-group-create')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 10));

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
