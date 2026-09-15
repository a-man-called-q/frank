import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:forui/forui.dart';

import '../../app/icons.dart';
import '../../app/layout/office_surface_frame.dart';
import '../../app/office_ui.dart';
import '../../app/theme.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/toolchain_models.dart';
import '../../core/models/taskboard_models.dart';
import '../../core/models/workspace_models.dart';

/// Toolchain inventory plus the approval-gated host installation flow.
class ToolchainSurface extends StatefulWidget {
  const ToolchainSurface({required this.workspace, super.key});

  final OfficeWorkspace workspace;

  @override
  State<ToolchainSurface> createState() => _ToolchainSurfaceState();
}

class _ToolchainSurfaceState extends State<ToolchainSurface> {
  Future<List<ToolchainRequirement>>? _future;
  final Set<String> _installing = <String>{};
  String? _error;

  OfficeProject? get _project {
    for (final project in widget.workspace.projects) {
      if (project.path.trim().isNotEmpty) return project;
    }
    return null;
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _future ??= context.read<FrankGateway>().loadToolchains(
      projectPath: _project?.path,
    );
  }

  Future<void> _install(ToolchainRequirement requirement) async {
    if (_installing.contains(requirement.manifestId)) return;
    final project = _project;
    if (project == null) {
      setState(
        () => _error =
            'Register a project with a canonical path before installing a toolchain.',
      );
      return;
    }
    setState(() {
      _error = null;
      _installing.add(requirement.manifestId);
    });
    try {
      final gateway = context.read<FrankGateway>();
      final taskboard = await gateway.loadTaskboard();
      final task = taskboard.tasks.firstOrNullWhere(
        (candidate) =>
            candidate.projectId == project.id &&
            candidate.agentId.isNotEmpty &&
            candidate.agentId != 'unassigned' &&
            (candidate.lane.canonical == TaskboardLane.running ||
                candidate.lane.canonical == TaskboardLane.review),
      );
      if (task == null) {
        throw StateError(
          'A Running or Review task assigned to an active agent is required for a scoped install.',
        );
      }
      final runners = await gateway.loadRunners();
      final runner = runners.firstOrNullWhere(
        (candidate) =>
            candidate.status == RunnerStatus.idle &&
            candidate.pathMappings.any(
              (mapping) =>
                  mapping.projectId == null || mapping.projectId == project.id,
            ),
      );
      if (runner == null) {
        throw StateError(
          'No idle host runner is paired for this project. Pair frank-runner first.',
        );
      }
      final plan = requirement.installPlan;
      if (plan == null) {
        throw StateError('This toolchain has no immutable installation preview.');
      }
      if (!mounted) return;
      final approved = await showFrankDialog<bool>(
        context: context,
        builder: (_) => _ToolchainApprovalDialog(
          requirement: requirement,
          plan: plan,
          task: task,
          runner: runner,
          project: project,
        ),
      );
      if (approved != true || !mounted) return;
      final approvalId = await gateway.requestToolchainApproval(
        agentId: task.agentId,
        taskId: task.id,
        operation: 'toolchain-install:${requirement.manifestId}:${plan.version}',
        cwd: project.path,
        project: project.name,
        reason:
            'Install or verify ${requirement.label} ${plan.version} on the paired host runner.',
      );
      await gateway.decideToolchainApproval(
        approvalId: approvalId,
        // Downloads and host installation are external/network effects, so a
        // task grant is intentionally not offered for this flow.
        decision: ToolchainApprovalDecision.allowOnce,
      );
      final result = await gateway.installToolchain(
        runnerId: runner.id,
        projectId: project.id,
        taskId: task.id,
        manifestId: requirement.manifestId,
        version: plan.version,
        projectPath: project.path,
        approvalId: approvalId,
      );
      if (result.status != ToolchainStatus.ready) {
        throw StateError(result.diagnostic ?? 'Toolchain installation failed.');
      }
      if (mounted) {
        setState(() {
          _future = gateway.loadToolchains(projectPath: project.path);
        });
      }
    } catch (error) {
      if (mounted) setState(() => _error = frankFriendlyError(error));
    } finally {
      if (mounted) setState(() => _installing.remove(requirement.manifestId));
    }
  }

