import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/chat/presentation/composer/base_composer.dart';

void main() {
  testWidgets('BaseComposer renders all provided slots correctly', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: BaseComposer(
            header: const Text('Header Slot'),
            attachmentPreview: const Text('Attachment Preview Slot'),
            input: const Text('Input Slot'),
            leadingActions: const [Text('Leading 1'), Text('Leading 2')],
            trailingActions: const [Text('Trailing 1'), Text('Trailing 2')],
            footer: const Text('Footer Slot'),
          ),
        ),
      ),
    );

    expect(find.text('Header Slot'), findsOneWidget);
    expect(find.text('Attachment Preview Slot'), findsOneWidget);
    expect(find.text('Input Slot'), findsOneWidget);
    expect(find.text('Leading 1'), findsOneWidget);
    expect(find.text('Leading 2'), findsOneWidget);
    expect(find.text('Trailing 1'), findsOneWidget);
    expect(find.text('Trailing 2'), findsOneWidget);
    expect(find.text('Footer Slot'), findsOneWidget);
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
}
