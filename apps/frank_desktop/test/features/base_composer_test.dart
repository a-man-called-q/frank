import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/chat/presentation/composer/base_composer.dart';

void main() {
  testWidgets('BaseComposer renders editor, attachment, and toolbar slots', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: BaseComposer(
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
      MaterialApp(
        home: Scaffold(
          body: BaseComposer(
            focusNode: focusNode,
            onTapBackground: () {
              tappedBackground = true;
            },
            input: TextField(focusNode: focusNode),
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
      MaterialApp(
        home: Scaffold(
          body: BaseComposer(
            focusNode: focusNode,
            input: TextField(focusNode: focusNode),
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
