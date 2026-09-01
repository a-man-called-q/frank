import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/features/shell/window_chrome.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('toggleZoom sends the native window zoom command', () async {
    const channel = MethodChannel('dev.frank.frankDesktop/windowChrome');
    String? receivedMethod;
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(channel, (call) async {
      receivedMethod = call.method;
      return null;
    });
    addTearDown(() => messenger.setMockMethodCallHandler(channel, null));

    final state = WindowChromeState();
    addTearDown(state.dispose);

    await state.toggleZoom();

    expect(receivedMethod, 'toggleZoom');
  });
}
