import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';

import '../../app/frank_logo.dart';
import '../../app/icons.dart';
import '../../app/theme.dart';
import '../../app/office_ui.dart';
import '../../core/auth/auth_models.dart';
import '../../core/auth/auth_repository.dart';
import '../../core/gateway/frank_gateway.dart';
import '../../core/models/connection_models.dart';
import '../floor/office_scene_floor.dart';
import '../shell/bloc/shell_bloc.dart';
import '../shell/office_shell.dart';
import '../shell/sidebar_effect.dart';

/// Login boundary for a self-hosted Frank server.
///
/// The shell is preloaded behind the form so the existing transition stays
/// instant after authentication. In production the repository is the HTTP
/// implementation created by [FrankApp]; fixture tests may inject the
/// explicit demo repository.
class LoginGate extends StatefulWidget {
  const LoginGate({
    required this.gateway,
    this.authRepository,
    this.showDemoBanner = false,
    this.sidebarEffectBuilder,
    super.key,
  });

  final FrankGateway gateway;
  final AuthRepository? authRepository;
  final bool showDemoBanner;
  final SidebarEffectBuilder? sidebarEffectBuilder;

  @override
  State<LoginGate> createState() => _LoginGateState();
}

enum _LoginPhase { form, submitting, fading, entered }

