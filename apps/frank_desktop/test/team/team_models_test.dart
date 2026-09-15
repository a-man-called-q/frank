import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/models/team_models.dart';

void main() {
  test('agent patches omit unset fields and preserve explicit clears', () {
    const patch = TeamAgentPatch(
      displayName: TeamPatchField<String>.set('Maya Chen'),
      modelOverride: TeamPatchField<String>.clear(),
    );

    expect(patch.toJson(), {
      'display_name': 'Maya Chen',
      'model_override': null,
      'clear_model_override': true,
    });
    expect(Map<String, Object?>.from(patch), patch.toJson());
    expect(const TeamAgentPatch().toJson(), isEmpty);
  });

  test('role model serialization distinguishes set, clear, and unset', () {
    const setPatch = TeamRolePatch(
      defaultModel: TeamPatchField<String>.set('openai/gpt-4o-mini'),
    );
    const clearPatch = TeamRolePatch(
      defaultModel: TeamPatchField<String>.clear(),
    );

    expect(setPatch.toJson(), {
      'default_model': 'openai/gpt-4o-mini',
      'clear_model': false,
    });
    expect(clearPatch.toJson(), {'default_model': null, 'clear_model': true});
    expect(const TeamRolePatch().toJson(), isEmpty);
  });

  test(
    'typed gateway adapters serialize patches through the legacy boundary',
    () {
      const patch = TeamRolePatch(name: TeamPatchField<String>.set('Reviewer'));

      expect(patch['name'], 'Reviewer');
      expect(patch['missing'], isNull);
      expect(patch.keys, contains('name'));
    },
  );

  test('new members only send identity, role, and an explicit model override', () {
    const member = TeamAgentDraft(
      displayName: 'Nia',
      roleId: 'role-researcher',
      modelOverride: 'openai/gpt-5-mini',
    );
    expect(member.toJson(), {
      'role_id': 'role-researcher',
      'display_name': 'Nia',
      'model_override': 'openai/gpt-5-mini',
    });
  });
}
