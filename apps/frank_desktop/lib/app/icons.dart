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
  static const activity = FLucideIcons.zap;
  static const ledger = FLucideIcons.receiptText;
  static const circle = FLucideIcons.circle;
  static const circleAlert = FLucideIcons.circleAlert;
  static const circleCheck = FLucideIcons.circleCheck;
  static const circleDashed = FLucideIcons.circleDashed;
  static const clock = FLucideIcons.clock;
  static const terminal = FLucideIcons.terminal;
  static const mail = FLucideIcons.mail;
  static const calendar = FLucideIcons.calendar;
  static const drive = FLucideIcons.hardDrive;
  static const browser = FLucideIcons.globe;
  static const database = FLucideIcons.database;
  static const approval = FLucideIcons.shieldCheck;
  static const undo = FLucideIcons.undo2;
  static const redo = FLucideIcons.redo2;
  static const save = FLucideIcons.save;
  static const publish = FLucideIcons.send;
  static const minimap = FLucideIcons.map;
  static const workflow = FLucideIcons.workflow;
  static const gitBranch = FLucideIcons.gitBranch;
  static const refresh = FLucideIcons.refreshCw;
  static const plus = FLucideIcons.plus;
  static const check = FLucideIcons.check;
  static const pause = FLucideIcons.pause;
  static const play = FLucideIcons.play;
  static const filter = FLucideIcons.filter;
  static const chevronDown = FLucideIcons.chevronDown;
  static const chevronUp = FLucideIcons.chevronUp;
  static const chevronRight = FLucideIcons.chevronRight;
  static const mic = FLucideIcons.mic;
  static const arrowUp = FLucideIcons.arrowUp;
  static const square = FLucideIcons.square;
  static const monitor = FLucideIcons.monitor;
  static const bot = FLucideIcons.bot;
  static const paperclip = FLucideIcons.paperclip;
  static const recenter = FLucideIcons.crosshair;
}

/// Keeps icon-bearing APIs readable at call sites that need an explicit type.
typedef FrankIcon = IconData;
