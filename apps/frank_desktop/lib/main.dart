import 'dart:io';

import 'package:flutter/material.dart';
import 'package:macos_window_utils/macos_window_utils.dart';
import 'package:macos_window_utils/widgets/transparent_macos_sidebar.dart';

import 'app/frank_app.dart';
import 'features/shell/sidebar_effect.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final sidebarEffectBuilder = await _initializeMacOSWindowEffects();
  runApp(FrankApp(sidebarEffectBuilder: sidebarEffectBuilder));
}

Future<SidebarEffectBuilder?> _initializeMacOSWindowEffects() async {
  if (!Platform.isMacOS) return null;

  try {
    await WindowManipulator.initialize();
    await WindowManipulator.setWindowBackgroundColorToClear();
    await WindowManipulator.makeTitlebarTransparent();
    await WindowManipulator.enableFullSizeContentView();
    await WindowManipulator.setMaterial(
      NSVisualEffectViewMaterial.windowBackground,
    );
    await WindowManipulator.overrideMacOSBrightness(dark: true);

    return (child) => TransparentMacOSSidebar(
      material: NSVisualEffectViewMaterial.sidebar,
      state: NSVisualEffectViewState.followsWindowActiveState,
      alphaValue: 1.0,
      child: child,
    );
  } on Object catch (error, stackTrace) {
    debugPrint('macOS sidebar visual effect unavailable: $error');
    debugPrint('$stackTrace');
    return null;
  }
}
