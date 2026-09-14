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
        value: controller.text,
        child: ExcludeSemantics(
          child: FTextField(
          key: const ValueKey('work-inbox-search-field'),
          control: FTextFieldControl.managed(
            controller: controller,
            onChange: (value) => onChanged(value.text),
          ),
          focusNode: focusNode,
          hint: 'Search workspace',
          prefixBuilder: (context, style, variants) =>
              FTextField.prefixIconBuilder(
                context,
                style,
                variants,
                const Icon(FrankIcons.search, size: 16),
              ),
          suffixBuilder: (context, style, _) => FButton.icon(
            key: const ValueKey('work-inbox-search-clear'),
            style: style.clearButtonStyle,
            semanticsLabel: 'Clear the workspace search',
            onPress: controller.text.isEmpty
                ? null
                : () {
                    controller.clear();
                    onClear();
                  },
            child: const Icon(FrankIcons.close, size: 16),
          ),
          ),
        ),
      ),
    );
  }
}
