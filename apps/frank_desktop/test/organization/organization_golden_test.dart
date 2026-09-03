import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/organization/bloc/organization_bloc.dart';
import 'package:frank_desktop/features/organization/organization_surface.dart';

import '../support/fake_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    final font = await rootBundle.load('assets/fonts/Geist-Variable.ttf');
    final loader = FontLoader('Geist')..addFont(Future<ByteData>.value(font));
    await loader.load();
    final lucideFont = await rootBundle.load(
      'packages/forui_assets/assets/lucide.ttf',
    );
    final lucideLoader = FontLoader('ForuiLucideIcons')
      ..addFont(Future<ByteData>.value(lucideFont));
    await lucideLoader.load();
  });

  testWidgets('staff selected with inspector', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    bloc.add(const OrganizationSelectionChanged(nodeId: 'staff-maya'));
    await tester.pump();
    await expectLater(
      find.byType(OrganizationSurface),
      matchesGoldenFile('goldens/organization-staff-selected.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('capability selected with inspector', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    bloc.add(const OrganizationSelectionChanged(nodeId: 'email-primary'));
    await tester.pump();
    await expectLater(
      find.byType(OrganizationSurface),
      matchesGoldenFile('goldens/organization-capability-selected.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('validation warning state', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    bloc.add(const OrganizationValidateRequested());
    await tester.pump();
    await expectLater(
      find.byType(OrganizationSurface),
      matchesGoldenFile('goldens/organization-validation-warning.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('compact inspector sheet', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(880, 640);
    addTearDown(tester.view.reset);
    final bloc = await _pumpOrganization(tester, size: const ui.Size(880, 640));
    addTearDown(bloc.close);
    bloc.add(const OrganizationSelectionChanged(nodeId: 'staff-maya'));
    await tester.pumpAndSettle();
    expect(bloc.state.selectedNodeId, 'staff-maya');
    expect(find.byType(BottomSheet), findsOneWidget);
    expect(find.text('Staff'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await expectLater(
      // The compact inspector is presented through MaterialApp's overlay,
      // outside OrganizationSurface's render subtree. Capture the overlay so
      // this golden proves the bottom sheet is actually visible.
      find.byType(Overlay),
      matchesGoldenFile('goldens/organization-compact.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}

Future<OrganizationBloc> _pumpOrganization(
  WidgetTester tester, {
  ui.Size size = const ui.Size(1600, 1000),
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
  final bloc = OrganizationBloc(
    gateway: FakeGateway(organization: fixtureOrganizationGraph()),
  );
  await tester.pumpWidget(
    MediaQuery(
      data: const MediaQueryData(disableAnimations: true),
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: buildFrankTheme(Brightness.dark),
        home: BlocProvider.value(
          value: bloc,
          child: OrganizationSurface(workspace: _workspace()),
        ),
      ),
    ),
  );
  await tester.pump(const Duration(milliseconds: 30));
  return bloc;
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
