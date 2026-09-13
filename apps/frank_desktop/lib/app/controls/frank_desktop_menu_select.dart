part of 'frank_desktop_menu.dart';

class FrankDesktopSelect<T> extends StatefulWidget {
  const FrankDesktopSelect({
    required this.value,
    required this.options,
    required this.onChanged,
    required this.child,
    this.semanticsLabel,
    this.matchTriggerWidth = true,
    this.menuWidth = 248,
    this.enabled = true,
    this.searchable = false,
    this.searchHint = 'Search',
    super.key,
  });

  final T value;
  final List<FrankDesktopSelectOption<T>> options;
  final ValueChanged<T> onChanged;
  final Widget child;
  final String? semanticsLabel;
  final bool matchTriggerWidth;
  final double menuWidth;
  final bool enabled;
  final bool searchable;
  final String searchHint;

  @override
  State<FrankDesktopSelect<T>> createState() => _FrankDesktopSelectState<T>();
}

class FrankDesktopSelectOption<T> {
  const FrankDesktopSelectOption({
    required this.value,
    required this.label,
    this.badge,
    this.enabled = true,
  });

  final T value;
  final String label;
  final String? badge;
  final bool enabled;
}

class _FrankDesktopSelectState<T> extends State<FrankDesktopSelect<T>> {
  late final FrankDesktopMenuController _controller;
  late final FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = FrankDesktopMenuController();
    _focusNode = FocusNode(debugLabel: widget.semanticsLabel ?? 'Select');
  }

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final selectedLabel = widget.options
        .where((option) => option.value == widget.value)
        .map((option) => option.label)
        .firstOrNull;
    final groups = [
      FrankMenuGroup([
        for (final option in widget.options)
          FrankMenuItem(
            label: option.label,
            badge: option.badge,
            checked: option.value == widget.value,
            enabled: option.enabled,
            onPressed: widget.enabled && option.enabled
                ? () => widget.onChanged(option.value)
                : null,
          ),
      ]),
    ];
    final groupsBuilder = widget.searchable
        ? (String query) {
            final normalized = query.trim().toLowerCase();
            return [
              FrankMenuGroup([
                for (final option in widget.options)
                  if (normalized.isEmpty ||
                      option.label.toLowerCase().contains(normalized))
                    FrankMenuItem(
                      label: option.label,
                      badge: option.badge,
                      checked: option.value == widget.value,
                      enabled: option.enabled,
                      onPressed: widget.enabled && option.enabled
                          ? () => widget.onChanged(option.value)
                          : null,
                    ),
              ]),
            ];
          }
        : null;
    return Semantics(
      container: true,
      button: true,
      label: widget.semanticsLabel,
      value: selectedLabel,
      child: FrankDesktopMenu(
        controller: _controller,
        kind: FrankDesktopMenuKind.select,
        width: widget.menuWidth,
        matchTriggerWidth: widget.matchTriggerWidth,
        enabled: widget.enabled,
        groupsBuilder: groupsBuilder,
        searchHint: widget.searchHint,
        openOnTap: true,
        returnFocusNode: _focusNode,
        groups: groups,
        child: Focus(
          focusNode: _focusNode,
          onKeyEvent: (_, event) {
            if (event is! KeyDownEvent) return KeyEventResult.ignored;
            if (event.logicalKey == LogicalKeyboardKey.enter ||
                event.logicalKey == LogicalKeyboardKey.space ||
                event.logicalKey == LogicalKeyboardKey.arrowDown) {
              _controller.open();
              return KeyEventResult.handled;
            }
            return KeyEventResult.ignored;
          },
          child: widget.child,
        ),
      ),
    );
  }
}

/// Canonical compact value picker used by all Frank feature surfaces.
///
/// The trigger deliberately owns its geometry so individual features cannot
/// drift into Material's default dropdown treatment again.
class FrankDesktopSelectField<T> extends StatelessWidget {
  const FrankDesktopSelectField({
    required this.value,
    required this.options,
    required this.onChanged,
    this.label,
    this.hint = 'Select',
    this.errorText,
    this.enabled = true,
    this.searchable = false,
    this.searchHint = 'Search',
    this.semanticsLabel,
    this.fieldKey,
    super.key,
  });

  final T value;
  final List<FrankDesktopSelectOption<T>> options;
  final ValueChanged<T> onChanged;
  final String? label;
  final String hint;
  final String? errorText;
  final bool enabled;
  final bool searchable;
  final String searchHint;
  final String? semanticsLabel;
  final Key? fieldKey;

  @override
  Widget build(BuildContext context) {
    final selected = options
        .where((option) => option.value == value)
        .map((option) => option.label)
        .firstOrNull;
    final trigger = Container(
      key: fieldKey,
      height: 30,
      padding: const EdgeInsets.symmetric(horizontal: 8),
      decoration: BoxDecoration(
        color: enabled ? FrankColors.panelRaised : FrankColors.panel,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              selected ?? hint,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                color: enabled && selected != null
                    ? FrankColors.ink
                    : FrankColors.muted,
                fontSize: 11,
              ),
            ),
          ),
          const SizedBox(width: 8),
          const Icon(
            FrankIcons.chevronDown,
            size: 15,
            color: FrankColors.muted,
          ),
        ],
      ),
    );
    final picker = FrankDesktopSelect<T>(
      value: value,
      options: options,
      onChanged: onChanged,
      enabled: enabled,
      searchable: searchable,
      searchHint: searchHint,
      semanticsLabel: semanticsLabel ?? label,
      child: trigger,
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        final content = Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (label != null) ...[
              Text(
                label!,
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
              const SizedBox(height: 5),
            ],
            picker,
            if (errorText != null) ...[
              const SizedBox(height: 4),
              Text(
                errorText!,
                style: const TextStyle(
                  color: FrankColors.failure,
                  fontSize: 11,
                ),
              ),
            ],
          ],
        );
        final sizedContent = constraints.hasBoundedWidth
            ? content
            : SizedBox(width: 200, child: content);
        return Semantics(
          container: true,
          enabled: enabled,
          label: semanticsLabel ?? label,
          value: selected ?? hint,
          child: sizedContent,
        );
      },
    );
  }
}
