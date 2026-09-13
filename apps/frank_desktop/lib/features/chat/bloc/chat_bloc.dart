import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/workspace_models.dart';

typedef MessageIdFactory = String Function();

enum ChatStatus { uninitialized, idle, sending, streaming, failure }

class ChatState {
  const ChatState({
    this.status = ChatStatus.uninitialized,
    this.messagesByContext = const {},
    this.activeContext,
    this.activeReplyToken,
    this.pendingReplyId,
    this.pendingContext,
    this.error,
  });

  static const _unset = Object();

  final ChatStatus status;
  final Map<ConversationContext, List<OfficeMessage>> messagesByContext;
  final ConversationContext? activeContext;
  final int? activeReplyToken;
  final String? pendingReplyId;
  final ConversationContext? pendingContext;
  final String? error;

  bool get isReady => status != ChatStatus.uninitialized;
  bool get generating =>
      status == ChatStatus.sending || status == ChatStatus.streaming;

  List<OfficeMessage> messagesFor(ConversationContext? context) {
    if (context == null) return const [];
    return messagesByContext[context] ?? const [];
  }

  ChatState copyWith({
    ChatStatus? status,
    Map<ConversationContext, List<OfficeMessage>>? messagesByContext,
    Object? activeContext = _unset,
    Object? activeReplyToken = _unset,
    Object? pendingReplyId = _unset,
    Object? pendingContext = _unset,
    Object? error = _unset,
  }) {
    return ChatState(
      status: status ?? this.status,
      messagesByContext: messagesByContext ?? this.messagesByContext,
      activeContext: identical(activeContext, _unset)
          ? this.activeContext
          : activeContext as ConversationContext?,
      activeReplyToken: identical(activeReplyToken, _unset)
          ? this.activeReplyToken
          : activeReplyToken as int?,
      pendingReplyId: identical(pendingReplyId, _unset)
          ? this.pendingReplyId
          : pendingReplyId as String?,
      pendingContext: identical(pendingContext, _unset)
          ? this.pendingContext
          : pendingContext as ConversationContext?,
      error: identical(error, _unset) ? this.error : error as String?,
    );
  }
}

sealed class ChatEvent {
  const ChatEvent();
}

final class ChatInitialized extends ChatEvent {
  const ChatInitialized(this.workspace);

  final OfficeWorkspace workspace;
}

final class ChatContextChanged extends ChatEvent {
  const ChatContextChanged(this.context);

  final ConversationContext? context;
}

final class ChatMessageSubmitted extends ChatEvent {
  const ChatMessageSubmitted({required this.context, required this.text});

  final ConversationContext context;
  final String text;
}

final class ChatReplyChunkArrived extends ChatEvent {
  const ChatReplyChunkArrived({
    required this.token,
    required this.context,
    required this.replyId,
    required this.chunk,
  });

  final int token;
  final ConversationContext context;
  final String replyId;
  final String chunk;
}

final class ChatReplyCompleted extends ChatEvent {
  const ChatReplyCompleted({
    required this.token,
    required this.context,
    required this.replyId,
  });

  final int token;
  final ConversationContext context;
  final String replyId;
}

final class ChatReplyFailed extends ChatEvent {
  const ChatReplyFailed({
    required this.token,
    required this.context,
    required this.replyId,
    required this.error,
  });

  final int token;
  final ConversationContext context;
  final String replyId;
  final String error;
}

final class ChatMessageStopRequested extends ChatEvent {
  const ChatMessageStopRequested();
}

class ChatBloc extends Bloc<ChatEvent, ChatState> {
  ChatBloc({
    required ChatGateway gateway,
    MessageIdFactory? messageIdFactory,
  })  : _gateway = gateway,
        _messageIdFactory = messageIdFactory ?? _defaultMessageId,
        super(const ChatState()) {
    on<ChatInitialized>(_initialize);
    on<ChatContextChanged>(_changeContext);
    on<ChatMessageSubmitted>(_submitMessage);
    on<ChatReplyChunkArrived>(_receiveChunk);
    on<ChatReplyCompleted>(_completeReply);
    on<ChatReplyFailed>(_failReply);
    on<ChatMessageStopRequested>(_stopMessage);
  }

