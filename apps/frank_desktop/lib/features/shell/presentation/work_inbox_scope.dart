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
    return Semantics(
      label: 'Project scope',
      child: FSelect<String?>.rich(
        key: const ValueKey('project-scope-selector'),
        format: (value) => value == null
            ? 'All projects'
            : projects
                  .firstWhere(
                    (project) => project.id == value,
                    orElse: () => projects.first,
                  )
                  .name,
        control: FSelectControl<String?>.lifted(
          value: selectedProjectId,
          onChange: onChanged,
        ),
        hint: selectedName,
        children: [
          FSelectItem<String?>.item(
            value: null,
            title: const Text('All projects'),
          ),
          for (final project in projects)
            FSelectItem<String?>.item(
              value: project.id,
              title: Text(project.name),
            ),
        ],
      ),
    );
  }
}
