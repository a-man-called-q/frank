import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// UI-only state for the native macOS window chrome.
///
/// The channel is intentionally best-effort. Flutter tests and non-macOS
/// hosts do not provide the native endpoint, so the shell simply stays in its
/// normal windowed layout when the channel is unavailable.
class WindowChromeState extends ChangeNotifier {
  static const _channelName = 'dev.frank.frankDesktop/windowChrome';
  static const _eventChannelName = 'dev.frank.frankDesktop/windowChromeEvents';
  static const _eventName = 'fullscreenChanged';

  final MethodChannel _methods = const MethodChannel(_channelName);
  final EventChannel _events = const EventChannel(_eventChannelName);
  StreamSubscription<Object?>? _subscription;
  bool _isFullscreen = false;

  bool get isFullscreen => _isFullscreen;

  Future<void> toggleZoom() async {
    try {
      await _methods.invokeMethod<void>('toggleZoom');
    } on MissingPluginException {
      // Expected in widget tests and non-macOS hosts.
    } on PlatformException {
      // A missing or unavailable host channel must not block the workspace.
    }
  }

  Future<void> attach() async {
    try {
      final initial = await _methods.invokeMethod<bool>('isFullscreen');
      if (initial != null) _setFullscreen(initial);
      _subscription = _events.receiveBroadcastStream().listen(
        (event) {
          if (event is Map && event['name'] == _eventName) {
            final value = event['isFullscreen'];
            if (value is bool) _setFullscreen(value);
          }
        },
        onError: (_) {
          // Native chrome is cosmetic; keep the windowed fallback.
        },
      );
    } on MissingPluginException {
      // Expected in widget tests and non-macOS hosts.
    } on PlatformException {
      // A missing or unavailable host channel must not block the workspace.
    }
  }

  void _setFullscreen(bool value) {
    if (_isFullscreen == value) return;
    _isFullscreen = value;
    notifyListeners();
  }

  @override
  void dispose() {
    unawaited(_subscription?.cancel());
    super.dispose();
  }
}
