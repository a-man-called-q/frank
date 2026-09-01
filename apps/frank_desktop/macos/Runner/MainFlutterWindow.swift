import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow, FlutterStreamHandler {
  private var fullscreenEventSink: FlutterEventSink?
  private var fullscreenObservers: [NSObjectProtocol] = []

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()

    let messenger = flutterViewController.engine.binaryMessenger
    let methodChannel = FlutterMethodChannel(
      name: "dev.frank.frankDesktop/windowChrome",
      binaryMessenger: messenger
    )
    methodChannel.setMethodCallHandler { [weak self] call, result in
      switch call.method {
      case "isFullscreen":
        result(self?.styleMask.contains(.fullScreen) ?? false)
      case "toggleZoom":
        self?.performZoom(nil)
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }

    let eventChannel = FlutterEventChannel(
      name: "dev.frank.frankDesktop/windowChromeEvents",
      binaryMessenger: messenger
    )
    eventChannel.setStreamHandler(self)

    let notificationCenter = NotificationCenter.default
    fullscreenObservers = [
      notificationCenter.addObserver(
        forName: NSWindow.didEnterFullScreenNotification,
        object: self,
        queue: .main
      ) { [weak self] _ in
        self?.sendFullscreenState(true)
      },
      notificationCenter.addObserver(
        forName: NSWindow.didExitFullScreenNotification,
        object: self,
        queue: .main
      ) { [weak self] _ in
        self?.sendFullscreenState(false)
      },
    ]

    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    styleMask.insert(.fullSizeContentView)
    isMovableByWindowBackground = true
    contentMinSize = NSSize(
      width: max(contentMinSize.width, 880),
      height: contentMinSize.height
    )
  }

  func onListen(
    withArguments arguments: Any?,
    eventSink events: @escaping FlutterEventSink
  ) -> FlutterError? {
    fullscreenEventSink = events
    sendFullscreenState(styleMask.contains(.fullScreen))
    return nil
  }

  func onCancel(withArguments arguments: Any?) -> FlutterError? {
    fullscreenEventSink = nil
    return nil
  }

  private func sendFullscreenState(_ isFullscreen: Bool) {
    fullscreenEventSink?([
      "name": "fullscreenChanged",
      "isFullscreen": isFullscreen,
    ])
  }

  deinit {
    for observer in fullscreenObservers {
      NotificationCenter.default.removeObserver(observer)
    }
  }
}
