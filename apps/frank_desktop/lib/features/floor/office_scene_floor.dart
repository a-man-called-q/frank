import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:forui/forui.dart';
import 'package:flutter_gpu/gpu.dart' as gpu;
import 'package:flutter_scene/scene.dart';
import 'package:vector_math/vector_math.dart' as vm;

import '../../app/theme.dart';

/// A resource loader seam used by widget tests.
typedef SceneResourceLoader = Future<void> Function();

/// Controls whether the retained office scene should schedule frame ticks.
///
/// The current floor is procedural and static, so callers use [static] or
/// [paused]. [live] is the explicit opt-in for the later milestone that adds
/// animated agents and scene-driven motion.
enum OfficeSceneActivity { static, live, paused }

extension OfficeSceneActivityX on OfficeSceneActivity {
  bool get shouldTick => this == OfficeSceneActivity.live;
}

/// The authored content to place in a retained scene.
///
/// Login uses an empty scene as a quiet backdrop while the office preset
/// remains the default for the workspace floor.
enum OfficeScenePreset { office, empty }

/// Owns the transient camera state for an interactive office floor.
///
/// The controller deliberately models the interactions currently supported by
/// the desktop floor: horizontal orbit, ground-plane pan, and orthographic
/// zoom. It is independent from the GPU scene, which keeps the gesture and
/// reset behavior deterministic in widget/unit tests and lets the composer
/// share the same reset action without reaching into the renderer.
class OfficeSceneController extends ChangeNotifier {
  OfficeSceneController({
    this.viewportWorldHeight = 11.0,
    this.minTargetX = -6.0,
    this.maxTargetX = 6.0,
    this.minTargetZ = -4.0,
    this.maxTargetZ = 4.0,
  }) : assert(viewportWorldHeight > 0),
       assert(minTargetX <= maxTargetX),
       assert(minTargetZ <= maxTargetZ);

  /// The world-space height represented by one viewport height.
  final double viewportWorldHeight;

  /// Ground-plane pan limits. They keep the camera over the current floor
  /// slab while leaving an inset so the room does not drift out of frame.
  final double minTargetX;
  final double maxTargetX;
  final double minTargetZ;
  final double maxTargetZ;

  static const double targetY = 0.8;
  static const double initialEyeX = 10.0;
  static const double initialEyeY = 8.5;
  static const double initialEyeZ = 12.0;
  static const double initialTargetX = 0.0;
  static const double initialTargetZ = 0.0;
  static const double initialZoom = 1.0;

  /// The zoom range keeps the authored floor useful without letting the
  /// orthographic lens become so wide or tight that the room disappears.
  static const double minZoom = 0.65;
  static const double maxZoom = 2.0;

  /// Scroll units are conventionally reported in 120-unit wheel notches.
  static const double zoomScrollSensitivity = 1.0 / 120.0;
  static const double zoomScrollSpeed = 0.15;

  // The horizontal orbit convention matches the eye used by the existing
  // scene: eye = target + (-sin(yaw), 0, -cos(yaw)) * horizontalDistance.
  static final double initialYaw = math.atan2(-initialEyeX, -initialEyeZ);

  final double _horizontalDistance = math.sqrt(
    initialEyeX * initialEyeX + initialEyeZ * initialEyeZ,
  );
  final double _verticalOffset = initialEyeY - targetY;

  double _targetX = initialTargetX;
  double _targetZ = initialTargetZ;
  double _yaw = initialYaw;
  double _zoom = initialZoom;
  bool _isReady = false;

  /// Current world-space point around which the camera looks.
  @visibleForTesting
  vm.Vector3 get target => vm.Vector3(_targetX, targetY, _targetZ);

  /// Current camera yaw in radians. Elevation is intentionally fixed.
  @visibleForTesting
  double get yaw => _yaw;

  /// Current orthographic zoom factor. `1` is the authored framing; values
  /// above one zoom in and values below one zoom out.
  @visibleForTesting
  double get zoom => _zoom;

