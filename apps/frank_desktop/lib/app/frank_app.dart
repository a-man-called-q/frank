import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

import '../core/fixtures/fixture_workspace.dart';
import '../core/gateway/frank_gateway.dart';
import '../features/shell/office_shell.dart';
import '../features/shell/sidebar_effect.dart';
import 'theme.dart';

class FrankApp extends StatelessWidget {
  const FrankApp({this.gateway, this.sidebarEffectBuilder, super.key});

  final FrankGateway? gateway;
  final SidebarEffectBuilder? sidebarEffectBuilder;

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
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
      ],
      builder: (context, child) {
        final brightness = Theme.of(context).brightness;
        final foruiTheme = buildFrankForuiTheme(brightness);
        return FTheme(
          data: foruiTheme,
          child: FTooltipGroup(child: child ?? const SizedBox.shrink()),
        );
      },
      home: OfficeShell(
        gateway: gateway ?? FixtureFrankGateway(),
        sidebarEffectBuilder: sidebarEffectBuilder,
      ),
    );
  }
}
