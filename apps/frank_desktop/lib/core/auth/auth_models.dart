import 'dart:convert';

/// The local owner identity returned by frankd after a successful login.
class AuthOwner {
  const AuthOwner({required this.id, required this.username});

  final String id;
  final String username;

  factory AuthOwner.fromJson(Map<String, dynamic> json) {
    final id = (json['id'] ?? json['owner_id'] ?? '').toString();
    final username = (json['username'] ?? json['user'] ?? '').toString();
    if (id.isEmpty || username.isEmpty) {
      throw const FormatException('auth response did not contain an owner');
    }
    return AuthOwner(id: id, username: username);
  }

  Map<String, dynamic> toJson() => {'id': id, 'username': username};
}

class AuthSession {
  const AuthSession({
    required this.accessToken,
    required this.expiresAt,
    required this.serverId,
    required this.owner,
    this.deviceId,
    this.sessionId,
  });

  final String accessToken;
  final DateTime expiresAt;
  final String serverId;
  final AuthOwner owner;
  final String? deviceId;
  final String? sessionId;

  bool get isExpired => !expiresAt.isAfter(DateTime.now().toUtc());

  Map<String, dynamic> toJson() => {
    'access_token': accessToken,
    'expires_at': expiresAt.toUtc().toIso8601String(),
    'server_id': serverId,
    'owner': owner.toJson(),
    if (deviceId != null) 'device_id': deviceId,
    if (sessionId != null) 'session_id': sessionId,
  };

  factory AuthSession.fromJson(Map<String, dynamic> json) {
    final nested = json['session'] is Map
        ? Map<String, dynamic>.from(json['session'] as Map)
        : const <String, dynamic>{};
    final token = (json['access_token'] ?? json['token'] ?? '').toString();
    final serverId = (json['server_id'] ?? json['serverId'] ?? '').toString();
    final ownerJson = json['owner'] is Map
        ? Map<String, dynamic>.from(json['owner'] as Map)
        : <String, dynamic>{
            'id': json['owner_id'] ?? json['user_id'],
            'username': json['username'] ?? json['user'],
          };
    final expiresAt = _parseTimestamp(
      json['expires_at'] ?? json['expiresAt'] ?? nested['expires_at'],
    );
    if (token.isEmpty || serverId.isEmpty || expiresAt == null) {
      throw const FormatException('auth response did not contain a session');
    }
    return AuthSession(
      accessToken: token,
      expiresAt: expiresAt,
      serverId: serverId,
      owner: AuthOwner.fromJson(ownerJson),
      deviceId: (json['device_id'] ?? json['deviceId'] ?? nested['device_id'])
          ?.toString(),
      sessionId:
          (json['session_id'] ?? json['sessionId'] ?? nested['session_id'])
              ?.toString(),
    );
  }
}

DateTime? _parseTimestamp(Object? value) {
  if (value is num) {
    final millis = value > 100000000000 ? value.toInt() : value.toInt() * 1000;
    return DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
  }
  if (value is String) {
    final parsed = DateTime.tryParse(value);
    if (parsed != null) return parsed.toUtc();
    final number = num.tryParse(value);
    if (number != null) return _parseTimestamp(number);
  }
  return null;
}

class AuthStatus {
  const AuthStatus({
    required this.configured,
    required this.authMethod,
    this.serverId,
  });

  final bool configured;
  final String authMethod;
  final String? serverId;

  factory AuthStatus.fromJson(Map<String, dynamic> json) => AuthStatus(
    configured:
        json['configured'] as bool? ?? json['setup_complete'] as bool? ?? false,
    authMethod: (json['auth_method'] ?? json['method'] ?? 'local').toString(),
    serverId: (json['server_id'] ?? json['serverId'])?.toString(),
  );
}

enum AuthFailureKind {
  configuration,
  invalidCredentials,
  rateLimited,
  setupRequired,
  unauthorized,
  protocolMismatch,
  network,
  tls,
  server,
  response,
}

class AuthFailure implements Exception {
  const AuthFailure({
    required this.kind,
    required this.message,
    this.statusCode,
    this.retryAfter,
  });

  final AuthFailureKind kind;
  final String message;
  final int? statusCode;
  final Duration? retryAfter;

  String get userMessage => message;

  @override
  String toString() => message;
}

String encodeAuthPayload(Map<String, dynamic> payload) => jsonEncode(payload);
