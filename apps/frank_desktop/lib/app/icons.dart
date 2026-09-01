import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

/// Frank's semantic icon vocabulary.
///
/// Forui already bundles the Lucide family used by the T3Code reference. A
/// small semantic registry keeps product code independent from individual
/// glyph names and gives us one place to tune the visual language later.
abstract final class FrankIcons {
  static const search = FLucideIcons.search;
  static const panelOpen = FLucideIcons.panelLeft;
  static const panelClose = FLucideIcons.panelLeftClose;
  static const dashboard = FLucideIcons.layoutDashboard;
  static const folder = FLucideIcons.folder;
  static const folderOpen = FLucideIcons.folderOpen;
  static const folderPlus = FLucideIcons.folderPlus;
  static const briefcase = FLucideIcons.briefcase;
  static const user = FLucideIcons.user;
  static const users = FLucideIcons.users;
  static const message = FLucideIcons.messageCircle;
  static const messageSquare = FLucideIcons.messageSquare;
  static const edit = FLucideIcons.pencil;
  static const editNote = FLucideIcons.squarePen;
  static const pin = FLucideIcons.pin;
  static const archive = FLucideIcons.archive;
  static const close = FLucideIcons.x;
  static const more = FLucideIcons.ellipsis;
  static const settings = FLucideIcons.settings;
  static const activity = FLucideIcons.zap;
  static const ledger = FLucideIcons.receiptText;
  static const circle = FLucideIcons.circle;
  static const circleAlert = FLucideIcons.circleAlert;
  static const circleCheck = FLucideIcons.circleCheck;
  static const circleDashed = FLucideIcons.circleDashed;
  static const clock = FLucideIcons.clock;
  static const terminal = FLucideIcons.terminal;
  static const gitBranch = FLucideIcons.gitBranch;
  static const refresh = FLucideIcons.refreshCw;
  static const plus = FLucideIcons.plus;
  static const check = FLucideIcons.check;
  static const pause = FLucideIcons.pause;
  static const play = FLucideIcons.play;
  static const filter = FLucideIcons.filter;
  static const chevronDown = FLucideIcons.chevronDown;
  static const chevronRight = FLucideIcons.chevronRight;
}

/// Keeps icon-bearing APIs readable at call sites that need an explicit type.
typedef FrankIcon = IconData;
