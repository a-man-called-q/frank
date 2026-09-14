import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import 'package:frank_desktop/app/theme.dart';

/// Shared host for widget and golden tests.
///
/// Keep tests on the same WidgetsApp + ForUI stack as the desktop entrypoint;
/// this catches accidental Flutter Material dependencies before they reach the
/// application shell.
class FrankTestApp extends StatelessWidget {
  const FrankTestApp({
    this.child,
    this.home,
    this.theme,
    this.debugShowCheckedModeBanner = false,
    this.builder,
    this.locale,
    this.supportedLocales,
    this.localizationsDelegates,
    this.withToaster = true,
    super.key,
  }) : assert(child != null || home != null, 'Provide child or home');

  final Widget? child;
  final Widget? home;
  final FThemeData? theme;
  final bool debugShowCheckedModeBanner;
  final TransitionBuilder? builder;
  final Locale? locale;
  final Iterable<Locale>? supportedLocales;
  final Iterable<LocalizationsDelegate<Object>>? localizationsDelegates;
  final bool withToaster;

  @override
  Widget build(BuildContext context) => WidgetsApp(
    color: FrankColors.canvas,
    debugShowCheckedModeBanner: debugShowCheckedModeBanner,
    pageRouteBuilder: <T>(settings, builder) => PageRouteBuilder<T>(
      settings: settings,
      pageBuilder: (context, animation, secondaryAnimation) => builder(context),
    ),
    locale: locale ?? const Locale('en', 'US'),
    supportedLocales: supportedLocales ?? FLocalizations.supportedLocales,
    localizationsDelegates:
        localizationsDelegates ?? FLocalizations.localizationsDelegates,
    builder: (context, child) {
      final themed = FTheme(
        data: theme ?? buildFrankTheme(),
        platform: FPlatformVariant.macOS,
        child: withToaster
            ? FToaster(
                child: FTooltipGroup(child: child ?? const SizedBox.shrink()),
              )
            : FTooltipGroup(child: child ?? const SizedBox.shrink()),
      );
      return builder?.call(context, themed) ?? themed;
    },
    home: home ?? child!,
  );
}
