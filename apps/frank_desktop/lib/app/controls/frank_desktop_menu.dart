import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../icons.dart';
import '../theme.dart';

part 'frank_desktop_menu_widgets.dart';
part 'frank_desktop_menu_select.dart';

enum FrankDesktopMenuKind { context, select, account }

class FrankMenuItem {
  const FrankMenuItem({
    required this.label,
    this.key,
    this.icon,
    this.onPressed,
    this.shortcut,
    this.badge,
    this.checked = false,
    this.enabled = true,
    this.hasSubmenu = false,
    this.semanticsLabel,
    this.destructive = false,
  });

  final String label;
  final Key? key;
  final IconData? icon;
  final VoidCallback? onPressed;
  final String? shortcut;
  final String? badge;
  final bool checked;
  final bool enabled;
  final bool hasSubmenu;
  final String? semanticsLabel;
  final bool destructive;

  bool get isEnabled => enabled && onPressed != null;
}

class FrankMenuGroup {
  const FrankMenuGroup(this.items);

  final List<FrankMenuItem> items;
}

/// Imperative handle used when a menu is anchored to a row but opened by a
/// separate ellipsis button inside that row.
class FrankDesktopMenuController {
  VoidCallback? _openCallback;
  ValueChanged<Offset>? _openAtCallback;
  VoidCallback? _closeCallback;
  VoidCallback? _toggleCallback;

  void open() => _openCallback?.call();

  /// Opens a context menu at a global pointer position.
  void openAt(Offset position) => _openAtCallback?.call(position);

  void close() => _closeCallback?.call();

  void toggle() => _toggleCallback?.call();
}

/// Coordinates transient menus and tooltips throughout the desktop shell.
///
/// Menu surfaces are inserted into the nearest overlay, so this scope keeps a
/// small registry of active surfaces across the sidebar, workspace, and
/// composer. A newly opened surface claims the registry slot and dismisses
/// the previous owner without stealing focus from the new trigger.
