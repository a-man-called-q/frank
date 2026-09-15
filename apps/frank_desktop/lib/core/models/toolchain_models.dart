import 'package:flutter/foundation.dart';

enum ToolchainStatus {
  missing,
  needsApproval,
  installing,
  ready,
  manualRequirement,
  failed,
  incompatible,
}

extension ToolchainStatusMetadata on ToolchainStatus {
  String get wireName => switch (this) {
    ToolchainStatus.missing => 'missing',
    ToolchainStatus.needsApproval => 'needs_approval',
    ToolchainStatus.installing => 'installing',
    ToolchainStatus.ready => 'ready',
    ToolchainStatus.manualRequirement => 'manual_requirement',
    ToolchainStatus.failed => 'failed',
    ToolchainStatus.incompatible => 'incompatible',
  };

  String get label => switch (this) {
    ToolchainStatus.missing => 'Missing',
    ToolchainStatus.needsApproval => 'Needs approval',
    ToolchainStatus.installing => 'Installing',
    ToolchainStatus.ready => 'Ready',
    ToolchainStatus.manualRequirement => 'Manual requirement',
    ToolchainStatus.failed => 'Failed',
    ToolchainStatus.incompatible => 'Incompatible',
  };
}

@immutable
class ToolchainInstallPlan {
  const ToolchainInstallPlan({
    required this.manifestId,
    required this.version,
    required this.source,
    required this.sha256,
    required this.sizeBytes,
    required this.installPath,
    required this.checks,
    required this.approvalScope,
  });

  factory ToolchainInstallPlan.fromJson(Map<String, dynamic> json) =>
      ToolchainInstallPlan(
        manifestId: json['manifest_id']?.toString() ?? '',
        version: json['version']?.toString() ?? '',
        source: json['source']?.toString() ?? 'Host requirement',
        sha256: json['sha256']?.toString() ?? '',
        sizeBytes: _int(json['size_bytes']),
        installPath: json['install_path']?.toString() ?? '',
        checks: [
          for (final value
              in json['checks'] is List
                  ? json['checks'] as List
                  : const <Object?>[])
            value.toString(),
        ],
        approvalScope: json['approval_scope']?.toString() ?? 'Task only',
      );

  final String manifestId;
  final String version;
  final String source;
  final String sha256;
  final int sizeBytes;
  final String installPath;
  final List<String> checks;
  final String approvalScope;
}

@immutable
class ToolchainRequirement {
  const ToolchainRequirement({
    required this.manifestId,
    required this.label,
    required this.requiredVersion,
    required this.status,
    this.detectedVersion,
    this.diagnostic,
    this.installPlan,
  });

  factory ToolchainRequirement.fromJson(Map<String, dynamic> json) =>
      ToolchainRequirement(
        manifestId: json['manifest_id']?.toString() ?? '',
        label: json['label']?.toString() ?? 'Toolchain',
        requiredVersion: json['required_version']?.toString() ?? '',
        status: _status(json['status']),
        detectedVersion: _nullableString(json['detected_version']),
        diagnostic: _nullableString(json['diagnostic']),
        installPlan: json['install_plan'] is Map
            ? ToolchainInstallPlan.fromJson(
                Map<String, dynamic>.from(json['install_plan'] as Map),
              )
            : null,
      );

  final String manifestId;
  final String label;
  final String requiredVersion;
  final ToolchainStatus status;
  final String? detectedVersion;
  final String? diagnostic;
  final ToolchainInstallPlan? installPlan;
}

enum RunnerStatus {
  pairing,
  idle,
  busy,
  offline,
  failed,
  unknown,
}

extension RunnerStatusMetadata on RunnerStatus {
  String get wireName => switch (this) {
    RunnerStatus.pairing => 'pairing',
    RunnerStatus.idle => 'idle',
    RunnerStatus.busy => 'busy',
    RunnerStatus.offline => 'offline',
    RunnerStatus.failed => 'failed',
    RunnerStatus.unknown => 'unknown',
  };