  final ChatGateway _gateway;
  final MessageIdFactory _messageIdFactory;
  StreamSubscription<String>? _replySubscription;
  int _replyToken = 0;

  static String _defaultMessageId() =>
      DateTime.now().microsecondsSinceEpoch.toString();

  void _initialize(ChatInitialized event, Emitter<ChatState> emit) {
    final messages = <ConversationContext, List<OfficeMessage>>{};
    for (final project in event.workspace.projects) {
      messages[ProjectConversationContext(projectId: project.id)] =
          List<OfficeMessage>.unmodifiable(project.messages);
      for (final mission in project.missions) {
        messages[MissionConversationContext(
          projectId: project.id,
          missionId: mission.id,
        )] = List<OfficeMessage>.unmodifiable(mission.messages);
      }
    }
    emit(
      ChatState(
        status: ChatStatus.idle,
        messagesByContext: _freezeMessages(messages),
      ),
    );
  }

  Future<void> _changeContext(
    ChatContextChanged event,
    Emitter<ChatState> emit,
  ) async {
    if (state.activeContext == event.context && !state.generating) {
      return;
    }
    await _cancelReply(emit, markStopped: true);
    if (isClosed) return;
    emit(
      state.copyWith(
        status: ChatStatus.idle,
        activeContext: event.context,
        activeReplyToken: null,
        pendingReplyId: null,
        pendingContext: null,
        error: null,
      ),
    );
  }

  Future<void> _submitMessage(
    ChatMessageSubmitted event,
    Emitter<ChatState> emit,
  ) async {
    final text = event.text.trim();
    if (text.isEmpty || state.generating || state.activeContext != event.context) {
      return;
    }

    final projectId = event.context.projectId;
    final missionId = switch (event.context) {
      ProjectConversationContext() => null,
      MissionConversationContext(:final missionId) => missionId,
    };
    final messageId = _messageIdFactory();
    final replyId = '$messageId-reply';
    final currentMessages = state.messagesFor(event.context);
    final messages = {
      ...state.messagesByContext,
      event.context: List<OfficeMessage>.unmodifiable([
        ...currentMessages,
        OfficeMessage(
          id: messageId,
          role: ChatRole.user,
          text: text,
        ),
        OfficeMessage(
          id: replyId,
          role: ChatRole.assistant,
          text: '',
          status: OfficeMessageStatus.pending,
        ),
      ]),
    };
    final token = ++_replyToken;
    emit(
      state.copyWith(
        status: ChatStatus.sending,
        messagesByContext: _freezeMessages(messages),
        pendingReplyId: replyId,
        pendingContext: event.context,
        activeReplyToken: token,
        error: null,
      ),
    );

    if (isClosed) return;
    _replySubscription = _gateway
        .replyTo(text, projectId: projectId, missionId: missionId)
        .listen(
          (chunk) {
            if (isClosed) return;
            add(
              ChatReplyChunkArrived(
                token: token,
                context: event.context,
                replyId: replyId,
                chunk: chunk,
              ),
            );
          },
          onError: (Object error, StackTrace stackTrace) {
            if (isClosed) return;
            add(
              ChatReplyFailed(
                token: token,
                context: event.context,
                replyId: replyId,
                error: error.toString(),
              ),
            );
          },
          onDone: () {
            if (isClosed) return;
            add(
              ChatReplyCompleted(
                token: token,
                context: event.context,
                replyId: replyId,
              ),
            );
          },
          cancelOnError: false,
        );
  }

