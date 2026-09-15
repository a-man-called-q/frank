import 'dart:math' as math;

import 'package:flutter/widgets.dart';
import 'package:flow_ui/flow_ui.dart';
import 'package:forui/forui.dart';
// Not redundant with flutter/material above, and not removable: flow_ui builds
// entirely on package:material_ui (29 of its files import it; it never imports
// Flutter's Material library). Its widgets therefore look up material_ui's own Material
// and MaterialLocalizations *types*, which are distinct from Flutter's despite
// the identical names. Dropping this import -- or either delegate below --
// makes flow_ui widgets assert "No MaterialLocalizations found" at runtime.
import 'package:material_ui/material_ui.dart' as mui;

import '../../app/icons.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';
import '../floor/office_scene_floor.dart';
import 'presentation/focusable_composer.dart';

const _composerMaxWidth = 760.0;
const _floorControlGap = 0.0;
const _floorControlWidth = 40.0;
const _floorControlIconSize = 14.0;
const _floorControlSlotWidth = _floorControlGap + _floorControlWidth;
const _chatRailMaxWidth = _composerMaxWidth + _floorControlSlotWidth;
const _chatLogMinHeight = 72.0;
const _chatLogMaxHeight = 280.0;

/// The composer card's own corner radius.
///
/// The transcript sits directly on top of that card, so it is inset by this
/// much on both sides: its square bottom corners then stop exactly where the
/// composer's border starts curving away, instead of overhanging it.
const _composerCornerRadius = 19.0;

class AccountExecutiveChat extends StatefulWidget {
  const AccountExecutiveChat({
    required this.executive,
    required this.project,
    required this.mission,
    required this.showNoMissionsNotice,
    required this.messages,
    required this.generating,
    required this.onSend,
    required this.onStop,
    this.renderFloor = true,
    this.sceneController,
    this.onMissionStatus,
    this.onRetryMissionPlan,
    super.key,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;

  /// Whether the Office conversation should explain that the selected project
  /// has no missions yet.
  final bool showNoMissionsNotice;
  final List<OfficeMessage> messages;
  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;
  final bool renderFloor;

  /// Shared camera controller owned by a caller that renders its own floor
  /// (for example the shell, which keeps one retained floor across every
  /// destination). When absent, this widget owns and disposes its own
  /// controller, which is the standalone/test path used with [renderFloor].
  final OfficeSceneController? sceneController;
  final Future<void> Function(MissionStatus status)? onMissionStatus;
  final Future<void> Function()? onRetryMissionPlan;

  @override
  State<AccountExecutiveChat> createState() => _AccountExecutiveChatState();
}

class _AccountExecutiveChatState extends State<AccountExecutiveChat> {
  OfficeSceneController? _ownedSceneController;

  OfficeSceneController get _sceneController =>
      widget.sceneController ??
      (_ownedSceneController ??= OfficeSceneController());

  @override
  void dispose() {
    _ownedSceneController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: _conversationLabel,
      child: LayoutBuilder(
        builder: (context, constraints) {
          final availableWidth = constraints.maxWidth.isFinite
              ? constraints.maxWidth
              : 760.0;
          final availableHeight = constraints.maxHeight.isFinite
              ? constraints.maxHeight
              : 640.0;
          final railWidth = math.min(
            _chatRailMaxWidth,
            math.max(1.0, availableWidth - 32),
          );
          final railHeight = math.min(
            560.0,
            math.max(1.0, availableHeight - 36),
          );
          final showConversationRail =
              availableWidth >= 360 && availableHeight >= 340;

          return Stack(
            fit: StackFit.expand,
            children: [
              if (widget.renderFloor) ...[
                Positioned.fill(
                  child: OfficeSceneFloor(controller: _sceneController),
                ),
              ],
              if (showConversationRail)
                Positioned.fill(
                  child: Align(
                    alignment: Alignment.bottomCenter,
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(16, 16, 16, 18),
                      child: SizedBox(
                        width: railWidth,
                        height: railHeight,
                        child: _ConversationRail(
                          executive: widget.executive,
                          project: widget.project,
                          mission: widget.mission,
                          showNoMissionsNotice: widget.showNoMissionsNotice,
                          messages: widget.messages,
                          generating: widget.generating,
                          onSend: widget.onSend,
                          onStop: widget.onStop,
                          onMissionStatus: widget.onMissionStatus,
                          onRetryMissionPlan: widget.onRetryMissionPlan,
                          sceneController: _sceneController,
                        ),
                      ),
                    ),
                  ),
                )
              else
                Positioned(
                  left: 16,
                  right: 16,
                  bottom: 16,
                  child: _CompactChatHint(
                    executive: widget.executive,
                    project: widget.project,
                    mission: widget.mission,
                  ),
                ),
            ],
          );
        },
      ),
    );
  }

