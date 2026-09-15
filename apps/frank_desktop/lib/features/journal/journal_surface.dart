import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:forui/forui.dart';

import '../../app/icons.dart';
import '../../app/layout/office_surface_frame.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/journal_models.dart';

/// Owner-only operational timeline. The daemon owns filtering and pagination;
/// this surface only keeps the current cursor and presents the sanitized
/// projection in a readable, offline-safe state.
class JournalSurface extends StatefulWidget {
  const JournalSurface({super.key});

  @override
  State<JournalSurface> createState() => _JournalSurfaceState();
}

class _JournalSurfaceState extends State<JournalSurface> {
  Future<JournalPage>? _future;
  final List<JournalEntry> _entries = [];
  int? _nextBeforeSequence;
  JournalEntryKind? _kind;
  JournalOutcome? _outcome;
  final _projectController = TextEditingController();
  final _missionController = TextEditingController();
  final _taskController = TextEditingController();
  final _agentController = TextEditingController();
  Object? _error;
  bool _loadingMore = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_future == null) _startInitialPage();
  }

  Future<JournalPage> _requestPage({int? beforeSequence}) =>
      context.read<FrankGateway>().loadJournal(
        beforeSequence: beforeSequence,
        limit: 50,
        projectId: _nullableQuery(_projectController.text),
        missionId: _nullableQuery(_missionController.text),
        taskId: _nullableQuery(_taskController.text),
        agentId: _nullableQuery(_agentController.text),
        kind: _kind,
        outcome: _outcome,
      );

  String? _nullableQuery(String value) {
    final query = value.trim();
    return query.isEmpty ? null : query;
  }

  @override
  void dispose() {
    _projectController.dispose();
    _missionController.dispose();
    _taskController.dispose();
    _agentController.dispose();
    super.dispose();
  }

  void _setFilter({JournalEntryKind? kind, JournalOutcome? outcome}) {
    setState(() {
      _kind = kind;
      _outcome = outcome;
      _entries.clear();
      _nextBeforeSequence = null;
      _error = null;
      _future = null;
    });
    _startInitialPage();
  }

  void _startInitialPage() {
    final future = _requestPage();
    _future = future;
    future.then(
      (page) {
        if (!mounted || !identical(_future, future)) return;
        setState(() {
          _entries
            ..clear()
            ..addAll(page.entries);
          _nextBeforeSequence = page.nextBeforeSequence;
          _error = null;
        });
      },
      onError: (Object error, StackTrace stackTrace) {
        if (!mounted || !identical(_future, future)) return;
        setState(() => _error = error);
      },
    );
  }

  Future<void> _loadMore() async {
    final cursor = _nextBeforeSequence;
    if (cursor == null || _loadingMore) return;
    setState(() => _loadingMore = true);
    try {
      final page = await _requestPage(beforeSequence: cursor);
      if (!mounted) return;
      setState(() {
        _entries.addAll(page.entries);
        _nextBeforeSequence = page.nextBeforeSequence;
        _loadingMore = false;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _loadingMore = false;
        _error = error;
      });
    }
  }

  Widget _filterButton({
    required String label,
    required bool selected,
    required VoidCallback onPress,
  }) => FButton(
    key: ValueKey('journal-filter-$label'),
    onPress: onPress,
    size: FButtonSizeVariant.sm,
    variant: selected ? FButtonVariant.primary : FButtonVariant.outline,
    child: Text(label),
  );

  Widget _scopeField({
    required String label,
    required TextEditingController controller,
  }) {
    final metrics = OfficeLayoutMetricsScope.maybeOf(context);
    return SizedBox(
      // Keep two fields per row usable in the compact 560px shell at 150%
      // text scale. The wide layout retains the more comfortable field size.
      width: metrics?.isCompact == true ? 176 : 220,
      child: FTextField(
        key: ValueKey('journal-scope-$label'),
        control: FTextFieldControl.managed(controller: controller),
        hint: label,
      ),
    );
  }

  Widget _filters() => Wrap(
    spacing: 8,
    runSpacing: 8,
    crossAxisAlignment: WrapCrossAlignment.center,
    children: [
      const Icon(FrankIcons.filter, size: FrankUiTokens.iconSize),
      _filterButton(
        label: 'All',
        selected: _kind == null && _outcome == null,
        onPress: () => _setFilter(),
      ),
      for (final kind in const [
        JournalEntryKind.task,
        JournalEntryKind.agentLifecycle,
        JournalEntryKind.approval,
        JournalEntryKind.check,
        JournalEntryKind.toolchain,
      ])
        _filterButton(
          label: kind.label,
          selected: _kind == kind && _outcome == null,
          onPress: () => _setFilter(kind: kind),
        ),
      for (final outcome in JournalOutcome.values)
        _filterButton(
          label: outcome.label,
          selected: _outcome == outcome && _kind == null,
          onPress: () => _setFilter(outcome: outcome),
        ),
    ],
  );

  Widget _scopeFilters() => Wrap(
    spacing: 8,
    runSpacing: 8,
    crossAxisAlignment: WrapCrossAlignment.center,
    children: [
      _scopeField(label: 'Project ID', controller: _projectController),
      _scopeField(label: 'Mission ID', controller: _missionController),
      _scopeField(label: 'Task ID', controller: _taskController),
      _scopeField(label: 'Agent ID', controller: _agentController),
      Builder(
        builder: (context) {
          final compact =
              OfficeLayoutMetricsScope.maybeOf(context)?.isCompact == true;
          return FButton(
            key: const ValueKey('journal-apply-scope-filters'),
            onPress: () {
              setState(() {
                _entries.clear();
                _nextBeforeSequence = null;
                _error = null;
                _future = null;
              });
              _startInitialPage();
            },
            size: FButtonSizeVariant.sm,
            variant: FButtonVariant.outline,
            prefix: compact
                ? null
                : const Icon(FrankIcons.refresh, size: 16),
            child: Text(compact ? 'Apply' : 'Apply scope'),
          );
        },
      ),
    ],
  );

  @override
  Widget build(BuildContext context) {
    return OfficeSurfaceFrame.page(
      key: const ValueKey('journal-page-frame'),
      scrollKey: const ValueKey('settings-journal-scroll'),
      fullWidth: true,
      header: const OfficePageHeader(
        title: 'Journal',
        description:
            'Review operational events globally or by agent and project.',
      ),
      slivers: [
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.only(top: 18, bottom: 14),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _filters(),
                const SizedBox(height: 10),
                _scopeFilters(),
              ],
            ),
          ),
        ),
        SliverToBoxAdapter(
          child: FutureBuilder<JournalPage>(
            future: _future,
            builder: (context, snapshot) {
              if (snapshot.connectionState == ConnectionState.waiting &&
                  _entries.isEmpty) {
                return const _JournalMessage(
                  icon: FrankIcons.clock,
                  title: 'Loading journal',
                  message: 'Reading the owner-only event projection…',
                );
              }
              if (snapshot.hasError && _entries.isEmpty) {
                return _JournalMessage(
                  icon: FrankIcons.cloudOffOutlined,
                  title: 'Journal unavailable',
                  message: frankFriendlyError(
                    snapshot.error,
                    fallback: 'The server could not load the journal.',
                  ),
                  action: FButton(
                    onPress: () {
                      setState(() {
                        _error = null;
                        _entries.clear();
                        _nextBeforeSequence = null;
                        _future = null;
                      });
                      _startInitialPage();
                    },
                    size: FButtonSizeVariant.sm,
                    variant: FButtonVariant.outline,
                    prefix: const Icon(FrankIcons.refresh, size: 16),
                    child: const Text('Retry'),
                  ),
                );
              }
              if (_error != null && _entries.isEmpty) {
                return _JournalMessage(
                  icon: FrankIcons.errorOutline,
                  title: 'Journal unavailable',
                  message: frankFriendlyError(_error),
                );
              }
              if (_entries.isEmpty) {
                return const _JournalMessage(
                  icon: FrankIcons.activity,
                  title: 'No events yet',
                  message:
                      'Frank keeps operational history here so handoffs and decisions can be reviewed later.',
                );
              }
              return Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final entry in _entries) _JournalEntryCard(entry),
                  if (_nextBeforeSequence != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: FButton(
                          onPress: _loadingMore ? null : _loadMore,
                          variant: FButtonVariant.outline,
                          prefix: const Icon(
                            FrankIcons.historyToggleOffOutlined,
                            size: 16,
                          ),
                          child: Text(
                            _loadingMore ? 'Loading…' : 'Load older events',
                          ),
                        ),
                      ),
                    ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}

class _JournalMessage extends StatelessWidget {
  const _JournalMessage({
    required this.icon,
    required this.title,
    required this.message,
    this.action,
  });

  final IconData icon;
  final String title;
  final String message;
  final Widget? action;

  @override
  Widget build(BuildContext context) => FrankPanel(
    child: FrankEmptyState(
      title: title,
      description: message,
      icon: icon,
      action: action,
    ),
  );
}

class _JournalEntryCard extends StatelessWidget {
  const _JournalEntryCard(this.entry);

  final JournalEntry entry;

  FrankStatusTone get _tone => switch (entry.outcome) {
    JournalOutcome.info => FrankStatusTone.neutral,
    JournalOutcome.pending => FrankStatusTone.attention,
    JournalOutcome.success => FrankStatusTone.success,
    JournalOutcome.failure => FrankStatusTone.failure,
    JournalOutcome.blocked => FrankStatusTone.attention,
  };

  @override
  Widget build(BuildContext context) {
    final refs = [
      if (entry.projectId != null) 'project ${entry.projectId}',
      if (entry.taskId != null) 'task ${entry.taskId}',
      if (entry.agentId != null) 'agent ${entry.agentId}',
      if (entry.checkRunId != null) 'check ${entry.checkRunId}',
    ];
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: FrankPanel(
        key: ValueKey('journal-entry-${entry.sequence}'),
        child: Semantics(
          container: true,
          label: '${entry.kind.label} ${entry.outcome.label}: ${entry.summary}',
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(FrankIcons.timelineOutlined, color: _tone.color, size: 18),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      spacing: 8,
                      runSpacing: 6,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        Text(
                          entry.kind.label,
                          style: const TextStyle(
                            color: FrankColors.ink,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        FrankStatusBadge(
                          label: entry.outcome.label,
                          tone: _tone,
                        ),
                        Text(
                          '#${entry.sequence} · ${entry.occurredAt.toLocal().toIso8601String().replaceFirst('T', ' ').split('.').first}',
                          style: const TextStyle(
                            color: FrankColors.muted,
                            fontSize: 11,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 6),
                    Text(entry.summary),
                    if (refs.isNotEmpty) ...[
                      const SizedBox(height: 6),
                      Text(
                        refs.join(' · '),
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontSize: 11,
                        ),
                      ),
                    ],
                    if (entry.detail != null) ...[
                      const SizedBox(height: 6),
                      Text(
                        entry.detail.toString(),
                        maxLines: 4,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontSize: 11,
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
