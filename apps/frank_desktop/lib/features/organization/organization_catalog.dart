import 'package:flutter/material.dart';

import '../../app/icons.dart';
import '../../core/models/organization_models.dart';

extension OrganizationCapabilityMetadata on OrganizationCapabilityKind {
  String get label => switch (this) {
    OrganizationCapabilityKind.email => 'Email',
    OrganizationCapabilityKind.calendar => 'Calendar',
    OrganizationCapabilityKind.taskboard => 'Taskboard',
    OrganizationCapabilityKind.drive => 'Drive',
    OrganizationCapabilityKind.browser => 'Browser',
    OrganizationCapabilityKind.terminal => 'Terminal',
    OrganizationCapabilityKind.database => 'Database',
  };

  String get category => switch (this) {
    OrganizationCapabilityKind.email ||
    OrganizationCapabilityKind.calendar => 'Communication',
    OrganizationCapabilityKind.taskboard => 'Operations',
    OrganizationCapabilityKind.drive ||
    OrganizationCapabilityKind.browser => 'Knowledge',
    OrganizationCapabilityKind.terminal ||
    OrganizationCapabilityKind.database => 'Execution',
  };

  IconData get icon => switch (this) {
    OrganizationCapabilityKind.email => FrankIcons.mail,
    OrganizationCapabilityKind.calendar => FrankIcons.calendar,
    OrganizationCapabilityKind.taskboard => FrankIcons.dashboard,
    OrganizationCapabilityKind.drive => FrankIcons.drive,
    OrganizationCapabilityKind.browser => FrankIcons.browser,
    OrganizationCapabilityKind.terminal => FrankIcons.terminal,
    OrganizationCapabilityKind.database => FrankIcons.database,
  };

  List<String> get permissions => switch (this) {
    OrganizationCapabilityKind.email => const ['read', 'send'],
    OrganizationCapabilityKind.calendar => const ['read', 'create', 'update'],
    OrganizationCapabilityKind.taskboard => const [
      'read',
      'create',
      'update',
      'assign',
    ],
    OrganizationCapabilityKind.drive => const ['read', 'write', 'share'],
    OrganizationCapabilityKind.browser => const ['browse', 'download'],
    OrganizationCapabilityKind.terminal => const ['execute'],
    OrganizationCapabilityKind.database => const ['inspect', 'read', 'write'],
  };

  bool get isSensitive =>
      this == OrganizationCapabilityKind.terminal ||
      this == OrganizationCapabilityKind.database;
}

extension OrganizationContextPolicyMetadata on OrganizationContextPolicy {
  String get label => switch (this) {
    OrganizationContextPolicy.minimumRequired => 'Minimum required',
    OrganizationContextPolicy.summaryAndArtifacts => 'Summary + artifacts',
    OrganizationContextPolicy.fullContext => 'Full context',
  };
}