  @override
  Widget build(BuildContext context) => OfficeSurfaceFrame.page(
    key: const ValueKey('toolchains-page-frame'),
    scrollKey: const ValueKey('settings-toolchains-scroll'),
    fullWidth: true,
    header: const OfficePageHeader(
      title: 'Runtime / Toolchains',
      description:
          'Inspect host SDK requirements and review the exact installation preview before approval.',
    ),
    slivers: [
      SliverToBoxAdapter(
        child: FutureBuilder<List<ToolchainRequirement>>(
          future: _future,
          builder: (context, snapshot) {
            if (snapshot.connectionState == ConnectionState.waiting) {
              return const _ToolchainMessage(
                icon: FrankIcons.clock,
                title: 'Loading toolchains',
                message: 'Checking the configured host runner…',
              );
            }
            if (snapshot.hasError) {
              return _ToolchainMessage(
                icon: FrankIcons.cloudOffOutlined,
                title: 'Toolchain status unavailable',
                message: frankFriendlyError(
                  snapshot.error,
                  fallback: 'The server could not inspect the host runner.',
                ),
                action: FButton(
                  onPress: () => setState(() {
                    _future = context.read<FrankGateway>().loadToolchains(
                      projectPath: _project?.path,
                    );
                  }),
                  prefix: const Icon(FrankIcons.refresh, size: 16),
                  child: const Text('Retry'),
                ),
              );
            }
            final requirements = snapshot.data ?? const [];
            if (requirements.isEmpty) {
              return const _ToolchainMessage(
                icon: FrankIcons.terminal,
                title: 'No toolchains detected',
                message:
                    'Add a local toolchain manifest or pair a host runner to inspect SDK requirements.',
              );
            }
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const FrankInlineNotice(
                  icon: FrankIcons.shieldOutlined,
                  tone: FrankStatusTone.neutral,
                  message:
                      'Installations stay user-local. Network downloads, privileged changes, and paths outside the task worktree require a separate approval.',
                ),
                const SizedBox(height: 12),
                if (_error != null) ...[
                  FrankInlineNotice(
                    icon: FrankIcons.circleAlert,
                    tone: FrankStatusTone.failure,
                    message: _error!,
                  ),
                  const SizedBox(height: 12),
                ],
                for (final requirement in requirements)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: _ToolchainCard(
                      requirement,
                      installing: _installing.contains(requirement.manifestId),
                      onInstall: () => _install(requirement),
                    ),
                  ),
              ],
            );
          },
        ),
      ),
    ],
  );
}

class _ToolchainMessage extends StatelessWidget {
  const _ToolchainMessage({
    required this.icon,
    required this.title,
    required this.message,
    this.action,
  });

  final IconData icon;
  final String title;
  final String message;
  final Widget? action;

  @override
  Widget build(BuildContext context) => FrankPanel(
    child: FrankEmptyState(
      icon: icon,
      title: title,
      description: message,
      action: action,
    ),
  );
}

class _ToolchainCard extends StatelessWidget {
  const _ToolchainCard(
    this.requirement, {
    required this.installing,
    required this.onInstall,
  });

  final ToolchainRequirement requirement;
  final bool installing;
  final VoidCallback onInstall;

  FrankStatusTone get _tone => switch (requirement.status) {
    ToolchainStatus.ready => FrankStatusTone.success,
    ToolchainStatus.needsApproval ||
    ToolchainStatus.manualRequirement => FrankStatusTone.attention,
    ToolchainStatus.installing => FrankStatusTone.working,
    ToolchainStatus.failed ||
    ToolchainStatus.incompatible => FrankStatusTone.failure,
    ToolchainStatus.missing => FrankStatusTone.neutral,
  };

