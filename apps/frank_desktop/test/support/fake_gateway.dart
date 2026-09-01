import 'dart:async';

import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/gateway/frank_gateway.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

class FakeGateway implements FrankGateway {
  FakeGateway({this.workspace, this.loadError});

  OfficeWorkspace? workspace;
  Object? loadError;
  int loadCalls = 0;
  final List<ReplyRequest> replies = [];
  Stream<String> Function(ReplyRequest request)? onReply;

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    loadCalls++;
    if (loadError != null) throw loadError!;
    return workspace ?? await FixtureFrankGateway().loadWorkspace();
  }

  @override
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  }) {
    final request = ReplyRequest(
      text: text,
      projectId: projectId,
      missionId: missionId,
    );
    replies.add(request);
    return onReply?.call(request) ?? const Stream<String>.empty();
  }
}

class ReplyRequest {
  const ReplyRequest({
    required this.text,
    required this.projectId,
    required this.missionId,
  });

  final String text;
  final String projectId;
  final String? missionId;
}

class ReplyStream {
  ReplyStream() : controller = StreamController<String>();

  final StreamController<String> controller;

  Stream<String> get stream => controller.stream;

  Future<void> close() => controller.close();
}
