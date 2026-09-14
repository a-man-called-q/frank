/// Connection and capability metadata shared by the transport and shell.
///
/// These types deliberately contain no credentials. They are safe to expose
/// to status bars, login gates, diagnostics, and mutation guards.
library;

enum FrankConnectionPhase {
  checking,
  connected,
  reconnecting,
  offline,
  incompatible,
}

extension FrankConnectionPhaseMetadata on FrankConnectionPhase {
  String get label => switch (this) {
    FrankConnectionPhase.checking => 'Checking',
    FrankConnectionPhase.connected => 'Connected',
    FrankConnectionPhase.reconnecting => 'Reconnecting',
    FrankConnectionPhase.offline => 'Offline',
    FrankConnectionPhase.incompatible => 'Update required',
  };

  String get detail => switch (this) {
    FrankConnectionPhase.checking => 'Checking Frank server compatibility.',
    FrankConnectionPhase.connected => 'Frank server is ready.',
    FrankConnectionPhase.reconnecting => 'Reconnecting to the Frank server.',
    FrankConnectionPhase.offline => 'Frank server is unreachable.',
    FrankConnectionPhase.incompatible =>
      'This app must be upgraded before it can make changes.',
  };
}

class FrankConnectionStatus {
  const FrankConnectionStatus({
    required this.phase,
    required this.appProtocolVersion,
    this.serverProtocolVersion,
    this.serverVersion,
    this.detail,
    this.lastConnectedAt,
  });

  const FrankConnectionStatus.initial({int appProtocolVersion = 2})
    : this(
        phase: FrankConnectionPhase.checking,
        appProtocolVersion: appProtocolVersion,
      );

  final FrankConnectionPhase phase;
  final int appProtocolVersion;
  final int? serverProtocolVersion;
  final String? serverVersion;
  final String? detail;
  final DateTime? lastConnectedAt;

  /// Mutations are only safe after compatibility and a successful request
  /// have been observed. This is intentionally derived rather than copied
  /// from remote input so a stale wire value cannot unlock a command.
  bool get canMutate => phase == FrankConnectionPhase.connected;

  String get label => phase.label;

  FrankConnectionStatus copyWith({
    FrankConnectionPhase? phase,
    int? appProtocolVersion,
    Object? serverProtocolVersion = _unset,
    Object? serverVersion = _unset,
    Object? detail = _unset,
    Object? lastConnectedAt = _unset,
  }) => FrankConnectionStatus(
    phase: phase ?? this.phase,
    appProtocolVersion: appProtocolVersion ?? this.appProtocolVersion,
    serverProtocolVersion: identical(serverProtocolVersion, _unset)
        ? this.serverProtocolVersion
        : serverProtocolVersion as int?,
    serverVersion: identical(serverVersion, _unset)
        ? this.serverVersion
        : serverVersion as String?,
    detail: identical(detail, _unset) ? this.detail : detail as String?,
    lastConnectedAt: identical(lastConnectedAt, _unset)
        ? this.lastConnectedAt
        : lastConnectedAt as DateTime?,
  );

  static const _unset = Object();

  @override
  String toString() =>
      'FrankConnectionStatus(${phase.name}, app=$appProtocolVersion, '
      'server=$serverProtocolVersion)';
}

/// Version range advertised by `/v2/capabilities`.
class FrankProtocolVersionRange {
  const FrankProtocolVersionRange({required this.min, required this.max});

  factory FrankProtocolVersionRange.fromJson(
    Object? value, {
    int fallback = 2,
  }) {
    if (value is Map) {
      final min = _int(value['min'] ?? value['minimum']) ?? fallback;
      final max = _int(value['max'] ?? value['maximum']) ?? min;
      return FrankProtocolVersionRange(min: min, max: max);
    }
    if (value is Iterable) {
      final versions = value.map(_int).whereType<int>().toList();
      if (versions.isNotEmpty) {
        versions.sort();
        return FrankProtocolVersionRange(
          min: versions.first,
          max: versions.last,
        );
      }
    }
    if (value is num) {
      final version = value.toInt();
      return FrankProtocolVersionRange(min: version, max: version);
    }
    return FrankProtocolVersionRange(min: fallback, max: fallback);
  }

  final int min;
  final int max;

  bool accepts(int version) => version >= min && version <= max;

  Map<String, int> toJson() => {'min': min, 'max': max};
}

/// Typed, redacted capability document returned by Frank server v2.
class FrankServerCapabilities {
  const FrankServerCapabilities({
    required this.protocolVersion,
    required this.supportedVersions,
    required this.minimumCompatibleClient,
    required this.serverVersion,
    required this.features,
    this.serverId,
    this.openRouter = const <String, Object?>{},
  });

  factory FrankServerCapabilities.fromJson(Map<String, dynamic> json) {
    final protocolVersion =
        _int(json['protocol_version'] ?? json['protocolVersion']) ?? 2;
    return FrankServerCapabilities(
      protocolVersion: protocolVersion,
      supportedVersions: FrankProtocolVersionRange.fromJson(
        json['supported_versions'] ?? json['supportedVersions'],
        fallback: protocolVersion,
      ),
      minimumCompatibleClient:
          _int(
            json['minimum_compatible_client'] ??
                json['minimumCompatibleClient'],
          ) ??
          1,
      serverVersion:
          (json['server_version'] ?? json['serverVersion'])?.toString() ??
          'Unknown',
      serverId: (json['server_id'] ?? json['serverId'])?.toString(),
      features: [
        for (final feature
            in json['features'] is List
                ? json['features'] as List
                : const <dynamic>[])
          if (feature != null) feature.toString(),
      ],
      openRouter: json['openrouter'] is Map
          ? Map<String, Object?>.from(json['openrouter'] as Map)
          : const <String, Object?>{},
    );
  }

  final int protocolVersion;
  final FrankProtocolVersionRange supportedVersions;
  final int minimumCompatibleClient;
  final String serverVersion;
  final String? serverId;
  final List<String> features;
  final Map<String, Object?> openRouter;

  bool isCompatibleWith(int appProtocolVersion) =>
      supportedVersions.accepts(appProtocolVersion) &&
      appProtocolVersion >= minimumCompatibleClient;

  bool supportsFeature(String feature) => features.contains(feature);

  FrankConnectionStatus statusFor(
    int appProtocolVersion,
  ) => FrankConnectionStatus(
    phase: isCompatibleWith(appProtocolVersion)
        ? FrankConnectionPhase.connected
        : FrankConnectionPhase.incompatible,
    appProtocolVersion: appProtocolVersion,
    serverProtocolVersion: protocolVersion,
    serverVersion: serverVersion,
    detail: isCompatibleWith(appProtocolVersion)
        ? null
        : 'Server supports protocol ${supportedVersions.min}–${supportedVersions.max}; '
              'this app uses $appProtocolVersion.',
    lastConnectedAt: isCompatibleWith(appProtocolVersion)
        ? DateTime.now().toUtc()
        : null,
  );
}

int? _int(Object? value) =>
    value is num ? value.toInt() : int.tryParse(value?.toString() ?? '');
