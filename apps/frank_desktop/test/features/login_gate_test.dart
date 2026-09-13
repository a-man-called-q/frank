import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/frank_app.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

import '../support/fake_gateway.dart';

void main() {
  testWidgets('title screen preloads the shell behind the login overlay', (
    tester,
  ) async {
    _setWindow(tester);
    await tester.pumpWidget(FrankApp(gateway: _fastGateway()));
    await tester.pump();
    // The zero-latency fixture still schedules one timer; drain it so this
    // initial-state test does not leave an outstanding async task behind.
    await tester.pump(const Duration(milliseconds: 1));

    expect(find.bySemanticsLabel('Frank login'), findsOneWidget);
    expect(find.byKey(const ValueKey('login-scene-stage')), findsOneWidget);
    expect(find.byKey(const ValueKey('login-ambient')), findsOneWidget);
    expect(find.byKey(const ValueKey('login-wordmark')), findsOneWidget);
    expect(find.byKey(const ValueKey('login-username-field')), findsOneWidget);
    expect(find.byKey(const ValueKey('login-password-field')), findsOneWidget);
    expect(find.byKey(const ValueKey('login-submit-button')), findsOneWidget);
    expect(find.byType(Card), findsNothing);
    expect(
      find.byKey(const ValueKey('preloaded-office-shell')),
      findsOneWidget,
    );
    expect(find.bySemanticsLabel('Office'), findsNothing);
    expect(
      tester
          .widget<FadeTransition>(
            find.byKey(const ValueKey('login-shell-fade')),
          )
          .opacity
          .value,
      closeTo(0.0, 1e-6),
    );

    await tester.tap(find.byKey(const ValueKey('login-submit-button')));
    await tester.pump();

    expect(find.text('Enter your username'), findsOneWidget);
    expect(find.text('Enter your password'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('valid mock login reveals the preloaded office without motion', (
    tester,
  ) async {
    _setWindow(tester);
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(disableAnimations: true),
        child: FrankApp(gateway: _fastGateway()),
      ),
    );
    await tester.pump();

    final shellElement = tester.element(
      find.byKey(const ValueKey('preloaded-office-shell')),
    );
    await _enterCredentials(tester);
    await tester.tap(find.byKey(const ValueKey('login-submit-button')));
    await tester.pump();
    expect(find.text('Preparing workspace…'), findsOneWidget);

    await tester.pump(const Duration(milliseconds: 600));
    await tester.pump();

    expect(find.byKey(const ValueKey('login-scene-stage')), findsNothing);
    expect(find.bySemanticsLabel('Frank login'), findsNothing);
    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(
      identical(
        shellElement,
        tester.element(find.byKey(const ValueKey('preloaded-office-shell'))),
      ),
      isTrue,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('login and shell crossfade together while retaining the shell', (
    tester,
  ) async {
    _setWindow(tester);
    await tester.pumpWidget(FrankApp(gateway: _fastGateway()));
    await tester.pump();
    final shellElement = tester.element(
      find.byKey(const ValueKey('preloaded-office-shell')),
    );

    await _enterCredentials(tester);
    await tester.tap(find.byKey(const ValueKey('login-submit-button')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pump(const Duration(milliseconds: 180));

    final loginOpacity = tester
        .widget<FadeTransition>(
          find.byKey(const ValueKey('login-overlay-fade')),
        )
        .opacity
        .value;
    final shellOpacity = tester
        .widget<FadeTransition>(find.byKey(const ValueKey('login-shell-fade')))
        .opacity
        .value;
    expect(loginOpacity, greaterThan(0.0));
    expect(loginOpacity, lessThan(1.0));
    expect(shellOpacity, greaterThan(0.0));
    expect(shellOpacity, lessThan(1.0));
    expect(
      identical(
        shellElement,
        tester.element(find.byKey(const ValueKey('preloaded-office-shell'))),
      ),
      isTrue,
    );

    await tester.pump(const Duration(milliseconds: 600));
    await tester.pump();
    expect(find.byKey(const ValueKey('login-overlay-fade')), findsNothing);
    expect(find.byKey(const ValueKey('global-nav-office')), findsOneWidget);
    expect(
      identical(
        shellElement,
        tester.element(find.byKey(const ValueKey('preloaded-office-shell'))),
      ),
      isTrue,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('transition waits for a slow shell bootstrap', (tester) async {
    _setWindow(tester);
    await tester.pumpWidget(
      FrankApp(gateway: _DelayedGateway(const Duration(milliseconds: 900))),
    );
    await tester.pump();
    await _enterCredentials(tester);
    await tester.tap(find.byKey(const ValueKey('login-submit-button')));
    await tester.pump();

    await tester.pump(const Duration(milliseconds: 600));
    expect(
      tester
          .widget<FadeTransition>(
            find.byKey(const ValueKey('login-overlay-fade')),
          )
          .opacity
          .value,
      closeTo(1.0, 1e-6),
    );
    expect(
      tester
          .widget<FadeTransition>(
            find.byKey(const ValueKey('login-shell-fade')),
          )
          .opacity
          .value,
      closeTo(0.0, 1e-6),
    );

    await tester.pump(const Duration(milliseconds: 300));
    await tester.pump(const Duration(milliseconds: 180));
    // The delayed gateway adds the fixture's default 180ms after its own
    // delay. Give the status callback a frame to begin the reveal.
    await tester.pump(const Duration(milliseconds: 180));
    expect(
      tester
          .widget<FadeTransition>(
            find.byKey(const ValueKey('login-overlay-fade')),
          )
          .opacity
          .value,
      lessThan(1.0),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('shell failures reveal the retryable shell state smoothly', (
    tester,
  ) async {
    _setWindow(tester);
    await tester.pumpWidget(
      FrankApp(gateway: FakeGateway(loadError: StateError('offline'))),
    );
    await tester.pump();
    // Let the prewarmed shell publish its failure before the form submit.
    await tester.pump(const Duration(milliseconds: 20));
    await _enterCredentials(tester);
    await tester.tap(find.byKey(const ValueKey('login-submit-button')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 600));
    await tester.pump(const Duration(milliseconds: 720));
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pump();

    expect(find.byKey(const ValueKey('login-overlay-fade')), findsNothing);
    expect(find.text('Retry'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('login form remains usable on a short desktop viewport', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(880, 340);
    addTearDown(tester.view.reset);
    await tester.pumpWidget(FrankApp(gateway: _fastGateway()));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 1));

    expect(find.byKey(const ValueKey('login-username-field')), findsOneWidget);
    expect(find.byType(SingleChildScrollView), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

Future<void> _enterCredentials(WidgetTester tester) async {
  await tester.enterText(
    find.byKey(const ValueKey('login-username-field')),
    'demo_owner',
  );
  await tester.enterText(
    find.byKey(const ValueKey('login-password-field')),
    'demo-password',
  );
}

FixtureFrankGateway _fastGateway() =>
    FixtureFrankGateway(latency: Duration.zero);

class _DelayedGateway extends FakeGateway {
  _DelayedGateway(this.delay);

  final Duration delay;

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    await Future<void>.delayed(delay);
    return super.loadWorkspace();
  }
}

void _setWindow(WidgetTester tester) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(1440, 900);
  addTearDown(tester.view.reset);
}
