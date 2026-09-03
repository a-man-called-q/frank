import 'fixture_organization.dart';
import '../gateway/frank_gateway.dart';
import '../models/organization_models.dart';
import '../models/workspace_models.dart';

class FixtureFrankGateway implements FrankGateway {
  FixtureFrankGateway({
    this.latency = const Duration(milliseconds: 180),
    this.organizationLatency = Duration.zero,
    this.organizationLoadError,
    this.organizationSaveError,
    this.organizationPublishError,
  });

  final Duration latency;
  final Duration organizationLatency;
  final Object? organizationLoadError;
  final Object? organizationSaveError;
  final Object? organizationPublishError;
  OrganizationGraph? _organizationDraft;
  OrganizationGraph? _organizationPublished;

  static const _ae = OfficeEmployee(
    id: 'ae-maya',
    name: 'Maya Chen',
    role: 'Account Executive',
    status: 'Available',
    initials: 'MC',
    color: 0xFF9A68A5,
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

  static const _northstarMessages = <OfficeMessage>[
    OfficeMessage(
      id: 'northstar-welcome',
      role: ChatRole.assistant,
      text:
          'Hi, I’m Maya, your Account Executive. Tell me what your team needs and I’ll turn it into a clear engagement for the office.',
    ),
    OfficeMessage(
      id: 'northstar-brief',
      role: ChatRole.user,
      text: 'I need a usable inventory workflow for a small logistics team.',
    ),
    OfficeMessage(
      id: 'northstar-brief-reply',
      role: ChatRole.assistant,
      text:
          'I’ve opened Northstar Inventory as a working brief. I can bring in a system analyst first, then staff the build once we agree on the scope.',
    ),
  ];

  static final _projects = <OfficeProject>[
    OfficeProject(
      id: 'northstar-inventory',
      name: 'Northstar Inventory',
      client: 'Northstar Logistics',
      status: ProjectStatus.active,
      progress: 0.62,
      team: ['Budi Santoso', 'Nia Alvarez'],
      summary: 'Warehouse inventory and replenishment dashboard.',
      messages: _northstarMessages,
      missions: [
        OfficeMission(
          id: 'northstar-discovery',
          title: 'Map warehouse intake',
          status: MissionStatus.active,
          updatedAt: DateTime.utc(2026, 8, 31, 15, 20),
          assignedAgentIds: const ['analyst-budi'],
          pendingApprovalCount: 1,
          messages: [
            OfficeMessage(
              id: 'northstar-discovery-welcome',
              role: ChatRole.assistant,
              text:
                  'I’ll map the warehouse intake flow first so the team can agree on the smallest useful workflow.',
            ),
          ],
        ),
        OfficeMission(
          id: 'northstar-dashboard',
          title: 'Design replenishment dashboard',
          status: MissionStatus.planned,
          updatedAt: DateTime.utc(2026, 8, 29, 10, 30),
          messages: [],
        ),
      ],
    ),
    OfficeProject(
      id: 'meridian-finance',
      name: 'Meridian Finance',
      client: 'Meridian & Co.',
      status: ProjectStatus.planning,
      progress: 0.18,
      team: ['Maya Chen', 'Dimas Pratama'],
      summary: 'A lightweight finance operations workspace.',
      messages: [
        OfficeMessage(
          id: 'meridian-welcome',
          role: ChatRole.assistant,
          text:
              'I can help turn Meridian’s finance needs into a focused project brief.',
        ),
      ],
      missions: [
        OfficeMission(
          id: 'meridian-intake',
          title: 'Define finance workflow',
          status: MissionStatus.planned,
          updatedAt: DateTime.utc(2026, 8, 27, 9, 10),
          messages: [],
        ),
        OfficeMission(
          id: 'meridian-controls',
          title: 'Review approval controls',
          status: MissionStatus.blocked,
          updatedAt: DateTime.utc(2026, 8, 30, 13, 45),
          messages: [],
        ),
      ],
    ),
    OfficeProject(
      id: 'atlas-handoff',
      name: 'Atlas Handoff',
      client: 'Atlas Studio',
      status: ProjectStatus.review,
      progress: 0.84,
      team: ['Budi Santoso', 'Dimas Pratama'],
      summary: 'Documentation and delivery readiness review.',
      messages: [
        OfficeMessage(
          id: 'atlas-welcome',
          role: ChatRole.assistant,
          text:
              'Atlas is in review. I can help close the remaining delivery and documentation gaps.',
        ),
      ],
      missions: [
        OfficeMission(
          id: 'atlas-readiness',
          title: 'Review delivery readiness',
          status: MissionStatus.complete,
          updatedAt: DateTime.utc(2026, 8, 26, 16, 5),
          assignedAgentIds: const ['accountant-dimas'],
          messages: [
            OfficeMessage(
              id: 'atlas-readiness-welcome',
              role: ChatRole.assistant,
              text:
                  'Let’s review the final handoff checklist and surface anything that still needs an owner.',
            ),
          ],
        ),
      ],
    ),
  ];

  @override
  Future<OfficeWorkspace> loadWorkspace() async {
    await Future<void>.delayed(latency);
    return OfficeWorkspace(
      name: 'Frank Agency',
      projects: _projects,
      employees: _employees,
      accountExecutive: _ae,
    );
  }

  @override
  Future<OrganizationGraph> loadOrganization() async {
    if (organizationLoadError != null) throw organizationLoadError!;
    if (organizationLatency > Duration.zero) {
      await Future<void>.delayed(organizationLatency);
    }
    _organizationDraft ??= _copyOrganization(fixtureOrganizationGraph());
    _organizationPublished ??= _copyOrganization(_organizationDraft!);
    return _copyOrganization(_organizationDraft!);
  }

  @override
  Future<OrganizationGraph> saveOrganizationDraft(
    OrganizationGraph graph,
  ) async {
    if (organizationSaveError != null) throw organizationSaveError!;
    if (latency > Duration.zero) await Future<void>.delayed(latency);
    final saved = graph.copyWith(draftRevision: graph.draftRevision + 1);
    _organizationDraft = _copyOrganization(saved);
    return _copyOrganization(saved);
  }

  @override
  Future<OrganizationGraph> publishOrganization(
    OrganizationGraph graph, {
    required int expectedPublishedRevision,
  }) async {
    if (organizationPublishError != null) throw organizationPublishError!;
    if (latency > Duration.zero) await Future<void>.delayed(latency);
    final actual =
        _organizationPublished?.publishedRevision ??
        fixtureOrganizationGraph().publishedRevision;
    if (actual != expectedPublishedRevision) {
      throw OrganizationRevisionConflict(expectedPublishedRevision, actual);
    }
    final published = graph.copyWith(publishedRevision: actual + 1);
    _organizationDraft = _copyOrganization(published);
    _organizationPublished = _copyOrganization(published);
    return _copyOrganization(published);
  }

  OrganizationGraph _copyOrganization(OrganizationGraph graph) =>
      OrganizationGraph.fromJson(graph.toJson());

  @override
  Stream<String> replyTo(
    String text, {
    required String projectId,
    String? missionId,
  }) async* {
    final subject = missionId == null ? 'the project brief' : 'that mission';
    final response = [
      'I’ll turn that into a clear plan for $subject. ',
      'First I’ll clarify the outcome, then I’ll suggest the smallest team needed. ',
      'You can approve the plan before anyone starts delivery.',
    ];
    for (final chunk in response) {
      await Future<void>.delayed(const Duration(milliseconds: 220));
      yield chunk;
    }
  }
}
