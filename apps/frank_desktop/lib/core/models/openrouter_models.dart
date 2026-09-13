/// Owner-facing OpenRouter status and catalog DTOs.
///
/// These models intentionally contain metadata only. The API key never
/// crosses the gateway boundary into Flutter.
library;

enum OpenRouterConnectionState { connected, error, notConfigured }

class OpenRouterConnection {
  const OpenRouterConnection({
    required this.configured,
    required this.credentialSource,
    required this.checkedAt,
    required this.catalogRefreshedAt,
    required this.diagnostic,
  });

  factory OpenRouterConnection.fromJson(Map<String, dynamic> json) {
    final configured = json['configured'] == true;
    final diagnostic = _nullableString(json['diagnostic']);
    return OpenRouterConnection(
      configured: configured,
      credentialSource: _nullableString(json['credential_source']),
      checkedAt: _parseTimestamp(json['checked_at']),
      catalogRefreshedAt: _parseTimestamp(json['catalog_refreshed_at']),
      diagnostic: diagnostic,
    );
  }

  final bool configured;
  final String? credentialSource;
  final DateTime? checkedAt;
  final DateTime? catalogRefreshedAt;
  final String? diagnostic;

  OpenRouterConnectionState get state {
    if (!configured) return OpenRouterConnectionState.notConfigured;
    return diagnostic == null
        ? OpenRouterConnectionState.connected
        : OpenRouterConnectionState.error;
  }

  String get credentialLabel => switch (credentialSource) {
    'keychain' => 'Managed by host keychain',
    'file' => 'Managed by server credential store',
    'environment' => 'Managed by server environment',
    _ => 'Not configured',
  };
}

class OpenRouterModel {
  const OpenRouterModel({
    required this.id,
    required this.name,
    required this.canonicalSlug,
    required this.contextLength,
    required this.inputPricePerToken,
    required this.outputPricePerToken,
    required this.supportedParameters,
    required this.deprecatedAt,
  });

  factory OpenRouterModel.fromJson(Map<String, dynamic> json) {
    final id = _string(json['id']);
    return OpenRouterModel(
      id: id,
      name: _string(json['name'], fallback: id),
      canonicalSlug: _nullableString(json['canonical_slug']) ?? id,
      contextLength: _int(json['context_length']),
      inputPricePerToken: _nullableString(json['input_price_per_token']),
      outputPricePerToken: _nullableString(json['output_price_per_token']),
      supportedParameters: [
        for (final value
            in json['supported_parameters'] is List
                ? json['supported_parameters'] as List
                : const <dynamic>[])
          if (value != null) value.toString(),
      ],
      deprecatedAt: _parseTimestamp(json['deprecated_at']),
    );
  }

  final String id;
  final String name;
  final String canonicalSlug;
  final int? contextLength;
  final String? inputPricePerToken;
  final String? outputPricePerToken;
  final List<String> supportedParameters;
  final DateTime? deprecatedAt;

  bool get supportsTools => supportedParameters.contains('tools');
  bool get deprecated => deprecatedAt != null;

  String get priceLabel {
    final input = inputPricePerToken ?? '—';
    final output = outputPricePerToken ?? '—';
    return 'in $input · out $output / token';
  }

  @override
  bool operator ==(Object other) =>
      other is OpenRouterModel && other.canonicalSlug == canonicalSlug;

  @override
  int get hashCode => canonicalSlug.hashCode;
}

class OpenRouterCatalog {
  const OpenRouterCatalog({
    required this.models,
    required this.refreshedAt,
    required this.stale,
  });

  factory OpenRouterCatalog.fromJson(Map<String, dynamic> json) {
    return OpenRouterCatalog(
      models: [
        for (final value
            in json['models'] is List
                ? json['models'] as List
                : const <dynamic>[])
          if (value is Map)
            OpenRouterModel.fromJson(Map<String, dynamic>.from(value)),
      ],
      refreshedAt: _parseTimestamp(json['refreshed_at']),
      stale: json['stale'] == true,
    );
  }

  final List<OpenRouterModel> models;
  final DateTime? refreshedAt;
  final bool stale;
}

String _string(Object? value, {String fallback = ''}) {
  final result = value?.toString() ?? '';
  return result.isEmpty ? fallback : result;
}

String? _nullableString(Object? value) {
  final result = value?.toString() ?? '';
  return result.isEmpty ? null : result;
}

int? _int(Object? value) =>
    value is num ? value.toInt() : int.tryParse(value?.toString() ?? '');

DateTime? _parseTimestamp(Object? value) {
  final raw = value?.toString();
  if (raw == null || raw.isEmpty) return null;
  final millis = int.tryParse(raw);
  if (millis != null) {
    return DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
  }
  return DateTime.tryParse(raw)?.toUtc();
}