  /// Current world-space camera eye, useful for deterministic assertions.
  @visibleForTesting
  vm.Vector3 get eye => _eyeFor(target);

  bool get isReady => _isReady;

  /// Whether reset would visibly change the current floor view.
  bool get canReset => _isReady && !_atInitialView;

  bool get _atInitialView =>
      (_targetX - initialTargetX).abs() < 1e-9 &&
      (_targetZ - initialTargetZ).abs() < 1e-9 &&
      (_yaw - initialYaw).abs() < 1e-9 &&
      (_zoom - initialZoom).abs() < 1e-9;

  /// Marks the controller as attached to a successfully initialized scene.
  /// This is public so the stage can keep the composer button disabled while
  /// resources are loading or the retryable GPU fallback is visible.
  void setReady(bool ready) {
    if (_isReady == ready) return;
    _isReady = ready;
    notifyListeners();
  }

  /// Restores the original framing, regardless of current scene readiness.
  void reset() {
    final changed = !_atInitialView;
    _targetX = initialTargetX;
    _targetZ = initialTargetZ;
    _yaw = initialYaw;
    _zoom = initialZoom;
    if (changed) notifyListeners();
  }

  /// Applies a primary-button drag as a ground-plane pan.
  @visibleForTesting
  void panByPixels(Offset delta, Size viewport) {
    if (!_isReady || delta == Offset.zero) return;
    final height = viewport.height.isFinite && viewport.height > 0
        ? viewport.height
        : 1.0;
    final pixelsToWorld = viewportWorldHeight / _zoom / height;

    final eye = _eyeFor(target);
    final forward = target - eye;
    forward.normalize();
    // Keep the basis in the same convention as flutter_scene's camera
    // controller: +X is the camera's screen-right axis and +Y is screen-up.
    // Using the opposite cross-product order makes a drag feel reversed.
    final right = vm.Vector3(0.0, 1.0, 0.0).cross(forward)..normalize();
    final up = forward.cross(right);
    final groundUp = vm.Vector3(up.x, 0.0, up.z);
    if (groundUp.length2 > 0) groundUp.normalize();

    // Dragging behaves like moving a map: the floor follows the pointer, so
    // the camera's look target shifts in the opposite screen direction.
    final shift =
        (right * (-delta.dx * pixelsToWorld)) +
        (groundUp * (delta.dy * pixelsToWorld));
    _setTarget(_targetX + shift.x, _targetZ + shift.z);
  }

  /// Applies a secondary-button drag as a horizontal-only orbit.
  @visibleForTesting
  void rotateByPixels(Offset delta, Size viewport) {
    if (!_isReady || delta.dx == 0) return;
    final width = viewport.width.isFinite && viewport.width > 0
        ? viewport.width
        : 1.0;
    _yaw += delta.dx / width * math.pi;
    notifyListeners();
  }

  /// Applies a mouse-wheel/trackpad scroll as an orthographic zoom.
  ///
  /// Flutter reports positive scroll deltas when scrolling down/away, so a
  /// positive value zooms out and a negative value zooms in. The exponential
  /// response keeps one wheel notch feeling consistent at either end of the
  /// range.
  @visibleForTesting
  void zoomByScroll(double scrollDelta) {
    if (!_isReady || !scrollDelta.isFinite || scrollDelta == 0.0) return;
    _setZoom(
      _zoom * math.exp(-scrollDelta * zoomScrollSensitivity * zoomScrollSpeed),
    );
  }

  /// Applies a pinch scale factor, where values above one zoom in and values
  /// below one zoom out. Trackpad pan/zoom events report this as a cumulative
  /// scale; the interaction surface converts that into a per-event factor.
  @visibleForTesting
  void zoomByScale(double scaleFactor) {
    if (!_isReady || !scaleFactor.isFinite || scaleFactor <= 0.0) return;
    _setZoom(_zoom * scaleFactor);
  }

