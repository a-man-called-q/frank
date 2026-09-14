import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:http/io_client.dart';

import '../auth/auth_models.dart';
import '../auth/auth_session_state.dart';
import '../models/connection_models.dart';

/// Stable API-version helpers shared by auth and feature transports.
abstract final class FrankApiVersion {
  static const int current = 2;
  static const String prefix = '/v2';

  static String path(String endpoint) {
    final normalized = endpoint.startsWith('/') ? endpoint : '/$endpoint';
    final withoutVersion = normalized.startsWith('$prefix/')
        ? normalized.substring(prefix.length)
        : normalized;
    return '$prefix$withoutVersion';
  }
}

/// Compile-time configuration for the desktop HTTP/TLS transport.
class FrankClientConfiguration {
  const FrankClientConfiguration({
    required this.serverUrl,
    this.caCertificatePemBase64 = '',
  });

  final String serverUrl;
  final String caCertificatePemBase64;

  static const fromEnvironment = FrankClientConfiguration(
    serverUrl: String.fromEnvironment('FRANK_SERVER_URL'),
    caCertificatePemBase64: String.fromEnvironment(
      'FRANK_SERVER_CA_PEM_BASE64',
    ),
  );

  String? get validationError {
    if (serverUrl.trim().isEmpty) return 'FRANK_SERVER_URL is not configured';
    final uri = Uri.tryParse(serverUrl);
    if (uri == null ||
        uri.scheme.toLowerCase() != 'https' ||
        uri.host.isEmpty) {
      return 'FRANK_SERVER_URL must be an HTTPS URL';
    }
    if (uri.userInfo.isNotEmpty ||
        uri.query.isNotEmpty ||
        uri.fragment.isNotEmpty) {
      return 'FRANK_SERVER_URL may not contain credentials or query parameters';
    }
    if (caCertificatePemBase64.isNotEmpty) {
      try {
        final decoded = base64.decode(caCertificatePemBase64);
        final pem = utf8.decode(decoded, allowMalformed: true);
        if (!pem.contains('-----BEGIN CERTIFICATE-----')) {
          return 'FRANK_SERVER_CA_PEM_BASE64 must contain a PEM certificate';
        }
        if (pem.contains('PRIVATE KEY')) {
          return 'FRANK_SERVER_CA_PEM_BASE64 may not contain a private key';
        }
      } on FormatException {
        return 'FRANK_SERVER_CA_PEM_BASE64 is not valid base64';
      }
    }
    return null;
  }

  Uri get uri =>
      Uri.parse(serverUrl).replace(path: _trimPath(Uri.parse(serverUrl).path));
}

/// The only owner of HTTP/TLS setup, bearer headers, and event cursors.
///
/// Feature gateways receive this transport (directly or through
/// [HttpAuthRepository]) and therefore cannot accidentally create a second
/// client with a different certificate policy or token source.
class FrankTransport {
  FrankTransport(
    this.configuration, {
    AuthSessionState? sessionState,
    http.Client? client,
  }) : sessionState = sessionState ?? AuthSessionState(),
       _client = client ?? _buildClient(configuration),
       _ownsClient = client == null;

  final FrankClientConfiguration configuration;
  final AuthSessionState sessionState;
  final http.Client _client;
  final bool _ownsClient;
  bool _disposed = false;
  FrankConnectionStatus _connectionStatus = const FrankConnectionStatus.initial();
  final StreamController<FrankConnectionStatus> _connectionStatusChanges =
      StreamController<FrankConnectionStatus>.broadcast(sync: true);

  static const _requestTimeout = Duration(seconds: 15);

  FrankConnectionStatus get connectionStatus => _connectionStatus;

  Stream<FrankConnectionStatus> get connectionStatusStream =>
      _connectionStatusChanges.stream;

  /// Updates the transport's observable lifecycle without creating another
  /// socket. The gateway uses this after capabilities negotiation, while the
  /// event stream uses it for reconnect/backoff transitions.
  void setConnectionStatus(FrankConnectionStatus status) {
    if (_disposed || status == _connectionStatus) return;
    _connectionStatus = status;
    if (!_connectionStatusChanges.isClosed) {
      _connectionStatusChanges.add(status);
    }
  }

  void markChecking() => setConnectionStatus(
    _connectionStatus.copyWith(
      phase: FrankConnectionPhase.checking,
      detail: FrankConnectionPhase.checking.detail,
    ),
  );

  void markCapabilities(FrankServerCapabilities capabilities) {
    setConnectionStatus(capabilities.statusFor(FrankApiVersion.current));
  }

  Future<Map<String, dynamic>> authorizedJson(
    String method,
    String path, {
    Map<String, dynamic>? body,
  }) {
    final token = sessionState.bearerToken;
    if (token == null || token.isEmpty) {
      throw const AuthFailure(
        kind: AuthFailureKind.unauthorized,
        message: 'Your session has expired. Log in again.',
      );
    }
    return request(method, path, bearerToken: token, body: body);
  }

