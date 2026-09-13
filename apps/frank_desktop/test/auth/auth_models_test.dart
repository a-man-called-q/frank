import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/auth/auth_models.dart';
import 'package:frank_desktop/core/auth/auth_repository.dart';

void main() {
  group('FrankClientConfiguration', () {
    test('accepts an HTTPS server and optional base64 CA', () {
      const config = FrankClientConfiguration(
        serverUrl: 'https://frank.example.test:37465/office/',
        caCertificatePemBase64:
            'LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCi0tLS0tRU5EIENFUlRJRklDQVRFLS0tLS0=',
      );

      expect(config.validationError, isNull);
      expect(config.uri.toString(), 'https://frank.example.test:37465/office');
    });

    test('rejects insecure, credentialed, query, and malformed settings', () {
      for (final config in [
        const FrankClientConfiguration(serverUrl: ''),
        const FrankClientConfiguration(serverUrl: 'http://localhost:37465'),
        const FrankClientConfiguration(
          serverUrl: 'https://user:secret@localhost:37465',
        ),
        const FrankClientConfiguration(
          serverUrl: 'https://localhost:37465?debug=true',
        ),
        const FrankClientConfiguration(
          serverUrl: 'https://localhost:37465',
          caCertificatePemBase64: 'not base64',
        ),
        const FrankClientConfiguration(
          serverUrl: 'https://localhost:37465',
          caCertificatePemBase64: 'Y2VydA==',
        ),
        const FrankClientConfiguration(
          serverUrl: 'https://localhost:37465',
          caCertificatePemBase64:
              'LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCi0tLS0tRU5EIENFUlRJRklDQVRFLS0tLS0tCi0tLS0tQkVHSU4gUFJJVkFURSBLRVktLS0tLQ==',
        ),
        const FrankClientConfiguration(
          serverUrl: 'https://localhost:37465',
          caCertificatePemBase64:
              'LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCi0tLS0tRU5EIENFUlRJRklDQVRFLS0tLS0tCi0tLS0tQkVHSU4gRU5DUllQVEVEIFBSSVZBVEUgS0VZLS0tLS0=',
        ),
      ]) {
        expect(config.validationError, isNotNull);
      }
    });
  });

  test('parses the login envelope and nested session identity', () {
    final session = AuthSession.fromJson({
      'access_token': 'opaque-token',
      'server_id': 'server-1',
      'owner': {'id': 'owner-1', 'username': 'Ada'},
      'session': {
        'session_id': 'session-1',
        'device_id': 'device-1',
        'expires_at': '2099-01-01T00:00:00Z',
      },
    });

    expect(session.accessToken, 'opaque-token');
    expect(session.owner.username, 'Ada');
    expect(session.sessionId, 'session-1');
    expect(session.deviceId, 'device-1');
    expect(session.isExpired, isFalse);
  });

  test('demo repository models the injected test-only session flow', () async {
    final repository = DemoAuthRepository();
    expect((await repository.status()).configured, isTrue);
    final session = await repository.login(
      username: 'Owner_Name',
      password: 'a sufficiently long password',
    );

    expect(session.owner.username, 'owner_name');
    expect(await repository.restore(), same(session));
    await repository.logout();
    expect(repository.currentSession, isNull);
  });
}