  /// Updates a retained camera node from the current controller state.
  void applyTo(Node cameraNode, {OfficeOrthographicProjection? projection}) {
    projection?.zoom = _zoom;
    cameraNode.lookAtFrom(_eyeFor(target), target);
  }

  void _setTarget(double x, double z) {
    final clampedX = x.clamp(minTargetX, maxTargetX).toDouble();
    final clampedZ = z.clamp(minTargetZ, maxTargetZ).toDouble();
    if ((_targetX - clampedX).abs() < 1e-9 &&
        (_targetZ - clampedZ).abs() < 1e-9) {
      return;
    }
    _targetX = clampedX;
    _targetZ = clampedZ;
    notifyListeners();
  }

  void _setZoom(double value) {
    final nextZoom = value.clamp(minZoom, maxZoom).toDouble();
    if ((_zoom - nextZoom).abs() < 1e-9) return;
    _zoom = nextZoom;
    notifyListeners();
  }

  vm.Vector3 _eyeFor(vm.Vector3 pivot) =>
      pivot +
      vm.Vector3(
        -math.sin(_yaw) * _horizontalDistance,
        _verticalOffset,
        -math.cos(_yaw) * _horizontalDistance,
      );
}

/// Routes desktop mouse drags to [OfficeSceneController] while keeping the
/// underlying SceneView pointer-transparent.
class OfficeSceneInteractionSurface extends StatefulWidget {
  const OfficeSceneInteractionSurface({
    required this.controller,
    required this.child,
    super.key,
  });

  final OfficeSceneController controller;
  final Widget child;

  @override
  State<OfficeSceneInteractionSurface> createState() =>
      _OfficeSceneInteractionSurfaceState();
}

class _OfficeSceneInteractionSurfaceState
    extends State<OfficeSceneInteractionSurface> {
  bool _dragging = false;
  int _dragButtons = 0;
  double _panZoomLastScale = 1.0;

  void _onPointerDown(PointerDownEvent event) {
    if (event.kind != PointerDeviceKind.mouse) return;
    final supported =
        event.buttons & (kPrimaryMouseButton | kSecondaryMouseButton);
    if (supported == 0) return;
    setState(() {
      _dragging = true;
      _dragButtons = supported;
    });
  }

  void _onPointerMove(PointerMoveEvent event) {
    if (event.kind != PointerDeviceKind.mouse || !_dragging) return;
    final size = context.size ?? Size.zero;
    final buttons = event.buttons == 0 ? _dragButtons : event.buttons;
    if ((buttons & kSecondaryMouseButton) != 0) {
      widget.controller.rotateByPixels(event.delta, size);
    } else if ((buttons & kPrimaryMouseButton) != 0) {
      widget.controller.panByPixels(event.delta, size);
    }
  }

  void _onPointerSignal(PointerSignalEvent event) {
    if (event is PointerScrollEvent) {
      // Mouse wheels and trackpad scroll signals use the same path, even
      // though the latter carries PointerDeviceKind.trackpad.
      if (event.kind != PointerDeviceKind.mouse &&
          event.kind != PointerDeviceKind.trackpad) {
        return;
      }
      widget.controller.zoomByScroll(event.scrollDelta.dy);
    } else if (event is PointerScaleEvent) {
      widget.controller.zoomByScale(event.scale);
    }
  }

  void _onPointerPanZoomStart(PointerPanZoomStartEvent event) {
    _panZoomLastScale = 1.0;
  }

  void _onPointerPanZoomUpdate(PointerPanZoomUpdateEvent event) {
    final scale = event.scale;
    if (!scale.isFinite || scale <= 0.0) return;
    final previousScale = _panZoomLastScale;
    _panZoomLastScale = scale;
    if (!previousScale.isFinite || previousScale <= 0.0) return;
    widget.controller.zoomByScale(scale / previousScale);
  }

  void _onPointerPanZoomEnd(PointerPanZoomEndEvent event) {
    _panZoomLastScale = 1.0;
  }

  void _stopDragging(PointerEvent event) {
    if (event.kind != PointerDeviceKind.mouse || !_dragging) return;
    setState(() {
      _dragging = false;
      _dragButtons = 0;
    });
  }

  @override
  Widget build(BuildContext context) {
    return MouseRegion(
      cursor: _dragging ? SystemMouseCursors.grabbing : SystemMouseCursors.grab,
      child: Listener(
        behavior: HitTestBehavior.opaque,
        onPointerDown: _onPointerDown,
        onPointerMove: _onPointerMove,
        onPointerSignal: _onPointerSignal,
        onPointerPanZoomStart: _onPointerPanZoomStart,
        onPointerPanZoomUpdate: _onPointerPanZoomUpdate,
        onPointerPanZoomEnd: _onPointerPanZoomEnd,
        onPointerUp: _stopDragging,
        onPointerCancel: _stopDragging,
        child: widget.child,
      ),
    );
  }
}

