import 'dart:async';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:http/http.dart' as http;

import 'auth_models.dart';
import 'auth_session_state.dart';
import 'auth_session_store.dart';
import '../transport/frank_transport.dart';

export '../transport/frank_transport.dart'
    show FrankApiVersion, FrankClientConfiguration;
export 'auth_session_state.dart';
export 'auth_session_store.dart';

abstract interface class AuthRepository {
  String get serverUrl;
  AuthSession? get currentSession;

  /// Shared active-session state. Feature code can observe lifecycle changes
  /// without receiving a bearer token or owning a second session cache.
  AuthSessionState get sessionState => AuthSessionState(currentSession);

  /// The production repository exposes its transport so remote gateways can
  /// share the exact TLS and bearer boundary. Fixture repositories return
  /// null and continue to serve their existing test contract.
  FrankTransport? get transport => null;

  /// Null when the platform keychain is available. A non-null value means the
  /// current session can still be used in memory, but it will not be restored
  /// after the process exits.
  String? get storageWarning => null;

  Future<AuthStatus> status();
  Future<AuthSession?> restore();
  Future<AuthSession> login({
    required String username,
    required String password,
  });
  Future<void> logout();
  Future<void> logoutAll();
  Future<void> changePassword({
    required String currentPassword,
    required String newPassword,
  });
  void dispose();
}

class UnconfiguredAuthRepository implements AuthRepository {
  UnconfiguredAuthRepository(this.configuration, {this.configurationMessage});

  final FrankClientConfiguration configuration;
  final String? configurationMessage;
  final AuthSessionState _sessionState = AuthSessionState();

  @override
  String get serverUrl => configuration.serverUrl;

  @override
  AuthSession? get currentSession => null;

  @override
  FrankTransport? get transport => null;

  @override
  AuthSessionState get sessionState => _sessionState;

  @override
  String? get storageWarning => null;

  AuthFailure get _failure => AuthFailure(
    kind: AuthFailureKind.configuration,
    message:
        configuration.validationError ??
        configurationMessage ??
        'Frank server is not configured',
  );

  @override
  Future<AuthStatus> status() => Future<AuthStatus>.error(_failure);

  @override
  Future<AuthSession?> restore() => Future<AuthSession?>.error(_failure);

  @override
  Future<AuthSession> login({
    required String username,
    required String password,
  }) => Future<AuthSession>.error(_failure);

  @override
  Future<void> logout() async {}

  @override
  Future<void> logoutAll() async {}

  @override
  Future<void> changePassword({
    required String currentPassword,
    required String newPassword,
  }) => Future<void>.error(_failure);

  @override
  void dispose() => _sessionState.dispose();
}

/// A deliberately explicit test-only auth implementation. Production's
/// entrypoint always supplies [HttpAuthRepository]; this class preserves the
/// existing fixture widget tests while they exercise shell transitions.
class DemoAuthRepository implements AuthRepository {
  DemoAuthRepository({this.username = 'demo'});

  final String username;
  final AuthSessionState _sessionState = AuthSessionState();

  @override
  String get serverUrl => 'https://demo.invalid:37465';

  @override
  AuthSession? get currentSession => _sessionState.activeSession;

  @override
  FrankTransport? get transport => null;

  @override
  AuthSessionState get sessionState => _sessionState;

  @override
  String? get storageWarning => null;

  @override
  Future<AuthStatus> status() async => const AuthStatus(
    configured: true,
    authMethod: 'demo',
    serverId: 'demo-server',
  );

  @override
  Future<AuthSession?> restore() async => _sessionState.activeSession;

  @override
  Future<AuthSession> login({
    required String username,
    required String password,
  }) async {
    final session = AuthSession(
      accessToken: 'demo-token',
      expiresAt: DateTime.now().toUtc().add(const Duration(days: 30)),
      serverId: 'demo-server',
      owner: AuthOwner(
        id: 'demo-owner',
        username: username.trim().toLowerCase(),
      ),
      deviceId: 'demo-device',
    );
    _sessionState.activate(session);
    return session;
  }

  @override
  Future<void> logout() async => _sessionState.clear();

  @override
  Future<void> logoutAll() async => _sessionState.clear();

  @override
  Future<void> changePassword({
    required String currentPassword,
    required String newPassword,
  }) async {}

  @override
  void dispose() => _sessionState.dispose();
}

class HttpAuthRepository implements AuthRepository {
  HttpAuthRepository(
    this.configuration, {
    FlutterSecureStorage? storage,
    AuthSessionStore? sessionStore,
    http.Client? client,
  }) : _sessionStore =
         sessionStore ?? FlutterSecureAuthSessionStore(storage: storage) {
    _transport = FrankTransport(
      configuration,
      sessionState: _sessionState,
      client: client,
    );
  }

  final FrankClientConfiguration configuration;
  final AuthSessionStore _sessionStore;
  final AuthSessionState _sessionState = AuthSessionState();
  late final FrankTransport _transport;
  String? _storageWarning;
  String? _serverId;

  AuthSession? get _session => _sessionState.activeSession;

  set _session(AuthSession? value) => _sessionState.setSession(value);

  @override
  String get serverUrl => configuration.serverUrl;

  @override
  AuthSession? get currentSession => _sessionState.activeSession;

  @override
  AuthSessionState get sessionState => _sessionState;

  @override
  FrankTransport get transport => _transport;

  @override
  String? get storageWarning => _storageWarning;

  @override
  Future<AuthStatus> status() async {
    final json = await _send('GET', '/v2/auth/status');
    final status = AuthStatus.fromJson(json);
    _serverId = status.serverId;
    return status;
  }