  String get label => switch (this) {
    RunnerStatus.pairing => 'Pairing',
    RunnerStatus.idle => 'Idle',
    RunnerStatus.busy => 'Busy',
    RunnerStatus.offline => 'Offline',
    RunnerStatus.failed => 'Failed',
    RunnerStatus.unknown => 'Unknown',
  };
}

@immutable
class RunnerPathMapping {
  const RunnerPathMapping({
    required this.daemonRoot,
    required this.hostRoot,
    this.projectId,
  });

  factory RunnerPathMapping.fromJson(Map<String, dynamic> json) =>
      RunnerPathMapping(
        daemonRoot: json['daemon_root']?.toString() ?? '',
        hostRoot: json['host_root']?.toString() ?? '',
        projectId: _nullableString(json['project_id']),
      );

  final String daemonRoot;
  final String hostRoot;
  final String? projectId;
}

@immutable
class RunnerInfo {
  const RunnerInfo({
    required this.id,
    required this.name,
    required this.host,
    required this.status,
    this.toolchains = const [],
    this.pathMappings = const [],
  });

  factory RunnerInfo.fromJson(Map<String, dynamic> json) => RunnerInfo(
    id: json['id']?.toString() ?? '',
    name: json['name']?.toString() ?? 'Host runner',
    host: json['host']?.toString() ?? '',
    status: _runnerStatus(json['status']),
    toolchains: [
      for (final value
          in json['toolchains'] is List
              ? json['toolchains'] as List
              : const <Object?>[])
        value.toString(),
    ],
    pathMappings: [
      for (final value
          in json['path_mappings'] is List
              ? json['path_mappings'] as List
              : const <Object?>[])
        if (value is Map)
          RunnerPathMapping.fromJson(Map<String, dynamic>.from(value)),
    ],
  );

  final String id;
  final String name;
  final String host;
  final RunnerStatus status;
  final List<String> toolchains;
  final List<RunnerPathMapping> pathMappings;

  bool get available => status == RunnerStatus.idle || status == RunnerStatus.busy;
}

@immutable
class ToolchainInstallResult {
  const ToolchainInstallResult({
    required this.id,
    required this.runnerId,
    required this.status,
    this.installedPath,
    this.diagnostic,
  });

  factory ToolchainInstallResult.fromJson(Map<String, dynamic> json) =>
      ToolchainInstallResult(
        id: json['id']?.toString() ?? '',
        runnerId: json['runner_id']?.toString() ?? '',
        status: _status(json['status']),
        installedPath: _nullableString(json['installed_path']),
        diagnostic: _nullableString(json['diagnostic']),
      );

  final String id;
  final String runnerId;
  final ToolchainStatus status;
  final String? installedPath;
  final String? diagnostic;
}

enum ToolchainApprovalDecision { allowOnce, allowForTask, deny }

int _int(Object? value) =>
    value is num ? value.toInt() : int.tryParse(value?.toString() ?? '') ?? 0;

String? _nullableString(Object? value) {
  final text = value?.toString().trim();
  return text == null || text.isEmpty ? null : text;
}

ToolchainStatus _status(Object? value) => switch (value?.toString()) {
  'missing' => ToolchainStatus.missing,
  'needs_approval' => ToolchainStatus.needsApproval,
  'installing' => ToolchainStatus.installing,
  'ready' => ToolchainStatus.ready,
  'manual_requirement' => ToolchainStatus.manualRequirement,
  'failed' => ToolchainStatus.failed,
  'incompatible' => ToolchainStatus.incompatible,
  _ => ToolchainStatus.failed,
};

RunnerStatus _runnerStatus(Object? value) => switch (value?.toString()) {
  'pairing' => RunnerStatus.pairing,
  'idle' => RunnerStatus.idle,
  'busy' => RunnerStatus.busy,
  'offline' => RunnerStatus.offline,
  'failed' => RunnerStatus.failed,
  _ => RunnerStatus.unknown,
};
