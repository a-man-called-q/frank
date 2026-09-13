part of 'work_inbox.dart';

class _ScopeSelector extends StatelessWidget {
  const _ScopeSelector({
    required this.projects,
    required this.selectedProjectId,
    required this.onChanged,
  });

  final List<OfficeProject> projects;
  final String? selectedProjectId;
  final ValueChanged<String?> onChanged;

  @override
  Widget build(BuildContext context) {
    var selectedName = 'All projects';
    for (final project in projects) {
      if (project.id == selectedProjectId) {
        selectedName = project.name;
        break;
      }
    }
    return FrankDesktopSelectField<String?>(
      fieldKey: const ValueKey('project-scope-selector'),
      value: selectedProjectId,
      options: [
        const FrankDesktopSelectOption<String?>(
          value: null,
          label: 'All projects',
        ),
        ...projects.map(
          (project) => FrankDesktopSelectOption<String?>(
            value: project.id,
            label: project.name,
          ),
        ),
      ],
      onChanged: onChanged,
      semanticsLabel: 'Project scope',
      hint: selectedName,
    );
  }
}
