import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Shell-wide keyboard contracts live in one place so platform modifiers do
/// not drift between the sidebar, search, and settings surfaces.
abstract final class ShellShortcutRegistry {
  static const toggleSidebarMac = SingleActivator(
    LogicalKeyboardKey.keyB,
    meta: true,
  );
  static const toggleSidebarControl = SingleActivator(
    LogicalKeyboardKey.keyB,
    control: true,
  );
  static const openSearchMac = SingleActivator(
    LogicalKeyboardKey.keyK,
    meta: true,
  );
  static const openSearchControl = SingleActivator(
    LogicalKeyboardKey.keyK,
    control: true,
  );
}
