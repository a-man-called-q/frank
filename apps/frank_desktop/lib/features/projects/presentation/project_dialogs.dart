import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import '../../../app/icons.dart';
import '../../../app/office_ui.dart';
import '../../../app/theme.dart';
import '../../../core/models/project_models.dart';

sealed class ProjectSetupResult {
  const ProjectSetupResult();
}

final class RegisterProjectResult extends ProjectSetupResult {
  const RegisterProjectResult(this.draft);

  final ProjectRegistrationDraft draft;
}

final class CloneProjectResult extends ProjectSetupResult {
  const CloneProjectResult(this.draft);

  final ProjectCloneDraft draft;
}

class AddProjectDialog extends StatefulWidget {
  const AddProjectDialog({this.browseDirectories, super.key});

  final Future<List<ProjectDirectoryEntry>> Function(String? path)?
  browseDirectories;

  @override
  State<AddProjectDialog> createState() => _AddProjectDialogState();
}

class _AddProjectDialogState extends State<AddProjectDialog> {
  final _name = TextEditingController();
  final _path = TextEditingController();
  final _branch = TextEditingController(text: 'main');
  final _url = TextEditingController();
  final _destination = TextEditingController();
  bool _clone = false;
  String? _error;
  List<ProjectDirectoryEntry> _directories = const [];
  bool _browsing = false;
  bool _nameEdited = false;
  bool _syncingName = false;

  @override
  void initState() {
    super.initState();
    _path.addListener(_syncNameFromPath);
    _name.addListener(_markNameEdited);
  }

  @override
  void dispose() {
    _path.removeListener(_syncNameFromPath);
    _name.removeListener(_markNameEdited);
    _name.dispose();
    _path.dispose();
    _branch.dispose();
    _url.dispose();
    _destination.dispose();
    super.dispose();
  }

  void _markNameEdited() {
    if (!_syncingName) _nameEdited = true;
  }

  void _syncNameFromPath() {
    if (_clone || _nameEdited) return;
    final basename = _basename(_path.text);
    _syncingName = true;
    _name.value = _name.value.copyWith(text: basename);
    _syncingName = false;
  }

  Future<void> _browse([String? path]) async {
    final loader = widget.browseDirectories;
    if (loader == null || _browsing) return;
    setState(() {
      _browsing = true;
      _error = null;
    });
    try {
      final entries = await loader(path);
      if (!mounted) return;
      setState(() {
        _directories = entries.where((entry) => entry.directory).toList();
        if (path != null && path.trim().isNotEmpty) {
          _path.text = path;
        }
      });
    } catch (error) {
      if (!mounted) return;
      setState(
        () => _error =
            'Server directories are unavailable: ${frankFriendlyError(error, fallback: 'Configure the frankd project root and try again.')}',
      );
    } finally {
      if (mounted) setState(() => _browsing = false);
    }
  }

