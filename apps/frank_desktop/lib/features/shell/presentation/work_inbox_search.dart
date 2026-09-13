part of 'work_inbox.dart';

class _SearchField extends StatelessWidget {
  const _SearchField({
    required this.controller,
    required this.focusNode,
    required this.onChanged,
    required this.onClear,
  });

  final TextEditingController controller;
  final FocusNode focusNode;
  final ValueChanged<String> onChanged;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    return Focus(
      onKeyEvent: (_, event) {
        if (event is! KeyDownEvent ||
            event.logicalKey != LogicalKeyboardKey.escape) {
          return KeyEventResult.ignored;
        }
        if (controller.text.isNotEmpty) {
          onClear();
          return KeyEventResult.handled;
        }
        focusNode.unfocus();
        return KeyEventResult.handled;
      },
      child: Semantics(
        textField: true,
        label: 'Search workspace',
        child: TextField(
          controller: controller,
          focusNode: focusNode,
          onChanged: onChanged,
          style: const TextStyle(color: FrankColors.ink, fontSize: 12),
          decoration: InputDecoration(
            isDense: true,
            filled: true,
            fillColor: FrankColors.panelRaised,
            prefixIcon: const Icon(FrankIcons.search, size: 16),
            prefixIconConstraints: const BoxConstraints.tightFor(width: 34),
            suffixIcon: controller.text.isEmpty
                ? null
                : Semantics(
                    button: true,
                    label: 'Clear the workspace search',
                    child: IconButton(
                      onPressed: onClear,
                      tooltip: 'Clear the workspace search',
                      icon: const Icon(FrankIcons.close, size: 15),
                      color: FrankColors.muted,
                      visualDensity: VisualDensity.compact,
                    ),
                  ),
            hintText: 'Search workspace',
            hintStyle: const TextStyle(color: FrankColors.muted, fontSize: 12),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 10,
              vertical: 9,
            ),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: const BorderSide(color: FrankColors.border),
            ),
          ),
        ),
      ),
    );
  }
}
