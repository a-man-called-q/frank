import 'package:flutter/material.dart';

import '../models/team_models.dart';
import '../models/workspace_models.dart';

/// The four agents used by the Team mockup.
///
/// Keep these values deterministic so widget and golden tests do not depend on
/// a live gateway or a clock. The employee IDs intentionally match the
/// existing workspace fixture IDs.
List<TeamAgentProfile> fixtureTeamProfiles(OfficeWorkspace workspace) {
  final profiles = <String, TeamAgentProfile>{
    for (final profile in _profiles) profile.employeeId: profile,
  };

  return [
    for (final employee in workspace.employees)
      profiles[employee.id] == null
          ? _fallbackProfile(employee)
          : _projectProfile(employee, profiles[employee.id]!),
  ];
}

TeamAgentProfile _projectProfile(
  OfficeEmployee employee,
  TeamAgentProfile metadata,
) {
  return TeamAgentProfile(
    employeeId: employee.id,
    roleId: metadata.roleId,
    roleRevision: metadata.roleRevision,
    name: employee.name,
    role: employee.role,
    specialization: metadata.specialization,
    initials: employee.initials,
    status: _teamStatusFor(employee.status),
    accentColor: metadata.accentColor,
    imageAsset: metadata.imageAsset,
    tagline: metadata.tagline,
    currentProject: metadata.currentProject,
    assignment: metadata.assignment,
    model: metadata.model,
    modelOverride: metadata.modelOverride,
    modelSource: metadata.modelSource,
    roleDefaultModel: metadata.roleDefaultModel,
    pendingModelOverride: metadata.pendingModelOverride,
    pendingModelChange: metadata.pendingModelChange,
    revision: metadata.revision,
    promptPack: metadata.promptPack,
    level: metadata.level,
    traits: metadata.traits,
    capabilities: metadata.capabilities,
    activity: metadata.activity,
  );
}