class _LoginGateState extends State<LoginGate>
    with TickerProviderStateMixin, WidgetsBindingObserver {
  static const _transitionDuration = Duration(milliseconds: 720);
  static const _idleDuration = Duration(milliseconds: 4000);

  final _formKey = GlobalKey<FormState>();
  final _usernameController = TextEditingController();
  final _passwordController = TextEditingController();
  final _passwordFocusNode = FocusNode(debugLabel: 'Login password');
  Completer<ShellLoadStatus> _shellReady = Completer<ShellLoadStatus>();

  late final AnimationController _transitionController;
  late final AnimationController _idleController;
  late final Animation<double> _loginOpacity;
  late final Animation<double> _shellOpacity;
  late final Animation<double> _idleMotion;
  Widget _shell = const SizedBox.shrink();
  late final AuthRepository _auth;

  _LoginPhase _phase = _LoginPhase.form;
  ShellLoadStatus _shellStatus = ShellLoadStatus.loading;
  bool _passwordVisible = false;
  String? _error;
  FrankConnectionStatus? _compatibilityStatus;
  bool _restoring = true;
  bool _shellMounted = false;
  Timer? _sessionTimer;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _auth = widget.authRepository ?? DemoAuthRepository();
    _transitionController = AnimationController(
      vsync: this,
      duration: _transitionDuration,
    );
    _idleController = AnimationController(vsync: this, duration: _idleDuration);
    _loginOpacity = Tween<double>(begin: 1.0, end: 0.0).animate(
      CurvedAnimation(parent: _transitionController, curve: Curves.easeInCubic),
    );
    _shellOpacity = Tween<double>(begin: 0.0, end: 1.0).animate(
      CurvedAnimation(
        parent: _transitionController,
        curve: Curves.easeOutCubic,
      ),
    );
    _idleMotion = CurvedAnimation(
      parent: _idleController,
      curve: Curves.easeInOutSine,
    );
    // Demo fixtures preserve the instant visual transition used by the
    // existing golden tests. A production remote gateway is mounted only
    // after login, so it never performs an unauthenticated snapshot request.
    if (widget.showDemoBanner || widget.authRepository == null) {
      _mountShell();
    }
    unawaited(_restoreSession());
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final disableAnimations =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    if (disableAnimations || _phase == _LoginPhase.entered) {
      _idleController.stop();
      _idleController.value = 0.0;
    } else if (!_idleController.isAnimating) {
      _idleController.repeat(reverse: true);
    }
  }

  @override
  void dispose() {
    _usernameController.dispose();
    _passwordController.dispose();
    _passwordFocusNode.dispose();
    _transitionController.dispose();
    _idleController.dispose();
    WidgetsBinding.instance.removeObserver(this);
    _sessionTimer?.cancel();
    _auth.dispose();
    super.dispose();
  }

  bool get _isFormEnabled => _phase == _LoginPhase.form;
  bool get _isShellInteractive => _phase == _LoginPhase.entered;

  void _onShellStatusChanged(ShellLoadStatus status) {
    _shellStatus = status;
    if (status != ShellLoadStatus.loading && !_shellReady.isCompleted) {
      _shellReady.complete(status);
    }
  }

  Future<ShellLoadStatus> _waitForShell() {
    if (_shellStatus != ShellLoadStatus.loading) {
      return Future<ShellLoadStatus>.value(_shellStatus);
    }
    return _shellReady.future;
  }

  Future<void> _submit() async {
    if (!_isFormEnabled || _restoring) return;
    if (!(_formKey.currentState?.validate() ?? false)) return;

    FocusManager.instance.primaryFocus?.unfocus();
    setState(() {
      _error = null;
      _phase = _LoginPhase.submitting;
    });

    try {
      await _auth.login(
        username: _usernameController.text,
        password: _passwordController.text,
      );
      // Fixture widget tests historically reserved a short button feedback
      // window. Keep that visual contract for the explicit demo repository;
      // production HTTP login has no artificial delay.
      if (_auth is DemoAuthRepository) {
        await Future<void>.delayed(const Duration(milliseconds: 600));
      }
    } on AuthFailure catch (error) {
      if (!mounted) return;
      if (error.kind == AuthFailureKind.protocolMismatch) {
        _showCompatibilityFailure(error);
        return;
      }
      setState(() {
        _error = error.userMessage;
        _phase = _LoginPhase.form;
      });
      return;
    } on Object {
      if (!mounted) return;
      setState(() {
        _error = 'Could not log in to the Frank server.';
        _phase = _LoginPhase.form;
      });
      return;
    }

    await _revealShell();
  }

  Future<void> _restoreSession() async {
    try {
      final capabilities = await widget.gateway.preflightCapabilities();
      if (capabilities != null &&
          !capabilities.isCompatibleWith(FrankApiVersion.current)) {
        if (!mounted) return;
        setState(() {
          _restoring = false;
          _compatibilityStatus = capabilities.statusFor(
            FrankApiVersion.current,
          );
          _error = null;
        });
        return;
      }
      final status = await _auth.status();
      if (!status.configured) {
        if (!mounted) return;
        setState(() {
          _restoring = false;
          _error = 'Set up the Frank owner account on the server first.';
        });
        return;
      }
      final session = await _auth.restore();
      if (!mounted) return;
      setState(() => _restoring = false);
      if (session != null) await _revealShell();
    } on AuthFailure catch (error) {
      if (!mounted) return;
      if (error.kind == AuthFailureKind.protocolMismatch) {
        _showCompatibilityFailure(error);
        return;
      }
      setState(() {
        _restoring = false;
        _error = error.userMessage;
      });
    } on Object {
      if (!mounted) return;
      setState(() {
        _restoring = false;
        _error =
            'Could not reach the Frank server. Check its address and try again.';
      });
    }
  }

  Future<void> _revealShell() async {
    if (!mounted) return;
    _mountShell();
    setState(() => _phase = _LoginPhase.submitting);
    await _waitForShell();
    if (!mounted) return;

    final disableAnimations =
        MediaQuery.maybeOf(context)?.disableAnimations ?? false;
    if (disableAnimations) {
      _idleController.stop();
      _transitionController.value = 1.0;
      _startSessionMonitor();
      setState(() => _phase = _LoginPhase.entered);
      return;
    }

    setState(() => _phase = _LoginPhase.fading);
    await _transitionController.forward(from: 0.0);
    if (!mounted) return;
    _idleController.stop();
    _startSessionMonitor();
    setState(() => _phase = _LoginPhase.entered);
  }

  void _mountShell() {
    if (_shellMounted || !mounted) return;
    _shellReady = Completer<ShellLoadStatus>();
    _shellStatus = ShellLoadStatus.loading;
    _shellMounted = true;
    _shell = OfficeShell(
      // Keep the stable identity used by the login transition tests and by
      // accessibility tooling that verifies the shell remains mounted while
      // the form fades away.
      key: const ValueKey('preloaded-office-shell'),
      gateway: widget.gateway,
      authRepository: _auth,
      showDemoBanner: widget.showDemoBanner,
      onLogout: _logout,
      onLogoutAll: _logoutAll,
      onChangePassword: _changePassword,
      sidebarEffectBuilder: widget.sidebarEffectBuilder,
      onStatusChanged: _onShellStatusChanged,
    );
    setState(() {});
  }

  void _startSessionMonitor() {
    _sessionTimer ??= Timer.periodic(
      const Duration(seconds: 60),
      (_) => unawaited(_checkSession()),
    );
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && _phase == _LoginPhase.entered) {
      unawaited(_checkSession());
    }
  }

  Future<void> _checkSession() async {
    if (!mounted || _phase != _LoginPhase.entered) return;
    try {
      final session = await _auth.restore();
      if (session == null && mounted) {
        _resetToLogin('Your session has expired. Log in again.');
      }
    } on AuthFailure catch (error) {
      if (error.kind == AuthFailureKind.unauthorized && mounted) {
        _resetToLogin(error.userMessage);
      }
    }
  }

  String? _validateUsername(String? value) {
    // The server only normalizes case; whitespace is not silently removed.
    // Validate the exact value that will be sent so a visually valid field
    // cannot become a generic authentication failure after submission.
    final username = value ?? '';
    if (username.isEmpty) return 'Enter your username';
    if (username.length < 3) return 'Username must be at least 3 characters';
    final validUsername =
        username.length <= 32 &&
        RegExp(r'^[A-Za-z0-9_.-]+$').hasMatch(username);
    if (!validUsername) {
      return 'Use a valid username';
    }
    return null;
  }

  String? _validatePassword(String? value) {
    if ((value ?? '').isEmpty) return 'Enter your password';
    return null;
  }

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: FrankColors.canvas,
      child: Stack(
        fit: StackFit.expand,
        children: [
          IgnorePointer(
            ignoring: !_isShellInteractive,
            child: FocusScope(
              canRequestFocus: _isShellInteractive,
              descendantsAreFocusable: _isShellInteractive,
              child: ExcludeSemantics(
                excluding: !_isShellInteractive,
                child: FadeTransition(
                  key: const ValueKey('login-shell-fade'),
                  opacity: _shellOpacity,
                  child: _shell,
                ),
              ),
            ),
          ),
          if (_phase != _LoginPhase.entered)
            FadeTransition(
              key: const ValueKey('login-overlay-fade'),
              opacity: _loginOpacity,
              child: _buildLoginSurface(),
            ),
        ],
      ),
    );
  }

  Widget _buildLoginSurface() {
    final compatibility = _compatibilityStatus;
    if (compatibility != null) {
      return _buildCompatibilitySurface(compatibility);
    }
    return IgnorePointer(
      ignoring: !_isFormEnabled,
      child: ExcludeSemantics(
        excluding: _phase == _LoginPhase.fading,
        child: Semantics(
          container: true,
          explicitChildNodes: true,
          label: 'Frank login',
          child: Stack(
            fit: StackFit.expand,
            children: [
              const OfficeSceneStage(
                key: ValueKey('login-scene-stage'),
                preset: OfficeScenePreset.empty,
                semanticLabel: 'Login background',
              ),
              IgnorePointer(
                child: DecoratedBox(
                  key: const ValueKey('login-ambient'),
                  decoration: BoxDecoration(
                    gradient: RadialGradient(
                      center: const Alignment(0.0, -0.18),
                      radius: 0.92,
                      colors: [
                        FrankColors.aubergineAccent.withValues(alpha: 0.16),
                        FrankColors.aubergineSoft.withValues(alpha: 0.24),
                        FrankColors.canvas.withValues(alpha: 0.98),
                      ],
                      stops: const [0.0, 0.46, 1.0],
                    ),
                  ),
                ),
              ),
              SafeArea(
                child: Center(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 24,
                      vertical: 48,
                    ),
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 328),
                      child: FocusScope(
                        canRequestFocus: _isFormEnabled,
                        descendantsAreFocusable: _isFormEnabled,
                        child: _buildForm(),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCompatibilitySurface(FrankConnectionStatus status) {
    return Semantics(
      container: true,
      label: 'Frank server compatibility required',
      explicitChildNodes: true,
      child: ColoredBox(
        color: FrankColors.canvas,
        child: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 460),
              child: FrankPanel(
                raised: true,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const Icon(
                      FrankIcons.warningAmberOutlined,
                      color: FrankColors.failure,
                      size: 28,
                    ),
                    const SizedBox(height: 14),
                    const Text(
                      'Update required',
                      textAlign: TextAlign.center,
                      style: TextStyle(
                        color: FrankColors.ink,
                        fontSize: 18,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      status.detail ??
                          'This Frank app is not compatible with the server.',
                      textAlign: TextAlign.center,
                      style: const TextStyle(color: FrankColors.muted),
                    ),
                    const SizedBox(height: 12),
                    Text(
                      'Server: ${widget.authRepository?.serverUrl ?? 'Configured Frank server'}',
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        color: FrankColors.muted,
                        fontSize: 11,
                        fontFamily: FrankTypography.monoFontFamily,
                      ),
                    ),
                    if (status.serverVersion case final version?) ...[
                      const SizedBox(height: 4),
                      Text(
                        'App protocol: ${status.appProtocolVersion} · Server version: $version · protocol ${status.serverProtocolVersion ?? 'unknown'}',
                        textAlign: TextAlign.center,
                        style: const TextStyle(
                          color: FrankColors.muted,
                          fontSize: 11,
                        ),
                      ),
                    ],
                    const SizedBox(height: 18),
                    const Text(
                      'Upgrade the Frank desktop app or server, then retry the compatibility check.',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: FrankColors.ink, fontSize: 12),
                    ),
                    const SizedBox(height: 18),
                    FButton(
                      key: const ValueKey('compatibility-retry-button'),
                      onPress: _retryConnection,
                      child: const Text('Retry'),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildForm() {
    final submitting = _phase == _LoginPhase.submitting;
    final enabled = _isFormEnabled && !_restoring;
    final brand = Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const FrankLogo(size: 104),
        const SizedBox(height: 14),
        const Text(
          'FRANK',
          key: ValueKey('login-wordmark'),
          style: TextStyle(
            color: FrankColors.ink,
            fontSize: 15,
            fontWeight: FontWeight.w700,
            letterSpacing: 5.2,
          ),
        ),
      ],
    );

    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          AnimatedBuilder(
            animation: _idleMotion,
            child: brand,
            builder: (context, child) {
              final lift = Tween<double>(
                begin: 1.5,
                end: -1.5,
              ).evaluate(_idleMotion);
              return Transform.translate(offset: Offset(0, lift), child: child);
            },
          ),
          const SizedBox(height: 34),
          Semantics(
            container: true,
            label: 'Username',
            child: ExcludeSemantics(
              child: FTextFormField(
                key: const ValueKey('login-username-field'),
                control: FTextFieldControl.managed(
                  controller: _usernameController,
                ),
                enabled: enabled,
                keyboardType: TextInputType.name,
                textInputAction: TextInputAction.next,
                onSubmit: (_) => _passwordFocusNode.requestFocus(),
                autofillHints: const [AutofillHints.username],
                validator: _validateUsername,
                label: const Text('Username'),
                hint: 'owner',
              ),
            ),
          ),
          const SizedBox(height: 12),
          Semantics(
            container: true,
            label: 'Password',
            child: ExcludeSemantics(
              child: FTextFormField(
                key: const ValueKey('login-password-field'),
                control: FTextFieldControl.managed(
                  controller: _passwordController,
                ),
                focusNode: _passwordFocusNode,
                enabled: enabled,
                obscureText: !_passwordVisible,
                textInputAction: TextInputAction.done,
                autofillHints: const [AutofillHints.password],
                onSubmit: (_) => unawaited(_submit()),
                validator: _validatePassword,
                label: const Text('Password'),
                hint: 'Your workspace password',
                suffixBuilder: (_, _, _) => FButton.icon(
                  onPress: !enabled
                      ? null
                      : () => setState(
                          () => _passwordVisible = !_passwordVisible,
                        ),
                  semanticsLabel: _passwordVisible
                      ? 'Hide password'
                      : 'Show password',
                  semanticsTooltip: _passwordVisible
                      ? 'Hide password'
                      : 'Show password',
                  child: Icon(
                    _passwordVisible
                        ? FrankIcons.visibilityOffOutlined
                        : FrankIcons.visibilityOutlined,
                    size: 18,
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(height: 20),
          SizedBox(
            height: FrankUiTokens.controlHeight + 4,
            child: FButton(
              key: const ValueKey('login-submit-button'),
              onPress: enabled ? () => unawaited(_submit()) : null,
              variant: FButtonVariant.primary,
              size: FButtonSizeVariant.md,
              child: submitting
                  ? Row(
                      mainAxisSize: MainAxisSize.min,
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        const FCircularProgress(
                          size: FCircularProgressSizeVariant.sm,
                        ),
                        SizedBox(width: 10),
                        const Text(
                          'Preparing workspace…',
                          overflow: TextOverflow.ellipsis,
                        ),
                      ],
                    )
                  : const Text('Log in'),
            ),
          ),
          const SizedBox(height: 16),
          if (_restoring)
            const Text(
              'Checking saved session…',
              textAlign: TextAlign.center,
              style: TextStyle(color: FrankColors.muted, fontSize: 11),
            )
          else if (_error != null)
            Column(
              children: [
                Text(
                  _error!,
                  key: const ValueKey('login-error'),
                  textAlign: TextAlign.center,
                  style: const TextStyle(
                    color: FrankColors.warningAmber,
                    fontSize: 11,
                  ),
                ),
                const SizedBox(height: 6),
                FButton(
                  key: const ValueKey('login-retry-button'),
                  onPress: _retryConnection,
                  variant: FButtonVariant.ghost,
                  child: const Text('Retry connection'),
                ),
              ],
            )
          else ...[
            if (_auth.storageWarning case final warning?) ...[
              Text(
                warning,
                textAlign: TextAlign.center,
                style: const TextStyle(
                  color: FrankColors.warningAmber,
                  fontSize: 11,
                ),
              ),
              const SizedBox(height: 6),
            ],
            const Text(
              'Connect to your self-hosted Frank server.',
              textAlign: TextAlign.center,
              style: TextStyle(
                color: FrankColors.muted,
                fontSize: 11,
                fontFamily: FrankTypography.monoFontFamily,
              ),
            ),
          ],
        ],
      ),
    );
  }

  Future<void> _logout() async {
    AuthFailure? failure;
    try {
      await _auth.logout();
    } on AuthFailure catch (error) {
      failure = error;
    } on Object {
      failure = const AuthFailure(
        kind: AuthFailureKind.network,
        message: 'Signed out on this device. The server could not be reached.',
      );
    }
    if (!mounted) return;
    _resetToLogin(failure?.userMessage);
  }

  Future<void> _logoutAll() async {
    AuthFailure? failure;
    try {
      await _auth.logoutAll();
    } on AuthFailure catch (error) {
      failure = error;
    } on Object {
      failure = const AuthFailure(
        kind: AuthFailureKind.network,
        message:
            'Signed out on this device. Other sessions could not be revoked.',
      );
    }
    if (!mounted) return;
    _resetToLogin(failure?.userMessage);
  }

  Future<void> _changePassword(String current, String next) async {
    await _auth.changePassword(currentPassword: current, newPassword: next);
    if (!mounted) return;
    _resetToLogin('Password changed. Log in again with your new password.');
  }

  void _retryConnection() {
    if (_restoring) return;
    setState(() {
      _restoring = true;
      _error = null;
      _compatibilityStatus = null;
    });
    unawaited(_restoreSession());
  }

  void _showCompatibilityFailure(AuthFailure error) {
    final current = widget.gateway.connectionStatus;
    setState(() {
      _restoring = false;
      _phase = _LoginPhase.form;
      _compatibilityStatus = current.copyWith(
        phase: FrankConnectionPhase.incompatible,
        detail: error.userMessage,
      );
      _error = null;
    });
  }

  void _resetToLogin(String? message) {
    _sessionTimer?.cancel();
    _sessionTimer = null;
    _transitionController.reset();
    _usernameController.clear();
    _passwordController.clear();
    setState(() {
      _error = message;
      _phase = _LoginPhase.form;
      _restoring = false;
    });
  }
}
