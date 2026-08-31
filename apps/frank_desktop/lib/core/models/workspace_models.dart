import 'package:flow_ui/flow_ui.dart';

enum OfficeDestination { office, team, activity, ledger, settings }

enum ProjectStatus { planning, active, review, delivered }

enum ChatRole { user, assistant }

class OfficeProject {
  const OfficeProject({
    required this.id,
    required this.name,
    required this.client,
    required this.status,
    required this.progress,
    required this.team,
    required this.summary,
  });

  final String id;
  final String name;
  final String client;
  final ProjectStatus status;
  final double progress;
  final List<String> team;
  final String summary;

  String get statusLabel => switch (status) {
        ProjectStatus.planning => 'Planning',
        ProjectStatus.active => 'Active',
        ProjectStatus.review => 'Review',
        ProjectStatus.delivered => 'Delivered',
      };
}

class OfficeEmployee {
  const OfficeEmployee({
    required this.id,
    required this.name,
    required this.role,
    required this.status,
    required this.initials,
    required this.color,
  });

  final String id;
  final String name;
  final String role;
  final String status;
  final String initials;
  final int color;
}

class OfficeMessage {
  const OfficeMessage({
    required this.id,
    required this.role,
    required this.text,
    this.status = FlowMessageStatus.complete,
  });

  final String id;
  final ChatRole role;
  final String text;
  final FlowMessageStatus status;
}

class OfficeWorkspace {
  const OfficeWorkspace({
    required this.name,
    required this.projects,
    required this.employees,
    required this.messages,
    required this.accountExecutive,
  });

  final String name;
  final List<OfficeProject> projects;
  final List<OfficeEmployee> employees;
  final List<OfficeMessage> messages;
  final OfficeEmployee accountExecutive;
}
