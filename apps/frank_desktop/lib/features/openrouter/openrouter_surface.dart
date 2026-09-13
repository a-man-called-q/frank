import 'package:flutter/material.dart';

import '../../app/controls/frank_desktop_menu.dart';
import '../../app/layout/office_surface_frame.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/openrouter_models.dart';

/// Owner-facing OpenRouter configuration. The widget only receives metadata
/// from [OpenRouterGateway]; the API key is entered directly into the gateway call
/// and is never represented in a widget model or snapshot.
class OpenRouterSurface extends StatefulWidget {
  const OpenRouterSurface({required this.gateway, super.key});

  final OpenRouterGateway gateway;

  @override
  State<OpenRouterSurface> createState() => _OpenRouterSurfaceState();
}

class _OpenRouterSurfaceState extends State<OpenRouterSurface> {
  late Future<OpenRouterConnection> _connection;
  late Future<OpenRouterCatalog> _catalog;
  final TextEditingController _keyController = TextEditingController();
  final TextEditingController _modelSearchController = TextEditingController();
  bool _showKeyInput = false;
  bool _busy = false;
  String? _actionError;
  String? _supervisorModel;
  OpenRouterConnection? _latestConnection;

  @override
  void initState() {
    super.initState();
    _supervisorModel = widget.gateway.cachedSupervisorModel;
    _load();
  }

  @override
  void didUpdateWidget(covariant OpenRouterSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.gateway, widget.gateway)) {
      _supervisorModel = widget.gateway.cachedSupervisorModel;
      _load();
    }
  }

  @override
  void dispose() {
    _keyController.dispose();
    _modelSearchController.dispose();
    super.dispose();
  }

  void _load() {
    _latestConnection = null;
    _connection = widget.gateway.loadOpenRouterConnection();
    _catalog = widget.gateway.loadOpenRouterModels();
  }

  @override
  Widget build(BuildContext context) {
    return OfficeSurfaceFrame.page(
      fullWidth: true,
      scrollKey: const ValueKey('settings-models-scroll'),
      header: OfficePageHeader(
        title: 'Models & OpenRouter',
        description:
            'Connect Frank to OpenRouter, choose tool-capable models, and set the supervisor model.',
        actions: _latestConnection?.configured == true
            ? Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  OutlinedButton.icon(
                    key: const ValueKey('provider-refresh-models'),
                    onPressed: _busy ? null : _refreshCatalog,
                    icon: const Icon(
                      Icons.refresh,
                      size: FrankUiTokens.iconSize,
                    ),
                    label: const Text('Refresh catalog'),
                  ),
                ],
              )
            : null,
      ),
      slivers: [
        SliverToBoxAdapter(
          child: FutureBuilder<OpenRouterConnection>(
            future: _connection,
            builder: (context, snapshot) {
              final connection = snapshot.data;
              if (connection != null &&
                  !identical(_latestConnection, connection)) {
                _latestConnection = connection;
                WidgetsBinding.instance.addPostFrameCallback((_) {
                  if (mounted) setState(() {});
                });
              }
              return Padding(
                padding: const EdgeInsets.only(bottom: 16),
                child: _ConnectionCard(
                  connection: connection,
                  loading: snapshot.connectionState != ConnectionState.done,
                  showKeyInput: _showKeyInput,
                  keyController: _keyController,
                  busy: _busy,
                  actionError: _actionError,
                  onToggleKey: () => setState(() {
                    _showKeyInput = !_showKeyInput;
                    _actionError = null;
                  }),
                  onSave: _saveCredential,
                  onTest: _testConnection,
                  onRemove: _removeCredential,
                ),
              );
            },
          ),
        ),
        SliverToBoxAdapter(
          child: FutureBuilder<OpenRouterCatalog>(
            future: _catalog,
            builder: (context, snapshot) => Padding(
              padding: const EdgeInsets.only(bottom: 16),
              child: _CatalogCard(
                catalog: snapshot.data,
                error: snapshot.error,
                loading: snapshot.connectionState != ConnectionState.done,
                searchController: _modelSearchController,
                supervisorModel: _supervisorModel,
                supervisorSaveBusy: _busy,
                snapshotRevision: widget.gateway.snapshotRevision,
                openRouterConfigured: _latestConnection?.configured == true,
                onSearchChanged: () => setState(() {}),
                onSupervisorChanged: _saveSupervisorModel,
              ),
            ),
          ),
        ),
      ],
    );
  }

  Future<void> _saveCredential() async {
    final key = _keyController.text.trim();
    if (key.isEmpty) {
      setState(() => _actionError = 'Enter an OpenRouter API key first.');
      return;
    }
    await _run(() async {
      final connection = await widget.gateway.saveOpenRouterCredential(key);
      if (!mounted) return;
      setState(() {
        _connection = Future.value(connection);
        _showKeyInput = false;
        _keyController.clear();
      });
      _refreshCatalog();
    });
  }

  Future<void> _testConnection() async {
    await _run(() async {
      final connection = await widget.gateway.testOpenRouterConnection();
      if (!mounted) return;
      setState(() => _connection = Future.value(connection));
    });
  }

  Future<void> _removeCredential() async {
    await _run(() async {
      final connection = await widget.gateway.removeOpenRouterCredential();
      if (!mounted) return;
      setState(() {
        _connection = Future.value(connection);
        _showKeyInput = false;
        _keyController.clear();
      });
      _refreshCatalog();
    });
  }

  Future<void> _refreshCatalog() async {
    if (!mounted) return;
    setState(
      () => _catalog = widget.gateway.loadOpenRouterModels(refresh: true),
    );
  }

  Future<void> _saveSupervisorModel(String? model) async {
    final previous = _supervisorModel;
    setState(() {
      _supervisorModel = model;
      _actionError = null;
    });
    final saved = await _run(
      () => widget.gateway.updateSupervisorModel(
        model: model,
        expectedRevision: widget.gateway.snapshotRevision,
      ),
    );
    if (!saved) {
      if (!mounted) return;
      setState(() => _supervisorModel = previous);
    }
  }

  Future<bool> _run(Future<void> Function() operation) async {
    if (_busy) return false;
    setState(() {
      _busy = true;
      _actionError = null;
    });
    var saved = false;
    try {
      await operation();
      saved = true;
    } catch (error) {
      if (mounted) setState(() => _actionError = _friendlyError(error));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
    return saved;
  }
}

