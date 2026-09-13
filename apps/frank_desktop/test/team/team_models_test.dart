import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/models/team_models.dart';

void main() {
  test('agent patches omit unset fields and preserve explicit clears', () {
    const patch = TeamAgentPatch(
      displayName: TeamPatchField<String>.set('Maya Chen'),
      modelOverride: TeamPatchField<String>.clear(),
      avatar: TeamPatchField<TeamAvatarSpec>.set(
        TeamAvatarSpec(palette: 'frank', seed: 7),
      ),
    );

    expect(patch.toJson(), {
      'display_name': 'Maya Chen',
      'model_override': null,
      'clear_model_override': true,
      'avatar': {'palette': 'frank', 'seed': 7},
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
}
