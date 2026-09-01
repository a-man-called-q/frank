import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frank_desktop/app/theme.dart';

void main() {
  test('Material interactions do not animate a pressed-state overlay', () {
    final theme = buildFrankTheme(Brightness.dark);
    const pressed = {WidgetState.pressed};

    expect(theme.splashFactory, same(NoSplash.splashFactory));
    expect(theme.splashColor, Colors.transparent);
    expect(theme.highlightColor, Colors.transparent);
    expect(
      theme.iconButtonTheme.style?.overlayColor?.resolve(pressed),
      Colors.transparent,
    );
    expect(
      theme.textButtonTheme.style?.overlayColor?.resolve(pressed),
      Colors.transparent,
    );
    expect(theme.filledButtonTheme.style?.animationDuration, Duration.zero);
  });
}