  Future<Map<String, dynamic>> request(
    String method,
    String path, {
    String? bearerToken,
    Map<String, dynamic>? body,
  }) async {
    if (_disposed) {
      throw const AuthFailure(
        kind: AuthFailureKind.network,
        message: 'The Frank transport has been closed.',
      );
    }
    final headers = <String, String>{
      'accept': 'application/json',
      if (body != null) 'content-type': 'application/json',
      if (bearerToken != null) 'authorization': 'Bearer $bearerToken',
    };
    final uri = configuration.uri.resolve(FrankApiVersion.path(path));
    try {
      final request = http.Request(method, uri)
        ..headers.addAll(headers)
        ..body = body == null ? '' : jsonEncode(body);
      final streamed = await _client.send(request).timeout(_requestTimeout);
      final response = await http.Response.fromStream(streamed);
      final decoded = response.body.isEmpty
          ? <String, dynamic>{}
          : Map<String, dynamic>.from(jsonDecode(response.body) as Map);
      if (response.statusCode >= 200 && response.statusCode < 300) {
        final previous = _connectionStatus;
        setConnectionStatus(
          previous.copyWith(
            phase: FrankConnectionPhase.connected,
            detail: null,
            lastConnectedAt: DateTime.now().toUtc(),
          ),
        );
        return decoded;
      }
      final failure = _failureForResponse(
        response.statusCode,
        decoded,
        response.headers,
      );
      if (failure.kind == AuthFailureKind.protocolMismatch ||
          response.statusCode == 426) {
        setConnectionStatus(
          _connectionStatus.copyWith(
            phase: FrankConnectionPhase.incompatible,
            detail: failure.message,
          ),
        );
      }
      throw failure;
    } on AuthFailure {
      rethrow;
    } on HandshakeException catch (error) {
      setConnectionStatus(
        _connectionStatus.copyWith(
          phase: FrankConnectionPhase.offline,
          detail: _friendlyNetworkError(error),
        ),
      );
      throw AuthFailure(
        kind: AuthFailureKind.tls,
        message: _friendlyNetworkError(error),
      );
    } on TlsException catch (error) {
      setConnectionStatus(
        _connectionStatus.copyWith(
          phase: FrankConnectionPhase.offline,
          detail: _friendlyNetworkError(error),
        ),
      );
      throw AuthFailure(
        kind: AuthFailureKind.tls,
        message: _friendlyNetworkError(error),
      );
    } on SocketException catch (error) {
      setConnectionStatus(
        _connectionStatus.copyWith(
          phase: FrankConnectionPhase.offline,
          detail: _friendlyNetworkError(error),
        ),
      );
      throw AuthFailure(
        kind: AuthFailureKind.network,
        message: _friendlyNetworkError(error),
      );
    } on TimeoutException {
      setConnectionStatus(
        _connectionStatus.copyWith(
          phase: FrankConnectionPhase.offline,
          detail: 'The Frank server did not respond.',
        ),
      );
      throw const AuthFailure(
        kind: AuthFailureKind.network,
        message:
            'The Frank server did not respond. Check the address and try again.',
      );
    } on FormatException {
      throw const AuthFailure(
        kind: AuthFailureKind.response,
        message: 'The Frank server returned an invalid response.',
      );
    } on Object catch (error) {
      throw AuthFailure(
        kind: AuthFailureKind.network,
        message: error.toString(),
      );
    }
  }

  /// Emits strictly increasing server event sequence numbers.
  ///
  /// Reconnecting state is local to this stream. A caller can stop listening
  /// on logout and a new listener can resume from the last emitted cursor.
  Stream<int> watchEvents({int after = 0}) async* {
    var cursor = after;
    var backoff = const Duration(milliseconds: 250);
    while (!_disposed && sessionState.activeSession != null) {
      HttpClient? socketClient;
      WebSocket? socket;
      try {
        final session = sessionState.activeSession;
        if (session == null) return;
        socketClient = _buildSocketClient(configuration);
        final uri = configuration.uri.replace(
          scheme: 'wss',
          path: FrankApiVersion.path('/events'),
          queryParameters: {'after': '$cursor'},
        );
        socket = await WebSocket.connect(
          uri.toString(),
          headers: {'authorization': 'Bearer ${session.accessToken}'},
          customClient: socketClient,
        );
        // A successful event-stream handshake is itself an online signal. It
        // must not require a separate status socket or a follow-up request to
        // make the shell leave reconnecting/offline.
        setConnectionStatus(
          _connectionStatus.copyWith(
            phase: FrankConnectionPhase.connected,
            detail: null,
            lastConnectedAt: DateTime.now().toUtc(),
          ),
        );
        backoff = const Duration(milliseconds: 250);
        await for (final frame in socket) {
          if (frame is! String) continue;
          final decoded = jsonDecode(frame);
          if (decoded is! Map) continue;
          final error = decoded['error'];
          if (error is Map &&
              error['code']?.toString().replaceAll('_', '-') ==
                  'resync-required') {
            try {
              final snapshot = await request(
                'GET',
                '/v2/snapshot',
                bearerToken: sessionState.bearerToken,
              );
              cursor =
                  int.tryParse(snapshot['event_seq']?.toString() ?? '') ??
                  cursor;
            } on Object {
              // Preserve the cursor when the resync snapshot is transiently
              // unavailable; reconnecting remains safe and bounded.
            }
            yield cursor;
            break;
          }
          final sequence = int.tryParse(decoded['seq']?.toString() ?? '');
          if (sequence == null || sequence <= cursor) continue;
          cursor = sequence;
          yield cursor;
        }
      } on Object {
        // A reconnecting feature surface keeps its last projection visible
        // while the daemon or network is restarting. Expose the backoff
        // lifecycle so the shell can explain why mutations are paused.
        setConnectionStatus(
          _connectionStatus.copyWith(
            phase: FrankConnectionPhase.offline,
            detail: 'The Frank event stream is unavailable.',
          ),
        );
      } finally {
        await socket?.close();
        socketClient?.close(force: true);
      }
      if (_disposed || sessionState.activeSession == null) return;
      setConnectionStatus(
        _connectionStatus.copyWith(
          phase: FrankConnectionPhase.reconnecting,
          detail: FrankConnectionPhase.reconnecting.detail,
        ),
      );
      await Future<void>.delayed(backoff);
      backoff = Duration(
        milliseconds: (backoff.inMilliseconds * 2).clamp(250, 5000),
      );
    }
  }

