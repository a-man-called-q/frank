import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/core/models/journal_models.dart';

void main() {
  test('parses Frank epoch-millisecond timestamps from the Journal API', () {
    final entry = JournalEntry.fromJson({
      'sequence': 7,
      'occurred_at': '1760000000123',
      'kind': 'check',
      'outcome': 'success',
      'summary': 'Flutter tests passed',
    });

    expect(
      entry.occurredAt,
      DateTime.fromMillisecondsSinceEpoch(1760000000123, isUtc: true),
    );
    expect(entry.kind, JournalEntryKind.check);
    expect(entry.outcome, JournalOutcome.success);
  });
}
