import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/organization/bloc/organization_bloc.dart';
import 'package:frank_desktop/features/organization/presentation/organization_surface.dart';

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
      find.byKey(const ValueKey('organization-golden-root')),
      matchesGoldenFile('goldens/organization-staff-selected.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('capability selected with inspector', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    bloc.add(const OrganizationSelectionChanged(nodeId: 'email-primary'));
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('organization-golden-root')),
      matchesGoldenFile('goldens/organization-capability-selected.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('validation warning state', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    bloc.add(const OrganizationValidateRequested());
    await tester.pump();
    await expectLater(
      find.byKey(const ValueKey('organization-golden-root')),
      matchesGoldenFile('goldens/organization-validation-warning.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('compact inspector covers the feature from the top', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(880, 640);
    addTearDown(tester.view.reset);
    final bloc = await _pumpOrganization(tester, size: const ui.Size(880, 640));
    addTearDown(bloc.close);
    bloc.add(const OrganizationSelectionChanged(nodeId: 'staff-maya'));
    await tester.pumpAndSettle();
    expect(bloc.state.selectedNodeId, 'staff-maya');
    expect(
      find.byKey(const ValueKey('organization-inspector-rail')),
      findsOneWidget,
    );
    final inspector = tester.getRect(
      find.byKey(const ValueKey('organization-inspector-rail')),
    );
    expect(inspector.left, 0);
    expect(inspector.width, 880);
    expect(inspector.top, 0);
    expect(inspector.bottom, 640);
    expect(find.text('Staff'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await expectLater(
      find.byKey(const ValueKey('organization-golden-root')),
      matchesGoldenFile('goldens/organization-compact.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('compact inspector supports 200% text scaling', (tester) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const ui.Size(880, 640);
    addTearDown(tester.view.reset);
    final bloc = await _pumpOrganization(
      tester,
      size: const ui.Size(880, 640),
      textScale: 2,
    );
    addTearDown(bloc.close);
    bloc.add(const OrganizationSelectionChanged(nodeId: 'staff-maya'));
    await tester.pumpAndSettle();
    final inspector = tester.getRect(
      find.byKey(const ValueKey('organization-inspector-rail')),
    );
    expect(inspector.left, 0);
    expect(inspector.width, 880);
    expect(inspector.top, 0);
    expect(inspector.bottom, 640);
    await expectLater(
      find.byKey(const ValueKey('organization-golden-root')),
      matchesGoldenFile('goldens/organization-compact-text-scale-200.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('add palette quick picks golden', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();
    await expectLater(
      find.byType(Overlay),
      matchesGoldenFile('goldens/organization-add-palette.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('add palette new group golden', (tester) async {
    final bloc = await _pumpOrganization(tester);
    addTearDown(bloc.close);
    await tester.tap(find.byKey(const ValueKey('organization-add')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('organization-add-search')),
      'group',
    );
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-group')));
    await tester.pumpAndSettle();
    await expectLater(
      find.byType(Overlay),
      matchesGoldenFile('goldens/organization-add-group.png'),
    );
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}

Future<OrganizationBloc> _pumpOrganization(
  WidgetTester tester, {
  ui.Size size = const ui.Size(1600, 1000),
  double textScale = 1,
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
  final bloc = OrganizationBloc(
    gateway: FakeGateway(organization: fixtureOrganizationGraph()),
  );
  await tester.pumpWidget(
    MediaQuery(
      data: MediaQueryData(
        disableAnimations: true,
        textScaler: TextScaler.linear(textScale),
      ),
      child: RepaintBoundary(
        key: const ValueKey('organization-golden-root'),
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: buildFrankTheme(Brightness.dark),
          home: BlocProvider.value(
            value: bloc,
            child: OrganizationSurface(workspace: _workspace()),
          ),
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
