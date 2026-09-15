import 'dart:ui' as ui;

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import '../support/frank_test_app.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flow_ui/flow_ui.dart';
import 'package:frank_desktop/app/theme.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/chat/account_chat.dart';
import 'package:frank_desktop/features/chat/presentation/focusable_composer.dart';
import 'package:frank_desktop/features/floor/office_scene_floor.dart';

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
    final transcriptBackground = tester.widget<DecoratedBox>(
      find.byKey(const ValueKey('passive-chat-transcript-background')),
    );
    final backgroundColor =
        (transcriptBackground.decoration as BoxDecoration).color!;
    // Color channels are stored as 8-bit values (38/255 ≈ 0.149).
    expect(backgroundColor.a, closeTo(0.15, 0.001));
    final scrollbar = tester.widget<RawScrollbar>(
      find.byKey(const ValueKey('chat-transcript-scrollbar')),
    );
    expect(scrollbar.interactive, isTrue);
    expect(scrollbar.thickness, 4);
    expect(scrollbar.radius, const Radius.circular(4));
    expect(scrollbar.trackVisibility, isFalse);
    final list = tester.widget<ListView>(
      find.byKey(const ValueKey('chat-transcript-list')),
    );
    expect(list.controller, isNotNull);
    expect(list.reverse, isTrue);
    expect(list.shrinkWrap, isTrue);
    expect(list.physics, isA<ClampingScrollPhysics>());
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('chat-transcript-list')),
        matching: find.byType(IgnorePointer),
      ),
      findsWidgets,
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
    final chatRect = tester.getRect(find.byType(AccountExecutiveChat));
    final floorPanel = find.byKey(const ValueKey('floor-control-panel'));
    expect(floorPanel, findsOneWidget);
    expect(
      find.descendant(
        of: floorPanel,
        matching: find.byKey(const ValueKey('floor-reset-view-button')),
      ),
      findsOneWidget,
    );
    final resetButton = tester.widget<FButton>(
      find.byKey(const ValueKey('floor-reset-view-button')),
    );
    // The headless host has no ready GPU scene, so reset remains visible but
    // disabled until the scene reports readiness.
    expect(resetButton.onPress, isNull);
    final floorPanelRect = tester.getRect(floorPanel);
    final floorPanelDecoration =
        (tester.widget<Container>(floorPanel).decoration as BoxDecoration);
    expect(
      floorPanelDecoration.color,
      FrankColors.panelRaised.withValues(alpha: 0.8),
    );
    expect(
      floorPanelDecoration.borderRadius,
      const BorderRadius.only(
        topLeft: Radius.zero,
        bottomLeft: Radius.zero,
        topRight: Radius.circular(14),
        bottomRight: Radius.circular(14),
      ),
    );
    expect(floorPanelDecoration.border, isNull);
    expect(floorPanelDecoration.boxShadow, isNull);
    expect(floorPanelRect.left, closeTo(composerRect.right, 0.1));
    expect(floorPanelRect.width, closeTo(40, 0.1));
    expect(floorPanelRect.right, closeTo(chatRect.right - 16, 0.1));
    expect(floorPanelRect.top, closeTo(composerRect.top + 19, 0.1));
    expect(floorPanelRect.bottom, closeTo(composerRect.bottom - 19, 0.1));
    expect(floorPanelRect.height, closeTo(composerRect.height - 38, 0.1));
    // The transcript stops where the composer's corner radius starts curving.
    expect(transcriptRect.left, closeTo(composerRect.left + 19, 0.1));
    expect(transcriptRect.right, closeTo(composerRect.right - 19, 0.1));
    expect(find.text('You:'), findsOneWidget);
    expect(find.text('Maya:'), findsOneWidget);
    expect(find.byType(FlowMarkdown), findsNWidgets(2));
    expect(find.byType(FlowMessage), findsNothing);
    expect(find.byType(FlowChatView), findsNothing);
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
    expect(
      tester
          .getRect(
            find.byKey(const ValueKey('passive-chat-transcript-background')),
          )
          .height,
      closeTo(72, 0.1),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('streaming stopped error and multiline Markdown stay visual', (
    tester,
  ) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [
        OfficeMessage(
          id: 'streaming-reply',
          role: ChatRole.assistant,
          text: 'Streaming **now**',
          status: OfficeMessageStatus.streaming,
        ),
        OfficeMessage(
          id: 'stopped-reply',
          role: ChatRole.assistant,
          text: 'Response stopped.',
          status: OfficeMessageStatus.stopped,
        ),
        OfficeMessage(
          id: 'error-reply',
          role: ChatRole.assistant,
          text: 'Gateway unavailable.\n\n- Try again',
          status: OfficeMessageStatus.error,
        ),
      ],
    );

    final markdown = tester.widgetList<FlowMarkdown>(find.byType(FlowMarkdown));
    expect(markdown, hasLength(3));
    expect(
      markdown
          .singleWhere((widget) => widget.text == 'Streaming **now**')
          .isStreaming,
      isTrue,
    );
    expect(
      markdown.singleWhere((widget) => widget.text == 'Response stopped.'),
      isNotNull,
    );
    expect(
      markdown
          .singleWhere(
            (widget) => widget.text == 'Gateway unavailable.\n\n- Try again',
          )
          .style
          ?.color,
      buildFrankTheme().colors.error,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('chat log grows with content between its height bounds', (
    tester,
  ) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: List.generate(
        5,
        (index) => OfficeMessage(
          id: 'medium-$index',
          role: index.isEven ? ChatRole.user : ChatRole.assistant,
          text: 'Message $index',
        ),
      ),
    );

    final height = tester
        .getRect(
          find.byKey(const ValueKey('passive-chat-transcript-background')),
        )
        .height;
    expect(height, greaterThan(72));
    expect(height, lessThan(280));
    expect(tester.takeException(), isNull);
  });

  testWidgets('right panel keeps its inset after multiline input', (
    tester,
  ) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [
        OfficeMessage(
          id: 'assistant-message',
          role: ChatRole.assistant,
          text: 'Ready.',
        ),
      ],
    );

    final composer = find.byKey(const ValueKey('composer-surface'));
    final panel = find.byKey(const ValueKey('floor-control-panel'));
    final initialHeight = tester.getRect(composer).height;
    await tester.enterText(
      find.byType(FTextField),
      'one\ntwo\nthree\nfour\nfive\nsix',
    );
    await tester.pumpAndSettle();

    final composerRect = tester.getRect(composer);
    final panelRect = tester.getRect(panel);
    expect(composerRect.height, greaterThan(initialHeight));
    expect(panelRect.top, closeTo(composerRect.top + 19, 0.1));
    expect(panelRect.bottom, closeTo(composerRect.bottom - 19, 0.1));
    expect(panelRect.height, closeTo(composerRect.height - 38, 0.1));
    expect(tester.takeException(), isNull);
  });

  testWidgets('chat and composer stay aligned on a narrow viewport', (
    tester,
  ) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [
        OfficeMessage(
          id: 'narrow-message',
          role: ChatRole.assistant,
          text: 'Narrow layout.',
        ),
      ],
      size: const Size(400, 700),
    );

    final chatRect = tester.getRect(
      find.byKey(const ValueKey('passive-chat-transcript-background')),
    );
    final composerRect = tester.getRect(
      find.byKey(const ValueKey('composer-surface')),
    );
    final panelRect = tester.getRect(
      find.byKey(const ValueKey('floor-control-panel')),
    );
    final accountChatRect = tester.getRect(find.byType(AccountExecutiveChat));
    expect(chatRect.left, closeTo(composerRect.left + 19, 0.1));
    expect(chatRect.right, closeTo(composerRect.right - 19, 0.1));
    expect(panelRect.left, closeTo(composerRect.right, 0.1));
    expect(panelRect.right, closeTo(accountChatRect.right - 16, 0.1));
    expect(tester.takeException(), isNull);
  });

  testWidgets('long transcripts stay bottom anchored and become scrollable', (
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

    final transcript = find.byKey(
      const ValueKey('passive-chat-transcript-background'),
    );
    final list = tester.widget<ListView>(
      find.byKey(const ValueKey('chat-transcript-list')),
    );
    final latest = find.byKey(const ValueKey('chat-log-row-latest-message'));
    expect(latest, findsOneWidget);
    expect(find.byKey(const ValueKey('chat-log-row-old-0')), findsNothing);
    expect(tester.getRect(transcript).height, closeTo(280, 0.1));
    expect(list.controller!.position.maxScrollExtent, greaterThan(0));
    expect(list.controller!.offset, closeTo(0, 0.1));
    expect(
      tester.getRect(latest).bottom,
      closeTo(tester.getRect(transcript).bottom - 16, 1),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('wheel over chat scrolls history without zooming the floor', (
    tester,
  ) async {
    final sceneController = OfficeSceneController()..setReady(true);
    addTearDown(sceneController.dispose);
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: List.generate(
        48,
        (index) => OfficeMessage(
          id: 'wheel-$index',
          role: index.isEven ? ChatRole.user : ChatRole.assistant,
          text: 'Scrollable message $index',
        ),
      ),
      sceneController: sceneController,
      withFloorInteraction: true,
    );

    final list = tester.widget<ListView>(
      find.byKey(const ValueKey('chat-transcript-list')),
    );
    final initialZoom = sceneController.zoom;
    expect(list.controller!.offset, 0);
    await tester.sendEventToBinding(
      PointerScrollEvent(
        kind: ui.PointerDeviceKind.mouse,
        position: tester.getCenter(
          find.byKey(const ValueKey('passive-chat-transcript-background')),
        ),
        scrollDelta: const Offset(0, -120),
      ),
    );
    await tester.pump();

    expect(list.controller!.offset, greaterThan(0));
    expect(sceneController.zoom, initialZoom);
    expect(tester.takeException(), isNull);
  });

  testWidgets('new messages follow latest unless history is being read', (
    tester,
  ) async {
    final messages = ValueNotifier<List<OfficeMessage>>(
      List.generate(
        48,
        (index) => OfficeMessage(
          id: 'history-$index',
          role: index.isEven ? ChatRole.user : ChatRole.assistant,
          text: 'History message $index',
        ),
      ),
    );
    addTearDown(messages.dispose);
    await _pumpMutableChat(
      tester,
      executive: executive,
      project: project,
      messages: messages,
    );

    var list = tester.widget<ListView>(
      find.byKey(const ValueKey('chat-transcript-list')),
    );
    list.controller!.jumpTo(120);
    await tester.pump();
    final readingOffset = list.controller!.offset;
    messages.value = [
      ...messages.value,
      const OfficeMessage(
        id: 'new-while-reading',
        role: ChatRole.assistant,
        text: 'Do not steal the scroll position.',
      ),
    ];
    await tester.pump();
    list = tester.widget<ListView>(
      find.byKey(const ValueKey('chat-transcript-list')),
    );
    expect(list.controller!.offset, closeTo(readingOffset, 0.1));

    list.controller!.jumpTo(0);
    messages.value = [
      ...messages.value,
      const OfficeMessage(
        id: 'new-at-latest',
        role: ChatRole.assistant,
        text: 'Follow this message.',
      ),
    ];
    await tester.pump();
    await tester.pump();
    expect(list.controller!.offset, closeTo(0, 0.1));
    expect(tester.takeException(), isNull);
  });

  testWidgets('empty state keeps suggestions out of the composer', (
    tester,
  ) async {
    await _pumpChat(
      tester,
      executive: executive,
      project: project,
      messages: const [],
    );

    expect(find.byType(FlowSuggestionGroup), findsNothing);
    expect(find.text('Turn a rough idea into a project brief'), findsNothing);
    expect(find.text('Suggest a team for my next project'), findsNothing);
    expect(find.byKey(const ValueKey('composer-surface')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

Future<void> _pumpChat(
  WidgetTester tester, {
  required OfficeEmployee executive,
  required OfficeProject project,
  required List<OfficeMessage> messages,
  ValueChanged<String>? onSend,
  Size size = const Size(900, 700),
  OfficeSceneController? sceneController,
  bool withFloorInteraction = false,
}) async {
  Widget chat = AccountExecutiveChat(
    executive: executive,
    project: project,
    mission: null,
    showNoMissionsNotice: true,
    messages: messages,
    generating: false,
    onSend: onSend ?? (_) {},
    onStop: () {},
    renderFloor: !withFloorInteraction,
    sceneController: sceneController,
  );
  if (withFloorInteraction) {
    chat = Stack(
      fit: StackFit.expand,
      children: [
        OfficeSceneInteractionSurface(
          controller: sceneController!,
          child: const ColoredBox(color: Color(0x00000000)),
        ),
        chat,
      ],
    );
  }

  await tester.pumpWidget(
    FrankTestApp(
      home: FScaffold(
        child: SizedBox(width: size.width, height: size.height, child: chat),
      ),
    ),
  );
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 50));
}

Future<void> _pumpMutableChat(
  WidgetTester tester, {
  required OfficeEmployee executive,
  required OfficeProject project,
  required ValueNotifier<List<OfficeMessage>> messages,
}) async {
  await tester.pumpWidget(
    FrankTestApp(
      home: FScaffold(
        child: SizedBox(
          width: 900,
          height: 700,
          child: ValueListenableBuilder<List<OfficeMessage>>(
            valueListenable: messages,
            builder: (context, value, child) => AccountExecutiveChat(
              executive: executive,
              project: project,
              mission: null,
              showNoMissionsNotice: true,
              messages: value,
              generating: false,
              onSend: (_) {},
              onStop: () {},
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 50));
}
