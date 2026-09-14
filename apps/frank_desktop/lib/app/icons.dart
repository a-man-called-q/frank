import 'package:flutter/widgets.dart';
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

  /// The broad Office mode switch, distinct from the Floor landing surface.
  static const office = FLucideIcons.building2;

  /// The 3D room/floor landing surface in the Office mode.
  static const floor = FLucideIcons.layers3;

  /// Columns communicate the task lanes more clearly than the dashboard grid.
  static const taskboard = FLucideIcons.kanban;
  static const taskList = FLucideIcons.listTodo;

  /// Journal entries are written events, so the notebook glyph is intentional.
  static const journal = FLucideIcons.notebookPen;
  static const settings = FLucideIcons.settings;
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
  static const back = FLucideIcons.arrowLeft;
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
  static const cloud = FLucideIcons.cloud;
  static const database = FLucideIcons.database;
  static const approval = FLucideIcons.shieldCheck;
  static const undo = FLucideIcons.undo2;
  static const redo = FLucideIcons.redo2;
  static const save = FLucideIcons.save;
  static const publish = FLucideIcons.send;
  static const send = FLucideIcons.send;
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
  static const monitorSmartphone = FLucideIcons.monitorSmartphone;
  static const keyRound = FLucideIcons.keyRound;
  static const logOut = FLucideIcons.logOut;
  static const bot = FLucideIcons.bot;
  static const paperclip = FLucideIcons.paperclip;
  static const recenter = FLucideIcons.crosshair;

  // Compatibility names for fixture/model metadata and compact feature rows.
  static const checkCircleOutline = FLucideIcons.circleCheck;
  static const boltOutlined = FLucideIcons.bolt;
  static const pauseCircleOutline = FLucideIcons.circlePause;
  static const rateReviewOutlined = FLucideIcons.messageSquareMore;
  static const errorOutline = FLucideIcons.circleAlert;
  static const cloudOffOutlined = FLucideIcons.cloudOff;
  static const mailOutline = FLucideIcons.mail;
  static const calendarTodayOutlined = FLucideIcons.calendar;
  static const viewKanbanOutlined = FLucideIcons.kanban;
  static const assignmentOutlined = FLucideIcons.clipboardList;
  static const language = FLucideIcons.languages;
  static const folderOpenOutlined = FLucideIcons.folderOpen;
  static const noteAddOutlined = FLucideIcons.notebookPen;
  static const terminalOutlined = FLucideIcons.terminal;
  static const storageOutlined = FLucideIcons.database;
  static const dashboardOutlined = FLucideIcons.layoutDashboard;
  static const tune = FLucideIcons.slidersHorizontal;
  static const visibilityOutlined = FLucideIcons.eye;
  static const workspacesOutlined = FLucideIcons.briefcaseBusiness;
  static const warningAmberOutlined = FLucideIcons.triangleAlert;
  static const lockOutline = FLucideIcons.lockKeyhole;
  static const link = FLucideIcons.link;
  static const openInNew = FLucideIcons.externalLink;
  static const keyboardArrowDown = FLucideIcons.chevronDown;
  static const keyboardArrowUp = FLucideIcons.chevronUp;
  static const arrowDropDown = FLucideIcons.chevronDown;
  static const moreVert = FLucideIcons.ellipsisVertical;
  static const menu = FLucideIcons.menu;
  static const checkBox = FLucideIcons.squareCheck;
  static const checkBoxOutlineBlank = FLucideIcons.square;
  static const radioButtonChecked = FLucideIcons.circleDot;
  static const radioButtonUnchecked = FLucideIcons.circle;
  static const indeterminateCheckBox = FLucideIcons.squareMinus;
  static const closeSmall = FLucideIcons.x;
  static const shieldOutlined = FLucideIcons.shield;
  static const flagOutlined = FLucideIcons.flag;
  static const callReceivedOutlined = FLucideIcons.phoneIncoming;
  static const timelineOutlined = FLucideIcons.timeline;
  static const visibilityOffOutlined = FLucideIcons.eyeOff;
  static const cloudOutlined = FLucideIcons.cloud;
  static const keyOutlined = FLucideIcons.keyRound;
  static const listAltOutlined = FLucideIcons.list;
  static const accountTreeOutlined = FLucideIcons.network;
  static const circleOutlined = FLucideIcons.circle;
  static const arrowOutward = FLucideIcons.arrowUpRight;
  static const autoAwesomeOutlined = FLucideIcons.sparkles;
  static const memoryOutlined = FLucideIcons.memoryStick;
  static const workOutline = FLucideIcons.briefcaseBusiness;
  static const badgeOutlined = FLucideIcons.badge;
  static const extensionOutlined = FLucideIcons.puzzle;
  static const groupsOutlined = FLucideIcons.usersRound;
  static const peopleOutline = FLucideIcons.users;
  static const personAddAlt1 = FLucideIcons.userRoundPlus;
  static const tuneOutlined = FLucideIcons.slidersHorizontal;
  static const historyToggleOffOutlined = FLucideIcons.history;
  static const schedule = FLucideIcons.calendarClock;
  static const hourglassEmpty = FLucideIcons.hourglass;
  static const arrowBack = FLucideIcons.arrowLeft;
  static const arrowForward = FLucideIcons.arrowRight;
  static const refreshOutlined = FLucideIcons.refreshCw;
  static const add = FLucideIcons.plus;
  static const remove = FLucideIcons.minus;
  static const copy = FLucideIcons.copy;
  static const searchOutlined = FLucideIcons.search;
  static const filterList = FLucideIcons.listFilter;
  static const infoOutline = FLucideIcons.info;
  static const helpOutline = FLucideIcons.circleHelp;
  static const logout = FLucideIcons.logOut;
  static const personOutline = FLucideIcons.user;
  static const groupOutlined = FLucideIcons.users;
  static const editOutlined = FLucideIcons.pencil;
  static const deleteOutline = FLucideIcons.trash2;
  static const archiveOutlined = FLucideIcons.archive;
  static const playArrow = FLucideIcons.play;
  static const pauseOutlined = FLucideIcons.pause;
  static const stop = FLucideIcons.square;
  static const sendOutlined = FLucideIcons.send;
  static const attachFile = FLucideIcons.paperclip;
  static const micNone = FLucideIcons.mic;
  static const expandMore = FLucideIcons.chevronDown;
  static const expandLess = FLucideIcons.chevronUp;
}

/// Keeps icon-bearing APIs readable at call sites that need an explicit type.
typedef FrankIcon = IconData;