  void dispose() {
    _disposed = true;
    if (_ownsClient) _client.close();
    _connectionStatusChanges.close();
  }
}

String _trimPath(String path) {
  if (path.isEmpty || path == '/') return '';
  return '/${path.replaceAll(RegExp(r'^/+|/+$'), '')}';
}

http.Client _buildClient(FrankClientConfiguration configuration) {
  final context = SecurityContext(withTrustedRoots: true);
  if (configuration.caCertificatePemBase64.isNotEmpty) {
    context.setTrustedCertificatesBytes(
      base64.decode(configuration.caCertificatePemBase64),
    );
  }
  final client = HttpClient(context: context)
    ..connectionTimeout = const Duration(seconds: 10)
    ..idleTimeout = const Duration(seconds: 30);
  // A custom CA extends trust; it never disables hostname verification.
  return IOClient(client);
}

HttpClient _buildSocketClient(FrankClientConfiguration configuration) {
  final context = SecurityContext(withTrustedRoots: true);
  if (configuration.caCertificatePemBase64.isNotEmpty) {
    context.setTrustedCertificatesBytes(
      base64.decode(configuration.caCertificatePemBase64),
    );
  }
  return HttpClient(context: context)
    ..connectionTimeout = const Duration(seconds: 10)
    ..idleTimeout = const Duration(seconds: 30);
}

AuthFailure _failureForResponse(
  int status,
  Map<String, dynamic> body,
  Map<String, String> headers,
) {
  final message = (body['message'] ?? body['error'] ?? 'Request failed')
      .toString();
  if (status == 401) {
    return AuthFailure(
      kind: AuthFailureKind.unauthorized,
      message: message == 'Request failed'
          ? 'Invalid username or password.'
          : message,
      statusCode: status,
    );
  }
  if (status == 429) {
    final seconds = int.tryParse(headers['retry-after'] ?? '');
    return AuthFailure(
      kind: AuthFailureKind.rateLimited,
      message: 'Too many login attempts. Try again later.',
      statusCode: status,
      retryAfter: seconds == null ? null : Duration(seconds: seconds),
    );
  }
  if (status == 409 || status == 428) {
    return AuthFailure(
      kind: AuthFailureKind.setupRequired,
      message: message == 'Request failed'
          ? 'Set up the Frank owner account on the server first.'
          : message,
      statusCode: status,
    );
  }
  if (status == 426) {
    return AuthFailure(
      kind: AuthFailureKind.protocolMismatch,
      message: 'This Frank app is not compatible with the server.',
      statusCode: status,
    );
  }
  if (status == 404) {
    return AuthFailure(
      kind: AuthFailureKind.protocolMismatch,
      message: 'This Frank server does not support local owner login.',
      statusCode: status,
    );
  }
  if (status == 503 && message.toLowerCase().contains('owner account')) {
    return AuthFailure(
      kind: AuthFailureKind.setupRequired,
      message: 'Set up the Frank owner account on the server first.',
      statusCode: status,
    );
  }
  return AuthFailure(
    kind: AuthFailureKind.server,
    message: message == 'Request failed'
        ? 'The Frank server returned an error ($status).'
        : message,
    statusCode: status,
  );
}

String _friendlyNetworkError(Object error) {
  final text = error.toString();
  if (text.contains('CERTIFICATE') || text.contains('certificate')) {
    return 'The server certificate is not trusted. Check FRANK_SERVER_CA_PEM_BASE64.';
  }
  return 'Could not connect to the Frank server.';
}