  void _receiveChunk(
    ChatReplyChunkArrived event,
    Emitter<ChatState> emit,
  ) {
    if (!_isCurrent(event.token, event.context, event.replyId)) return;
    final messages = state.messagesFor(event.context);
    if (messages.isEmpty || messages.last.id != event.replyId) return;
    final current = messages.last;
    final updated = current.copyWith(
      text: current.text + event.chunk,
      status: OfficeMessageStatus.streaming,
    );
    emit(_replaceLastMessage(event.context, updated).copyWith(status: ChatStatus.streaming));
  }

  Future<void> _completeReply(
    ChatReplyCompleted event,
    Emitter<ChatState> emit,
  ) async {
    if (!_isCurrent(event.token, event.context, event.replyId)) return;
    final messages = state.messagesFor(event.context);
    if (messages.isEmpty || messages.last.id != event.replyId) return;
    await _replySubscription?.cancel();
    _replySubscription = null;
    emit(
      _replaceLastMessage(
        event.context,
        messages.last.copyWith(status: OfficeMessageStatus.complete),
      ).copyWith(
        status: ChatStatus.idle,
        activeReplyToken: null,
        pendingReplyId: null,
        pendingContext: null,
      ),
    );
  }

  Future<void> _failReply(
    ChatReplyFailed event,
    Emitter<ChatState> emit,
  ) async {
    if (!_isCurrent(event.token, event.context, event.replyId)) return;
    final messages = state.messagesFor(event.context);
    if (messages.isEmpty || messages.last.id != event.replyId) return;
    await _replySubscription?.cancel();
    _replySubscription = null;
    emit(
      _replaceLastMessage(
        event.context,
        messages.last.copyWith(
          text: 'The fixture could not respond: ${event.error}',
          status: OfficeMessageStatus.error,
        ),
      ).copyWith(
        status: ChatStatus.failure,
        activeReplyToken: null,
        pendingReplyId: null,
        pendingContext: null,
        error: event.error,
      ),
    );
  }

  Future<void> _stopMessage(
    ChatMessageStopRequested event,
    Emitter<ChatState> emit,
  ) async {
    await _cancelReply(emit, markStopped: true);
    if (isClosed) return;
    emit(
      state.copyWith(
        status: ChatStatus.idle,
        activeReplyToken: null,
        pendingReplyId: null,
        pendingContext: null,
        error: null,
      ),
    );
  }

  Future<void> _cancelReply(
    Emitter<ChatState> emit, {
    required bool markStopped,
  }) async {
    ++_replyToken;
    final subscription = _replySubscription;
    _replySubscription = null;
    await subscription?.cancel();
    if (!markStopped || state.pendingReplyId == null || state.pendingContext == null) {
      return;
    }
    final context = state.pendingContext!;
    final messages = state.messagesFor(context);
    if (messages.isEmpty || messages.last.id != state.pendingReplyId) return;
    emit(
      _replaceLastMessage(
        context,
        messages.last.copyWith(
          text: 'Response stopped.',
          status: OfficeMessageStatus.stopped,
        ),
      ),
    );
  }

  bool _isCurrent(int token, ConversationContext context, String replyId) =>
      token == _replyToken &&
      state.activeReplyToken == token &&
      state.pendingReplyId == replyId &&
      state.pendingContext == context;

  ChatState _replaceLastMessage(
    ConversationContext context,
    OfficeMessage replacement,
  ) {
    final messages = state.messagesFor(context);
    return state.copyWith(
      messagesByContext: _freezeMessages({
        ...state.messagesByContext,
        context: List<OfficeMessage>.unmodifiable([
          ...messages.take(messages.length - 1),
          replacement,
        ]),
      }),
    );
  }

  static Map<ConversationContext, List<OfficeMessage>> _freezeMessages(
    Map<ConversationContext, List<OfficeMessage>> messages,
  ) {
    return Map.unmodifiable(
      messages.map(
        (context, values) =>
            MapEntry(context, List<OfficeMessage>.unmodifiable(values)),
      ),
    );
  }

  @override
  Future<void> close() async {
    ++_replyToken;
    await _replySubscription?.cancel();
    _replySubscription = null;
    return super.close();
  }
}
