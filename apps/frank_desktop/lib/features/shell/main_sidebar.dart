import 'package:flutter/material.dart';
import 'package:forui/forui.dart';

import '../../app/theme.dart';
import '../../core/models/workspace_models.dart';

class MainSidebar extends StatelessWidget {
  const MainSidebar({
    required this.collapsed,
    required this.selected,
    required this.workspaceName,
    required this.onCollapse,
    required this.onSelect,
    super.key,
  });

  final bool collapsed;
  final OfficeDestination selected;
  final String workspaceName;
  final VoidCallback onCollapse;
  final ValueChanged<OfficeDestination> onSelect;

  @override
  Widget build(BuildContext context) {
    final label = collapsed ? const SizedBox.shrink() : null;
    return AnimatedContainer(
      duration: const Duration(milliseconds: 180),
      width: collapsed ? 76 : 238,
      decoration: const BoxDecoration(
        color: FrankColors.panel,
        border: Border(right: BorderSide(color: FrankColors.border)),
      ),
      child: FSidebar(
        header: _SidebarHeader(
          collapsed: collapsed,
          workspaceName: workspaceName,
          onCollapse: onCollapse,
        ),
        children: [
          FSidebarGroup(
            children: [
              _item(
                destination: OfficeDestination.office,
                icon: Icons.space_dashboard_outlined,
                title: 'Office',
              ),
              _item(
                destination: OfficeDestination.team,
                icon: Icons.groups_outlined,
                title: 'Team',
              ),
              _item(
                destination: OfficeDestination.activity,
                icon: Icons.bolt_outlined,
                title: 'Activity',
              ),
            ],
            label: label ?? const Text('Workspace'),
          ),
          FSidebarGroup(
            children: [
              _item(
                destination: OfficeDestination.ledger,
                icon: Icons.receipt_long_outlined,
                title: 'Ledger',
              ),
            ],
            label: label ?? const Text('Operations'),
          ),
        ],
        footer: _SidebarFooter(
          collapsed: collapsed,
          onSettings: () => onSelect(OfficeDestination.settings),
        ),
      ),
    );
  }

  FSidebarItem _item({
    required OfficeDestination destination,
    required IconData icon,
    required String title,
  }) {
    return FSidebarItem(
      selected: selected == destination,
      icon: Icon(icon, size: 18),
      label: Text(title),
      onPress: () => onSelect(destination),
    );
  }
}

class _SidebarHeader extends StatelessWidget {
  const _SidebarHeader({
    required this.collapsed,
    required this.workspaceName,
    required this.onCollapse,
  });

  final bool collapsed;
  final String workspaceName;
  final VoidCallback onCollapse;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 14, 10, 14),
      child: Row(
        children: [
          Container(
            width: 32,
            height: 32,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: FrankColors.amber.withValues(alpha: 0.14),
              borderRadius: BorderRadius.circular(10),
            ),
            child: const Text(
              'F',
              style: TextStyle(
                color: FrankColors.amber,
                fontWeight: FontWeight.w800,
              ),
            ),
          ),
          if (!collapsed) ...[
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'FRANK',
                    style: TextStyle(
                      color: FrankColors.ink,
                      fontSize: 12,
                      fontWeight: FontWeight.w800,
                      letterSpacing: 1.3,
                    ),
                  ),
                  Text(
                    workspaceName,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      color: FrankColors.muted,
                      fontSize: 11,
                    ),
                  ),
                ],
              ),
            ),
          ],
          IconButton(
            onPressed: onCollapse,
            tooltip: collapsed ? 'Expand sidebar' : 'Collapse sidebar',
            icon: Icon(
              collapsed ? Icons.keyboard_double_arrow_right : Icons.keyboard_double_arrow_left,
              size: 18,
              color: FrankColors.muted,
            ),
          ),
        ],
      ),
    );
  }
}

class _SidebarFooter extends StatelessWidget {
  const _SidebarFooter({required this.collapsed, required this.onSettings});

  final bool collapsed;
  final VoidCallback onSettings;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(10),
      child: Row(
        mainAxisAlignment: collapsed
            ? MainAxisAlignment.center
            : MainAxisAlignment.start,
        children: [
          IconButton(
            onPressed: onSettings,
            tooltip: 'Settings',
            icon: const Icon(Icons.settings_outlined, size: 18),
          ),
          if (!collapsed)
            const Expanded(
              child: Text(
                'Prototype workspace',
                overflow: TextOverflow.ellipsis,
                style: TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
            ),
        ],
      ),
    );
  }
}
