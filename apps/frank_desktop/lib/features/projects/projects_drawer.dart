import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';

class ProjectsDrawer extends StatelessWidget {
  const ProjectsDrawer({
    required this.projects,
    required this.selectedProjectId,
    required this.onSelect,
    required this.onClose,
    super.key,
  });

  final List<OfficeProject> projects;
  final String selectedProjectId;
  final ValueChanged<String> onSelect;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 326,
      decoration: const BoxDecoration(
        color: FrankColors.panel,
        border: Border(left: BorderSide(color: FrankColors.border)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 20, 12, 14),
            child: Row(
              children: [
                const Expanded(
                  child: Text(
                    'Projects',
                    style: TextStyle(fontSize: 16, fontWeight: FontWeight.w700),
                  ),
                ),
                IconButton(
                  onPressed: onClose,
                  tooltip: 'Close projects drawer',
                  icon: const Icon(Icons.close, size: 18),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 18),
            child: FButton(
              onPress: () {},
              prefix: const Icon(Icons.add, size: 17),
              child: const Text('New project'),
            ),
          ),
          const SizedBox(height: 14),
          const Divider(height: 1, color: FrankColors.border),
          Expanded(
            child: ListView.separated(
              padding: const EdgeInsets.all(14),
              itemCount: projects.length,
              separatorBuilder: (_, _) => const SizedBox(height: 9),
              itemBuilder: (context, index) {
                final project = projects[index];
                return _ProjectTile(
                  project: project,
                  selected: project.id == selectedProjectId,
                  onTap: () => onSelect(project.id),
                );
              },
            ),
          ),
          const Padding(
            padding: EdgeInsets.fromLTRB(18, 8, 18, 18),
            child: Text(
              'Projects are shown locally for this prototype. Live agency data arrives in the next milestone.',
              style: TextStyle(color: FrankColors.muted, fontSize: 11, height: 1.4),
            ),
          ),
        ],
      ),
    );
  }
}

class _ProjectTile extends StatelessWidget {
  const _ProjectTile({
    required this.project,
    required this.selected,
    required this.onTap,
  });

  final OfficeProject project;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final border = selected ? FrankColors.amber : FrankColors.border;
    final background = selected ? FrankColors.amberSoft : FrankColors.panelRaised;
    final statusColor = switch (project.status) {
      ProjectStatus.planning => FrankColors.blue,
      ProjectStatus.active => FrankColors.green,
      ProjectStatus.review => FrankColors.amber,
      ProjectStatus.delivered => const Color(0xFFBE9DEB),
    };
    return Semantics(
      button: true,
      selected: selected,
      label: '${project.name}, ${project.statusLabel}',
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(12),
          child: Container(
            padding: const EdgeInsets.all(13),
            decoration: BoxDecoration(
              color: background,
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: border),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        project.name,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                    Icon(Icons.circle, size: 7, color: statusColor),
                  ],
                ),
                const SizedBox(height: 4),
                Text(
                  project.client,
                  style: const TextStyle(color: FrankColors.muted, fontSize: 11),
                ),
                const SizedBox(height: 10),
                ClipRRect(
                  borderRadius: BorderRadius.circular(99),
                  child: LinearProgressIndicator(
                    value: project.progress,
                    minHeight: 4,
                    backgroundColor: FrankColors.border,
                    valueColor: AlwaysStoppedAnimation<Color>(statusColor),
                  ),
                ),
                const SizedBox(height: 7),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        project.statusLabel,
                        style: TextStyle(color: statusColor, fontSize: 10),
                      ),
                    ),
                    Text(
                      '${(project.progress * 100).round()}%',
                      style: const TextStyle(color: FrankColors.muted, fontSize: 10),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Text(
                  project.summary,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    color: FrankColors.ink,
                    fontSize: 11,
                    height: 1.35,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
