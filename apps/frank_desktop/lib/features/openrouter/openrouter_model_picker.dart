import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import '../../app/icons.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';
import '../../core/models/openrouter_models.dart';

/// The canonical model selector used by the supervisor, role defaults, and
/// agent overrides. It keeps deprecated and missing stored values visible for
/// inspection while preventing deprecated models from being newly selected.
enum FrankModelPickerFilter { all, free, paid, tools }

class FrankOpenRouterModelPicker extends StatefulWidget {
  const FrankOpenRouterModelPicker({
    required this.label,
    required this.value,
    required this.models,
    required this.enabled,
    required this.onChanged,
    this.hint = 'Choose a model',
    super.key,
  });

  final String label;
  final String? value;
  final List<OpenRouterModel> models;
  final bool enabled;
  final ValueChanged<String?> onChanged;
  final String hint;

  @override
  State<FrankOpenRouterModelPicker> createState() =>
      _FrankOpenRouterModelPickerState();
}

class _FrankOpenRouterModelPickerState
    extends State<FrankOpenRouterModelPicker> {
  FrankModelPickerFilter _filter = FrankModelPickerFilter.all;

  bool _matchesFilter(OpenRouterModel model) {
    if (model.canonicalSlug == widget.value) return true;
    return switch (_filter) {
      FrankModelPickerFilter.all => true,
      FrankModelPickerFilter.free => model.isFree,
      FrankModelPickerFilter.paid => model.isPaid,
      FrankModelPickerFilter.tools => model.supportsTools,
    };
  }

  @override
  Widget build(BuildContext context) {
    final bySlug = <String, OpenRouterModel>{
      for (final model in widget.models) model.canonicalSlug: model,
    };
    final missingValue =
        widget.value != null && !bySlug.containsKey(widget.value);
    final visibleModels = widget.models.where(_matchesFilter).toList();
    final searchableValues = [
      if (missingValue) widget.value!,
      for (final model in visibleModels) model.canonicalSlug,
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        FrankSegmentedControl<FrankModelPickerFilter>(
          key: const ValueKey('openrouter-model-picker-filter'),
          value: _filter,
          items: const [
            (FrankModelPickerFilter.all, 'All', null),
            (FrankModelPickerFilter.free, 'Free', null),
            (FrankModelPickerFilter.paid, 'Paid', null),
            (
              FrankModelPickerFilter.tools,
              'Tools',
              FrankIcons.extensionOutlined,
            ),
          ],
          onChanged: (filter) => setState(() => _filter = filter),
        ),
        const SizedBox(height: 8),
        FSelect<String?>.searchBuilder(
          control: FSelectControl<String?>.lifted(
            value: widget.value,
            onChange: widget.onChanged,
          ),
          format: (selected) {
            if (selected == null || selected.isEmpty) return '';
            final model = bySlug[selected];
            return model == null
                ? '$selected · Missing from catalog'
                : model.canonicalSlug;
          },
          filter: (query) {
            final normalized = query.trim().toLowerCase();
            return [
              for (final slug in searchableValues)
                if (normalized.isEmpty ||
                    slug.toLowerCase().contains(normalized) ||
                    (bySlug[slug]?.name.toLowerCase().contains(normalized) ??
                        false))
                  slug,
            ];
          },
          contentBuilder: (context, style, values) => [
            if (missingValue && values.contains(widget.value))
              FSelectItem<String?>.item(
                value: widget.value,
                title: Text(
                  '${widget.value} · Missing from catalog',
                  style: const TextStyle(color: FrankColors.warningAmber),
                ),
              ),
            for (final model in visibleModels)
              if (values.contains(model.canonicalSlug) &&
                  (!model.deprecated || model.canonicalSlug == widget.value))
                FSelectItem<String?>.item(
                  value: model.canonicalSlug,
                  title: Text(
                    '${model.name} · ${model.canonicalSlug} · ${model.priceTier}',
                  ),
                ),
          ],
          label: Text(widget.label),
          hint: widget.hint,
          enabled: widget.enabled,
        ),
      ],
    );
  }
}
