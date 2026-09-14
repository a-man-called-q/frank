import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import '../support/frank_test_app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/features/openrouter/openrouter_surface.dart';

import '../support/fake_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('shows setup-first provider state with locked previews', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 900);
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      _app(
        OpenRouterSurface(gateway: FixtureFrankGateway(latency: Duration.zero)),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('OpenRouter'), findsOneWidget);
    expect(find.text('Not configured'), findsNWidgets(2));
    expect(find.byKey(const ValueKey('provider-add-api-key')), findsOneWidget);
    expect(find.byKey(const ValueKey('provider-refresh-models')), findsNothing);
    expect(find.byKey(const ValueKey('provider-model-search')), findsNothing);
    expect(find.text('GPT-4o mini'), findsNothing);
    expect(find.text('Context 128k'), findsNothing);
    expect(find.text('Tools'), findsNothing);
    expect(find.text('Locked'), findsNWidgets(2));
    expect(
      find.byKey(const ValueKey('provider-remove-credential')),
      findsNothing,
    );
  });

  testWidgets('environment credentials cannot be removed from Flutter', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    tester.view.physicalSize = const Size(1200, 900);
    addTearDown(tester.view.reset);

    await tester.pumpWidget(_app(OpenRouterSurface(gateway: FakeGateway())));
    await tester.pumpAndSettle();

    expect(find.text('Connected'), findsOneWidget);
    expect(
      find.textContaining('Managed by server environment'),
      findsOneWidget,
    );
    expect(
      find.byKey(const ValueKey('provider-remove-credential')),
      findsNothing,
    );
    expect(
      find.byKey(const ValueKey('provider-replace-credential')),
      findsNothing,
    );
    expect(
      find.byKey(const ValueKey('provider-test-connection')),
      findsOneWidget,
    );
    expect(find.text('Refresh catalog'), findsOneWidget);
  });
}

Widget _app(Widget child) {
  return FrankTestApp(
    theme: buildFrankTheme(),
    home: FScaffold(child: child),
  );
}
