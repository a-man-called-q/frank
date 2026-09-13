import 'dart:async';

import 'package:frank_desktop/core/transport/frank_transport.dart';
import 'package:http/http.dart' as http;

typedef JsonRequestHandler =
    Future<Map<String, dynamic>> Function(
      String method,
      String path, {
      Map<String, dynamic>? body,
    });

/// In-memory transport for gateway tests. It exercises the same transport
/// interface as production without exposing an AuthRepository-shaped seam.
class TestFrankTransport extends FrankTransport {
  TestFrankTransport(this.handler)
    : super(
        const FrankClientConfiguration(serverUrl: 'https://test.invalid'),
        client: _NoopClient(),
      );

  final JsonRequestHandler handler;

  @override
  Future<Map<String, dynamic>> authorizedJson(
    String method,
    String path, {
    Map<String, dynamic>? body,
  }) => handler(method, path, body: body);

  @override
  Stream<int> watchEvents({int after = 0}) => const Stream<int>.empty();
}

class _NoopClient extends http.BaseClient {
  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    throw StateError('Test transport request was not stubbed.');
  }
}