  String get _conversationLabel {
    final contextLabel = widget.mission == null
        ? widget.project.name
        : widget.mission!.title;
    return 'Conversation with ${widget.executive.name} for $contextLabel';
  }
}

class _CompactChatHint extends StatelessWidget {
  const _CompactChatHint({
    required this.executive,
    required this.project,
    required this.mission,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;

  @override
  Widget build(BuildContext context) {
    final shortExecutiveName = executive.name.split(' ').first;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
      decoration: BoxDecoration(
        color: FrankColors.panel.withValues(alpha: 0.92),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: FrankColors.border),
      ),
      child: Text(
        '$shortExecutiveName · ${mission?.title ?? project.name} · Resize to chat',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        textAlign: TextAlign.center,
        style: const TextStyle(color: FrankColors.muted, fontSize: 11),
      ),
    );
  }
}

class _ConversationRail extends StatelessWidget {
  const _ConversationRail({
    required this.executive,
    required this.project,
    required this.mission,
    required this.showNoMissionsNotice,
    required this.messages,
    required this.generating,
    required this.onSend,
    required this.onStop,
    required this.sceneController,
    this.onMissionStatus,
    this.onRetryMissionPlan,
  });

  final OfficeEmployee executive;
  final OfficeProject project;
  final OfficeMission? mission;
  final bool showNoMissionsNotice;
  final List<OfficeMessage> messages;
  final bool generating;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;
  final OfficeSceneController sceneController;
  final Future<void> Function(MissionStatus status)? onMissionStatus;
  final Future<void> Function()? onRetryMissionPlan;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final railWidth = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : _chatRailMaxWidth;
        final composerWidth = math.max(1.0, railWidth - _floorControlSlotWidth);

