import '../models/workspace_models.dart';

abstract interface class FrankGateway {
  Future<OfficeWorkspace> loadWorkspace();

  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  });
}