  @override
  Future<AuthSession?> restore() async {
    // Once a session is active, keep it usable for the lifetime of this
    // process even when the OS keychain becomes unavailable. Revalidation is
    // still authoritative; secure storage is only the persistence layer.
    final active = _session;
    if (active != null) {
      if (active.isExpired) {
        _session = null;
        await _clearStoredSession();
        return null;
      }
      try {
        _session = await _me(active.accessToken, active.serverId);
        return _session;
      } on AuthFailure catch (error) {
        if (error.kind == AuthFailureKind.unauthorized) {
          _session = null;
          await _clearStoredSession();
          return null;
        }
        rethrow;
      }
    }

    AuthSession? stored;
    try {
      // status() is the authority for server identity. LoginGate calls it
      // first, but restore() remains correct when invoked directly by a
      // lifecycle monitor or an integration test.
      if (_serverId == null) {
        try {
          await status();
        } on Object {
          // A previously persisted session can still be inspected below;
          // server validation remains authoritative once a token is found.
        }
      }
      stored = await _sessionStore.read(configuration.serverUrl, _serverId);
      // Sessions written by the first client build were keyed only by URL.
      // Read that slot once so an upgrade can move the token to the server-ID
      // scoped key without making users log in unnecessarily.
      stored ??= await _sessionStore.read(configuration.serverUrl, null);
    } on Object {
      _markStorageUnavailable();
      return null;
    }
    if (stored == null) return null;
    try {
      if (stored.isExpired) {
        await _clearStoredSession();
        return null;
      }
      _serverId = stored.serverId;
      _session = await _me(stored.accessToken, stored.serverId);
      await _persistSession(_session!);
      return _session;
    } on AuthFailure catch (error) {
      if (error.kind == AuthFailureKind.unauthorized) {
        await _clearStoredSession();
        _session = null;
        return null;
      }
      rethrow;
    } on FormatException {
      await _clearStoredSession();
      _session = null;
      return null;
    }
  }

  @override
  Future<AuthSession> login({
    required String username,
    required String password,
  }) async {
    final response = await _send(
      'POST',
      '/v2/auth/login',
      body: {'username': username, 'password': password},
    );
    try {
      final session = AuthSession.fromJson(response);
      _serverId = session.serverId;
      _session = session;
      await _persistSession(session);
      return session;
    } on FormatException catch (error) {
      throw AuthFailure(kind: AuthFailureKind.response, message: error.message);
    }
  }

  @override
  Future<void> logout() async {
    final token = _session?.accessToken;
    _serverId = _session?.serverId ?? _serverId;
    _session = null;
    await _clearStoredSession();
    if (token == null) return;
    await _sendAuthorized('POST', '/v2/auth/logout', token: token);
  }

  @override
  Future<void> logoutAll() async {
    final token = _session?.accessToken;
    _serverId = _session?.serverId ?? _serverId;
    _session = null;
    await _clearStoredSession();
    if (token == null) return;
    await _sendAuthorized('POST', '/v2/auth/logout-all', token: token);
  }

  @override
  Future<void> changePassword({
    required String currentPassword,
    required String newPassword,
  }) async {
    final token = _requireToken();
    await _sendAuthorized(
      'POST',
      '/v2/auth/password',
      token: token,
      body: {'current_password': currentPassword, 'new_password': newPassword},
    );
    _session = null;
    await _clearStoredSession();
  }

  Future<AuthSession> _me(String token, String serverId) async {
    final response = await _sendAuthorized('GET', '/v2/auth/me', token: token);
    return AuthSession.fromJson({
      ...response,
      'access_token': token,
      'server_id': response['server_id'] ?? serverId,
    });
  }

  String _requireToken() {
    final token = _session?.accessToken;
    if (token == null || token.isEmpty) {
      throw const AuthFailure(
        kind: AuthFailureKind.unauthorized,
        message: 'Your session has expired. Log in again.',
      );
    }
    return token;
  }

  Future<Map<String, dynamic>> _send(
    String method,
    String path, {
    Map<String, dynamic>? body,
  }) async {
    return _sendAuthorized(method, path, body: body);
  }

  Future<Map<String, dynamic>> _sendAuthorized(
    String method,
    String path, {
    String? token,
    Map<String, dynamic>? body,
  }) => _transport.request(method, path, bearerToken: token, body: body);

  Future<void> _persistSession(AuthSession session) async {
    try {
      _serverId = session.serverId;
      await _sessionStore.write(configuration.serverUrl, session.serverId, session);
      final readBack = await _sessionStore.read(
        configuration.serverUrl,
        session.serverId,
      );
      if (readBack?.accessToken != session.accessToken) {
        _markStorageUnavailable();
        return;
      }
      // The URL-only slot was used by the first desktop build. Once the
      // server-scoped write is verified, remove that legacy copy so logout
      // and future restores cannot resurrect an old server identity.
      await _sessionStore.delete(configuration.serverUrl, null);
    } on Object {
      _markStorageUnavailable();
    }
  }

  Future<void> _clearStoredSession() async {
    for (final serverId in {_serverId, null}) {
      try {
        await _sessionStore.delete(configuration.serverUrl, serverId);
      } on Object {
        _markStorageUnavailable();
      }
    }
  }

  void _markStorageUnavailable() {
    _storageWarning =
        'Secure storage is unavailable; this login will last only until the app closes.';
  }

  @override
  void dispose() {
    _session = null;
    _sessionState.dispose();
    _transport.dispose();
  }
}
