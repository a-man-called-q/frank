import 'dart:async';

import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';
import 'package:frank_desktop/core/gateway/frank_gateway.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/core/models/workspace_models.dart';

class FakeGateway implements FrankGateway {
  FakeGateway({this.workspace, this.loadError, this.organization});

  OfficeWorkspace? workspace;
  Object? loadError;
  OrganizationGraph? organization;
  OrganizationGraph? publishedOrganization;
  Object? organizationLoadError;
  Object? organizationSaveError;
  Object? organizationPublishError;
  int loadCalls = 0;
  int organizationLoadCalls = 0;
  final List<OrganizationGraph> organizationSaves = [];
  final List<OrganizationGraph> organizationPublishes = [];
  final List<ReplyRequest> replies = [];
  Stream<String> Function(ReplyRequest request)? onReply;

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    loadCalls++;
    if (loadError != null) throw loadError!;
    return workspace ?? await FixtureFrankGateway().loadWorkspace();
  }

  @override
  Future<OrganizationGraph> loadOrganization() async {
    organizationLoadCalls++;
    if (organizationLoadError != null) throw organizationLoadError!;
    organization ??= await FixtureFrankGateway().loadOrganization();
    publishedOrganization ??= organization;
    return organization!;
  }

  @override
  Future<OrganizationGraph> saveOrganizationDraft(
    OrganizationGraph graph,
  ) async {
    organizationSaves.add(graph);
    if (organizationSaveError != null) throw organizationSaveError!;
    organization = graph.copyWith(draftRevision: graph.draftRevision + 1);
    return organization!;
  }

  @override
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  }) async {
    organizationPublishes.add(graph);
    if (organizationPublishError != null) throw organizationPublishError!;
    final actual =
        publishedOrganization?.publishedRevision ??
        organization?.publishedRevision ??
        0;
    if (actual != expectedPublishedRevision) {
      throw OrganizationRevisionConflict(expectedPublishedRevision, actual);
    }
    organization = graph.copyWith(publishedRevision: actual + 1);
    publishedOrganization = organization;
    return organization!;
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
