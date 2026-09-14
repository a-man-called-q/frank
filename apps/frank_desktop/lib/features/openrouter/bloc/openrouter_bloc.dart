import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/openrouter_models.dart';

enum OpenRouterConnectionPhase { loading, connected, notConfigured, error }

enum OpenRouterCatalogPhase { idle, loading, ready, stale, error }

class OpenRouterState {
  const OpenRouterState({
    this.connectionPhase = OpenRouterConnectionPhase.loading,
    this.catalogPhase = OpenRouterCatalogPhase.idle,
    this.connection,
    this.catalog,
    this.catalogRefreshedAt,
    this.supervisorModel,
    this.actionError,
    this.actionInFlight = false,
    this.started = false,
  });

  static const _unset = Object();

  final OpenRouterConnectionPhase connectionPhase;
  final OpenRouterCatalogPhase catalogPhase;
  final OpenRouterConnection? connection;
  final OpenRouterCatalog? catalog;
  final DateTime? catalogRefreshedAt;
  final String? supervisorModel;
  final Object? actionError;
  final bool actionInFlight;
  final bool started;

  List<OpenRouterModel> get models => catalog?.models ?? const [];

  OpenRouterState copyWith({
    OpenRouterConnectionPhase? connectionPhase,
    OpenRouterCatalogPhase? catalogPhase,
    Object? connection = _unset,
    Object? catalog = _unset,
    Object? catalogRefreshedAt = _unset,
    Object? supervisorModel = _unset,
    Object? actionError = _unset,
    bool? actionInFlight,
    bool? started,
  }) => OpenRouterState(
    connectionPhase: connectionPhase ?? this.connectionPhase,
    catalogPhase: catalogPhase ?? this.catalogPhase,
    connection: identical(connection, _unset)
        ? this.connection
        : connection as OpenRouterConnection?,
    catalog: identical(catalog, _unset)
        ? this.catalog
        : catalog as OpenRouterCatalog?,
    catalogRefreshedAt: identical(catalogRefreshedAt, _unset)
        ? this.catalogRefreshedAt
        : catalogRefreshedAt as DateTime?,
    supervisorModel: identical(supervisorModel, _unset)
        ? this.supervisorModel
        : supervisorModel as String?,
    actionError: identical(actionError, _unset)
        ? this.actionError
        : actionError,
    actionInFlight: actionInFlight ?? this.actionInFlight,
    started: started ?? this.started,
  );
}

sealed class OpenRouterEvent {
  const OpenRouterEvent();
}

final class OpenRouterStarted extends OpenRouterEvent {
  const OpenRouterStarted({this.refresh = false});

  final bool refresh;
}

final class OpenRouterCredentialSaveRequested extends OpenRouterEvent {
  const OpenRouterCredentialSaveRequested(this.apiKey);

  final String apiKey;
}

final class OpenRouterCredentialRemoveRequested extends OpenRouterEvent {
  const OpenRouterCredentialRemoveRequested();
}

final class OpenRouterConnectionTestRequested extends OpenRouterEvent {
  const OpenRouterConnectionTestRequested();
}

final class OpenRouterCatalogRefreshRequested extends OpenRouterEvent {
  const OpenRouterCatalogRefreshRequested();
}

final class OpenRouterSupervisorModelChanged extends OpenRouterEvent {
  const OpenRouterSupervisorModelChanged(this.model);

  final String? model;
}

/// Session-scoped provider state shared by Models & OpenRouter and Team.
///
/// The bloc intentionally owns metadata only. Credentials remain inside the
/// gateway, and a failed refresh never discards the last-known catalog or the
/// supervisor model already projected by frankd.
class OpenRouterBloc extends Bloc<OpenRouterEvent, OpenRouterState> {
  OpenRouterBloc({required OpenRouterGateway gateway})
    : _gateway = gateway,
      super(OpenRouterState(supervisorModel: gateway.cachedSupervisorModel)) {
    on<OpenRouterStarted>(_handleStarted);
    on<OpenRouterCredentialSaveRequested>(_saveCredential);
    on<OpenRouterCredentialRemoveRequested>(_removeCredential);
    on<OpenRouterConnectionTestRequested>(_testConnection);
    on<OpenRouterCatalogRefreshRequested>(_refreshCatalog);
    on<OpenRouterSupervisorModelChanged>(_changeSupervisorModel);
  }

  final OpenRouterGateway _gateway;
  bool _hasStarted = false;
  bool _loading = false;

  Future<void> _handleStarted(
    OpenRouterStarted event,
    Emitter<OpenRouterState> emit,
  ) async {
    if (_loading || (_hasStarted && !event.refresh)) return;
    _hasStarted = true;
    _loading = true;
    emit(
      state.copyWith(
        connectionPhase: OpenRouterConnectionPhase.loading,
        catalogPhase: OpenRouterCatalogPhase.loading,
        actionError: null,
        started: true,
      ),
    );
    try {
      await _loadConnection(emit);
      await _loadCatalog(emit, refresh: event.refresh);
    } finally {
      _loading = false;
    }
  }