        return Localizations(
          locale: const Locale('en', 'US'),
          delegates: const [
            DefaultWidgetsLocalizations.delegate,
            mui.DefaultMaterialLocalizations.delegate,
          ],
          child: mui.Material(
            // A canvas Material absorbs hit tests even with a transparent color.
            // Transparency keeps Flow UI's inherited Material context without
            // turning the whole rail into an interaction shield over the floor.
            type: mui.MaterialType.transparency,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (mission != null && onMissionStatus != null) ...[
                  _MissionLifecycleBar(
                    mission: mission!,
                    onStatus: onMissionStatus!,
                    onRetryPlanning: onRetryMissionPlan,
                  ),
                  const SizedBox(height: 8),
                ],
                if (showNoMissionsNotice && project.missions.isEmpty) ...[
                  Align(
                    alignment: Alignment.centerLeft,
                    child: Semantics(
                      container: true,
                      label: 'No missions yet',
                      child: Text(
                        'No missions yet',
                        style: TextStyle(
                          color: FrankColors.muted,
                          fontSize: 12,
                        ),
                      ),
                    ),
                  ),
                ],
                Expanded(
                  child: Align(
                    alignment: Alignment.bottomLeft,
                    child: SizedBox(
                      width: composerWidth,
                      child: messages.isEmpty
                          ? const SizedBox.shrink()
                          : Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: _composerCornerRadius,
                              ),
                              child: _PassiveConversationLog(
                                executive: executive,
                                messages: messages,
                              ),
                            ),
                    ),
                  ),
                ),
                if (messages.isEmpty) const SizedBox(height: 8),
                Stack(
                  children: [
                    Row(
                      children: [
                        SizedBox(
                          width: composerWidth,
                          child: FocusableComposer(
                            generating: generating,
                            onSend: onSend,
                            onStop: onStop,
                            executive: executive,
                            project: project,
                            mission: mission,
                          ),
                        ),
                        const SizedBox(width: _floorControlSlotWidth),
                      ],
                    ),
                    Positioned(
                      top: _composerCornerRadius,
                      right: 0,
                      bottom: _composerCornerRadius,
                      width: _floorControlWidth,
                      child: AnimatedBuilder(
                        animation: sceneController,
                        builder: (context, child) => _FloorControlPanel(
                          enabled: sceneController.canReset,
                          onPressed: sceneController.reset,
                        ),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _MissionLifecycleBar extends StatefulWidget {
  const _MissionLifecycleBar({
    required this.mission,
    required this.onStatus,
    this.onRetryPlanning,
  });

  final OfficeMission mission;
  final Future<void> Function(MissionStatus status) onStatus;
  final Future<void> Function()? onRetryPlanning;

  @override
  State<_MissionLifecycleBar> createState() => _MissionLifecycleBarState();
}

class _MissionLifecycleBarState extends State<_MissionLifecycleBar> {
  bool _busy = false;
  String? _error;

  Future<void> _run(Future<void> Function() action) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) setState(() => _error = frankFriendlyError(error));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final mission = widget.mission;
    final action = switch (mission.status) {
      MissionStatus.draft when !mission.hasPlan => null,
      MissionStatus.draft => ('Start', MissionStatus.active),
      MissionStatus.active => ('Pause', MissionStatus.paused),
      MissionStatus.paused => ('Resume', MissionStatus.active),
      MissionStatus.blocked when mission.hasPlan => (
        'Resume',
        MissionStatus.active,
      ),
      MissionStatus.blocked => null,
      _ =>
        mission.allTasksDone
            ? ('Complete mission', MissionStatus.completed)
            : null,
    };
    final preparing = mission.status == MissionStatus.draft && !mission.hasPlan;
    final message =
        _error ??
        mission.lastError ??
        (preparing
            ? 'Preparing plan…'
            : mission.hasPlan
            ? '${mission.doneTaskCount}/${mission.totalTaskCount} tasks complete'
            : 'Mission is ready for planning');
    final largeText = MediaQuery.textScalerOf(context).scale(1) >= 1.5;
    final actionButton = switch ((
      mission.lastError != null && widget.onRetryPlanning != null,
      action,
    )) {
      (true, _) =>
        largeText
            ? FButton.raw(
                onPress: _busy ? null : () => _run(widget.onRetryPlanning!),
                variant: FButtonVariant.outline,
                size: FButtonSizeVariant.sm,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 12,
                  ),
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    child: Text(_busy ? 'Working…' : 'Retry planning'),
                  ),
                ),
              )
            : FButton(
                onPress: _busy ? null : () => _run(widget.onRetryPlanning!),
                variant: FButtonVariant.outline,
                size: FButtonSizeVariant.sm,
                child: const Text('Retry planning'),
              ),
      (false, final value?) =>
        largeText
            ? FButton.raw(
                onPress: _busy
                    ? null
                    : () => _run(() => widget.onStatus(value.$2)),
                size: FButtonSizeVariant.sm,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 12,
                  ),
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    child: Text(_busy ? 'Working…' : value.$1),
                  ),
                ),
              )
            : FButton(
                onPress: _busy
                    ? null
                    : () => _run(() => widget.onStatus(value.$2)),
                size: FButtonSizeVariant.sm,
                child: Text(_busy ? 'Working…' : value.$1),
              ),
      _ => null,
    };
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        color: FrankColors.panel.withValues(alpha: .94),
        borderRadius: BorderRadius.circular(FrankUiTokens.controlRadius),
        border: Border.all(
          color: mission.lastError == null
              ? FrankColors.border
              : FrankColors.failure,
        ),
      ),
      child: largeText
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  message,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    color: mission.lastError == null
                        ? FrankColors.muted
                        : FrankColors.failure,
                    fontSize: 12,
                  ),
                ),
                if (actionButton != null) ...[
                  const SizedBox(height: 8),
                  Align(alignment: Alignment.centerRight, child: actionButton),
                ],
              ],
            )
          : Row(
              children: [
                Expanded(
                  child: Text(
                    message,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      color: mission.lastError == null
                          ? FrankColors.muted
                          : FrankColors.failure,
                      fontSize: 12,
                    ),
                  ),
                ),
                ?actionButton,
              ],
            ),
    );
  }
}

/// A compact, content-sized MMORPG-style transcript.
///
/// Message content is pointer-passive, while the viewport itself owns wheel
/// and trackpad scrolling once the history exceeds its maximum height.
class _PassiveConversationLog extends StatefulWidget {
  const _PassiveConversationLog({
    required this.executive,
    required this.messages,
  });

  final OfficeEmployee executive;
  final List<OfficeMessage> messages;

  @override
  State<_PassiveConversationLog> createState() =>
      _PassiveConversationLogState();
}

class _PassiveConversationLogState extends State<_PassiveConversationLog> {
  static const _latestThreshold = 24.0;

  final ScrollController _scrollController = ScrollController();

