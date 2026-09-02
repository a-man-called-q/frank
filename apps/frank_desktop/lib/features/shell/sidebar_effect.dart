import 'package:flutter/widgets.dart';

/// Wraps the visible sidebar bounds in a platform-native visual effect.
///
/// The builder is intentionally injected at the app boundary so widget tests
/// and non-macOS builds never need to call a native window channel.
typedef SidebarEffectBuilder = Widget Function(Widget child);