  Future<void> _loadConnection(Emitter<OpenRouterState> emit) async {
    try {
      final connection = await _gateway.loadOpenRouterConnection();
      if (isClosed) return;
      emit(
        state.copyWith(
          connection: connection,
          connectionPhase: _connectionPhase(connection),
          catalogRefreshedAt:
              connection.catalogRefreshedAt ?? state.catalogRefreshedAt,
          supervisorModel: _gateway.cachedSupervisorModel,
          actionError: null,
        ),
      );
    } on Object catch (error) {
      if (isClosed) return;
      emit(
        state.copyWith(
          connectionPhase: OpenRouterConnectionPhase.error,
          actionError: error,
        ),
      );
    }
  }

  Future<void> _loadCatalog(
    Emitter<OpenRouterState> emit, {
    required bool refresh,
  }) async {
    if (!isClosed) {
      emit(
        state.copyWith(
          catalogPhase: OpenRouterCatalogPhase.loading,
          actionError: null,
        ),
      );
    }
    try {
      final catalog = await _gateway.loadOpenRouterModels(refresh: refresh);
      if (isClosed) return;
      emit(
        state.copyWith(
          catalog: catalog,
          catalogPhase: catalog.stale
              ? OpenRouterCatalogPhase.stale
              : OpenRouterCatalogPhase.ready,
          catalogRefreshedAt: catalog.refreshedAt,
          actionError: null,
        ),
      );
    } on Object catch (error) {
      if (isClosed) return;
      emit(
        state.copyWith(
          catalogPhase: state.catalog == null
              ? OpenRouterCatalogPhase.error
              : OpenRouterCatalogPhase.stale,
          actionError: error,
        ),
      );
    }
  }

  Future<void> _saveCredential(
    OpenRouterCredentialSaveRequested event,
    Emitter<OpenRouterState> emit,
  ) async {
    final key = event.apiKey.trim();
    if (key.isEmpty) {
      emit(
        state.copyWith(
          actionError: ArgumentError('OpenRouter API key is required.'),
        ),
      );
      return;
    }
    await _runAction(emit, () async {
      final connection = await _gateway.saveOpenRouterCredential(key);
      if (isClosed) return;
      emit(
        state.copyWith(
          connection: connection,
          connectionPhase: _connectionPhase(connection),
          actionError: null,
        ),
      );
      await _loadCatalog(emit, refresh: true);
    });
  }

  Future<void> _removeCredential(
    OpenRouterCredentialRemoveRequested event,
    Emitter<OpenRouterState> emit,
  ) async {
    await _runAction(emit, () async {
      final connection = await _gateway.removeOpenRouterCredential();
      if (isClosed) return;
      emit(
        state.copyWith(
          connection: connection,
          connectionPhase: _connectionPhase(connection),
          actionError: null,
        ),
      );
      await _loadCatalog(emit, refresh: true);
    });
  }

  Future<void> _testConnection(
    OpenRouterConnectionTestRequested event,
    Emitter<OpenRouterState> emit,
  ) async {
    await _runAction(emit, () async {
      final connection = await _gateway.testOpenRouterConnection();
      if (isClosed) return;
      emit(
        state.copyWith(
          connection: connection,
          connectionPhase: _connectionPhase(connection),
          actionError: null,
        ),
      );
      if (connection.configured) await _loadCatalog(emit, refresh: true);
    });
  }

  Future<void> _refreshCatalog(
    OpenRouterCatalogRefreshRequested event,
    Emitter<OpenRouterState> emit,
  ) async {
    await _runAction(emit, () => _loadCatalog(emit, refresh: true));
  }

  Future<void> _changeSupervisorModel(
    OpenRouterSupervisorModelChanged event,
    Emitter<OpenRouterState> emit,
  ) async {
    final previous = state.supervisorModel;
    emit(state.copyWith(supervisorModel: event.model, actionError: null));
    await _runAction(
      emit,
      () async {
        await _gateway.updateSupervisorModel(
          model: event.model,
          expectedRevision: _gateway.snapshotRevision,
        );
        if (!isClosed) {
          emit(state.copyWith(supervisorModel: event.model, actionError: null));
        }
      },
      onError: () {
        if (!isClosed) emit(state.copyWith(supervisorModel: previous));
      },
    );
  }

  Future<void> _runAction(
    Emitter<OpenRouterState> emit,
    Future<void> Function() action, {
    bool setBusy = true,
    void Function()? onError,
  }) async {
    if (state.actionInFlight) return;
    if (setBusy) emit(state.copyWith(actionInFlight: true, actionError: null));
    try {
      await action();
    } on Object catch (error) {
      onError?.call();
      if (!isClosed) emit(state.copyWith(actionError: error));
    } finally {
      if (setBusy && !isClosed) emit(state.copyWith(actionInFlight: false));
    }
  }

  OpenRouterConnectionPhase _connectionPhase(OpenRouterConnection connection) =>
      switch (connection.state) {
        OpenRouterConnectionState.connected =>
          OpenRouterConnectionPhase.connected,
        OpenRouterConnectionState.notConfigured =>
          OpenRouterConnectionPhase.notConfigured,
        OpenRouterConnectionState.error => OpenRouterConnectionPhase.error,
      };
}
