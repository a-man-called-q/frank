import '../../app/icons.dart';
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
    initials: 'MC',
    status: TeamAgentStatus.available,
    accentColor: 0xFF9A68A5,
    imageAsset: 'assets/characters/team/maya-chen.png',
    tagline: 'Turns a fuzzy brief into a focused way forward.',
    currentProject: 'Meridian Finance',
    assignment: 'Define finance workflow',
    model: 'openai/gpt-4o-mini',
    roleDefaultModel: 'openai/gpt-4o-mini',
    capabilities: [
      TeamCapability(label: 'Email', icon: FrankIcons.mailOutline),
      TeamCapability(label: 'Calendar', icon: FrankIcons.calendarTodayOutlined),
      TeamCapability(label: 'Taskboard', icon: FrankIcons.viewKanbanOutlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Brief ready for handoff',
        detail: 'Meridian Finance · Define finance workflow',
        timeLabel: '12 min ago',
        icon: FrankIcons.assignmentOutlined,
      ),
      TeamActivityEvent(
        label: 'Checked in with client',
        detail: 'Email · Meridian & Co.',
        timeLabel: '34 min ago',
        icon: FrankIcons.mailOutline,
      ),
      TeamActivityEvent(
        label: 'Became available',
        detail: 'Ready for the next brief',
        timeLabel: '1 hr ago',
        icon: FrankIcons.checkCircleOutline,
      ),
    ],
  ),
  TeamAgentProfile(
    employeeId: 'analyst-budi',
    roleId: 'role-system-analyst',
    name: 'Budi Santoso',
    role: 'System Analyst',
    initials: 'BS',
    status: TeamAgentStatus.working,
    accentColor: 0xFF82B7E8,
    imageAsset: 'assets/characters/team/budi-santoso.png',
    tagline: 'Finds the shape of a system before anyone builds it.',
    currentProject: 'Northstar Inventory',
    assignment: 'Map warehouse intake',
    model: 'anthropic/claude-3.5-sonnet',
    roleDefaultModel: 'anthropic/claude-3.5-sonnet',
    capabilities: [
      TeamCapability(label: 'Browser', icon: FrankIcons.language),
      TeamCapability(label: 'Drive', icon: FrankIcons.folderOpenOutlined),
      TeamCapability(label: 'Taskboard', icon: FrankIcons.viewKanbanOutlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Started mapping intake flow',
        detail: 'Northstar Inventory · Discovery',
        timeLabel: '6 min ago',
        icon: FrankIcons.boltOutlined,
      ),
      TeamActivityEvent(
        label: 'Added warehouse notes',
        detail: 'Drive · 4 artifacts updated',
        timeLabel: '21 min ago',
        icon: FrankIcons.noteAddOutlined,
      ),
      TeamActivityEvent(
        label: 'Approval requested',
        detail: 'Waiting for the smallest useful scope',
        timeLabel: '48 min ago',
        icon: FrankIcons.rateReviewOutlined,
      ),
    ],
  ),
  TeamAgentProfile(
    employeeId: 'programmer-nia',
    roleId: 'role-junior-programmer',
    name: 'Nia Alvarez',
    role: 'Junior Programmer',
    initials: 'NA',
    status: TeamAgentStatus.idle,
    accentColor: 0xFF77C69B,
    imageAsset: 'assets/characters/team/nia-alvarez.png',
    tagline: 'Makes practical things feel surprisingly simple.',
    currentProject: 'Northstar Inventory',
    assignment: 'Design replenishment dashboard',
    model: 'openai/gpt-4o-mini',
    roleDefaultModel: 'openai/gpt-4o-mini',
    capabilities: [
      TeamCapability(label: 'Terminal', icon: FrankIcons.terminalOutlined),
      TeamCapability(
        label: 'Database inspect',
        icon: FrankIcons.storageOutlined,
      ),
      TeamCapability(label: 'Drive', icon: FrankIcons.folderOpenOutlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Drafted dashboard outline',
        detail: 'Northstar Inventory · Replenishment',
        timeLabel: '26 min ago',
        icon: FrankIcons.dashboardOutlined,
      ),
      TeamActivityEvent(
        label: 'Inspected inventory tables',
        detail: 'Database · Read-only session',
        timeLabel: '1 hr ago',
        icon: FrankIcons.storageOutlined,
      ),
      TeamActivityEvent(
        label: 'Paused between assignments',
        detail: 'No action required',
        timeLabel: '2 hrs ago',
        icon: FrankIcons.pauseCircleOutline,
      ),
    ],
  ),
  TeamAgentProfile(
    employeeId: 'accountant-dimas',
    roleId: 'role-accountant',
    name: 'Dimas Pratama',
    role: 'Accountant',
    initials: 'DP',
    status: TeamAgentStatus.reviewing,
    accentColor: 0xFFBE9DEB,
    imageAsset: 'assets/characters/team/dimas-pratama.png',
    tagline: 'Keeps the important details honest and easy to audit.',
    currentProject: 'Meridian Finance',
    assignment: 'Review approval controls',
    model: 'google/gemini-2.5-flash',
    roleDefaultModel: 'google/gemini-2.5-flash',
    capabilities: [
      TeamCapability(label: 'Taskboard', icon: FrankIcons.viewKanbanOutlined),
      TeamCapability(label: 'Drive', icon: FrankIcons.folderOpenOutlined),
      TeamCapability(label: 'Approval Review', icon: FrankIcons.shieldOutlined),
    ],
    activity: [
      TeamActivityEvent(
        label: 'Reviewing approval controls',
        detail: 'Meridian Finance · Controls',
        timeLabel: '9 min ago',
        icon: FrankIcons.rateReviewOutlined,
      ),
      TeamActivityEvent(
        label: 'Flagged an evidence gap',
        detail: 'Drive · Needs one supporting artifact',
        timeLabel: '42 min ago',
        icon: FrankIcons.flagOutlined,
      ),
      TeamActivityEvent(
        label: 'Accepted handoff',
        detail: 'From Maya · Meridian Finance',
        timeLabel: '1 hr ago',
        icon: FrankIcons.callReceivedOutlined,
      ),
    ],
  ),
];

TeamAgentProfile _fallbackProfile(OfficeEmployee employee) {
  return TeamAgentProfile(
    employeeId: employee.id,
    name: employee.name,
    role: employee.role,
    initials: employee.initials,
    status: _teamStatusFor(employee.status),
    accentColor: employee.color,
    imageAsset: 'assets/branding/frank-logo.png',
    tagline: 'A new member of the Frank office.',
    currentProject: 'No active project',
    assignment: 'Waiting for an assignment',
    model: 'Unconfigured',
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
