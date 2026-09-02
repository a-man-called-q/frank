import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/chat/presentation/composer/base_composer.dart';
import 'package:frank_desktop/features/chat/presentation/composer/frank_composer.dart';

void main() {
  const employee = OfficeEmployee(
    id: 'maya-chen',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF4FD1C5,
  );

  const project = OfficeProject(
    id: 'frank-project',
    name: 'Frank Engine',
    client: 'Frank Client',
    summary: 'Summary',
    status: ProjectStatus.active,
    progress: 0.5,
    team: [],
    missions: [],
    messages: [],
  );

  testWidgets(
    'FrankComposer renders header, model selector, footer and input',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FrankComposer(
              generating: false,
              onSend: (_) {},
              onStop: () {},
              executive: employee,
              project: project,
              showHeader: true,
              showFooter: true,
            ),
          ),
        ),
      );

      expect(find.text('frank engine'), findsOneWidget);
      expect(find.text('Gemini 3.7 Flash High'), findsOneWidget);
      expect(find.text('Local'), findsOneWidget);
      expect(find.text('Maya Chen'), findsOneWidget);
      expect(find.bySemanticsLabel('Message Maya'), findsOneWidget);
      expect(find.bySemanticsLabel('Send message'), findsOneWidget);
    },
  );

  testWidgets(
    'Typing text enables send button and clicking it submits message',
    (tester) async {
      String? sentText;

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FrankComposer(
              generating: false,
              onSend: (text) => sentText = text,
              onStop: () {},
              executive: employee,
              project: project,
            ),
          ),
        ),
      );

      final textField = find.byType(TextField);
      await tester.enterText(textField, 'Hello Frank');
      await tester.pump();

      final sendButton = find.bySemanticsLabel('Send message');
      await tester.tap(sendButton);
      await tester.pump();

      expect(sentText, 'Hello Frank');
      expect(tester.widget<TextField>(textField).controller?.text, isEmpty);
    },
  );

  testWidgets(
    'Pressing Enter submits text, while Shift+Enter inserts newline',
    (tester) async {
      String? sentText;

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FrankComposer(
              generating: false,
              onSend: (text) => sentText = text,
              onStop: () {},
              executive: employee,
              project: project,
            ),
          ),
        ),
      );

      final textField = find.byType(TextField);
      await tester.enterText(textField, 'Execute mission');
      await tester.pump();

      // Press Enter to submit
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(sentText, 'Execute mission');
    },
  );

  testWidgets('Generating state shows stop button and triggers onStop', (
    tester,
  ) async {
    var stopped = false;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FrankComposer(
            generating: true,
            onSend: (_) {},
            onStop: () => stopped = true,
            executive: employee,
            project: project,
          ),
        ),
      ),
    );

    expect(find.bySemanticsLabel('Stop generation'), findsOneWidget);
    await tester.tap(find.bySemanticsLabel('Stop generation'));
    await tester.pump();

    expect(stopped, isTrue);
  });

  testWidgets('Pressing Escape during generation triggers onStop', (
    tester,
  ) async {
    var stopped = false;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FrankComposer(
            generating: true,
            onSend: (_) {},
            onStop: () => stopped = true,
            executive: employee,
            project: project,
          ),
        ),
      ),
    );

    // Focus input
    await tester.tap(find.bySemanticsLabel('Message Maya'));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();

    expect(stopped, isTrue);
  });

  testWidgets('Model selector dropdown allows picking a new model', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FrankComposer(
            generating: false,
            onSend: (_) {},
            onStop: () {},
            executive: employee,
            project: project,
          ),
        ),
      ),
    );

    expect(find.text('Gemini 3.7 Flash High'), findsOneWidget);

    await tester.tap(find.text('Gemini 3.7 Flash High'));
    await tester.pumpAndSettle();

    expect(find.text('Claude 3.7 Sonnet'), findsOneWidget);
    expect(find.text('Codex 5.3'), findsOneWidget);

    await tester.tap(find.text('Claude 3.7 Sonnet'));
    await tester.pumpAndSettle();

    expect(find.text('Claude 3.7 Sonnet Thinking'), findsOneWidget);
  });

  testWidgets(
    'reset view side accessory invokes callback without focusing input',
    (tester) async {
      final focusNode = FocusNode(debugLabel: 'reset-view-test-focus');
      addTearDown(focusNode.dispose);
      var resetCount = 0;

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FrankComposer(
              focusNode: focusNode,
              generating: false,
              onSend: (_) {},
              onStop: () {},
              onResetView: () => resetCount += 1,
              resetViewEnabled: true,
            ),
          ),
        ),
      );

      final resetButton = find.byKey(
        const ValueKey('composer-reset-view-button'),
      );
      expect(resetButton, findsOneWidget);
      expect(tester.getSize(resetButton), const Size(44, 44));
      expect(find.bySemanticsLabel('Reset floor view'), findsOneWidget);

      await tester.tap(resetButton);
      await tester.pump();

      expect(resetCount, 1);
      expect(focusNode.hasFocus, isFalse);
    },
  );

  testWidgets(
    'reset view side accessory stays visible but disabled at center',
    (tester) async {
      var resetCount = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FrankComposer(
              generating: false,
              onSend: (_) {},
              onStop: () {},
              onResetView: () => resetCount += 1,
            ),
          ),
        ),
      );

      final resetButton = tester.widget<IconButton>(
        find.byKey(const ValueKey('composer-reset-view-button')),
      );
      expect(resetButton.onPressed, isNull);
      await tester.tap(
        find.byKey(const ValueKey('composer-reset-view-button')),
      );
      await tester.pump();
      expect(resetCount, 0);
    },
  );

  testWidgets('reset view side panel follows a growing composer', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FrankComposer(
            generating: false,
            onSend: (_) {},
            onStop: () {},
            onResetView: () {},
            resetViewEnabled: true,
          ),
        ),
      ),
    );

    final composer = find.byType(BaseComposer);
    final textField = find.byType(TextField);
    final initialHeight = tester.getSize(composer).height;
    await tester.enterText(textField, 'one\ntwo\nthree\nfour\nfive');
    await tester.pump();

    final composerRect = tester.getRect(composer);
    final resetRect = tester.getRect(
      find.byKey(const ValueKey('composer-reset-view-button')),
    );
    expect(composerRect.height, greaterThan(initialHeight));
    expect(resetRect.height, 44);
    expect(resetRect.top, greaterThanOrEqualTo(composerRect.top));
    expect(resetRect.bottom, lessThanOrEqualTo(composerRect.bottom));
  });
}
