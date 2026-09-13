part of 'package:frank_desktop/features/shell/main_sidebar.dart';

class _SidebarFooter extends StatefulWidget {
  const _SidebarFooter({
    required this.dense,
    this.authRepository,
    this.onLogout,
    this.onLogoutAll,
    this.onChangePassword,
  });

  final bool dense;
  final AuthRepository? authRepository;
  final Future<void> Function()? onLogout;
  final Future<void> Function()? onLogoutAll;
  final Future<void> Function(String currentPassword, String newPassword)?
  onChangePassword;

  @override
  State<_SidebarFooter> createState() => _SidebarFooterState();
}

class _SidebarFooterState extends State<_SidebarFooter> {
  late final FrankDesktopMenuController _accountController;

  @override
  void initState() {
    super.initState();
    _accountController = FrankDesktopMenuController();
  }

  void _toggleAccount() {
    _accountController.toggle();
  }

  @override
  Widget build(BuildContext context) {
    final height = widget.dense ? 32.0 : 38.0;
    if (widget.authRepository == null) return _legacyFooter(height);
    final username =
        widget.authRepository?.currentSession?.owner.username ?? 'Owner';
    final storageWarning = widget.authRepository?.storageWarning;
    return Padding(
      padding: EdgeInsets.all(widget.dense ? 8 : 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (storageWarning case final warning?) ...[
            Tooltip(
              message: warning,
              child: Semantics(
                container: true,
                label: 'Secure storage notice',
                value: warning,
                child: Row(
                  children: [
                    const Icon(
                      Icons.lock_outline,
                      size: 12,
                      color: FrankColors.warningAmber,
                    ),
                    const SizedBox(width: 5),
                    const Expanded(
                      child: Text(
                        'Secure storage',
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(color: FrankColors.muted, fontSize: 9),
                      ),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 6),
          ],
          FrankDesktopMenu(
            key: const ValueKey('sidebar-user-menu'),
            controller: _accountController,
            kind: FrankDesktopMenuKind.account,
            matchTriggerWidth: true,
            groups: _accountMenu(),
            semanticsLabel: 'Account settings',
            child: Tooltip(
              message: 'Account settings',
              child: Semantics(
                key: const ValueKey('sidebar-user-button'),
                container: true,
                button: true,
                label: 'Account $username',
                hint: 'Open account settings',
                child: OutlinedButton(
                  onPressed: _toggleAccount,
                  style: OutlinedButton.styleFrom(
                    alignment: Alignment.centerLeft,
                    minimumSize: Size.zero,
                    fixedSize: Size.fromHeight(height),
                    padding: EdgeInsets.symmetric(
                      horizontal: widget.dense ? 8 : 10,
                    ),
                    backgroundColor: FrankColors.panelRaised.withValues(
                      alpha: .28,
                    ),
                    foregroundColor: FrankColors.ink,
                    side: BorderSide(
                      color: FrankColors.border.withValues(alpha: .8),
                    ),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(8),
                    ),
                  ),
                  child: Row(
                    children: [
                      ExcludeSemantics(
                        child: CircleAvatar(
                          radius: widget.dense ? 10 : 11,
                          backgroundColor: FrankColors.aubergine.withValues(
                            alpha: .26,
                          ),
                          child: Text(
                            username.characters.first.toUpperCase(),
                            style: TextStyle(
                              color: FrankColors.ink,
                              fontSize: widget.dense ? 11 : 12,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                      ),
                      SizedBox(width: widget.dense ? 7 : 8),
                      Flexible(
                        child: Text(
                          username,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      const SizedBox(width: 4),
                      const Icon(FrankIcons.more, size: 15),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  List<FrankMenuGroup> _accountMenu() {
    VoidCallback invoke(_AccountAction action) => () {
      unawaited(_handleAction(context, action));
    };

    return [
      FrankMenuGroup([
        FrankMenuItem(
          key: const ValueKey('sidebar-account-action-change-password'),
          label: 'Change password',
          icon: FrankIcons.keyRound,
          enabled: widget.onChangePassword != null,
          onPressed: widget.onChangePassword == null
              ? null
              : invoke(_AccountAction.changePassword),
        ),
        FrankMenuItem(
          key: const ValueKey('sidebar-account-action-logout-all'),
          label: 'Log out all devices',
          icon: FrankIcons.monitorSmartphone,
          enabled: widget.onLogoutAll != null,
          onPressed: widget.onLogoutAll == null
              ? null
              : invoke(_AccountAction.logoutAll),
        ),
      ]),
      FrankMenuGroup([
        FrankMenuItem(
          key: const ValueKey('sidebar-account-action-logout'),
          label: 'Log out',
          icon: FrankIcons.logOut,
          destructive: true,
          enabled: widget.onLogout != null,
          onPressed: widget.onLogout == null
              ? null
              : invoke(_AccountAction.logout),
        ),
      ]),
    ];
  }

  Widget _legacyFooter(double height) => Padding(
    padding: EdgeInsets.all(widget.dense ? 8 : 12),
    child: Tooltip(
      message: 'User profile is not available yet',
      child: Semantics(
        key: const ValueKey('sidebar-user-button'),
        container: true,
        button: true,
        enabled: false,
        label: 'User',
        hint: 'User profile is not available yet',
        child: OutlinedButton(
          onPressed: null,
          style: OutlinedButton.styleFrom(
            alignment: Alignment.centerLeft,
            minimumSize: Size.zero,
            fixedSize: Size.fromHeight(height),
            padding: EdgeInsets.symmetric(horizontal: widget.dense ? 8 : 10),
            backgroundColor: FrankColors.panelRaised.withValues(alpha: .28),
            disabledForegroundColor: FrankColors.muted,
            side: BorderSide(color: FrankColors.border.withValues(alpha: .8)),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(8),
            ),
          ),
          child: Row(
            children: [
              ExcludeSemantics(
                child: CircleAvatar(
                  radius: widget.dense ? 10 : 11,
                  backgroundColor: FrankColors.aubergine.withValues(alpha: .26),
                  child: Text(
                    'U',
                    style: TextStyle(
                      color: FrankColors.ink,
                      fontSize: widget.dense ? 11 : 12,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                ),
              ),
              SizedBox(width: widget.dense ? 7 : 8),
              const Flexible(
                child: Text(
                  'User',
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 12),
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );

  Future<void> _handleAction(
    BuildContext context,
    _AccountAction action,
  ) async {
    switch (action) {
      case _AccountAction.logout:
        await widget.onLogout?.call();
      case _AccountAction.logoutAll:
        await widget.onLogoutAll?.call();
      case _AccountAction.changePassword:
        final callback = widget.onChangePassword;
        if (callback == null || !context.mounted) return;
        final values = await showDialog<(String, String)>(
          context: context,
          builder: (_) => const _ChangePasswordDialog(),
        );
        if (values == null || !context.mounted) return;
        try {
          await callback(values.$1, values.$2);
        } on Object catch (error) {
          if (!context.mounted) return;
          ScaffoldMessenger.of(
            context,
          ).showSnackBar(SnackBar(content: Text(error.toString())));
        }
    }
  }
}

class _ChangePasswordDialog extends StatefulWidget {
  const _ChangePasswordDialog();

  @override
  State<_ChangePasswordDialog> createState() => _ChangePasswordDialogState();
}

class _ChangePasswordDialogState extends State<_ChangePasswordDialog> {
  final _formKey = GlobalKey<FormState>();
  final _current = TextEditingController();
  final _next = TextEditingController();

  @override
  void dispose() {
    _current.dispose();
    _next.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Change password'),
    content: Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextFormField(
            controller: _current,
            obscureText: true,
            decoration: const InputDecoration(labelText: 'Current password'),
            validator: (value) => value == null || value.isEmpty
                ? 'Enter your current password'
                : null,
          ),
          TextFormField(
            controller: _next,
            obscureText: true,
            decoration: const InputDecoration(labelText: 'New password'),
            validator: (value) => value == null || value.runes.length < 15
                ? 'Use at least 15 characters'
                : value.runes.length > 128
                ? 'Use at most 128 characters'
                : null,
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () {
          if (_formKey.currentState?.validate() ?? false) {
            Navigator.of(context).pop((_current.text, _next.text));
          }
        },
        child: const Text('Change password'),
      ),
    ],
  );
}