const _profiles = <TeamAgentProfile>[
  TeamAgentProfile(
    employeeId: 'ae-maya',
    roleId: 'role-account-executive',
    name: 'Maya Chen',
    role: 'Account Executive',
    specialization: 'Generalist',
    initials: 'MC',
    status: TeamAgentStatus.available,
    accentColor: 0xFF9A68A5,
    imageAsset: 'assets/characters/team/maya-chen.png',
    tagline: 'Turns a fuzzy brief into a focused way forward.',
    currentProject: 'Meridian Finance',
    assignment: 'Define finance workflow',
    model: 'openai/gpt-4o-mini',
    roleDefaultModel: 'openai/gpt-4o-mini',
    promptPack: 'caveman',
    level: 'full',
    traits: ['Warm', 'Decisive', 'Client-minded'],
    capabilities: [
      TeamCapability(label: 'Email', icon: Icons.mail_outline),
      TeamCapability(label: 'Calendar', icon: Icons.calendar_today_outlined),
      TeamCapability(label: 'Taskboard', icon: Icons.view_kanban_outlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Brief ready for handoff',
        detail: 'Meridian Finance · Define finance workflow',
        timeLabel: '12 min ago',
        icon: Icons.assignment_outlined,
      ),
      TeamActivityEvent(
        label: 'Checked in with client',
        detail: 'Email · Meridian & Co.',
        timeLabel: '34 min ago',
        icon: Icons.mail_outline,
      ),
      TeamActivityEvent(
        label: 'Became available',
        detail: 'Ready for the next brief',
        timeLabel: '1 hr ago',
        icon: Icons.check_circle_outline,
      ),
    ],
  ),
  TeamAgentProfile(
    employeeId: 'analyst-budi',
    roleId: 'role-system-analyst',
    name: 'Budi Santoso',
    role: 'System Analyst',
    specialization: 'Researcher',
    initials: 'BS',
    status: TeamAgentStatus.working,
    accentColor: 0xFF82B7E8,
    imageAsset: 'assets/characters/team/budi-santoso.png',
    tagline: 'Finds the shape of a system before anyone builds it.',
    currentProject: 'Northstar Inventory',
    assignment: 'Map warehouse intake',
    model: 'anthropic/claude-3.5-sonnet',
    roleDefaultModel: 'anthropic/claude-3.5-sonnet',
    promptPack: 'caveman',
    level: 'full',
    traits: ['Analytical', 'Patient', 'Thorough'],
    capabilities: [
      TeamCapability(label: 'Browser', icon: Icons.language),
      TeamCapability(label: 'Drive', icon: Icons.folder_open_outlined),
      TeamCapability(label: 'Taskboard', icon: Icons.view_kanban_outlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Started mapping intake flow',
        detail: 'Northstar Inventory · Discovery',
        timeLabel: '6 min ago',
        icon: Icons.bolt_outlined,
      ),
      TeamActivityEvent(
        label: 'Added warehouse notes',
        detail: 'Drive · 4 artifacts updated',
        timeLabel: '21 min ago',
        icon: Icons.note_add_outlined,
      ),
      TeamActivityEvent(
        label: 'Approval requested',
        detail: 'Waiting for the smallest useful scope',
        timeLabel: '48 min ago',
        icon: Icons.rate_review_outlined,
      ),
    ],
  ),
  TeamAgentProfile(
    employeeId: 'programmer-nia',
    roleId: 'role-junior-programmer',
    name: 'Nia Alvarez',
    role: 'Junior Programmer',
    specialization: 'Builder',
    initials: 'NA',
    status: TeamAgentStatus.idle,
    accentColor: 0xFF77C69B,
    imageAsset: 'assets/characters/team/nia-alvarez.png',
    tagline: 'Makes practical things feel surprisingly simple.',
    currentProject: 'Northstar Inventory',
    assignment: 'Design replenishment dashboard',
    model: 'openai/gpt-4o-mini',
    roleDefaultModel: 'openai/gpt-4o-mini',
    promptPack: 'caveman',
    level: 'full',
    traits: ['Curious', 'Practical', 'Methodical'],
    capabilities: [
      TeamCapability(label: 'Terminal', icon: Icons.terminal_outlined),
      TeamCapability(label: 'Database inspect', icon: Icons.storage_outlined),
      TeamCapability(label: 'Drive', icon: Icons.folder_open_outlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Drafted dashboard outline',
        detail: 'Northstar Inventory · Replenishment',
        timeLabel: '26 min ago',
        icon: Icons.dashboard_outlined,
      ),
      TeamActivityEvent(
        label: 'Inspected inventory tables',
        detail: 'Database · Read-only session',
        timeLabel: '1 hr ago',
        icon: Icons.storage_outlined,
      ),
      TeamActivityEvent(
        label: 'Paused between assignments',
        detail: 'No action required',
        timeLabel: '2 hrs ago',
        icon: Icons.pause_circle_outline,
      ),
    ],
  ),
  TeamAgentProfile(
    employeeId: 'accountant-dimas',
    roleId: 'role-accountant',
    name: 'Dimas Pratama',
    role: 'Accountant',
    specialization: 'Reviewer',
    initials: 'DP',
    status: TeamAgentStatus.reviewing,
    accentColor: 0xFFBE9DEB,
    imageAsset: 'assets/characters/team/dimas-pratama.png',
    tagline: 'Keeps the important details honest and easy to audit.',
    currentProject: 'Meridian Finance',
    assignment: 'Review approval controls',
    model: 'google/gemini-2.5-flash',
    roleDefaultModel: 'google/gemini-2.5-flash',
    promptPack: 'caveman',
    level: 'full',
    traits: ['Precise', 'Cautious', 'Fair'],
    capabilities: [
      TeamCapability(label: 'Taskboard', icon: Icons.view_kanban_outlined),
      TeamCapability(label: 'Drive', icon: Icons.folder_open_outlined),
      TeamCapability(label: 'Approval Review', icon: Icons.shield_outlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Reviewing approval controls',
        detail: 'Meridian Finance · Controls',
        timeLabel: '9 min ago',
        icon: Icons.rate_review_outlined,
      ),
      TeamActivityEvent(
        label: 'Flagged an evidence gap',
        detail: 'Drive · Needs one supporting artifact',
        timeLabel: '42 min ago',
        icon: Icons.flag_outlined,
      ),
      TeamActivityEvent(
        label: 'Accepted handoff',
        detail: 'From Maya · Meridian Finance',
        timeLabel: '1 hr ago',
        icon: Icons.call_received_outlined,
      ),
    ],
  ),
];

TeamAgentProfile _fallbackProfile(OfficeEmployee employee) {
  return TeamAgentProfile(
    employeeId: employee.id,
    name: employee.name,
    role: employee.role,
    specialization: null,
    initials: employee.initials,
    status: _teamStatusFor(employee.status),
    accentColor: employee.color,
    imageAsset: 'assets/branding/frank-logo.png',
    tagline: 'A new member of the Frank office.',
    currentProject: 'No active project',
    assignment: 'Waiting for an assignment',
    model: 'Unconfigured',
    promptPack: 'caveman',
    level: 'full',
    traits: const ['Ready', 'Helpful', 'Reliable'],
    capabilities: const [],
    activity: const [],
  );
}

TeamAgentStatus _teamStatusFor(String status) => switch (status.toLowerCase()) {
  'available' => TeamAgentStatus.available,
  'working' => TeamAgentStatus.working,
  'reviewing' => TeamAgentStatus.reviewing,
  'blocked' => TeamAgentStatus.blocked,
  'offline' => TeamAgentStatus.offline,
  _ => TeamAgentStatus.idle,
};
