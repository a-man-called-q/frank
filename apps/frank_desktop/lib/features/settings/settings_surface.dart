import 'package:flutter/material.dart';

import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';

class SettingsSurface extends StatelessWidget {
  const SettingsSurface({
    required this.section,
    required this.workspace,
    super.key,
  });

  final SettingsSection section;
  final OfficeWorkspace workspace;

  @override
  Widget build(BuildContext context) {
    final title = switch (section) {
      SettingsSection.projects => 'Projects',
      SettingsSection.team => 'Team',
      SettingsSection.activity => 'Activity',
      SettingsSection.ledger => 'Ledger',
    };
    final description = switch (section) {
      SettingsSection.projects =>
        'Review active and archived projects for Frank Agency.',
      SettingsSection.team => 'Manage the agents available to Frank Agency.',
      SettingsSection.activity =>
        'Review important events across projects and missions.',
      SettingsSection.ledger =>
        'Inspect usage, budget, and attribution for the agency.',
    };

    return ColoredBox(
      color: FrankColors.canvas,
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 760),
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: const TextStyle(color: FrankColors.ink, fontSize: 24),
                ),
                const SizedBox(height: 8),
                Text(
                  description,
                  style: const TextStyle(color: FrankColors.muted),
                ),
                const SizedBox(height: 24),
                if (section == SettingsSection.projects)
                  ProjectsSettingsCard(workspace: workspace)
                else
                  SettingsPlaceholderCard(
                    section: section,
                    workspace: workspace,
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class SettingsPlaceholderCard extends StatelessWidget {
  const SettingsPlaceholderCard({
    required this.section,
    required this.workspace,
    super.key,
  });

  final SettingsSection section;
  final OfficeWorkspace workspace;

  @override
  Widget build(BuildContext context) {
    final (icon, message) = switch (section) {
      SettingsSection.projects => (
        FrankIcons.folder,
        'Project management will be connected to Frank actions in the next milestone.',
      ),
      SettingsSection.team => (
        FrankIcons.users,
        '${workspace.employees.length} fixture agents are available for the next milestone.',
      ),
      SettingsSection.activity => (
        FrankIcons.activity,
        'The agency activity stream will be connected to frankd in the next milestone.',
      ),
      SettingsSection.ledger => (
        FrankIcons.ledger,
        'Measured usage and cost attribution will appear here once the ledger transport is connected.',
      ),
    };
    return Container(
      padding: const EdgeInsets.all(20),
      decoration: BoxDecoration(
        color: FrankColors.panel,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: FrankColors.border),
      ),
      child: Row(
        children: [
          Icon(icon, color: FrankColors.amber),
          const SizedBox(width: 14),
          Expanded(
            child: Text(
              message,
              style: const TextStyle(color: FrankColors.ink, height: 1.4),
            ),
          ),
        ],
      ),
    );
  }
}

class ProjectsSettingsCard extends StatelessWidget {
  const ProjectsSettingsCard({required this.workspace, super.key});

  final OfficeWorkspace workspace;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const Text(
          'Active Projects',
          style: TextStyle(color: FrankColors.amber, fontSize: 13),
        ),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: FrankColors.panel,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: FrankColors.border),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final project in workspace.projects)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 6),
                  child: Row(
                    children: [
                      const Icon(
                        FrankIcons.folder,
                        size: 17,
                        color: FrankColors.muted,
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Text(
                          project.name,
                          style: const TextStyle(color: FrankColors.ink),
                        ),
                      ),
                      Text(
                        project.statusLabel,
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontSize: 12,
                        ),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 24),
        const Text(
          'Archived Projects',
          style: TextStyle(color: FrankColors.amber, fontSize: 13),
        ),
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.all(20),
          decoration: BoxDecoration(
            color: FrankColors.panel,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: FrankColors.border),
          ),
          child: const Row(
            children: [
              Icon(FrankIcons.archive, color: FrankColors.amber),
              SizedBox(width: 14),
              Expanded(
                child: Text(
                  'No archived projects yet. Persistence will be connected in a future milestone.',
                  style: TextStyle(color: FrankColors.ink, height: 1.4),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class NoProjectSurface extends StatelessWidget {
  const NoProjectSurface({super.key});

  @override
  Widget build(BuildContext context) {
    return const ColoredBox(
      color: FrankColors.canvas,
      child: Center(
        child: Text(
          'No projects yet',
          style: TextStyle(color: FrankColors.muted, fontSize: 18),
        ),
      ),
    );
  }
}