/// The orthographic lens used by the office foundation.
///
/// The vertical world-space size is fixed while the horizontal size follows
/// the viewport aspect ratio. This keeps the three-quarter room from
/// stretching when the conversation rail or workspace sidebar is resized.
@visibleForTesting
class OfficeOrthographicProjection extends CameraProjection {
  OfficeOrthographicProjection({
    this.verticalSize = 11.0,
    this.near = 0.1,
    this.far = 100.0,
  }) : assert(verticalSize > 0),
       assert(near >= 0),
       assert(far > near);

  /// The authored world-space height visible through the lens at zoom 1.
  final double verticalSize;

  /// Orthographic zoom factor. Values above one show a smaller world span.
  double _zoom = 1.0;

  double get zoom => _zoom;

  set zoom(double value) {
    assert(value.isFinite && value > 0);
    if (!value.isFinite || value <= 0) return;
    _zoom = value;
  }

  /// The near depth plane, expressed in camera space.
  final double near;

  /// The far depth plane, expressed in camera space.
  final double far;

  @override
  vm.Matrix4 getProjectionMatrix(double aspectRatio, {vm.Vector2? jitter}) {
    final safeAspect = aspectRatio.isFinite && aspectRatio > 0
        ? aspectRatio
        : 1.0;
    final halfHeight = verticalSize / zoom / 2.0;
    final halfWidth = halfHeight * safeAspect;
    final depth = far - near;
    final jitterX = jitter?.x ?? 0.0;
    final jitterY = jitter?.y ?? 0.0;

    // Flutter Scene uses a 0..1 depth range. Jitter is a clip-space offset
    // so it remains constant across the orthographic depth range.
    return vm.Matrix4(
      1.0 / halfWidth,
      0.0,
      0.0,
      0.0,
      0.0,
      1.0 / halfHeight,
      0.0,
      0.0,
      0.0,
      0.0,
      1.0 / depth,
      0.0,
      jitterX,
      jitterY,
      -near / depth,
      1.0,
    );
  }
}

/// Owns the retained office scene and composes optional glass content above it.
///
/// The stage keeps the scene/resource handle alive while its foreground
/// changes between Settings sections. Its filter is applied only to the scene
/// visual, leaving loading/error/retry controls and the foreground sharp.
class OfficeSceneStage extends StatefulWidget {
  const OfficeSceneStage({
    this.activity = OfficeSceneActivity.static,
    this.preset = OfficeScenePreset.office,
    this.blurSigma = 0.0,
    this.scrimColor = const Color(0x00000000),
    this.semanticLabel = 'Stylized 3D office floor',
    this.foreground,
    this.controller,
    this.initializeResources,
    super.key,
  }) : assert(blurSigma >= 0 && blurSigma < double.infinity);

  /// Whether this stage should schedule scene animation ticks.
  final OfficeSceneActivity activity;

  /// Selects the authored content for the retained scene.
  final OfficeScenePreset preset;

  /// The blur radius applied to the retained scene visual only.
  final double blurSigma;

