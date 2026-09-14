import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';

import '../../../core/gateway/frank_gateway.dart';
import '../../../core/models/connection_models.dart';
import '../../../core/transport/frank_transport.dart';

class FrankConnectionState {
  const FrankConnectionState({
    this.status = const FrankConnectionStatus.initial(),
    this.capabilities,
    this.error,
  });

  final FrankConnectionStatus status;
  final FrankServerCapabilities? capabilities;
  final String? error;

  bool get canMutate => status.canMutate;
  bool get isFixture => status.detail == 'Demo data';

  FrankConnectionState copyWith({
    FrankConnectionStatus? status,
    Object? capabilities = _unset,
    Object? error = _unset,
  }) => FrankConnectionState(
    status: status ?? this.status,
    capabilities: identical(capabilities, _unset)
        ? this.capabilities
        : capabilities as FrankServerCapabilities?,
    error: identical(error, _unset) ? this.error : error as String?,
  );

  static const _unset = Object();
}

sealed class FrankConnectionEvent {
  const FrankConnectionEvent();
}

final class FrankConnectionStarted extends FrankConnectionEvent {
  const FrankConnectionStarted();
}

final class FrankConnectionRetryRequested extends FrankConnectionEvent {
  const FrankConnectionRetryRequested();
}

final class FrankConnectionStatusChanged extends FrankConnectionEvent {
  const FrankConnectionStatusChanged(this.status);

  final FrankConnectionStatus status;
}

class ConnectionBloc
    extends Bloc<FrankConnectionEvent, FrankConnectionState> {
  ConnectionBloc({required FrankGateway gateway})
    : _gateway = gateway,
      super(FrankConnectionState(status: gateway.connectionStatus)) {
    on<FrankConnectionStarted>(_start);
    on<FrankConnectionRetryRequested>(_retry);
    on<FrankConnectionStatusChanged>(
      (event, emit) => emit(state.copyWith(status: event.status, error: null)),
    );
    _statusSubscription = gateway.watchConnectionStatus().listen(
      (status) => add(FrankConnectionStatusChanged(status)),
    );
  }

  final FrankGateway _gateway;
  StreamSubscription<FrankConnectionStatus>? _statusSubscription;
  int _generation = 0;

  Future<void> _start(
    FrankConnectionStarted event,
    Emitter<FrankConnectionState> emit,
  ) => _check(emit, refresh: false);

  Future<void> _retry(
    FrankConnectionRetryRequested event,
    Emitter<FrankConnectionState> emit,
  ) => _check(emit, refresh: true);

  Future<void> _check(
    Emitter<FrankConnectionState> emit, {
    required bool refresh,
  }) async {
    final generation = ++_generation;
    final initial = _gateway.isFixture
        ? _gateway.connectionStatus
        : const FrankConnectionStatus.initial();
    emit(state.copyWith(status: initial, error: null));
    try {
      final capabilities = await _gateway.preflightCapabilities(
        refresh: refresh,
      );
      if (isClosed || generation != _generation) return;
      final next = capabilities?.statusFor(FrankApiVersion.current) ??
          _gateway.connectionStatus;
      emit(
        state.copyWith(
          status: next,
          capabilities: capabilities,
          error: next.phase == FrankConnectionPhase.incompatible
              ? next.detail
              : null,
        ),
      );
    } on Object catch (error) {
      if (isClosed || generation != _generation) return;
      final current = _gateway.connectionStatus;
      final status = current.phase == FrankConnectionPhase.incompatible
          ? current
          : current.copyWith(
              phase: FrankConnectionPhase.offline,
              detail: error.toString(),
            );
      emit(state.copyWith(status: status, error: error.toString()));
    }
  }

  @override
  Future<void> close() async {
    await _statusSubscription?.cancel();
    return super.close();
  }
}
