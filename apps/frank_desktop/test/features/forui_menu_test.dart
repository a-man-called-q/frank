import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:forui/forui.dart';

import '../support/frank_test_app.dart';

void main() {
  testWidgets('ForUI select supports nullable values and disabled options', (
    tester,
  ) async {
    String? selected;
    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: FSelect<String?>.rich(
            key: const ValueKey('status-select'),
            format: (value) => value ?? 'Any status',
            hint: 'Any status',
            autoHide: true,
            control: FSelectControl<String?>.lifted(
              value: selected,
              onChange: (value) => selected = value,
            ),
            children: [
              FSelectItem<String?>.item(
                title: const Text('Any status'),
                value: null,
                key: const ValueKey('status-any'),
              ),
              FSelectItem<String?>.item(
                title: const Text('Active'),
                value: 'active',
                key: const ValueKey('status-active'),
              ),
              FSelectItem<String?>.item(
                title: const Text('Disabled'),
                value: 'disabled',
                enabled: false,
                key: const ValueKey('status-disabled'),
              ),
            ],
          ),
        ),
      ),
    );

    final select = tester.widget<FSelect<String?>>(
      find.byKey(const ValueKey('status-select')),
    );
    expect(select.autoHide, isTrue);
    expect(selected, isNull);
  });

  testWidgets('ForUI popover menu toggles, invokes an item, and dismisses', (
    tester,
  ) async {
    var invoked = false;
    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: FPopoverMenu(
            key: const ValueKey('actions-menu'),
            semanticsLabel: 'Actions',
            menu: [
              FItemGroup(
                children: [
                  FItem(
                    key: const ValueKey('action-delete'),
                    title: const Text('Delete'),
                    variant: FItemVariant.destructive,
                    onPress: () => invoked = true,
                  ),
                ],
              ),
            ],
            builder: (context, controller, _) => FButton(
              key: const ValueKey('actions-trigger'),
              onPress: controller.toggle,
              child: const Text('Actions'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.byKey(const ValueKey('actions-trigger')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('action-delete')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('action-delete')));
    await tester.pumpAndSettle();
    expect(invoked, isTrue);
    expect(find.byKey(const ValueKey('action-delete')), findsOneWidget);
  });
}
