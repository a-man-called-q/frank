import 'dart:async';

typedef SnapshotLoader = Future<Map<String, dynamic>> Function();

typedef SnapshotCommandSender =
    Future<Map<String, dynamic>> Function({
      required String type,
      required Map<String, dynamic> data,
      int? expectedRevision,
    });

typedef SnapshotEventStreamFactory = Stream<int> Function({int after});

/// The single raw snapshot boundary used by remote desktop projections.
///
/// A load is single-flight, failed loads are never cached, and event cursors
/// invalidate the last projection without inventing a second projection
/// cache. Logout clears both in-flight ownership and the visible snapshot so
/// a late response from the old session cannot repopulate the UI.
class SnapshotStore {
  SnapshotStore({
    required this.loader,
    this.commandSender,
    this.eventStreamFactory,
  });

  final SnapshotLoader loader;
  final SnapshotCommandSender? commandSender;
  final SnapshotEventStreamFactory? eventStreamFactory;

  Map<String, dynamic>? _snapshot;
  Future<Map<String, dynamic>>? _inFlight;
  StreamSubscription<int>? _events;
  final StreamController<void> _invalidations =
      StreamController<void>.broadcast(sync: true);
  int _generation = 0;
  int _revision = 0;
  int _eventSeq = 0;
  bool _loggedOut = false;
  bool _stale = false;
  bool _disposed = false;

  Map<String, dynamic>? get snapshot =>
      _snapshot == null ? null : Map<String, dynamic>.unmodifiable(_snapshot!);

  int get revision => _revision;

  int get eventSeq => _eventSeq;

  bool get hasSnapshot => _snapshot != null;

  /// True after an event invalidation until the latest snapshot is adopted.
  /// Mutations must use the refreshed revision rather than the revision that
  /// was current before the event arrived.
  bool get isStale => _stale;

  /// The one invalidation stream for this authenticated snapshot. Consumers
  /// must subscribe here rather than opening a second event socket; this
  /// keeps taskboard, project, and organization projections on one cursor.
  Stream<void> get invalidations => _invalidations.stream;

  Stream<void> watchInvalidations() {
    _ensureEventSubscription();
    return _invalidations.stream;
  }

  Future<Map<String, dynamic>> load({bool force = false}) {
    if (_disposed) {
      return Future<Map<String, dynamic>>.error(
        StateError('Snapshot store has been disposed.'),
      );
    }
    if (_loggedOut) _loggedOut = false;
    _ensureEventSubscription();
    if (!force && _snapshot != null) {
      return Future<Map<String, dynamic>>.value(
        Map<String, dynamic>.from(_snapshot!),
      );
    }
    final pending = _inFlight;
    if (pending != null) return pending;

    final generation = _generation;
    final future = loader().then((value) {
      final normalized = Map<String, dynamic>.from(value);
      // A logout or event invalidation that happened while the request was in
      // flight wins over the late response.
      if (!_disposed && !_loggedOut && generation == _generation) {
        _snapshot = normalized;
        _stale = false;
        final valueRevision = int.tryParse(
          normalized['revision']?.toString() ?? '',
        );
        if (valueRevision != null) _revision = valueRevision;
        final valueEventSeq = int.tryParse(
          normalized['event_seq']?.toString() ?? '',
        );
        if (valueEventSeq != null && valueEventSeq > _eventSeq) {
          _eventSeq = valueEventSeq;
        }
      }
      return normalized;
    });
    _inFlight = future;
    future.then<void>(
      (_) {
        if (identical(_inFlight, future)) _inFlight = null;
      },
      onError: (_, _) {
        if (identical(_inFlight, future)) _inFlight = null;
      },
    );
    return future;
  }

  Future<Map<String, dynamic>> refresh() => load(force: true);

  /// Invalidate the raw snapshot after a command or an event notice.
  void invalidate({int? revision, int? eventSeq}) {
    _generation++;
    _snapshot = null;
    _stale = true;
    if (revision != null) _revision = revision;
    if (eventSeq != null && eventSeq > _eventSeq) _eventSeq = eventSeq;
    if (!_disposed) _invalidations.add(null);
  }

  /// Adopt a conflict payload without issuing a second HTTP request. This is
  /// used by append-only create commands before their single safe retry.
  void adoptLatest(Map<String, dynamic> value) {
    if (_disposed || _loggedOut) return;
    final normalized = Map<String, dynamic>.from(value);
    _generation++;
    _snapshot = normalized;
    _stale = false;
    final valueRevision = int.tryParse(normalized['revision']?.toString() ?? '');
    if (valueRevision != null) _revision = valueRevision;
    final valueEventSeq = int.tryParse(normalized['event_seq']?.toString() ?? '');
    if (valueEventSeq != null && valueEventSeq > _eventSeq) _eventSeq = valueEventSeq;
    if (!_disposed) _invalidations.add(null);
  }

  Future<Map<String, dynamic>> command({
    required String type,
    Map<String, dynamic> data = const <String, dynamic>{},
    int? expectedRevision,
  }) async {
    final sender = commandSender;
    if (sender == null) {
      throw StateError('Snapshot commands are unavailable on this store.');
    }
    final response = await sender(
      type: type,
      data: Map<String, dynamic>.from(data),
      expectedRevision: expectedRevision,
    );
    final error = response['error'];
    if (error is Map) {
      throw SnapshotCommandConflict.fromResponse(
        Map<String, dynamic>.from(error),
        response,
        expectedRevision: expectedRevision,
      );
    }
    final eventSeq = int.tryParse(response['event_seq']?.toString() ?? '');
    invalidate(eventSeq: eventSeq);
    return response;
  }

  /// Drop all data and stop reconnecting event listeners for the old session.
  void logout() {
    _loggedOut = true;
    _generation++;
    _snapshot = null;
    _inFlight = null;
    _events?.cancel();
    _events = null;
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _generation++;
    _snapshot = null;
    _inFlight = null;
    _events?.cancel();
    _events = null;
    _invalidations.close();
  }

  void _ensureEventSubscription() {
    if (_disposed || _events != null || eventStreamFactory == null) return;
    _events = eventStreamFactory!(after: _eventSeq).listen(
      (sequence) {
        if (_disposed || sequence <= _eventSeq) return;
        _eventSeq = sequence;
        invalidate(eventSeq: sequence);
      },
      onDone: () => _events = null,
      onError: (_, _) => _events = null,
      cancelOnError: false,
    );
  }
}

class SnapshotCommandConflict implements Exception {
  SnapshotCommandConflict({
    required this.code,
    required this.message,
    required this.response,
    this.expectedRevision,
    this.actualRevision,
    this.latestSnapshot,
  });

  final String code;
  final String message;
  final int? expectedRevision;
  final int? actualRevision;
  final Map<String, dynamic>? latestSnapshot;
  final Map<String, dynamic> response;

  factory SnapshotCommandConflict.fromResponse(
    Map<String, dynamic> error,
    Map<String, dynamic> response, {
    int? expectedRevision,
  }) {
    final latest = error['latest_snapshot'] ?? response['latest_snapshot'];
    return SnapshotCommandConflict(
      code: (error['code'] ?? 'command-failed').toString(),
      message: (error['message'] ?? 'Command failed').toString(),
      expectedRevision: expectedRevision,
      actualRevision: int.tryParse(
        (error['actual_revision'] ?? error['actual'] ?? '').toString(),
      ),
      latestSnapshot: latest is Map ? Map<String, dynamic>.from(latest) : null,
      response: Map<String, dynamic>.from(response),
    );
  }

  @override
  String toString() => message;
}
