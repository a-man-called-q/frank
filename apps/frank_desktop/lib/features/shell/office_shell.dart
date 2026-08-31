import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flow_ui/flow_ui.dart';

import '../../core/gateway/frank_gateway.dart';
import '../../core/models/workspace_models.dart';
import '../chat/account_chat.dart';
import '../floor/empty_office_floor.dart';
import '../projects/projects_drawer.dart';
import 'main_sidebar.dart';

class OfficeShell extends StatefulWidget {
  const OfficeShell({required this.gateway, super.key});

  final FrankGateway gateway;

  @override
  State<OfficeShell> createState() => _OfficeShellState();
}

class _OfficeShellState extends State<OfficeShell> {
  OfficeWorkspace? _workspace;
  OfficeDestination _destination = OfficeDestination.office;
  String? _selectedProjectId;
  bool _projectsOpen = true;
  bool _sidebarCollapsed = false;
  bool _loading = true;
  String? _loadError;
  bool _generating = false;
  String? _pendingReplyId;
  StreamSubscription<String>? _replySubscription;
  List<FlowMessageData> _messages = const [];

  @override
  void initState() {
    super.initState();
    _loadWorkspace();
  }

  @override
  void dispose() {
    _replySubscription?.cancel();
    super.dispose();
  }

  Future<void> _loadWorkspace() async {
    try {
      final workspace = await widget.gateway.loadWorkspace();
      if (!mounted) return;
      setState(() {
        _workspace = workspace;
        _selectedProjectId = workspace.projects.isEmpty
            ? null
            : workspace.projects.first.id;
        _messages = workspace.messages.map(_toFlowMessage).toList();
        _loading = false;
      });
    } on Object catch (error) {
      if (!mounted) return;
      setState(() {
        _loadError = error.toString();
        _loading = false;
      });
    }
  }

  FlowMessageData _toFlowMessage(OfficeMessage message) {
    return FlowMessageData.text(
      id: message.id,
      role: message.role == ChatRole.user
          ? FlowMessageRole.user
          : FlowMessageRole.assistant,
      text: message.text,
    ).copyWith(status: message.status);
  }

  Future<void> _sendMessage(String value) async {
    final text = value.trim();
    final workspace = _workspace;
    if (text.isEmpty || workspace == null || _generating) return;

    final messageId = DateTime.now().microsecondsSinceEpoch.toString();
    final replyId = '$messageId-reply';
    setState(() {
      _messages = [
        ..._messages,
        FlowMessageData.text(
          id: messageId,
          role: FlowMessageRole.user,
          text: text,
        ),
        FlowMessageData(
          id: replyId,
          role: FlowMessageRole.assistant,
          status: FlowMessageStatus.pending,
        ),
      ];
      _pendingReplyId = replyId;
      _generating = true;
    });

    var streamed = '';
    await _replySubscription?.cancel();
    _replySubscription = widget.gateway
        .replyTo(text, projectId: _selectedProjectId ?? 'unassigned')
        .listen(
          (chunk) {
            if (!mounted || _pendingReplyId != replyId) return;
            streamed += chunk;
            _replacePendingReply(
              replyId,
              _messages.last.copyWith(
                parts: [FlowTextPart(streamed)],
                status: FlowMessageStatus.streaming,
              ),
            );
          },
          onError: (Object error, StackTrace stackTrace) {
            if (!mounted || _pendingReplyId != replyId) return;
            _replacePendingReply(
              replyId,
              _messages.last.copyWith(
                parts: [FlowTextPart('The fixture could not respond: $error')],
                status: FlowMessageStatus.error,
              ),
            );
            setState(() {
              _pendingReplyId = null;
              _generating = false;
            });
          },
          onDone: () {
            if (!mounted || _pendingReplyId != replyId) return;
            setState(() {
              _messages = [
                ..._messages.take(_messages.length - 1),
                _messages.last.copyWith(status: FlowMessageStatus.complete),
              ];
              _pendingReplyId = null;
              _generating = false;
            });
          },
          cancelOnError: false,
        );
  }

  void _replacePendingReply(String replyId, FlowMessageData replacement) {
    if (_messages.isEmpty || _messages.last.id != replyId) return;
    setState(() {
      _messages = [..._messages.take(_messages.length - 1), replacement];
    });
  }

  Future<void> _stopMessage() async {
    await _replySubscription?.cancel();
    if (!mounted || _pendingReplyId == null) return;
    setState(() {
      _messages = [
        ..._messages.take(_messages.length - 1),
        _messages.last.copyWith(
          parts: const [FlowTextPart('Response stopped.')],
          status: FlowMessageStatus.complete,
        ),
      ];
      _pendingReplyId = null;
      _generating = false;
    });
  }

  void _selectProject(String id) {
    setState(() {
      _selectedProjectId = id;
    });
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) return const _LoadingShell();
    if (_loadError != null || _workspace == null) {
      return _ErrorShell(error: _loadError ?? 'Workspace unavailable');
    }

    final workspace = _workspace!;
    final selectedProject = workspace.projects.firstWhere(
      (project) => project.id == _selectedProjectId,
      orElse: () => workspace.projects.first,
    );

