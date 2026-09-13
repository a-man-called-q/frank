import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/gateway/snapshot_store.dart';

void main() {
  test('concurrent loads share one request', () async {
    final gate = Completer<Map<String, dynamic>>();
    var calls = 0;
    final store = SnapshotStore(
      loader: () {
        calls++;
        return gate.future;
      },
    );

    final first = store.load();
    final second = store.load();
    expect(identical(first, second), isTrue);
    expect(calls, 1);

    gate.complete({'revision': 4, 'event_seq': 9});
    expect(await first, {'revision': 4, 'event_seq': 9});
    expect(store.revision, 4);
    expect(store.eventSeq, 9);
    expect(await store.load(), {'revision': 4, 'event_seq': 9});
    expect(calls, 1);
  });

  test('rejected loads are not cached', () async {
    var calls = 0;
    final store = SnapshotStore(
      loader: () async {
        calls++;
        if (calls == 1) throw StateError('temporary');
        return {'revision': 2};
      },
    );

    await expectLater(store.load(), throwsStateError);
    expect(store.snapshot, isNull);
    expect(await store.load(), {'revision': 2});
    expect(calls, 2);
  });

  test('event cursor invalidates the visible snapshot', () async {
    final events = StreamController<int>();
    final store = SnapshotStore(
      loader: () async => {'revision': 1, 'event_seq': 3},
      eventStreamFactory: ({int after = 0}) => events.stream,
    );

    await store.load();
    expect(store.snapshot, isNotNull);
    events.add(4);
    await Future<void>.delayed(Duration.zero);
    expect(store.snapshot, isNull);
    expect(store.eventSeq, 4);
    await events.close();
    store.dispose();
  });

  test('logout wins over a late snapshot response', () async {
    final gate = Completer<Map<String, dynamic>>();
    final store = SnapshotStore(loader: () => gate.future);
    final pending = store.load();

    store.logout();
    gate.complete({'revision': 8});
    expect(await pending, {'revision': 8});
    expect(store.snapshot, isNull);
    expect(store.hasSnapshot, isFalse);
  });

  test('command conflicts retain the latest snapshot for the caller', () async {
    final store = SnapshotStore(
      loader: () async => const <String, dynamic>{},
      commandSender:
          ({
            required String type,
            required Map<String, dynamic> data,
            int? expectedRevision,
          }) async => {
            'error': {
              'code': 'organization-revision-conflict',
              'message': 'stale edit',
            },
            'latest_snapshot': {'revision': 12},
          },
    );

    await expectLater(
      store.command(type: 'save_organization_draft', expectedRevision: 11),
      throwsA(
        isA<SnapshotCommandConflict>()
            .having(
              (error) => error.code,
              'code',
              'organization-revision-conflict',
            )
            .having((error) => error.latestSnapshot, 'latest', {
              'revision': 12,
            }),
      ),
    );
  });
}
