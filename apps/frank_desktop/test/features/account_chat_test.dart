import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flow_ui/flow_ui.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/chat/account_chat.dart';
import 'package:frank_desktop/features/chat/presentation/focusable_composer.dart';

void main() {
  const executive = OfficeEmployee(
    id: 'maya-chen',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
  );

  const project = OfficeProject(
    id: 'frank-project',
    name: 'Frank Engine',
    client: 'Frank Client',
    status: ProjectStatus.active,
    progress: 0.5,
    team: [],
    summary: 'Summary',
    messages: [],
    missions: [],
  );

  testWidgets('chat transcript uses named rows and keeps the composer', (
    tester,
  ) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [
        OfficeMessage(
          id: 'user-message',
          role: ChatRole.user,
          text: 'I need an inventory workflow.',
        ),
        OfficeMessage(
          id: 'assistant-message',
          role: ChatRole.assistant,
          text: 'I can help with **that**.',
        ),
      ],
    );

    expect(
      find.byKey(const ValueKey('passive-chat-transcript')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('chat-transcript-list')), findsOneWidget);
    expect(
      tester
          .widget<IgnorePointer>(
            find.byKey(const ValueKey('passive-chat-transcript')),
          )
          .ignoring,
      isTrue,
    );
    final transcriptBackground = tester.widget<AnimatedContainer>(
      find.byKey(const ValueKey('passive-chat-transcript-background')),
    );
    final backgroundColor =
        (transcriptBackground.decoration as BoxDecoration).color!;
    expect(backgroundColor.a, closeTo(0, 1e-6));
    final mouse = await tester.createGesture(kind: ui.PointerDeviceKind.mouse);
    await mouse.addPointer();
    await mouse.moveTo(
      tester.getCenter(
        find.byKey(const ValueKey('passive-chat-transcript-background')),
      ),
    );
    await tester.pump(const Duration(milliseconds: 120));
    final hoveredBackground = tester.widget<AnimatedContainer>(
      find.byKey(const ValueKey('passive-chat-transcript-background')),
    );
    expect(
      (hoveredBackground.decoration as BoxDecoration).color!.a,
      closeTo(0.07, 1e-6),
    );
    await mouse.moveTo(const Offset(1, 1));
    await mouse.removePointer();
    await tester.pump(const Duration(milliseconds: 120));
    final idleBackground = tester.widget<AnimatedContainer>(
      find.byKey(const ValueKey('passive-chat-transcript-background')),
    );
    expect(
      (idleBackground.decoration as BoxDecoration).color!.a,
      closeTo(0, 1e-6),
    );
    final clip = tester.widget<ClipRRect>(
      find.byKey(const ValueKey('passive-chat-transcript-clip')),
    );
    expect(
      clip.borderRadius,
      const BorderRadius.only(
        topLeft: Radius.circular(14),
        topRight: Radius.circular(14),
      ),
    );
    final transcriptRect = tester.getRect(
      find.byKey(const ValueKey('passive-chat-transcript-background')),
    );
    final composerRect = tester.getRect(
      find.byKey(const ValueKey('composer-surface')),
    );
    expect(transcriptRect.left, closeTo(composerRect.left + 18, 0.1));
    expect(transcriptRect.right, closeTo(composerRect.right - 18, 0.1));
    expect(find.text('You:'), findsOneWidget);
    expect(find.text('Maya:'), findsOneWidget);
    expect(find.byType(FlowMarkdown), findsNWidgets(2));
    expect(find.byType(FlowMessage), findsNothing);
    expect(find.byType(FlowChatView), findsNothing);
    expect(find.byType(CircleAvatar), findsNothing);
    expect(find.byType(FocusableComposer), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('pending replies use the named thinking row', (tester) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [
        OfficeMessage(
          id: 'pending-reply',
          role: ChatRole.assistant,
          text: '',
          status: OfficeMessageStatus.pending,
        ),
      ],
    );

    expect(find.text('Maya:'), findsOneWidget);
    expect(find.text('is thinking…'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('long transcripts stay bottom anchored and clip old rows', (
    tester,
  ) async {
    final messages = [
      ...List<OfficeMessage>.generate(
        48,
        (index) => OfficeMessage(
          id: 'old-$index',
          role: index.isEven ? ChatRole.user : ChatRole.assistant,
          text: 'Old message $index',
        ),
      ),
      const OfficeMessage(
        id: 'latest-message',
        role: ChatRole.assistant,
        text: 'Latest message',
      ),
    ];
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: messages,
    );

    final transcript = find.byKey(const ValueKey('passive-chat-transcript'));
    final latest = find.byKey(const ValueKey('chat-log-row-latest-message'));
    expect(latest, findsOneWidget);
    expect(find.byKey(const ValueKey('chat-log-row-old-0')), findsNothing);
    expect(
      tester.getRect(latest).bottom,
      closeTo(tester.getRect(transcript).bottom - 8, 1),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('empty-state suggestions remain interactive outside the log', (
    tester,
  ) async {
    String? sent;
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [],
      onSend: (value) => sent = value,
    );

    final suggestion = find.text('Turn a rough idea into a project brief');
    expect(suggestion, findsOneWidget);
    await tester.tap(suggestion);
    await tester.pump();

    expect(sent, 'Help me turn this rough idea into a project brief.');
    expect(tester.takeException(), isNull);
  });
}

Future<void> _pumpChat(
  WidgetTester tester, {
  required OfficeEmployee executive,
  required OfficeProject project,
  required List<OfficeMessage> messages,
  ValueChanged<String>? onSend,
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SizedBox(
          width: 900,
          height: 700,
          child: AccountExecutiveChat(
            executive: executive,
            project: project,
            mission: null,
            officeView: true,
            messages: messages,
            generating: false,
            onSend: onSend ?? (_) {},
            onStop: () {},
          ),
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 50));
}
