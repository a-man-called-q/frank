import 'dart:async';

import 'auth_models.dart';

/// The process-local owner session shared by authentication and transport.
///
/// Keeping the active bearer in one small state object prevents feature
/// gateways from growing their own copies of a token.  The stream is useful
/// to transports and stores that need to tear down reconnect loops on logout;
/// consumers only receive the typed session, never a raw credential event.
class AuthSessionState {
  AuthSessionState([AuthSession? session]) : _session = session;

  AuthSession? _session;
  StreamController<AuthSession?>? _changes;
  bool _disposed = false;

  AuthSession? get activeSession => _session;

  /// Compatibility spelling for callers that model the state as a session.
  AuthSession? get session => _session;

  String? get bearerToken => _session?.accessToken;

  bool get isAuthenticated => _session != null && !_session!.isExpired;

  Stream<AuthSession?> get changes {
    final existing = _changes;
    if (existing != null) return existing.stream;
    final created = StreamController<AuthSession?>.broadcast(sync: true);
    _changes = created;
    return created.stream;
  }

  void setSession(AuthSession? session) {
    if (_disposed || identical(_session, session)) return;
    _session = session;
    _changes?.add(session);
  }

  void activate(AuthSession session) => setSession(session);

  void clear() => setSession(null);

  void dispose() {
    _disposed = true;
    _session = null;
    final changes = _changes;
    _changes = null;
    changes?.close();
  }
}
