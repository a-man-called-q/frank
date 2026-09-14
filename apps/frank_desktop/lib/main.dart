import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:macos_window_utils/macos_window_utils.dart';

import 'app/frank_app.dart';
import 'core/auth/auth_repository.dart';
import 'features/shell/sidebar_effect.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final sidebarEffectBuilder = await _initializeMacOSWindowEffects();
  final configuration = FrankClientConfiguration.fromEnvironment;
  final authRepository = _createAuthRepository(configuration);
  runApp(
    FrankApp(
      authRepository: authRepository,
      sidebarEffectBuilder: sidebarEffectBuilder,
    ),
  );
}

AuthRepository _createAuthRepository(FrankClientConfiguration configuration) {
  final validationError = configuration.validationError;
  if (validationError != null) {
    return UnconfiguredAuthRepository(configuration);
  }
  try {
    return HttpAuthRepository(configuration);
  } on Object catch (error) {
    return UnconfiguredAuthRepository(
      configuration,
      configurationMessage:
          'The configured Frank CA certificate could not be loaded: $error',
    );
  }
}

Future<SidebarEffectBuilder?> _initializeMacOSWindowEffects() async {
  if (defaultTargetPlatform != TargetPlatform.macOS) return null;

  try {
    await WindowManipulator.initialize();
    await WindowManipulator.setWindowBackgroundColorToClear();
    await WindowManipulator.makeTitlebarTransparent();
    await WindowManipulator.enableFullSizeContentView();
    await WindowManipulator.setMaterial(NSVisualEffectViewMaterial.sidebar);
    await WindowManipulator.overrideMacOSBrightness(dark: true);

    return (Widget child) => child;
  } on Object catch (error, stackTrace) {
    debugPrint('macOS sidebar visual effect unavailable: $error');
    debugPrint('$stackTrace');
    return null;
  }
}
