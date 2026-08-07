use frank_app::{DashboardSnapshot, TargetOperation};

/// The four screens of the desktop control panel. Mirrors `Page` in the
/// current `apps/frank-gui/src/App.tsx` (`"overview"|"personas"|
/// "integrations"|"settings"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Overview,
    Personas,
    Integrations,
    Settings,
}

/// Which mutating action currently owns the single in-flight slot. Replaces
/// the ad-hoc string keys (`"use:{id}@{ver}"`, `"{target_id}:{operation}"`,
/// ...) that `apps/frank-gui/src/hooks/useAsyncAction.ts` builds by hand --
/// a typo in one of those strings is a silent no-op at runtime; a typo here
/// does not compile.
///
/// `LaunchAtLogin` carries the intended enabled/disabled value so that once
/// the settings save round-trips (`Message::SettingsSaved`), `reduce` still
/// knows whether to additionally toggle OS autostart on or off -- see the
/// `SettingsSaved` arm in `reducer.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingKey {
    Toggle,
    PackAdd,
    DefaultLevel,
    LaunchAtLogin(bool),
    CloseToTray,
    PackUse {
        selector: String,
    },
    PackRemove {
        selector: String,
    },
    Target {
        target_id: String,
        operation: TargetOperation,
    },
}

/// Which prepared plan a confirmation dialog is standing in front of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRef {
    Target(String),
    Pack(String),
}

/// Replaces the browser's blocking `window.confirm(...)`. iced's message
/// loop cannot block, so the confirmation becomes explicit model state
/// rendered as an overlay (see the plan's "Modal konfirmasi" section) and
/// resolved by `Message::ConfirmAccepted` / `Message::ConfirmDismissed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub prompt: String,
    pub actions: Vec<String>,
    pub plan: PlanRef,
}

/// The whole desktop app's state. Pages are pure functions of `Model` plus
/// the current `DashboardSnapshot`, exactly as the React pages are pure
/// functions of `snapshot` today.
#[derive(Debug, Clone, Default)]
pub struct Model {
    pub page: Page,
    pub snapshot: Option<DashboardSnapshot>,
    pub error: Option<String>,
    /// Monotonic counter, bumped on every `RefreshRequested`. A
    /// `SnapshotLoaded` whose `generation` no longer matches is a stale,
    /// superseded response and is dropped -- the direct port of `App.tsx`'s
    /// `latestRequest` ref.
    pub snapshot_generation: u64,
    /// The single in-flight mutating action, if any. While `Some`, every
    /// mutating button in the UI is disabled (see `reduce`'s guard at the
    /// top of every `*Requested`/`*Toggled` arm).
    pub pending: Option<PendingKey>,
    pub confirm: Option<Confirm>,
    /// Whether a window is currently open (shown) rather than hidden to the
    /// tray or never created. Deliberately not `iced::window::Id` -- this
    /// crate has no iced dependency yet (see M-2/M-3 split in the plan);
    /// the real `Id` is owned by the `crates/frank-gui` shell.
    pub window_open: bool,
    /// Set once the platform layer reports the real OS autostart state, so
    /// the Settings page can warn if it has drifted from
    /// `snapshot.settings.gui.launch_at_login` (today this can silently
    /// diverge with no way to notice).
    pub autostart_actual: Option<bool>,
}
