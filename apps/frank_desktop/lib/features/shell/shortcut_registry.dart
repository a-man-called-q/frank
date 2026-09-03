import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// Shell-wide keyboard contracts live in one place so platform modifiers do
/// not drift between the sidebar, search, and Office surfaces.
///
/// The [SingleActivator] constants are the declarative form, usable directly in
/// a [Shortcuts] map. The `matches*` helpers are the imperative form for raw
/// [HardwareKeyboard] handlers, which is what the shell itself uses — it needs
/// to observe keys before any focused widget claims them. Both forms read from
/// the same activators, so a modifier can only be changed in one place.
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

  /// Whether [event] is the "toggle the workspace sidebar" chord.
  ///
  /// Accepts the Cmd and Ctrl spellings so one binding works on macOS and on
  /// the other desktop hosts, and rejects the chord when an extra modifier is
  /// held — `SingleActivator` matches modifiers exactly, so Cmd+Shift+B stays
  /// free for whatever wants to claim it later.
  static bool matchesToggleSidebar(KeyEvent event, HardwareKeyboard state) =>
      toggleSidebarMac.accepts(event, state) ||
      toggleSidebarControl.accepts(event, state);

  /// Whether [event] is the "reveal the sidebar and focus search" chord.
  static bool matchesOpenSearch(KeyEvent event, HardwareKeyboard state) =>
      openSearchMac.accepts(event, state) ||
      openSearchControl.accepts(event, state);
}
