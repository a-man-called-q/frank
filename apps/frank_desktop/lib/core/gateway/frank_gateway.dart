import '../models/organization_models.dart';
import '../models/workspace_models.dart';

abstract interface class FrankGateway {
  Future<OfficeWorkspace> loadWorkspace();

  Future<OrganizationGraph> loadOrganization();

  Future<OrganizationGraph> saveOrganizationDraft(OrganizationGraph graph);

  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  });

  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  });
}
