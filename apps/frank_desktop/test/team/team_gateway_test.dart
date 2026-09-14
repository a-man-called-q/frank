import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/gateway/http_frank_gateway.dart';
import 'package:frank_desktop/core/models/team_models.dart';
import '../support/fake_transport.dart';

void main() {
  test('typed Team patches preserve revision and wire clear state', () async {
    final commands = <Map<String, dynamic>>[];
    final transport = TestFrankTransport((method, path, {body}) async {
      if (method == 'GET' && path == '/v2/capabilities') {
        return const {
          'features': ['team'],
        };
      }
      if (method == 'POST' && path == '/v2/commands') {
        commands.add(body!);
        return const <String, dynamic>{};
      }
      if (method == 'GET' && path == '/v2/snapshot') {
        return const {
          'revision': 8,
          'agents': <dynamic>[],
          'roles': <dynamic>[],
        };
      }
      fail('Unexpected request: $method $path');
    });
    final gateway = HttpFrankGateway(transport);

    await gateway.updateAgentPatch(
      agentId: 'agent-1',
      patch: const TeamAgentPatch(
        modelOverride: TeamPatchField<String>.clear(),
      ),
      expectedRevision: 7,
    );
    await gateway.updateRolePatch(
      roleId: 'role-1',
      patch: const TeamRolePatch(
        defaultModel: TeamPatchField<String>.set('openai/gpt-4o-mini'),
      ),
      expectedRevision: 8,
    );

    expect(commands, hasLength(2));
    final first = commands[0];
    expect(first['expected_revision'], 7);
    expect((first['command'] as Map<String, dynamic>)['data'], {
      'agent_id': 'agent-1',
      'patch': {'model_override': null, 'clear_model_override': true},
    });

    final second = commands[1];
    expect(second['expected_revision'], 8);
    expect((second['command'] as Map<String, dynamic>)['data'], {
      'role_id': 'role-1',
      'patch': {'default_model': 'openai/gpt-4o-mini', 'clear_model': false},
    });
  });
}