class _ConnectionCard extends StatelessWidget {
  const _ConnectionCard({
    required this.connection,
    required this.loading,
    required this.showKeyInput,
    required this.keyController,
    required this.busy,
    required this.actionError,
    required this.onToggleKey,
    required this.onSave,
    required this.onTest,
    required this.onRemove,
  });

  final OpenRouterConnection? connection;
  final bool loading;
  final bool showKeyInput;
  final TextEditingController keyController;
  final bool busy;
  final String? actionError;
  final VoidCallback onToggleKey;
  final VoidCallback onSave;
  final VoidCallback onTest;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final state = connection?.state;
    final tone = switch (state) {
      OpenRouterConnectionState.connected => FrankStatusTone.success,
      OpenRouterConnectionState.error => FrankStatusTone.failure,
      OpenRouterConnectionState.notConfigured => FrankStatusTone.attention,
      null => FrankStatusTone.neutral,
    };
    final label = loading
        ? 'Checking…'
        : switch (state) {
            OpenRouterConnectionState.connected => 'Connected',
            OpenRouterConnectionState.error => 'Connection error',
            OpenRouterConnectionState.notConfigured => 'Not configured',
            null => 'Unavailable',
          };
    final environmentManaged = connection?.credentialSource == 'environment';
    final canRemove = connection?.configured == true && !environmentManaged;

