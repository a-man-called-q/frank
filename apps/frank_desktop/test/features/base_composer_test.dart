import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import '../support/frank_test_app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/chat/presentation/composer/base_composer.dart';

void main() {
  testWidgets('BaseComposer renders editor, attachment, and toolbar slots', (
    tester,
  ) async {
    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: BaseComposer(
            attachmentPreview: const Text('Attachment Preview Slot'),
            input: const Text('Input Slot'),
            toolbarLeading: const Text('Context Slot'),
            toolbarTrailing: const Text('Action Slot'),
          ),
        ),
      ),
    );

    expect(find.text('Attachment Preview Slot'), findsOneWidget);
    expect(find.text('Input Slot'), findsOneWidget);
    expect(find.text('Context Slot'), findsOneWidget);
    expect(find.text('Action Slot'), findsOneWidget);
  });

  testWidgets('Tapping BaseComposer container requests focus on focusNode', (
    tester,
  ) async {
    final focusNode = FocusNode(debugLabel: 'test-input-focus');
    var tappedBackground = false;

    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: BaseComposer(
            focusNode: focusNode,
            onTapBackground: () {
              tappedBackground = true;
            },
            input: FTextField(focusNode: focusNode),
          ),
        ),
      ),
    );

    expect(focusNode.hasFocus, isFalse);

    // Tap outside textfield but inside container
    await tester.tap(find.byType(BaseComposer));
    await tester.pump();

    expect(focusNode.hasFocus, isTrue);
    expect(tappedBackground, isTrue);

    focusNode.dispose();
  });

  testWidgets('BaseComposer exposes a stable focus-aware surface', (
    tester,
  ) async {
    final focusNode = FocusNode(debugLabel: 'focus-aware-surface');
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: BaseComposer(
            focusNode: focusNode,
            input: FTextField(focusNode: focusNode),
          ),
        ),
      ),
    );

    final surface = find.byKey(const ValueKey('composer-surface'));
    expect(surface, findsOneWidget);
    final resting = tester.widget<AnimatedContainer>(surface).decoration;

    focusNode.requestFocus();
    await tester.pump();

    final focused = tester.widget<AnimatedContainer>(surface).decoration;
    expect(focused, isNot(resting));
  });
}
