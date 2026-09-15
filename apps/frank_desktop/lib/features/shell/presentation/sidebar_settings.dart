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
  @override
  Widget build(BuildContext context) {
    if (widget.authRepository == null) return _legacyFooter();
    final username =
        widget.authRepository?.currentSession?.owner.username ?? 'Owner';
    final storageWarning = widget.authRepository?.storageWarning;
    final largeText = MediaQuery.textScalerOf(context).scale(1) >= 1.5;
    return Padding(
      padding: EdgeInsets.all(widget.dense ? 8 : 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (storageWarning case final warning?) ...[
            FTooltip(
              tipBuilder: (_, _) => Text(warning),
              child: Semantics(
                container: true,
                label: 'Secure storage notice',
                value: warning,
                child: Row(
                  children: [
                    const Icon(
                      FrankIcons.lockOutline,
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
          LayoutBuilder(
            builder: (context, constraints) {
              // FPopoverMenu is rendered in a portal, so it does not inherit
              // the footer button's width from the surrounding Column. Give
              // the portal the same finite width as the button instead of
              // falling back to Forui's intrinsic 150-250px menu range.
              final menuWidth = constraints.maxWidth;
              return FPopoverMenu(
                key: const ValueKey('sidebar-user-menu'),
                groupId: 'sidebar-account-menu',
                // Account actions can be opened while a project menu is
                // closing. Disable the portal's width/scale transition so
                // the menu items never get laid out against a transient 0px
                // constraint.
                style: FPopoverMenuStyleDelta.delta(
                  minWidth: menuWidth,
                  maxWidth: menuWidth,
                  motion: FPopoverMotion.none,
                ),
                menuBuilder: (context, controller, _) =>
                    _accountMenu(controller),
                semanticsLabel: 'Account settings',
                builder: (_, controller, _) => Semantics(
                  key: const ValueKey('sidebar-user-button'),
                  container: true,
                  explicitChildNodes: true,
                  button: true,
                  label: 'Account owner',
                  hint: 'Open account settings',
                  onTap: controller.toggle,
                  child: ExcludeSemantics(
                    child: largeText
                        ? FButton.icon(
                            onPress: controller.toggle,
                            size: FButtonSizeVariant.sm,
                            semanticsLabel: 'Account owner',
                            semanticsTooltip: 'Open account settings',
                            child: const Icon(FrankIcons.user, size: 16),
                          )
                        : FButton.raw(
                            onPress: controller.toggle,
                            size: FButtonSizeVariant.sm,
                            child: Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 9,
                              ),
                              child: Row(
                                children: [
                                  FAvatar.raw(
                                    size: widget.dense ? 20 : 22,
                                    child: Text(
                                      username.characters.first.toUpperCase(),
                                    ),
                                  ),
                                  const SizedBox(width: 4),
                                  Expanded(
                                    child: Text(
                                      username,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
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
              );
            },
          ),
        ],
      ),
    );
  }

  List<FItemGroupMixin> _accountMenu(FPopoverController controller) {
    VoidCallback invoke(_AccountAction action) => () {
      unawaited(controller.hide());
      unawaited(_handleAction(context, action));
    };

    return [
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('sidebar-account-action-change-password'),
            title: const Text('Change password'),
            prefix: const Icon(FrankIcons.keyRound),
            enabled: widget.onChangePassword != null,
            semanticsLabel: 'Change password',
            onPress: widget.onChangePassword == null
                ? null
                : invoke(_AccountAction.changePassword),
          ),
          FItem(
            key: const ValueKey('sidebar-account-action-logout-all'),
            title: const Text('Log out all devices'),
            prefix: const Icon(FrankIcons.monitorSmartphone),
            enabled: widget.onLogoutAll != null,
            semanticsLabel: 'Log out all devices',
            onPress: widget.onLogoutAll == null
                ? null
                : invoke(_AccountAction.logoutAll),
          ),
        ],
      ),
      FItemGroup(
        children: [
          FItem(
            key: const ValueKey('sidebar-account-action-logout'),
            title: const Text('Log out'),
            prefix: const Icon(FrankIcons.logOut),
            variant: FItemVariant.destructive,
            enabled: widget.onLogout != null,
            semanticsLabel: 'Log out',
            onPress: widget.onLogout == null
                ? null
                : invoke(_AccountAction.logout),
          ),
        ],
      ),
    ];
  }

  Widget _legacyFooter() => Padding(
    padding: EdgeInsets.all(widget.dense ? 8 : 12),
    child: Builder(
      builder: (context) {
        final largeText = MediaQuery.textScalerOf(context).scale(1) >= 1.5;
        return Semantics(
          key: const ValueKey('sidebar-user-button'),
          container: true,
          button: true,
          enabled: false,
          label: 'User',
          hint: 'User profile is not available yet',
          child: largeText
              ? FButton.raw(
                  onPress: null,
                  semanticsLabel: 'User',
                  semanticsTooltip: 'User profile is not available yet',
                  size: FButtonSizeVariant.sm,
                  variant: FButtonVariant.outline,
                  child: const SizedBox(
                    width: 40,
                    height: 40,
                    child: Center(child: Icon(FrankIcons.user, size: 16)),
                  ),
                )
              : FButton(
                  onPress: null,
                  variant: FButtonVariant.outline,
                  size: FButtonSizeVariant.sm,
                  mainAxisSize: MainAxisSize.max,
                  mainAxisAlignment: MainAxisAlignment.start,
                  prefix: largeText
                      ? null
                      : FAvatar.raw(
                          size: widget.dense ? 20 : 22,
                          child: const Text('U'),
                        ),
                  builder: (_, _, _, _, _, child) => Expanded(
                    child: FittedBox(
                      fit: BoxFit.scaleDown,
                      alignment: Alignment.centerLeft,
                      child: child!,
                    ),
                  ),
                  child: const Text('User', style: TextStyle(fontSize: 12)),
                ),
        );
      },
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
        final values = await showFrankDialog<(String, String)>(
          context: context,
          builder: (_) => const _ChangePasswordDialog(),
        );
        if (values == null || !context.mounted) return;
        try {
          await callback(values.$1, values.$2);
        } on Object catch (error) {
          if (!context.mounted) return;
          showFrankToast(context, error.toString());
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
  Widget build(BuildContext context) => FDialog(
    builder: (context, style) => Padding(
      padding: const EdgeInsets.all(20),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('Change password', style: style.titleTextStyle),
          const SizedBox(height: 16),
          Form(
            key: _formKey,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Semantics(
                  container: true,
                  label: 'Current password',
                  child: ExcludeSemantics(
                    child: FTextFormField(
                      key: const ValueKey('change-password-current'),
                      control: FTextFieldControl.managed(controller: _current),
                      obscureText: true,
                      label: const Text('Current password'),
                      validator: (value) => value == null || value.isEmpty
                          ? 'Enter your current password'
                          : null,
                    ),
                  ),
                ),
                const SizedBox(height: 10),
                Semantics(
                  container: true,
                  label: 'New password',
                  child: ExcludeSemantics(
                    child: FTextFormField(
                      key: const ValueKey('change-password-new'),
                      control: FTextFieldControl.managed(controller: _next),
                      obscureText: true,
                      label: const Text('New password'),
                      validator: (value) =>
                          value == null || value.runes.length < 15
                          ? 'Use at least 15 characters'
                          : value.runes.length > 128
                          ? 'Use at most 128 characters'
                          : null,
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 20),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              FButton(
                onPress: () => Navigator.of(context).pop(),
                variant: FButtonVariant.ghost,
                child: const Text('Cancel'),
              ),
              const SizedBox(width: 8),
              FButton(
                onPress: () {
                  if (_formKey.currentState?.validate() ?? false) {
                    Navigator.of(context).pop((_current.text, _next.text));
                  }
                },
                child: const Text('Change password'),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}