    return FrankPanel(
      key: const ValueKey('provider-connection-card'),
      raised: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(
                Icons.cloud_outlined,
                color: FrankColors.aubergineAccent,
              ),
              const SizedBox(width: 10),
              const Expanded(
                child: Text(
                  'OpenRouter',
                  style: TextStyle(
                    color: FrankColors.ink,
                    fontSize: 16,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              FrankStatusBadge(label: label, tone: tone),
            ],
          ),
          const SizedBox(height: 10),
          Text(
            environmentManaged
                ? 'Managed by server environment. The key cannot be viewed or removed from Flutter.'
                : connection?.credentialLabel ??
                      'Store the credential on the frankd host; it never enters the event log.',
            style: const TextStyle(color: FrankColors.muted, height: 1.4),
          ),
          if (connection?.diagnostic case final diagnostic?) ...[
            const SizedBox(height: 8),
            Text(
              frankFriendlyError(diagnostic),
              key: const ValueKey('provider-connection-diagnostic'),
              style: const TextStyle(color: FrankColors.failure, height: 1.35),
            ),
          ],
          if (connection?.checkedAt case final checkedAt?) ...[
            const SizedBox(height: 7),
            Text(
              'Last checked ${_formatDate(checkedAt)}',
              style: const TextStyle(color: FrankColors.muted, fontSize: 11),
            ),
          ],
          if (showKeyInput && !environmentManaged) ...[
            const SizedBox(height: 16),
            TextField(
              key: const ValueKey('provider-api-key-field'),
              controller: keyController,
              obscureText: true,
              enabled: !busy,
              autofillHints: const [AutofillHints.password],
              decoration: const InputDecoration(
                labelText: 'OpenRouter API key',
                hintText: 'sk-or-v1-…',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 10),
            FrankPrimaryAction(
              key: const ValueKey('provider-save-credential'),
              label: 'Save credential',
              onPressed: busy ? null : onSave,
              icon: Icons.save,
            ),
          ],
          if (actionError case final error?) ...[
            const SizedBox(height: 10),
            Text(
              error,
              key: const ValueKey('provider-action-error'),
              style: const TextStyle(color: FrankColors.failure),
            ),
          ],
          const SizedBox(height: 14),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              if (!environmentManaged)
                if (connection?.configured == true)
                  OutlinedButton(
                    key: const ValueKey('provider-replace-credential'),
                    onPressed: busy ? null : onToggleKey,
                    child: Text(showKeyInput ? 'Cancel' : 'Replace'),
                  )
                else if (!loading)
                  FrankPrimaryAction(
                    key: const ValueKey('provider-add-api-key'),
                    label: showKeyInput ? 'Cancel' : 'Add API key',
                    icon: Icons.key_outlined,
                    onPressed: busy ? null : onToggleKey,
                  ),
              if (connection?.configured == true)
                OutlinedButton(
                  key: const ValueKey('provider-test-connection'),
                  onPressed: busy ? null : onTest,
                  child: const Text('Test connection'),
                ),
              if (canRemove)
                TextButton(
                  key: const ValueKey('provider-remove-credential'),
                  onPressed: busy ? null : onRemove,
                  child: const Text('Remove'),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _CatalogCard extends StatelessWidget {
  const _CatalogCard({
    required this.catalog,
    required this.error,
    required this.loading,
    required this.searchController,
    required this.supervisorModel,
    required this.supervisorSaveBusy,
    required this.snapshotRevision,
    required this.openRouterConfigured,
    required this.onSearchChanged,
    required this.onSupervisorChanged,
  });

  final OpenRouterCatalog? catalog;
  final Object? error;
  final bool loading;
  final TextEditingController searchController;
  final String? supervisorModel;
  final bool supervisorSaveBusy;
  final int snapshotRevision;
  final bool openRouterConfigured;
  final VoidCallback onSearchChanged;
  final ValueChanged<String?> onSupervisorChanged;

  @override
  Widget build(BuildContext context) {
    final models = catalog?.models ?? const <OpenRouterModel>[];
    final query = searchController.text.trim().toLowerCase();
    final filtered = models
        .where(
          (model) =>
              query.isEmpty ||
              model.name.toLowerCase().contains(query) ||
              model.canonicalSlug.toLowerCase().contains(query),
        )
        .toList();
    final availableSlugs = {for (final model in models) model.canonicalSlug};
    final supervisorValue = availableSlugs.contains(supervisorModel)
        ? supervisorModel
        : null;
    if (!openRouterConfigured) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          FrankPanel(
            key: const ValueKey('provider-supervisor-card'),
            child: const _LockedOpenRouterPreview(
              title: 'Supervisor model',
              message: 'Locked until an OpenRouter API key is added.',
            ),
          ),
          const SizedBox(height: 12),
          FrankPanel(
            key: const ValueKey('provider-model-catalog-card'),
            child: const _LockedOpenRouterPreview(
              title: 'OpenRouter model catalog',
              message: 'Model browsing is locked until the provider is set up.',
            ),
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FrankPanel(
          key: const ValueKey('provider-supervisor-card'),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Supervisor model',
                style: TextStyle(
                  color: FrankColors.ink,
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 5),
              const Text(
                'The supervisor also runs through OpenRouter. Select an exact canonical slug; Frank never substitutes a missing model.',
                style: TextStyle(color: FrankColors.muted, height: 1.4),
              ),
              const SizedBox(height: 14),
              if (loading)
                const LinearProgressIndicator()
              else if (error != null)
                _CatalogState(
                  icon: Icons.cloud_off_outlined,
                  message:
                      'Model catalog unavailable. Configure OpenRouter and try again.',
                  error: true,
                )
              else if (models.isEmpty)
                _CatalogState(
                  icon: Icons.list_alt_outlined,
                  message: 'No tool-capable OpenRouter models are available.',
                )
              else
                FrankDesktopSelectField<String?>(
                  key: const ValueKey('provider-supervisor-picker'),
                  fieldKey: const ValueKey(
                    'provider-supervisor-picker-trigger',
                  ),
                  value: supervisorValue,
                  label: 'Supervisor canonical model slug',
                  hint: 'Choose a model',
                  options: [
                    for (final model in models)
                      FrankDesktopSelectOption<String?>(
                        value: model.canonicalSlug,
                        label: '${model.name} · ${model.canonicalSlug}',
                      ),
                  ],
                  enabled: !supervisorSaveBusy,
                  onChanged: onSupervisorChanged,
                ),
              if (catalog?.stale == true) ...[
                const SizedBox(height: 8),
                const Text(
                  'Showing the last-known-good catalog because OpenRouter could not be reached.',
                  style: TextStyle(
                    color: FrankColors.warningAmber,
                    fontSize: 12,
                  ),
                ),
              ],
              if (supervisorModel != null && supervisorValue == null) ...[
                const SizedBox(height: 8),
                Text(
                  'Configured supervisor model is not in the current catalog: $supervisorModel',
                  style: const TextStyle(
                    color: FrankColors.failure,
                    fontSize: 12,
                  ),
                ),
              ],
              Text(
                'Snapshot revision $snapshotRevision',
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        FrankPanel(
          key: const ValueKey('provider-model-catalog-card'),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  const Expanded(
                    child: Text(
                      'OpenRouter model catalog',
                      style: TextStyle(
                        color: FrankColors.ink,
                        fontSize: 15,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  Text(
                    '${models.length} models',
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 12,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 5),
              const Text(
                'Only models advertising text and tools are returned by frankd.',
                style: TextStyle(color: FrankColors.muted, height: 1.4),
              ),
              const SizedBox(height: 14),
              TextField(
                key: const ValueKey('provider-model-search'),
                controller: searchController,
                onChanged: (_) => onSearchChanged(),
                decoration: const InputDecoration(
                  prefixIcon: Icon(Icons.search),
                  labelText: 'Search models',
                  border: OutlineInputBorder(),
                ),
              ),
              const SizedBox(height: 14),
              if (loading)
                const LinearProgressIndicator()
              else if (error != null)
                _CatalogState(
                  icon: Icons.cloud_off_outlined,
                  message:
                      'Catalog cannot be displayed until the provider is reachable.',
                  error: true,
                )
              else if (models.isEmpty)
                _CatalogState(
                  icon: Icons.list_alt_outlined,
                  message: 'The catalog is empty.',
                )
              else if (filtered.isEmpty)
                const Text(
                  'No models match this search.',
                  style: TextStyle(color: FrankColors.muted),
                )
              else ...[
                for (final model in filtered.take(100))
                  Padding(
                    padding: const EdgeInsets.only(bottom: 8),
                    child: _ModelTile(model: model),
                  ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

class _ModelTile extends StatelessWidget {
  const _ModelTile({required this.model});

  final OpenRouterModel model;

  @override
  Widget build(BuildContext context) {
    return Container(
      key: ValueKey('provider-model-${model.canonicalSlug}'),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: FrankColors.panelRaised,
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        border: Border.all(color: FrankColors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  model.name,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              if (model.supportsTools)
                const _MetadataBadge(
                  label: 'Tools',
                  tone: FrankStatusTone.success,
                ),
              if (model.deprecated) ...[
                const SizedBox(width: 6),
                const _MetadataBadge(
                  label: 'Deprecated',
                  tone: FrankStatusTone.attention,
                ),
              ],
            ],
          ),
          const SizedBox(height: 4),
          SelectableText(
            model.canonicalSlug,
            style: const TextStyle(
              color: FrankColors.aubergineAccent,
              fontSize: 11,
              fontFamily: FrankTypography.monoFontFamily,
            ),
          ),
          const SizedBox(height: 7),
          Wrap(
            spacing: 14,
            runSpacing: 5,
            children: [
              Text(
                'Context ${_contextLabel(model.contextLength)}',
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
              Text(
                model.priceLabel,
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _MetadataBadge extends StatelessWidget {
  const _MetadataBadge({required this.label, required this.tone});

  final String label;
  final FrankStatusTone tone;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 4),
    decoration: BoxDecoration(
      color: tone.softColor,
      borderRadius: BorderRadius.circular(6),
      border: Border.all(color: tone.color.withValues(alpha: .35)),
    ),
    child: Text(
      label,
      style: TextStyle(
        color: tone.color,
        fontSize: 10,
        fontWeight: FontWeight.w600,
      ),
    ),
  );
}

class _LockedOpenRouterPreview extends StatelessWidget {
  const _LockedOpenRouterPreview({required this.title, required this.message});

  final String title;
  final String message;

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        const Icon(
          Icons.lock_outline,
          size: FrankUiTokens.iconSize,
          color: FrankColors.muted,
        ),
        const SizedBox(width: 10),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontSize: 14,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 3),
              Text(
                message,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: FrankUiTokens.metadataTextSize,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(width: 10),
        const FrankStatusBadge(
          label: 'Locked',
          tone: FrankStatusTone.neutral,
          icon: Icons.lock_outline,
          compact: true,
        ),
      ],
    );
  }
}

class _CatalogState extends StatelessWidget {
  const _CatalogState({
    required this.icon,
    required this.message,
    this.error = false,
  });

  final IconData icon;
  final String message;
  final bool error;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Icon(
        icon,
        size: FrankUiTokens.iconSize,
        color: error ? FrankColors.failure : FrankColors.warningAmber,
      ),
      const SizedBox(width: 8),
      Expanded(
        child: Text(message, style: const TextStyle(color: FrankColors.muted)),
      ),
    ],
  );
}

String _friendlyError(Object error) {
  return frankFriendlyError(error, fallback: 'The provider action failed.');
}

String _formatDate(DateTime value) {
  final local = value.toLocal();
  String two(int number) => number.toString().padLeft(2, '0');
  return '${local.year}-${two(local.month)}-${two(local.day)} ${two(local.hour)}:${two(local.minute)}';
}

String _contextLabel(int? value) {
  if (value == null) return '—';
  if (value >= 1000000) return '${(value / 1000000).toStringAsFixed(1)}M';
  if (value >= 1000) return '${(value / 1000).toStringAsFixed(0)}k';
  return value.toString();
}