  /// A pointer-transparent tint placed between the scene and [foreground].
  final Color scrimColor;

  /// Accessibility label for the scene and its loading/error states.
  final String semanticLabel;

  /// Interactive content composed above the scene and scrim.
  final Widget? foreground;

  /// Optional controller that enables desktop navigation for this stage.
  /// Stages without a controller remain pointer-transparent static backdrops.
  final OfficeSceneController? controller;

  /// Overrides static resource initialization for deterministic failure tests.
  @visibleForTesting
  final SceneResourceLoader? initializeResources;

  @override
  State<OfficeSceneStage> createState() => _OfficeSceneStageState();
}

/// Backwards-compatible floor wrapper.
///
/// Existing callers can keep constructing [OfficeSceneFloor]; the retained
/// lifecycle now lives in the nested [OfficeSceneStage]. New compositions can
/// use the stage directly when they need glass or a foreground.
class OfficeSceneFloor extends StatelessWidget {
  const OfficeSceneFloor({
    super.key,
    this.activity = OfficeSceneActivity.static,
    this.preset = OfficeScenePreset.office,
    this.blurSigma = 0.0,
    this.scrimColor = const Color(0x00000000),
    this.semanticLabel = 'Stylized 3D office floor',
    this.foreground,
    this.controller,
    this.initializeResources,
  });

  final OfficeSceneActivity activity;
  final OfficeScenePreset preset;
  final double blurSigma;
  final Color scrimColor;
  final String semanticLabel;
  final Widget? foreground;
  final OfficeSceneController? controller;
  @visibleForTesting
  final SceneResourceLoader? initializeResources;

  @override
  Widget build(BuildContext context) {
    return OfficeSceneStage(
      key: const ValueKey('office-scene-stage'),
      activity: activity,
      preset: preset,
      blurSigma: blurSigma,
      scrimColor: scrimColor,
      semanticLabel: semanticLabel,
      foreground: foreground,
      controller: controller,
      initializeResources: initializeResources,
    );
  }
}

class _OfficeSceneStageState extends State<OfficeSceneStage> {
  late Future<_OfficeSceneResult> _sceneFuture;
  _OfficeSceneHandle? _sceneHandle;

  String get _loadingSemanticLabel =>
      widget.semanticLabel == 'Stylized 3D office floor'
      ? 'Office floor loading'
      : '${widget.semanticLabel} loading';

  String get _unavailableSemanticLabel =>
      widget.semanticLabel == 'Stylized 3D office floor'
      ? 'Office floor unavailable'
      : '${widget.semanticLabel} unavailable';

  @override
  void initState() {
    super.initState();
    widget.controller?.addListener(_onControllerChanged);
    _sceneFuture = _loadScene();
  }

