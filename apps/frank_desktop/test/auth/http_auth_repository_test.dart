import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/auth/auth_models.dart';
import 'package:frank_desktop/core/auth/auth_repository.dart';
import 'package:http/http.dart' as http;

void main() {
  const configuration = FrankClientConfiguration(
    serverUrl: 'https://frank.test:37465',
  );

  test('login persists a session that a new repository can restore', () async {
    final store = _MemoryAuthSessionStore();
    final client = _AuthClient((request) {
      switch (request.url.path) {
        case '/v2/auth/login':
          return _jsonResponse(_loginResponse());
        case '/v2/auth/status':
          return _jsonResponse(_statusResponse());
        case '/v2/auth/me':
          expect(request.headers['authorization'], 'Bearer persistent-token');
          return _jsonResponse(_meResponse());
        default:
          fail('unexpected request: ${request.method} ${request.url.path}');
      }
    });

    final first = HttpAuthRepository(
      configuration,
      sessionStore: store,
      client: client,
    );
    addTearDown(first.dispose);

    final loggedIn = await first.login(
      username: 'owner',
      password: 'correct horse battery staple',
    );
    expect(loggedIn.accessToken, 'persistent-token');
    expect(await store.read(configuration.serverUrl, 'server-1'), isNotNull);

    final second = HttpAuthRepository(
      configuration,
      sessionStore: store,
      client: client,
    );
    addTearDown(second.dispose);

    final restored = await second.restore();
    expect(restored?.accessToken, 'persistent-token');
    expect(restored?.owner.username, 'owner');
    expect(second.currentSession, same(restored));
  });

  test('logout removes the persisted session', () async {
    final store = _MemoryAuthSessionStore();
    final client = _AuthClient((request) {
      switch (request.url.path) {
        case '/v2/auth/login':
          return _jsonResponse(_loginResponse());
        case '/v2/auth/status':
          return _jsonResponse(_statusResponse());
        case '/v2/auth/logout':
          expect(request.headers['authorization'], 'Bearer persistent-token');
          return _jsonResponse(<String, dynamic>{});
        default:
          fail('unexpected request: ${request.method} ${request.url.path}');
      }
    });

    final first = HttpAuthRepository(
      configuration,
      sessionStore: store,
      client: client,
    );
    addTearDown(first.dispose);
    await first.login(username: 'owner', password: 'correct password');
    await first.logout();

    expect(await store.read(configuration.serverUrl, 'server-1'), isNull);
    expect(await store.read(configuration.serverUrl), isNull);

    final second = HttpAuthRepository(
      configuration,
      sessionStore: store,
      client: client,
    );
    addTearDown(second.dispose);
    expect(await second.restore(), isNull);
  });
}

Map<String, dynamic> _loginResponse() {
  return {
    'access_token': 'persistent-token',
    'expires_at': '2099-01-01T00:00:00Z',
    'server_id': 'server-1',
    'owner': {'id': 'owner-1', 'username': 'owner'},
    'session': {
      'session_id': 'session-1',
      'device_id': 'device-1',
      'expires_at': '2099-01-01T00:00:00Z',
    },
  };
}

Map<String, dynamic> _statusResponse() => {
  'configured': true,
  'auth_method': 'local-password',
  'server_id': 'server-1',
};

Map<String, dynamic> _meResponse() => {
  'owner': {'id': 'owner-1', 'username': 'owner'},
  'session': {
    'session_id': 'session-1',
    'device_id': 'device-1',
    'expires_at': '2099-01-01T00:00:00Z',
  },
};

http.Response _jsonResponse(Map<String, dynamic> body) => http.Response(
  jsonEncode(body),
  200,
  headers: const {'content-type': 'application/json'},
);

class _MemoryAuthSessionStore implements AuthSessionStore {
  final Map<String, AuthSession> _sessions = {};

  String _key(String serverUrl, String? serverId) => '$serverUrl|$serverId';

  @override
  Future<AuthSession?> read(String serverUrl, [String? serverId]) async =>
      _sessions[_key(serverUrl, serverId)];

  @override
  Future<void> write(
    String serverUrl,
    String? serverId,
    AuthSession session,
  ) async {
    _sessions[_key(serverUrl, serverId)] = session;
  }

  @override
  Future<void> delete(String serverUrl, [String? serverId]) async {
    _sessions.remove(_key(serverUrl, serverId));
  }
}

class _AuthClient extends http.BaseClient {
  _AuthClient(this.handler);

  final http.Response Function(http.BaseRequest request) handler;

  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    final response = handler(request);
    return http.StreamedResponse(
      Stream<List<int>>.value(response.bodyBytes),
      response.statusCode,
      headers: response.headers,
      request: request,
    );
  }
}
