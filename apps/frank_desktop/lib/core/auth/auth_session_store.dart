import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

import 'auth_models.dart';

/// Persistence boundary for authenticated sessions.
///
/// Implementations must keep the access token in secure OS storage. The
/// repository deliberately treats failures as a warning and keeps the
/// current session in memory, so an unavailable keychain never blocks a
/// successful login.
abstract interface class AuthSessionStore {
  Future<AuthSession?> read(String serverUrl, [String? serverId]);

  Future<void> write(String serverUrl, String? serverId, AuthSession session);

  Future<void> delete(String serverUrl, [String? serverId]);
}

/// Production implementation backed by flutter_secure_storage's macOS
/// Keychain integration. A null server id addresses the legacy URL-only key.
class FlutterSecureAuthSessionStore implements AuthSessionStore {
  const FlutterSecureAuthSessionStore({FlutterSecureStorage? storage})
    : _storage = storage ?? const FlutterSecureStorage();

  final FlutterSecureStorage _storage;

  @override
  Future<AuthSession?> read(String serverUrl, [String? serverId]) async {
    final encoded = await _storage.read(key: _key(serverUrl, serverId));
    if (encoded == null || encoded.isEmpty) return null;
    final decoded = jsonDecode(encoded);
    if (decoded is! Map) throw const FormatException('invalid stored session');
    return AuthSession.fromJson(Map<String, dynamic>.from(decoded));
  }

  @override
  Future<void> write(String serverUrl, String? serverId, AuthSession session) =>
      _storage.write(
        key: _key(serverUrl, serverId),
        value: jsonEncode(session.toJson()),
      );

  @override
  Future<void> delete(String serverUrl, [String? serverId]) =>
      _storage.delete(key: _key(serverUrl, serverId));

  static String keyFor(String serverUrl, String? serverId) =>
      _key(serverUrl, serverId);
}

String _key(String serverUrl, String? serverId) {
  final encodedUrl = _storageSlot(serverUrl);
  if (serverId == null || serverId.isEmpty) {
    return 'frank.session.$encodedUrl';
  }
  return 'frank.session.$encodedUrl.${_storageSlot(serverId)}';
}

String _storageSlot(String value) => base64Url
    .encode(utf8.encode(value))
    .replaceAll('=', '')
    .replaceAll('/', '_')
    .replaceAll('+', '-');