  void _submit() {
    if (_clone) {
      final url = _url.text.trim();
      final destination = _destination.text.trim();
      if (url.isEmpty || destination.isEmpty) {
        setState(
          () => _error = 'Repository URL and server destination are required.',
        );
        return;
      }
      Navigator.pop(
        context,
        CloneProjectResult(
          ProjectCloneDraft(url: url, destination: destination),
        ),
      );
      return;
    }
    final path = _path.text.trim();
    final name = _name.text.trim().isEmpty
        ? _basename(path)
        : _name.text.trim();
    if (path.isEmpty || name.isEmpty) {
      setState(() => _error = 'Choose a server project directory and name.');
      return;
    }
    Navigator.pop(
      context,
      RegisterProjectResult(
        ProjectRegistrationDraft(
          name: name,
          path: path,
          baseBranch: _branch.text.trim().isEmpty
              ? 'main'
              : _branch.text.trim(),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => FDialog(
    builder: (context, style) => _dialogLayout(
      title: Text('Add project', style: style.titleTextStyle),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text(
            'Project files live on the frankd host. Choose how to add them.',
            style: TextStyle(color: FrankColors.muted),
          ),
          const SizedBox(height: 14),
          Row(
            children: [
              Expanded(
                child: FButton(
                  onPress: () => setState(() {
                    _clone = false;
                    _error = null;
                  }),
                  variant: _clone
                      ? FButtonVariant.outline
                      : FButtonVariant.primary,
                  child: const Text('Register existing'),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: FButton(
                  onPress: () => setState(() {
                    _clone = true;
                    _error = null;
                  }),
                  variant: _clone
                      ? FButtonVariant.primary
                      : FButtonVariant.outline,
                  child: const Text('Clone repository'),
                ),
              ),
            ],
          ),
          const SizedBox(height: 14),
          if (_clone) ...[
            FTextField(
              control: FTextFieldControl.managed(controller: _url),
              autofocus: true,
              label: const Text('Repository URL'),
              hint: 'https://github.com/org/repo.git',
            ),
            const SizedBox(height: 10),
            FTextField(
              control: FTextFieldControl.managed(controller: _destination),
              label: const Text('Server destination'),
              hint: '/srv/frank/projects/repo',
            ),
          ] else ...[
            FButton(
              onPress: _browsing
                  ? null
                  : widget.browseDirectories == null
                  ? () => setState(
                      () => _error =
                          'Configure the Frank server project root before browsing directories.',
                    )
                  : () => _browse(
                      _path.text.trim().isEmpty ? null : _path.text.trim(),
                    ),
              variant: FButtonVariant.outline,
              prefix: const Icon(FrankIcons.folderOpen, size: 15),
              child: Text(
                _browsing ? 'Browsing server…' : 'Browse server directories',
              ),
              semanticsTooltip: widget.browseDirectories == null
                  ? 'Configure the Frank server before browsing directories'
                  : 'Browse directories on the frankd host',
            ),
            if (_directories.isNotEmpty) ...[
              const SizedBox(height: 8),
              for (final entry in _directories)
                FButton(
                  onPress: () => _browse(
                    '${_path.text.trim().replaceAll(RegExp(r'[/\\]+$'), '')}/${entry.name}',
                  ),
                  variant: FButtonVariant.ghost,
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: Text(entry.name),
                  ),
                ),
            ],
            const SizedBox(height: 10),
            FTextField(
              control: FTextFieldControl.managed(controller: _path),
              autofocus: true,
              label: const Text('Server project directory'),
              hint: '/srv/frank/projects/repo',
            ),
            const SizedBox(height: 10),
            FTextField(
              control: FTextFieldControl.managed(controller: _name),
              label: const Text('Project name'),
              hint: _basename(_path.text),
            ),
            const SizedBox(height: 10),
            FTextField(
              control: FTextFieldControl.managed(controller: _branch),
              label: const Text('Base branch'),
            ),
          ],
          if (_error case final error?) ...[
            const SizedBox(height: 10),
            FrankActionFeedback(message: error, tone: FrankStatusTone.failure),
          ],
        ],
      ),
      actions: _dialogActions(
        cancel: () => Navigator.of(context).pop(),
        confirm: _submit,
        label: _clone ? 'Start clone' : 'Register project',
      ),
    ),
  );
}

class CreateMissionDialog extends StatefulWidget {
  const CreateMissionDialog({required this.projectName, super.key});

  final String projectName;

  @override
  State<CreateMissionDialog> createState() => _CreateMissionDialogState();
}

class _CreateMissionDialogState extends State<CreateMissionDialog> {
  final _objective = TextEditingController();
  String? _error;

  @override
  void dispose() {
    _objective.dispose();
    super.dispose();
  }

  void _submit() {
    final objective = _objective.text.trim();
    if (objective.isEmpty) {
      setState(() => _error = 'Objective is required.');
      return;
    }
    Navigator.pop(context, objective);
  }

  @override
  Widget build(BuildContext context) => FDialog(
    builder: (context, style) => _dialogLayout(
      title: Text('New mission', style: style.titleTextStyle),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Create a mission for ${widget.projectName}.'),
          const SizedBox(height: 12),
          FTextField(
            control: FTextFieldControl.managed(controller: _objective),
            autofocus: true,
            label: const Text('Objective'),
            hint: 'Describe the outcome you want the team to deliver',
            maxLines: 3,
            onSubmit: (_) => _submit(),
          ),
          if (_error case final error?) ...[
            const SizedBox(height: 10),
            FrankActionFeedback(message: error, tone: FrankStatusTone.failure),
          ],
        ],
      ),
      actions: _dialogActions(
        cancel: () => Navigator.of(context).pop(),
        confirm: _submit,
        label: 'Create mission',
      ),
    ),
  );
}

String _basename(String path) {
  final trimmed = path.trim().replaceAll(RegExp(r'[/\\]+$'), '');
  if (trimmed.isEmpty) return '';
  return trimmed.split(RegExp(r'[/\\]')).last;
}

class RenameProjectDialog extends StatefulWidget {
  const RenameProjectDialog({required this.projectName, super.key});

  final String projectName;

  @override
  State<RenameProjectDialog> createState() => _RenameProjectDialogState();
}