  @override
  void didUpdateWidget(covariant OfficeSceneStage oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.preset != widget.preset) {
      _sceneHandle = null;
      _sceneFuture = _loadScene();
    }
    if (!identical(oldWidget.controller, widget.controller)) {
      oldWidget.controller?.removeListener(_onControllerChanged);
      oldWidget.controller?.setReady(false);
      widget.controller?.addListener(_onControllerChanged);
      widget.controller?.setReady(_sceneHandle != null);
      if (_sceneHandle != null && widget.controller != null) {
        widget.controller!.applyTo(
          _sceneHandle!.cameraNode,
          projection: _orthographicProjection(_sceneHandle!.camera),
        );
      }
    }
  }

  @override
  void dispose() {
    widget.controller?.removeListener(_onControllerChanged);
    widget.controller?.setReady(false);
    super.dispose();
  }

  void _onControllerChanged() {
    if (!mounted) return;
    final handle = _sceneHandle;
    final controller = widget.controller;
    if (handle != null && controller != null) {
      controller.applyTo(
        handle.cameraNode,
        projection: _orthographicProjection(handle.camera),
      );
    }
    // SceneView repaints on widget updates even when autoTick is disabled.
    setState(() {});
  }

  Future<_OfficeSceneResult> _loadScene() async {
    final controller = widget.controller;
    try {
      controller?.setReady(false);
      // Empty backdrops have nothing to draw and need no GPU resources.
      if (widget.preset == OfficeScenePreset.empty) {
        return const _OfficeSceneResult.empty();
      }
      if (widget.initializeResources == null) {
        // A plain `flutter test` VM has no Impeller context. Probe it before
        // starting Scene's shared-resource future so GPU failures stay inside
        // this widget's retryable error state instead of surfacing as an
        // unhandled asynchronous test exception.
        try {
          gpu.gpuContext;
        } catch (_) {
          return const _OfficeSceneResult.failure();
        }
      }
      final initializeResources =
          widget.initializeResources ?? Scene.initializeStaticResources;
      await initializeResources();

      // Constructing Scene touches the Flutter GPU context. It is intentionally
      // delayed until the shared shader/material resources are ready.
      final scene = Scene();
      final cameraNode = Node(name: 'office-camera');
      final projection = OfficeOrthographicProjection();
      final cameraComponent = CameraComponent(
        projection: projection,
        activateOnMount: true,
      );
      cameraNode.lookAtFrom(
        vm.Vector3(10.0, 8.5, 12.0),
        vm.Vector3(0.0, 0.8, 0.0),
      );
      controller?.applyTo(cameraNode, projection: projection);
      cameraNode.addComponent(cameraComponent);
      scene.add(cameraNode);
      scene.directionalLight = DirectionalLight(
        direction: vm.Vector3(-0.35, -1.0, -0.25),
        color: vm.Vector3(1.0, 0.86, 0.72),
        intensity: 2.0,
      );
      if (widget.preset == OfficeScenePreset.office) {
        scene.addAll(_officeNodes());
      }

      if (mounted && identical(widget.controller, controller)) {
        controller?.setReady(true);
      }
      return _OfficeSceneResult.ready(
        _OfficeSceneHandle(scene, cameraNode, cameraComponent.toCamera()),
      );
    } catch (error) {
      if (mounted && identical(widget.controller, controller)) {
        controller?.setReady(false);
      }
      debugPrint('Office scene unavailable: $error');
      return const _OfficeSceneResult.failure();
    }
  }

  /// Reads the projection from the camera that SceneView actually renders.
  ///
  /// Keeping this as a type-checked lookup avoids a second, non-nullable
  /// projection field on the retained handle. Besides preventing the two
  /// references from ever drifting, it also makes a hot-reloaded stage safe:
  /// handles created by an older isolate can still be used while the new
  /// widget code is rebuilding.
  OfficeOrthographicProjection? _orthographicProjection(Camera camera) {
    final projection = camera.projection;
    return projection is OfficeOrthographicProjection ? projection : null;
  }

  List<Node> _officeNodes() => [
    _box(
      name: 'floor-slab',
      size: vm.Vector3(14.0, 0.35, 10.0),
      position: vm.Vector3(0.0, -0.175, 0.0),
      color: FrankColors.panelRaised,
    ),
    _box(
      name: 'back-wall',
      size: vm.Vector3(14.0, 3.4, 0.32),
      position: vm.Vector3(0.0, 1.55, 4.84),
      color: FrankColors.panel,
    ),
    _box(
      name: 'side-wall',
      size: vm.Vector3(0.32, 3.4, 10.0),
      position: vm.Vector3(-6.84, 1.55, 0.0),
      color: FrankColors.panel,
    ),
    _box(
      name: 'central-platform',
      size: vm.Vector3(5.2, 0.2, 3.6),
      position: vm.Vector3(0.0, 0.28, 0.0),
      color: FrankColors.aubergineSoft,
    ),
    _box(
      name: 'accent-strip-left',
      size: vm.Vector3(0.34, 0.08, 2.5),
      position: vm.Vector3(-2.05, 0.42, -0.05),
      color: FrankColors.aubergine,
    ),
    _box(
      name: 'accent-strip-right',
      size: vm.Vector3(0.34, 0.08, 2.5),
      position: vm.Vector3(2.05, 0.42, -0.05),
      color: FrankColors.warningAmber,
    ),
    _box(
      name: 'accent-strip-front',
      size: vm.Vector3(3.7, 0.08, 0.22),
      position: vm.Vector3(0.0, 0.42, -1.56),
      color: FrankColors.statusSuccess,
    ),
  ];

  Node _box({
    required String name,
    required vm.Vector3 size,
    required vm.Vector3 position,
    required Color color,
  }) {
    final material = PhysicallyBasedMaterial()
      ..baseColorFactor = _sceneColor(color)
      ..metallicFactor = 0.05
      ..roughnessFactor = 0.86;
    return Node(name: name, mesh: Mesh(CuboidGeometry(size), material))
      ..position = position
      ..shadowStatic = true;
  }

  vm.Vector4 _sceneColor(Color color) =>
      vm.Vector4(color.r, color.g, color.b, color.a);

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: widget.semanticLabel,
      child: ClipRect(
        child: FutureBuilder<_OfficeSceneResult>(
          future: _sceneFuture,
          builder: (context, snapshot) {
            final frame = _frameFor(snapshot);
            return Stack(
              fit: StackFit.expand,
              children: [
                const ColoredBox(color: FrankColors.canvas),
                RepaintBoundary(
                  key: const ValueKey('office-scene-visual-boundary'),
                  child: ImageFiltered(
                    key: const ValueKey('office-scene-image-filter'),
                    imageFilter: ui.ImageFilter.blur(
                      sigmaX: widget.blurSigma,
                      sigmaY: widget.blurSigma,
                    ),
                    enabled: widget.blurSigma > 0,
                    child: frame.visual,
                  ),
                ),
                if (widget.scrimColor.a > 0)
                  Positioned.fill(
                    child: IgnorePointer(
                      child: ColoredBox(color: widget.scrimColor),
                    ),
                  ),
                if (widget.foreground != null)
                  Positioned.fill(child: widget.foreground!),
                if (frame.overlay != null)
                  Positioned.fill(child: frame.overlay!),
              ],
            );
          },
        ),
      ),
    );
  }

  _OfficeSceneFrame _frameFor(AsyncSnapshot<_OfficeSceneResult> snapshot) {
    if (widget.preset == OfficeScenePreset.empty) {
      _sceneHandle = null;
      return const _OfficeSceneFrame(
        visual: ColoredBox(color: FrankColors.canvas),
        overlay: null,
      );
    }
    if (snapshot.connectionState != ConnectionState.done) {
      _sceneHandle = null;
      return _OfficeSceneFrame(
        visual: Semantics(
          container: true,
          explicitChildNodes: true,
          label: _loadingSemanticLabel,
          child: widget.preset == OfficeScenePreset.empty
              ? const ColoredBox(color: FrankColors.canvas)
              : const CustomPaint(painter: _OfficePlaceholderPainter()),
        ),
        overlay: const Align(
          alignment: Alignment.topRight,
          child: Padding(
            padding: EdgeInsets.all(14),
            child: _OfficeSceneLoadingBadge(),
          ),
        ),
      );
    }

    final result = snapshot.data;
    if (result == null || result.handle == null) {
      _sceneHandle = null;
      return _OfficeSceneFrame(
        visual: const ColoredBox(color: FrankColors.canvas),
        overlay: Align(
          alignment: Alignment.topRight,
          child: Padding(
            padding: const EdgeInsets.all(14),
            child: _OfficeSceneError(
              semanticLabel: _unavailableSemanticLabel,
              onRetry: () {
                setState(() {
                  _sceneFuture = _loadScene();
                });
              },
            ),
          ),
        ),
      );
    }

    _sceneHandle = result.handle;
    final sceneView = IgnorePointer(
      child: SceneView(
        result.handle!.scene,
        key: const ValueKey('office-scene-view'),
        camera: result.handle!.camera,
        autoTick: widget.activity.shouldTick,
      ),
    );
    return _OfficeSceneFrame(
      visual: widget.controller == null
          ? sceneView
          : OfficeSceneInteractionSurface(
              controller: widget.controller!,
              child: sceneView,
            ),
      overlay: null,
    );
  }
}