  @override
  Widget build(BuildContext context) {
    final plan = requirement.installPlan;
    return FrankPanel(
      key: ValueKey('toolchain-${requirement.manifestId}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 10,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Icon(FrankIcons.terminal, size: 18, color: _tone.color),
              Text(
                requirement.label,
                style: const TextStyle(
                  color: FrankColors.ink,
                  fontWeight: FontWeight.w700,
                  fontSize: 15,
                ),
              ),
              FrankStatusBadge(
                label: installing ? ToolchainStatus.installing.label : requirement.status.label,
                tone: installing ? FrankStatusTone.working : _tone,
              ),
              Text(
                'Required ${requirement.requiredVersion}',
                style: const TextStyle(color: FrankColors.muted, fontSize: 11),
              ),
              if (requirement.detectedVersion != null)
                Text(
                  'Detected ${requirement.detectedVersion}',
                  style: const TextStyle(
                    color: FrankColors.muted,
                    fontSize: 11,
                  ),
                ),
            ],
          ),
          if (requirement.diagnostic != null) ...[
            const SizedBox(height: 8),
            Text(
              requirement.diagnostic!,
              style: const TextStyle(color: FrankColors.muted, fontSize: 12),
            ),
          ],
          if (plan != null) ...[
            const SizedBox(height: 14),
            const Text(
              'Installation preview',
              style: TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 8),
            _DetailGrid(
              details: [
                ('Source', plan.source),
                ('Version', plan.version),
                ('Download size', _size(plan.sizeBytes)),
                ('SHA-256', plan.sha256),
                ('Install path', plan.installPath),
                ('Approval scope', plan.approvalScope),
                ('Checks', plan.checks.join(', ')),
              ],
            ),
            if (!installing &&
                requirement.status != ToolchainStatus.ready &&
                requirement.status != ToolchainStatus.incompatible) ...[
              const SizedBox(height: 14),
              Align(
                alignment: Alignment.centerLeft,
                child: FButton(
                  key: ValueKey('toolchain-install-${requirement.manifestId}'),
                  onPress: onInstall,
              prefix: const Icon(FrankIcons.approval, size: 16),
                  child: Text(
                    requirement.status == ToolchainStatus.manualRequirement
                        ? 'Verify with host runner'
                        : 'Approve once & install',
                  ),
                ),
              ),
            ],
          ],
        ],
      ),
    );
  }
}

class _ToolchainApprovalDialog extends StatelessWidget {
  const _ToolchainApprovalDialog({
    required this.requirement,
    required this.plan,
    required this.task,
    required this.runner,
    required this.project,
  });

  final ToolchainRequirement requirement;
  final ToolchainInstallPlan plan;
  final TaskboardTask task;
  final RunnerInfo runner;
  final OfficeProject project;

  @override
  Widget build(BuildContext context) => FDialog(
    builder: (context, style) => Padding(
      padding: const EdgeInsets.all(20),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text('Approve toolchain install', style: style.titleTextStyle),
            const SizedBox(height: 8),
            Text(
              '${requirement.label} ${plan.version} will be installed or verified on ${runner.name}.',
            ),
            const SizedBox(height: 12),
            FrankInlineNotice(
              icon: FrankIcons.approval,
              tone: FrankStatusTone.attention,
              message:
                  'This is one approval for this exact artifact and task. Network, credentials, privileged changes, and paths outside the project remain separately protected.',
            ),
            const SizedBox(height: 12),
            _DetailGrid(
              details: [
                ('Project', project.name),
                ('Task', task.title),
                ('Runner', runner.name),
                ('Source', plan.source),
                ('Version', plan.version),
                ('Download size', _size(plan.sizeBytes)),
                ('SHA-256', plan.sha256),
                ('Install path', plan.installPath),
                ('Checks', plan.checks.join(', ')),
              ],
            ),
            const SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                FButton(
                  onPress: () => Navigator.of(context).pop(false),
                  variant: FButtonVariant.ghost,
                  child: const Text('Cancel'),
                ),
                const SizedBox(width: 8),
                FButton(
                  onPress: () => Navigator.of(context).pop(true),
                  child: const Text('Allow once & install'),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}

class _DetailGrid extends StatelessWidget {
  const _DetailGrid({required this.details});

  final List<(String, String)> details;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 26,
    runSpacing: 12,
    children: [
      for (final (label, value) in details)
        SizedBox(
          width: 250,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                label,
                style: const TextStyle(
                  color: FrankColors.muted,
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: .4,
                ),
              ),
              const SizedBox(height: 3),
              Text(value),
            ],
          ),
        ),
    ],
  );
}

String _size(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KiB';
  return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MiB';
}

extension _ToolchainFirstOrNullWhere<T> on Iterable<T> {
  T? firstOrNullWhere(bool Function(T value) test) {
    for (final value in this) {
      if (test(value)) return value;
    }
    return null;
  }
}
