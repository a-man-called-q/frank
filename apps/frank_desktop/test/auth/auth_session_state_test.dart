import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/auth/auth_models.dart';
import 'package:frank_desktop/core/auth/auth_session_state.dart';

void main() {
  test('session state owns activation and logout notifications', () async {
    final state = AuthSessionState();
    final changes = <AuthSession?>[];
    final subscription = state.changes.listen(changes.add);
    final session = AuthSession(
      accessToken: 'secret-token',
      expiresAt: DateTime.now().toUtc().add(const Duration(hours: 1)),
      serverId: 'server',
      owner: const AuthOwner(id: 'owner', username: 'owner'),
    );

    state.activate(session);
    state.clear();
    expect(state.activeSession, isNull);
    expect(state.bearerToken, isNull);
    expect(changes, [session, null]);

    await subscription.cancel();
    state.dispose();
  });
}
