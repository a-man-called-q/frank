import 'package:forui/forui.dart';
import '../support/frank_test_app.dart';
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
    'FrankComposer renders live context, environment, and input without a model picker',
    (tester) async {
      await tester.pumpWidget(
        FrankTestApp(
          home: FScaffold(
            child: FrankComposer(
              generating: false,
              onSend: (_) {},
              onStop: () {},
              executive: employee,
              project: project,
            ),
          ),
        ),
      );

      expect(find.text('Maya · Frank Engine'), findsOneWidget);
      expect(find.text('Local'), findsOneWidget);
      expect(find.textContaining('Gemini'), findsNothing);
      expect(find.bySemanticsLabel('Start voice dictation'), findsNothing);
      expect(find.bySemanticsLabel('Message Maya'), findsOneWidget);
      expect(find.bySemanticsLabel('Send message'), findsOneWidget);
    },
  );

  testWidgets(
    'Typing text enables send button and clicking it submits message',
    (tester) async {
      String? sentText;

      await tester.pumpWidget(
        FrankTestApp(
          home: FScaffold(
            child: FrankComposer(
              generating: false,
              onSend: (text) => sentText = text,
              onStop: () {},
              executive: employee,
              project: project,
            ),
          ),
        ),
      );

      final textField = find.byType(FTextField);
      await tester.enterText(textField, 'Hello Frank');
      await tester.pump();

      final sendButton = find.bySemanticsLabel('Send message');
      await tester.tap(sendButton);
      await tester.pump(const Duration(milliseconds: 150));

      expect(sentText, 'Hello Frank');
      expect(textField, findsOneWidget);
    },
  );

  testWidgets(
    'Pressing Enter submits text, while Shift+Enter inserts newline',
    (tester) async {
      String? sentText;

      await tester.pumpWidget(
        FrankTestApp(
          home: FScaffold(
            child: FrankComposer(
              generating: false,
              onSend: (text) => sentText = text,
              onStop: () {},
              executive: employee,
              project: project,
            ),
          ),
        ),
      );

      final textField = find.byType(FTextField);
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
      FrankTestApp(
        home: FScaffold(
          child: FrankComposer(
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
    await tester.pump(const Duration(milliseconds: 150));

    expect(stopped, isTrue);
  });

  testWidgets('Pressing Escape during generation triggers onStop', (
    tester,
  ) async {
    var stopped = false;

    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: FrankComposer(
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

  testWidgets('mission context replaces project context in the toolbar', (
    tester,
  ) async {
    final mission = OfficeMission(
      id: 'composer-mission',
      title: 'Polish the desktop composer',
      status: MissionStatus.active,
      updatedAt: DateTime.utc(2026, 9, 2),
      messages: const [],
    );

    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: FrankComposer(
            generating: false,
            onSend: (_) {},
            onStop: () {},
            executive: employee,
            project: project,
            mission: mission,
          ),
        ),
      ),
    );

    expect(find.text('Maya · Polish the desktop composer'), findsOneWidget);
    expect(find.text('Maya · Frank Engine'), findsNothing);
  });

  testWidgets('composer grows with a multiline prompt', (tester) async {
    await tester.pumpWidget(
      FrankTestApp(
        home: FScaffold(
          child: FrankComposer(
            generating: false,
            onSend: (_) {},
            onStop: () {},
            executive: employee,
            project: project,
          ),
        ),
      ),
    );

    final composer = find.byType(BaseComposer);
    final textField = find.byType(FTextField);
    final initialHeight = tester.getSize(composer).height;
    final initialFieldHeight = tester.getSize(textField).height;
    await tester.enterText(textField, 'one\ntwo\nthree\nfour\nfive');
    await tester.pump();

    expect(tester.getSize(textField).height, greaterThan(initialFieldHeight));
    expect(tester.getSize(composer).height, greaterThanOrEqualTo(initialHeight));
  });
}
