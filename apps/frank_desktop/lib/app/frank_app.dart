import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

import '../core/fixtures/fixture_workspace.dart';
import '../core/auth/auth_repository.dart';
import '../core/gateway/frank_gateway.dart';
import '../core/gateway/http_frank_gateway.dart';
import '../features/login/login_gate.dart';
import '../features/shell/office_shell.dart';
import '../features/shell/sidebar_effect.dart';
import 'theme.dart';

class FrankApp extends StatelessWidget {
  const FrankApp({
    this.gateway,
    this.authRepository,
    this.sidebarEffectBuilder,
    this.showLogin = true,
    super.key,
  });

  final FrankGateway? gateway;
  final AuthRepository? authRepository;
  final SidebarEffectBuilder? sidebarEffectBuilder;
  final bool showLogin;

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
      home: Builder(
        builder: (context) {
          final transport = authRepository?.transport;
          final resolvedGateway =
              gateway ??
              (transport != null
                  ? HttpFrankGateway(transport)
                  : FixtureFrankGateway());
          if (showLogin) {
            return LoginGate(
              gateway: resolvedGateway,
              authRepository: authRepository,
              showDemoBanner:
                  authRepository == null ||
                  authRepository is DemoAuthRepository,
              sidebarEffectBuilder: sidebarEffectBuilder,
            );
          }
          return OfficeShell(
            gateway: resolvedGateway,
            authRepository: authRepository,
            sidebarEffectBuilder: sidebarEffectBuilder,
          );
        },
      ),
    );
  }
}
