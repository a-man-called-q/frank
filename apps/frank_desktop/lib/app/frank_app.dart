import 'package:flutter/widgets.dart';
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
    this.withToaster = true,
    super.key,
  });

  final FrankGateway? gateway;
  final AuthRepository? authRepository;
  final SidebarEffectBuilder? sidebarEffectBuilder;
  final bool showLogin;
  /// Allows widget tests that exercise transient overlays to isolate their
  /// semantics tree; production always leaves the ForUI toaster enabled.
  final bool withToaster;

  @override
  Widget build(BuildContext context) {
    return WidgetsApp(
      title: 'Frank',
      debugShowCheckedModeBanner: false,
      color: FrankColors.canvas,
      pageRouteBuilder: <T>(settings, builder) => PageRouteBuilder<T>(
        settings: settings,
        pageBuilder: (context, animation, secondaryAnimation) =>
            builder(context),
      ),
      locale: const Locale('en', 'US'),
      supportedLocales: FLocalizations.supportedLocales,
      localizationsDelegates: FLocalizations.localizationsDelegates,
      builder: (context, child) {
        return FTheme(
          data: buildFrankTheme(),
          platform: FPlatformVariant.macOS,
          child: withToaster
              ? FToaster(
                  child: FTooltipGroup(
                    child: child ?? const SizedBox.shrink(),
                  ),
                )
              : FTooltipGroup(child: child ?? const SizedBox.shrink()),
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
