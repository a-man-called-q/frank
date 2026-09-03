import 'package:flutter/material.dart';

/// The presentation status shown by the Team roster.
///
/// This is deliberately separate from [OfficeEmployee.status]. The current
/// workspace DTO is a remote-facing compatibility type, while the Team
/// surface needs a small, typed vocabulary for chips, icons, and filtering.
enum TeamAgentStatus { available, working, idle, reviewing, blocked, offline }

extension TeamAgentStatusMetadata on TeamAgentStatus {
  String get label => switch (this) {
    TeamAgentStatus.available => 'Available',
    TeamAgentStatus.working => 'Working',
    TeamAgentStatus.idle => 'Idle',
    TeamAgentStatus.reviewing => 'Reviewing',
    TeamAgentStatus.blocked => 'Blocked',
    TeamAgentStatus.offline => 'Offline',
  };

  Color get color => switch (this) {
    TeamAgentStatus.available => const Color(0xFF77C69B),
    TeamAgentStatus.working => const Color(0xFF82B7E8),
    TeamAgentStatus.idle => const Color(0xFF9A9D9B),
    TeamAgentStatus.reviewing => const Color(0xFFE2A84B),
    TeamAgentStatus.blocked => const Color(0xFFE47B7B),
    TeamAgentStatus.offline => const Color(0xFF65686C),
  };

  IconData get icon => switch (this) {
    TeamAgentStatus.available => Icons.check_circle_outline,
    TeamAgentStatus.working => Icons.bolt_outlined,
    TeamAgentStatus.idle => Icons.pause_circle_outline,
    TeamAgentStatus.reviewing => Icons.rate_review_outlined,
    TeamAgentStatus.blocked => Icons.error_outline,
    TeamAgentStatus.offline => Icons.cloud_off_outlined,
  };
}

/// A deterministic, presentation-only profile for the first Team mockup.
///
/// The model intentionally lives outside [OfficeWorkspace] and the gateway
/// contract. When the remote agent DTO arrives, the fixture can be replaced
/// without changing the Team surface's rendering API.
class TeamAgentProfile {
  const TeamAgentProfile({
    required this.employeeId,
    required this.name,
    required this.role,
    required this.initials,
    required this.status,
    required this.accentColor,
    required this.imageAsset,
    required this.tagline,
    required this.currentProject,
    required this.assignment,
    required this.provider,
    required this.model,
    required this.promptPack,
    required this.level,
    required this.traits,
    required this.capabilities,
    required this.activity,
  });

  final String employeeId;
  final String name;
  final String role;
  final String initials;
  final TeamAgentStatus status;
  final int accentColor;
  final String imageAsset;
  final String tagline;
  final String currentProject;
  final String assignment;
  final String provider;
  final String model;
  final String promptPack;
  final String level;
  final List<String> traits;
  final List<TeamCapability> capabilities;
  final List<TeamActivityEvent> activity;

  Color get accent => Color(accentColor);

  String get providerSummary => '$provider · $model';
}

class TeamCapability {
  const TeamCapability({required this.label, required this.icon});

  final String label;
  final IconData icon;
}

class TeamActivityEvent {
  const TeamActivityEvent({
    required this.label,
    required this.detail,
    required this.timeLabel,
    required this.icon,
  });

  final String label;
  final String detail;
  final String timeLabel;
  final IconData icon;
}
