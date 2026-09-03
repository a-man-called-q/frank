import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/fixtures/fixture_organization.dart';
import 'package:frank_desktop/core/models/organization_models.dart';
import 'package:frank_desktop/features/organization/organization_validator.dart';

void main() {
  test('accepts only agency relation pairings', () {
    final graph = fixtureOrganizationGraph();
    final staff = graph.nodes.firstWhere(
      (node) => node.kind == OrganizationNodeKind.staff,
    );
    final otherStaff = graph.nodes.firstWhere(
      (node) => node.kind == OrganizationNodeKind.staff && node.id != staff.id,
    );
    final capability = graph.nodes.firstWhere(
      (node) => node.kind == OrganizationNodeKind.capability,
    );
    final approval = graph.nodes.firstWhere(
      (node) => node.kind == OrganizationNodeKind.approval,
    );

    expect(
      inferOrganizationRelationKind(source: staff, target: otherStaff),
      OrganizationRelationKind.handoff,
    );
    expect(
      inferOrganizationRelationKind(source: staff, target: capability),
      OrganizationRelationKind.toolAccess,
    );
    expect(
      inferOrganizationRelationKind(source: staff, target: approval),
      OrganizationRelationKind.review,
    );
    expect(
      inferOrganizationRelationKind(source: capability, target: approval),
      isNull,
    );
  });

  test('structural errors block and handoff loops only warn', () {
    final graph = fixtureOrganizationGraph();
    final withoutStaff = graph.copyWith(
      nodes: graph.nodes
          .where((node) => node.kind != OrganizationNodeKind.staff)
          .toList(),
      relations: const [],
    );
    expect(validateOrganization(withoutStaff).hasErrors, isTrue);

    final staff = graph.nodes
        .where((node) => node.kind == OrganizationNodeKind.staff)
        .take(2)
        .toList();
    final loop = OrganizationGraph(
      id: 'loop',
      draftRevision: 0,
      publishedRevision: 0,
      nodes: staff,
      relations: [
        OrganizationRelation(
          id: 'forward',
          kind: OrganizationRelationKind.handoff,
          sourceNodeId: staff.first.id,
          targetNodeId: staff.last.id,
        ),
        OrganizationRelation(
          id: 'back',
          kind: OrganizationRelationKind.handoff,
          sourceNodeId: staff.last.id,
          targetNodeId: staff.first.id,
        ),
      ],
    );
    final validation = validateOrganization(loop);
    expect(validation.hasErrors, isFalse);
    expect(
      validation.issues.map((issue) => issue.message),
      contains(contains('Handoff loop')),
    );
  });

  test('duplicate employee assignments are structural errors', () {
    final graph = fixtureOrganizationGraph();
    final maya = graph.nodes.firstWhere((node) => node.id == 'staff-maya');
    final duplicate = maya.copyWith(label: 'Maya copy');
    final invalid = graph.copyWith(nodes: [...graph.nodes, duplicate]);

    final validation = validateOrganization(invalid);
    expect(validation.hasErrors, isTrue);
    expect(
      validation.issues.map((issue) => issue.message),
      contains(contains('appears more than once')),
    );
  });

  test('rejects permissions that the selected capability does not expose', () {
    final graph = fixtureOrganizationGraph();
    final invalid = graph.copyWith(
      relations: [
        ...graph.relations,
        const OrganizationRelation(
          id: 'invalid-email-permission',
          kind: OrganizationRelationKind.toolAccess,
          sourceNodeId: 'staff-maya',
          targetNodeId: 'email-primary',
          permissions: ['delete'],
        ),
      ],
    );

    final validation = validateOrganization(invalid);
    expect(validation.hasErrors, isTrue);
    expect(
      validation.issues.map((issue) => issue.message),
      contains(contains('not available')),
    );
  });

  test('requires a permission on every tool-access relation', () {
    final graph = fixtureOrganizationGraph();
    final invalid = graph.copyWith(
      relations: [
        ...graph.relations,
        const OrganizationRelation(
          id: 'missing-tool-permission',
          kind: OrganizationRelationKind.toolAccess,
          sourceNodeId: 'staff-maya',
          targetNodeId: 'email-primary',
        ),
      ],
    );

    final validation = validateOrganization(invalid);
    expect(validation.hasErrors, isTrue);
    expect(
      validation.issues.map((issue) => issue.message),
      contains(contains('needs a permission')),
    );
  });

  test('validation issues serialize independently from the flow library', () {
    const validation = OrganizationValidation([
      OrganizationValidationIssue(
        severity: OrganizationIssueSeverity.warning,
        message: 'Needs a profile',
        nodeId: 'email-primary',
      ),
    ]);

    expect(
      OrganizationValidation.fromJson(validation.toJson()).toJson(),
      validation.toJson(),
    );
  });
}
