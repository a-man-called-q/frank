import '../gateway/frank_gateway.dart';
import '../models/workspace_models.dart';

class FixtureFrankGateway implements FrankGateway {
  static const _ae = OfficeEmployee(
    id: 'ae-maya',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFFE2A84B,
  );

  static const _employees = <OfficeEmployee>[
    _ae,
    OfficeEmployee(
      id: 'analyst-budi',
      name: 'Budi Santoso',
      role: 'System Analyst',
      status: 'Working',
      initials: 'BS',
      color: 0xFF82B7E8,
    ),
    OfficeEmployee(
      id: 'programmer-nia',
      name: 'Nia Alvarez',
      role: 'Junior Programmer',
      status: 'Idle',
      initials: 'NA',
      color: 0xFF77C69B,
    ),
    OfficeEmployee(
      id: 'accountant-dimas',
      name: 'Dimas Pratama',
      role: 'Accountant',
      status: 'Reviewing',
      initials: 'DP',
      color: 0xFFBE9DEB,
    ),
  ];

  static const _projects = <OfficeProject>[
    OfficeProject(
      id: 'northstar-inventory',
      name: 'Northstar Inventory',
      client: 'Northstar Logistics',
      status: ProjectStatus.active,
      progress: 0.62,
      team: ['Budi Santoso', 'Nia Alvarez'],
      summary: 'Warehouse inventory and replenishment dashboard.',
    ),
    OfficeProject(
      id: 'meridian-finance',
      name: 'Meridian Finance',
      client: 'Meridian & Co.',
      status: ProjectStatus.planning,
      progress: 0.18,
      team: ['Maya Chen', 'Dimas Pratama'],
      summary: 'A lightweight finance operations workspace.',
    ),
    OfficeProject(
      id: 'atlas-handoff',
      name: 'Atlas Handoff',
      client: 'Atlas Studio',
      status: ProjectStatus.review,
      progress: 0.84,
      team: ['Budi Santoso', 'Dimas Pratama'],
      summary: 'Documentation and delivery readiness review.',
    ),
  ];

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    await Future<void>.delayed(const Duration(milliseconds: 180));
    return const OfficeWorkspace(
      name: 'Frank Agency',
      projects: _projects,
      employees: _employees,
      accountExecutive: _ae,
      messages: [
        OfficeMessage(
          id: 'welcome',
          role: ChatRole.assistant,
          text:
              'Hi, I’m Maya, your Account Executive. Tell me what your team needs and I’ll turn it into a clear engagement for the office.',
        ),
        OfficeMessage(
          id: 'brief',
          role: ChatRole.user,
          text: 'I need a usable inventory workflow for a small logistics team.',
        ),
        OfficeMessage(
          id: 'brief-reply',
          role: ChatRole.assistant,
          text:
              'I’ve opened Northstar Inventory as a working brief. I can bring in a system analyst first, then staff the build once we agree on the scope.',
        ),
      ],
    );
  }

  @override
  Stream<String> replyTo(String text, {required String projectId}) async* {
    const response = [
      'I’ll turn that into a brief for the office. ',
      'First I’ll clarify the outcome, then I’ll suggest the smallest team needed. ',
      'You can approve the plan before anyone starts delivery.',
    ];
    for (final chunk in response) {
      await Future<void>.delayed(const Duration(milliseconds: 220));
      yield chunk;
    }
  }
}