    return Scaffold(
      body: SafeArea(
        child: Row(
          children: [
            MainSidebar(
              collapsed: _sidebarCollapsed,
              selected: _destination,
              workspaceName: workspace.name,
              onCollapse: () => setState(() {
                _sidebarCollapsed = !_sidebarCollapsed;
              }),
              onSelect: (destination) => setState(() {
                _destination = destination;
              }),
            ),
            Expanded(
              child: _destination == OfficeDestination.office
                  ? _OfficeHome(
                      workspace: workspace,
                      messages: _messages,
                      generating: _generating,
                      selectedProject: selectedProject,
                      projectsOpen: _projectsOpen,
                      onToggleProjects: () => setState(() {
                        _projectsOpen = !_projectsOpen;
                      }),
                      onSend: _sendMessage,
                      onStop: _stopMessage,
                    )
                  : _EmptyDestination(destination: _destination),
            ),
            if (_projectsOpen && _destination == OfficeDestination.office)
              ProjectsDrawer(
                projects: workspace.projects,
                selectedProjectId: selectedProject.id,
                onSelect: _selectProject,
                onClose: () => setState(() {
                  _projectsOpen = false;
                }),
              ),
          ],
        ),
      ),
    );
  }
}

class _OfficeHome extends StatelessWidget {
  const _OfficeHome({
    required this.workspace,
    required this.messages,
    required this.generating,
    required this.selectedProject,
    required this.projectsOpen,
    required this.onToggleProjects,
    required this.onSend,
    required this.onStop,
  });

  final OfficeWorkspace workspace;
  final List<FlowMessageData> messages;
  final bool generating;
  final OfficeProject selectedProject;
  final bool projectsOpen;
  final VoidCallback onToggleProjects;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _TopBar(
          project: selectedProject,
          projectsOpen: projectsOpen,
          onToggleProjects: onToggleProjects,
        ),
        Expanded(
          child: Column(
            children: [
              const SizedBox(
                height: 156,
                child: EmptyOfficeFloor(),
              ),
              Expanded(
                child: AccountExecutiveChat(
                  executive: workspace.accountExecutive,
                  project: selectedProject,
                  messages: messages,
                  generating: generating,
                  onSend: onSend,
                  onStop: onStop,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _TopBar extends StatelessWidget {
  const _TopBar({
    required this.project,
    required this.projectsOpen,
    required this.onToggleProjects,
  });

  final OfficeProject project;
  final bool projectsOpen;
  final VoidCallback onToggleProjects;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Container(
      height: 72,
      padding: const EdgeInsets.symmetric(horizontal: 24),
      decoration: BoxDecoration(
        color: colors.surface,
        border: Border(bottom: BorderSide(color: colors.outlineVariant)),
      ),
      child: Row(
        children: [
          const Icon(Icons.auto_awesome, size: 18, color: Color(0xFFE2A84B)),
          const SizedBox(width: 12),
          const Text(
            'Office',
            style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
          ),
          const SizedBox(width: 16),
          Container(
            width: 1,
            height: 22,
            color: colors.outlineVariant,
          ),
          const SizedBox(width: 16),
          Flexible(
            child: Text(
              project.name,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: colors.onSurfaceVariant),
            ),
          ),
          const SizedBox(width: 16),
          _StatusPill(label: 'Local prototype', color: const Color(0xFF77C69B)),
          const SizedBox(width: 12),
          IconButton(
            onPressed: onToggleProjects,
            tooltip: projectsOpen ? 'Hide projects' : 'Show projects',
            icon: Icon(
              projectsOpen ? Icons.view_sidebar_outlined : Icons.view_sidebar,
            ),
          ),
        ],
      ),
    );
  }
}

class _StatusPill extends StatelessWidget {
  const _StatusPill({required this.label, required this.color});

  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(99),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.circle, size: 7, color: color),
          const SizedBox(width: 6),
          Text(
            label,
            style: TextStyle(
              color: color,
              fontSize: 11,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}

class _EmptyDestination extends StatelessWidget {
  const _EmptyDestination({required this.destination});

  final OfficeDestination destination;

  @override
  Widget build(BuildContext context) {
    final title = switch (destination) {
      OfficeDestination.team => 'Team',
      OfficeDestination.activity => 'Activity',
      OfficeDestination.ledger => 'Ledger',
      OfficeDestination.settings => 'Settings',
      OfficeDestination.office => 'Office',
    };
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.construction_outlined, size: 32, color: Colors.amber[300]),
          const SizedBox(height: 14),
          Text(title, style: const TextStyle(fontSize: 20)),
          const SizedBox(height: 6),
          Text(
            'This surface is reserved for the next agency milestone.',
            style: TextStyle(color: Theme.of(context).colorScheme.onSurfaceVariant),
          ),
        ],
      ),
    );
  }
}

class _LoadingShell extends StatelessWidget {
  const _LoadingShell();

  @override
  Widget build(BuildContext context) {
    return const Scaffold(
      body: Center(child: CircularProgressIndicator()),
    );
  }
}

class _ErrorShell extends StatelessWidget {
  const _ErrorShell({required this.error});

  final String error;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Text('Could not load the local workspace.\n$error'),
      ),
    );
  }
}