  @override
  void didUpdateWidget(covariant _PassiveConversationLog oldWidget) {
    super.didUpdateWidget(oldWidget);
    final wasNearLatest =
        !_scrollController.hasClients ||
        _scrollController.offset <= _latestThreshold;
    if (!wasNearLatest) return;

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scrollController.hasClients) return;
      _scrollController.jumpTo(0);
    });
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedSize(
      key: const ValueKey('passive-chat-transcript'),
      duration: const Duration(milliseconds: 140),
      curve: Curves.easeOutCubic,
      alignment: Alignment.bottomCenter,
      child: ConstrainedBox(
        constraints: const BoxConstraints(
          minHeight: _chatLogMinHeight,
          maxHeight: _chatLogMaxHeight,
        ),
        child: ClipRRect(
          key: const ValueKey('passive-chat-transcript-clip'),
          borderRadius: const BorderRadius.only(
            topLeft: Radius.circular(14),
            topRight: Radius.circular(14),
          ),
          child: DecoratedBox(
            key: const ValueKey('passive-chat-transcript-background'),
            decoration: BoxDecoration(color: const Color(0x26000000)),
            child: RawScrollbar(
              key: const ValueKey('chat-transcript-scrollbar'),
              controller: _scrollController,
              thumbVisibility: false,
              trackVisibility: false,
              interactive: true,
              thickness: 4,
              radius: const Radius.circular(4),
              mainAxisMargin: 8,
              crossAxisMargin: 4,
              child: ListView.builder(
                key: const ValueKey('chat-transcript-list'),
                controller: _scrollController,
                reverse: true,
                shrinkWrap: true,
                primary: false,
                physics: const ClampingScrollPhysics(),
                padding: const EdgeInsets.all(16),
                itemCount: widget.messages.length,
                itemBuilder: (context, index) {
                  // A reversed list starts with the newest message at the bottom.
                  final message =
                      widget.messages[widget.messages.length - 1 - index];
                  return Padding(
                    key: ValueKey('chat-log-message-${message.id}'),
                    padding: EdgeInsets.only(
                      top: index == widget.messages.length - 1 ? 0 : 12,
                    ),
                    child: IgnorePointer(
                      child: _ChatLogMessage(
                        executive: widget.executive,
                        message: message,
                      ),
                    ),
                  );
                },
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Dedicated floor-control panel attached to the composer's right edge.
///
/// This is intentionally a separate surface from the chat transcript. It
/// mirrors the transcript's 19px vertical inset and keeps the center button in
/// its own hit-test region, while the transcript above owns history scrolling.
class _FloorControlPanel extends StatelessWidget {
  const _FloorControlPanel({required this.enabled, required this.onPressed});

  final bool enabled;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Floor controls',
      child: Container(
        key: const ValueKey('floor-control-panel'),
        padding: const EdgeInsets.symmetric(horizontal: 4),
        decoration: BoxDecoration(
          // Neutral raised surface matches the composer while allowing a
          // little of the floor to show through.
          color: FrankColors.panelRaised.withValues(alpha: 0.8),
          borderRadius: const BorderRadius.only(
            topLeft: Radius.zero,
            bottomLeft: Radius.zero,
            topRight: Radius.circular(14),
            bottomRight: Radius.circular(14),
          ),
        ),
        child: Center(
          child: _RecenterFloorButton(enabled: enabled, onPressed: onPressed),
        ),
      ),
    );
  }
}

/// Center button inside [_FloorControlPanel].
class _RecenterFloorButton extends StatelessWidget {
  const _RecenterFloorButton({required this.enabled, required this.onPressed});

  final bool enabled;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      enabled: enabled,
      label: 'Reset floor view',
      child: FButton.icon(
        key: const ValueKey('floor-reset-view-button'),
        onPress: enabled ? onPressed : null,
        semanticsTooltip: 'Reset floor view',
        size: FButtonSizeVariant.sm,
        child: const Icon(FrankIcons.recenter, size: _floorControlIconSize),
      ),
    );
  }
}

class _ChatLogMessage extends StatelessWidget {
  const _ChatLogMessage({required this.executive, required this.message});

  final OfficeEmployee executive;
  final OfficeMessage message;

  @override
  Widget build(BuildContext context) {
    final assistant = message.role == ChatRole.assistant;
    final speaker = assistant ? executive.name.split(' ').first : 'You';
    final nameColor = assistant ? Color(executive.color) : FrankColors.muted;
    final bodyColor = switch (message.status) {
      OfficeMessageStatus.error => FrankColors.failure,
      OfficeMessageStatus.stopped => FrankColors.muted,
      _ => FrankColors.ink,
    };
    final isThinking =
        message.status == OfficeMessageStatus.pending &&
        message.text.trim().isEmpty;

    return Semantics(
      container: true,
      label: '$speaker: ${isThinking ? 'is thinking…' : message.text}',
      child: Row(
        key: ValueKey('chat-log-row-${message.id}'),
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '$speaker:',
            style: TextStyle(
              color: nameColor,
              fontSize: 12,
              fontWeight: FontWeight.w700,
              height: 1.45,
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: isThinking
                ? const Text(
                    'is thinking…',
                    style: TextStyle(
                      color: FrankColors.muted,
                      fontSize: 13,
                      height: 1.45,
                    ),
                  )
                : message.text.isEmpty
                ? const SizedBox.shrink()
                : FlowMarkdown(
                    text: message.text,
                    isStreaming:
                        message.status == OfficeMessageStatus.streaming,
                    style: TextStyle(
                      color: bodyColor,
                      fontSize: 13,
                      height: 1.45,
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}