class _OfficeSceneFrame {
  const _OfficeSceneFrame({required this.visual, required this.overlay});

  final Widget visual;
  final Widget? overlay;
}

class _OfficeSceneHandle {
  const _OfficeSceneHandle(this.scene, this.cameraNode, this.camera);

  final Scene scene;
  final Node cameraNode;
  final Camera camera;
}

class _OfficeSceneResult {
  const _OfficeSceneResult.ready(this.handle) : error = null;

  const _OfficeSceneResult.empty() : handle = null, error = null;

  const _OfficeSceneResult.failure() : handle = null, error = true;

  final _OfficeSceneHandle? handle;
  final bool? error;
}

class _OfficeSceneLoadingBadge extends StatelessWidget {
  const _OfficeSceneLoadingBadge();

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: FrankColors.panel.withValues(alpha: 0.9),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: FrankColors.border),
      ),
      child: const Padding(
        padding: EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        child: Text(
          'OFFICE SCENE · INITIALIZING',
          style: TextStyle(
            color: FrankColors.muted,
            fontFamily: FrankTypography.monoFontFamily,
            fontSize: 10,
            letterSpacing: 1.1,
          ),
        ),
      ),
    );
  }
}

class _OfficeSceneError extends StatelessWidget {
  const _OfficeSceneError({required this.onRetry, required this.semanticLabel});

