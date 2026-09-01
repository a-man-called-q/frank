import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

import '../core/fixtures/fixture_workspace.dart';
import '../core/gateway/frank_gateway.dart';
import '../features/shell/office_shell.dart';
import 'theme.dart';

class FrankApp extends StatelessWidget {
  const FrankApp({this.gateway, super.key});

  final FrankGateway? gateway;

  @override
  Widget build(BuildContext context) {
    final lightTheme = buildFrankTheme(Brightness.light);
    final darkTheme = buildFrankTheme(Brightness.dark);

    return MaterialApp(
      title: 'Frank',
      debugShowCheckedModeBanner: false,
      theme: lightTheme,
      darkTheme: darkTheme,
      themeMode: ThemeMode.dark,
      builder: (context, child) {
        final brightness = Theme.of(context).brightness;
        final foruiTheme = brightness == Brightness.dark
            ? FTheme.neutral.dark.desktop
            : FTheme.neutral.light.desktop;
        return FTheme(
          data: foruiTheme,
          child: FTooltipGroup(child: child ?? const SizedBox.shrink()),
        );
      },
      home: OfficeShell(gateway: gateway ?? FixtureFrankGateway()),
    );
  }
}