class _RenameProjectDialogState extends State<RenameProjectDialog> {
  late final TextEditingController _controller;
  String? _error;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.projectName);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final value = _controller.text.trim();
    if (value.isEmpty) {
      setState(() => _error = 'Project name cannot be empty.');
      return;
    }
    Navigator.of(context).pop(value);
  }

  @override
  Widget build(BuildContext context) {
    return FDialog(
      builder: (context, style) => _dialogLayout(
        title: Text('Rename project', style: style.titleTextStyle),
        content: FTextField(
          control: FTextFieldControl.managed(
            controller: _controller,
            onChange: (_) {
              if (_error != null) setState(() => _error = null);
            },
          ),
          autofocus: true,
          label: const Text('Project name'),
          error: _error == null ? null : Text(_error!),
          onSubmit: (_) => _submit(),
        ),
        actions: _dialogActions(
          cancel: () => Navigator.of(context).pop(),
          confirm: _submit,
          label: 'Rename',
        ),
      ),
    );
  }
}

class RenameMissionDialog extends StatefulWidget {
  const RenameMissionDialog({required this.missionTitle, super.key});

  final String missionTitle;

  @override
  State<RenameMissionDialog> createState() => _RenameMissionDialogState();
}

class _RenameMissionDialogState extends State<RenameMissionDialog> {
  late final TextEditingController _controller;
  String? _error;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.missionTitle);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final value = _controller.text.trim();
    if (value.isEmpty) {
      setState(() => _error = 'Mission name cannot be empty.');
      return;
    }
    Navigator.of(context).pop(value);
  }

  @override
  Widget build(BuildContext context) {
    return FDialog(
      builder: (context, style) => _dialogLayout(
        title: Text('Rename mission', style: style.titleTextStyle),
        content: FTextField(
          control: FTextFieldControl.managed(
            controller: _controller,
            onChange: (_) {
              if (_error != null) setState(() => _error = null);
            },
          ),
          autofocus: true,
          label: const Text('Mission name'),
          error: _error == null ? null : Text(_error!),
          onSubmit: (_) => _submit(),
        ),
        actions: _dialogActions(
          cancel: () => Navigator.of(context).pop(),
          confirm: _submit,
          label: 'Rename',
        ),
      ),
    );
  }
}

class ProjectConfirmationDialog extends StatelessWidget {
  const ProjectConfirmationDialog({
    required this.title,
    required this.message,
    required this.confirmLabel,
    this.destructive = false,
    super.key,
  });

  final String title;
  final String message;
  final String confirmLabel;
  final bool destructive;

  @override
  Widget build(BuildContext context) {
    return FDialog(
      builder: (context, style) => _dialogLayout(
        title: Text(title, style: style.titleTextStyle),
        content: Text(message),
        actions: [
          FButton(
            onPress: () => Navigator.of(context).pop(false),
            variant: FButtonVariant.ghost,
            child: const Text('Cancel'),
          ),
          FButton(
            onPress: () => Navigator.of(context).pop(true),
            variant: destructive
                ? FButtonVariant.destructive
                : FButtonVariant.primary,
            child: Text(confirmLabel),
          ),
        ],
      ),
    );
  }
}

class LoadingShell extends StatelessWidget {
  const LoadingShell({super.key});

  @override
  Widget build(BuildContext context) {
    return const FScaffold(child: Center(child: FCircularProgress()));
  }
}

class ErrorShell extends StatelessWidget {
  const ErrorShell({required this.error, this.onRetry, super.key});

  final String error;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final friendlyError = frankFriendlyError(
      error,
      fallback: 'The local workspace could not be loaded.',
    );
    return FScaffold(
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              'Could not load the local workspace.\n$friendlyError',
              textAlign: TextAlign.center,
            ),
            if (onRetry != null) ...[
              const SizedBox(height: 16),
              FButton(onPress: onRetry, child: const Text('Retry')),
            ],
          ],
        ),
      ),
    );
  }
}

Widget _dialogLayout({
  required Widget title,
  required Widget content,
  required List<Widget> actions,
}) => Padding(
  padding: const EdgeInsets.all(20),
  child: Column(
    mainAxisSize: MainAxisSize.min,
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      title,
      const SizedBox(height: 16),
      content,
      const SizedBox(height: 20),
      Row(mainAxisAlignment: MainAxisAlignment.end, children: actions),
    ],
  ),
);

List<Widget> _dialogActions({
  required VoidCallback cancel,
  required VoidCallback confirm,
  required String label,
}) => [
  FButton(
    onPress: cancel,
    variant: FButtonVariant.ghost,
    child: const Text('Cancel'),
  ),
  const SizedBox(width: 8),
  FButton(onPress: confirm, child: Text(label)),
];