  final VoidCallback onRetry;
  final String semanticLabel;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: semanticLabel,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 280),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: FrankColors.panel.withValues(alpha: 0.94),
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: FrankColors.border),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(18, 14, 18, 12),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text(
                  '3D FLOOR UNAVAILABLE',
                  style: TextStyle(
                    color: FrankColors.ink,
                    fontFamily: FrankTypography.monoFontFamily,
                    fontSize: 10,
                    letterSpacing: 1.0,
                  ),
                ),
                const SizedBox(height: 5),
                const Text(
                  'The GPU scene could not initialize.',
                  style: TextStyle(color: FrankColors.muted, fontSize: 11),
                ),
                const SizedBox(height: 10),
                SizedBox(
                  width: double.infinity,
                  child: FButton.raw(
                    onPress: onRetry,
                    size: FButtonSizeVariant.sm,
                    child: const Padding(
                      padding: EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 12,
                      ),
                      child: Text('Retry'),
                    ),
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

class _OfficePlaceholderPainter extends CustomPainter {
  const _OfficePlaceholderPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = FrankColors.border.withValues(alpha: 0.24)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    final center = Offset(size.width * 0.52, size.height * 0.58);
    final floorWidth = math.min(size.width * 0.74, 720.0);
    final floorHeight = math.min(size.height * 0.46, 360.0);
    final left = center.dx - floorWidth / 2;
    final right = center.dx + floorWidth / 2;
    final top = center.dy - floorHeight / 2;
    final bottom = center.dy + floorHeight / 2;

    canvas.drawLine(Offset(left, top), Offset(right, top), paint);
    canvas.drawLine(Offset(left, top), Offset(center.dx, bottom), paint);
    canvas.drawLine(Offset(right, top), Offset(center.dx, bottom), paint);
    for (var index = 1; index < 5; index++) {
      final t = index / 5;
      final y = uiLerp(top, bottom, t);
      canvas.drawLine(
        Offset(left + floorWidth * t * 0.5, y),
        Offset(right - floorWidth * t * 0.5, y),
        paint,
      );
    }
  }

  double uiLerp(double a, double b, double t) => a + (b - a) * t;

  @override
  bool shouldRepaint(covariant _OfficePlaceholderPainter oldDelegate) => false;
}
