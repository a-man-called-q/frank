import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_workspace.dart';

void main() {
  test('fixture gateway streams a mission-aware reply', () async {
    final gateway = FixtureFrankGateway();
    final chunks = await gateway
        .replyTo(
          'Plan the intake',
          projectId: 'northstar-inventory',
          missionId: 'northstar-discovery',
        )
        .toList();

    expect(chunks, hasLength(3));
    expect(chunks.join(), contains('that mission'));
  });
}
