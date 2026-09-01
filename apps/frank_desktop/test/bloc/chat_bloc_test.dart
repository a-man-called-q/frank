import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';
import 'package:frank_desktop/features/chat/bloc/chat_bloc.dart';

import '../support/fake_gateway.dart';

void main() {
  late OfficeWorkspace workspace;
  late ProjectConversationContext projectContext;
  late MissionConversationContext missionContext;

  setUp(() async {
    workspace = await FixtureFrankGateway().loadWorkspace();
    projectContext = const ProjectConversationContext(
      projectId: 'northstar-inventory',
    );
    missionContext = const MissionConversationContext(
      projectId: 'northstar-inventory',
      missionId: 'northstar-discovery',
    );
  });

  test('initializes pure message threads for projects and missions', () async {
    final bloc = ChatBloc(gateway: FakeGateway());
    addTearDown(bloc.close);

    final ready = bloc.stream.firstWhere((state) => state.isReady);
    bloc.add(ChatInitialized(workspace));
    final state = await ready;

    expect(state.messagesFor(projectContext), hasLength(3));
    expect(state.messagesFor(missionContext), hasLength(1));
    expect(state.messagesFor(projectContext).first.role, ChatRole.assistant);
  });

  test('streams a reply and completes it', () async {
    final reply = ReplyStream();
    final gateway = FakeGateway()..onReply = (_) => reply.stream;
    final bloc = ChatBloc(
      gateway: gateway,
      messageIdFactory: () => 'message-1',
    );
    addTearDown(bloc.close);
    addTearDown(reply.close);

    bloc.add(ChatInitialized(workspace));
    await bloc.stream.firstWhere((state) => state.isReady);
    bloc.add(ChatContextChanged(projectContext));
    await bloc.stream.firstWhere(
      (state) => state.activeContext == projectContext,
    );

    bloc.add(
      const ChatMessageSubmitted(
        context: ProjectConversationContext(projectId: 'northstar-inventory'),
        text: '  Plan this  ',
      ),
    );
    await bloc.stream.firstWhere((state) => state.generating);
    expect(gateway.replies.single.text, 'Plan this');

    reply.controller.add('First ');
    await bloc.stream.firstWhere(
      (state) => state.messagesFor(projectContext).last.text == 'First ',
    );
    reply.controller.add('reply');
    await bloc.stream.firstWhere(
      (state) => state.messagesFor(projectContext).last.text == 'First reply',
    );
    await reply.close();
    final completed = await bloc.stream.firstWhere(
      (state) =>
          !state.generating &&
          state.messagesFor(projectContext).last.status ==
              OfficeMessageStatus.complete,
    );
    expect(completed.pendingReplyId, isNull);
  });

  test('stops an active reply and ignores late chunks', () async {
    final reply = ReplyStream();
    final gateway = FakeGateway()..onReply = (_) => reply.stream;
    final bloc = ChatBloc(
      gateway: gateway,
      messageIdFactory: () => 'message-2',
    );
    addTearDown(bloc.close);
    addTearDown(reply.close);

    bloc.add(ChatInitialized(workspace));
    await bloc.stream.firstWhere((state) => state.isReady);
    bloc.add(ChatContextChanged(projectContext));
    await bloc.stream.firstWhere(
      (state) => state.activeContext == projectContext,
    );
    bloc.add(
      const ChatMessageSubmitted(
        context: ProjectConversationContext(projectId: 'northstar-inventory'),
        text: 'Draft',
      ),
    );
    await bloc.stream.firstWhere((state) => state.generating);
    bloc.add(const ChatMessageStopRequested());
    final stopped = await bloc.stream.firstWhere((state) => !state.generating);
    expect(stopped.messagesFor(projectContext).last.text, 'Response stopped.');
    expect(
      stopped.messagesFor(projectContext).last.status,
      OfficeMessageStatus.stopped,
    );

    reply.controller.add('late');
    await Future<void>.delayed(const Duration(milliseconds: 10));
    expect(
      bloc.state.messagesFor(projectContext).last.text,
      'Response stopped.',
    );
  });

  test(
    'changing context cancels the previous stream and seeds the new one',
    () async {
      final firstReply = ReplyStream();
      final gateway = FakeGateway()..onReply = (_) => firstReply.stream;
      final bloc = ChatBloc(
        gateway: gateway,
        messageIdFactory: () => 'message-3',
      );
      addTearDown(bloc.close);
      addTearDown(firstReply.close);

      bloc.add(ChatInitialized(workspace));
      await bloc.stream.firstWhere((state) => state.isReady);
      bloc.add(ChatContextChanged(projectContext));
      await bloc.stream.firstWhere(
        (state) => state.activeContext == projectContext,
      );
      bloc.add(
        const ChatMessageSubmitted(
          context: ProjectConversationContext(projectId: 'northstar-inventory'),
          text: 'Switch',
        ),
      );
      await bloc.stream.firstWhere((state) => state.generating);

      bloc.add(ChatContextChanged(missionContext));
      final changed = await bloc.stream.firstWhere(
        (state) => state.activeContext == missionContext && !state.generating,
      );
      expect(
        changed.messagesFor(projectContext).last.text,
        'Response stopped.',
      );
      expect(changed.messagesFor(missionContext), hasLength(1));

      firstReply.controller.add('late');
      await Future<void>.delayed(const Duration(milliseconds: 10));
      expect(bloc.state.messagesFor(missionContext), hasLength(1));
    },
  );

  test(
    'reports stream errors and blocks duplicate submissions while generating',
    () async {
      final reply = StreamController<String>();
      final gateway = FakeGateway()..onReply = (_) => reply.stream;
      final bloc = ChatBloc(
        gateway: gateway,
        messageIdFactory: () => 'message-4',
      );
      addTearDown(bloc.close);
      addTearDown(reply.close);

      bloc.add(ChatInitialized(workspace));
      await bloc.stream.firstWhere((state) => state.isReady);
      bloc.add(ChatContextChanged(projectContext));
      await bloc.stream.firstWhere(
        (state) => state.activeContext == projectContext,
      );
      bloc.add(
        const ChatMessageSubmitted(
          context: ProjectConversationContext(projectId: 'northstar-inventory'),
          text: 'One',
        ),
      );
      await bloc.stream.firstWhere((state) => state.generating);
      bloc.add(
        const ChatMessageSubmitted(
          context: ProjectConversationContext(projectId: 'northstar-inventory'),
          text: 'Two',
        ),
      );
      await Future<void>.delayed(const Duration(milliseconds: 10));
      expect(gateway.replies, hasLength(1));

      reply.addError(StateError('fixture offline'));
      final failed = await bloc.stream.firstWhere(
        (state) => state.status == ChatStatus.failure,
      );
      expect(
        failed.messagesFor(projectContext).last.text,
        contains('fixture offline'),
      );
    },
  );

  test('disposal cancels an active reply subscription', () async {
    var cancelled = false;
    final reply = StreamController<String>(
      onCancel: () {
        cancelled = true;
      },
    );
    final gateway = FakeGateway()..onReply = (_) => reply.stream;
    final bloc = ChatBloc(
      gateway: gateway,
      messageIdFactory: () => 'message-5',
    );
    addTearDown(reply.close);

    bloc.add(ChatInitialized(workspace));
    await bloc.stream.firstWhere((state) => state.isReady);
    bloc.add(ChatContextChanged(projectContext));
    await bloc.stream.firstWhere(
      (state) => state.activeContext == projectContext,
    );
    bloc.add(
      const ChatMessageSubmitted(
        context: ProjectConversationContext(projectId: 'northstar-inventory'),
        text: 'Keep this stream open',
      ),
    );
    await bloc.stream.firstWhere((state) => state.generating);

    await bloc.close();
    expect(cancelled, isTrue);
  });
}
