use std::path::PathBuf;

use frank_app::{
    DashboardSnapshot, OperationResult, PackOperationResult, PackPlanPreview, PlanPreview,
    TargetOperation, UserSettings,
};

use crate::model::Page;

/// Tray-originated events, kept as their own type so `reduce` can match on
/// them without the platform shell needing to know about every other
/// `Message` variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMessage {
    Open,
    Quit,
    SetLevel(Option<String>),
}

/// All events `reduce` can react to.
///
/// `Message` must be `Clone` (iced's `button(..).on_press(msg)` stores and
/// clones it), which is why every backend call result carries `String`
/// rather than `frank_app::AppError` -- `AppError` is not `Clone`. The
/// `.map_err(|e| e.to_string())` this implies does not disappear from the
/// port, it just moves from the old Tauri IPC boundary to here; see
/// `reducer::perform_request`.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Navigate(Page),
    RefreshRequested,
    DismissError,
    SnapshotLoaded {
        generation: u64,
        result: Result<DashboardSnapshot, String>,
    },

    ToggleActive,
    LevelSet(Result<Option<String>, String>),

    AddPackRequested,
    DirectoryPicked(Option<PathBuf>),
    PackUseRequested {
        selector: String,
    },
    PackRemoveRequested {
        selector: String,
    },
    PackPrepared(Result<PackPlanPreview, String>),
    PackApplied(Result<PackOperationResult, String>),

    TargetChangeRequested {
        target_id: String,
        operation: TargetOperation,
    },
    TargetPrepared(Result<PlanPreview, String>),
    TargetApplied(Result<OperationResult, String>),

    DefaultLevelSelected(String),
    LaunchAtLoginToggled(bool),
    CloseToTrayToggled(bool),
    SettingsSaved(Result<UserSettings, String>),
    /// Carries the intended enabled/disabled value alongside the result so
    /// `reduce` can build the exact same error string the Tauri command
    /// used to: `"settings saved, but autostart could not be {enabled,disabled}: {e}"`.
    AutostartApplied {
        enabled: bool,
        result: Result<(), String>,
    },
    AutostartStateLoaded(bool),

    ConfirmAccepted,
    ConfirmDismissed,

    Tray(TrayMessage),
    /// The runtime has pumped at least once and it is now safe to create
    /// platform UI (tray icon, menu) -- see the plan's M-0 spike finding
    /// that these cannot be created before the event loop is running.
    RuntimeReady,
    /// A second process instance handed off to this one (single-instance
    /// lock already held) and asked to bring the window forward.
    ShowRequested,
    WindowOpened,
    CloseRequested,
    /// A periodic wakeup with no meaning of its own; the shell uses it to
    /// check external state synchronously (e.g. whether a second-instance
    /// show-request file has appeared) without frank-gui-core needing to
    /// know what that state is. `reduce` treats it as a pure no-op --
    /// callers that need to act on it intercept and translate it into a
    /// real message *before* calling `reduce`, they never expect an
    /// `Effect` back from `Tick` itself.
    Tick,
}
