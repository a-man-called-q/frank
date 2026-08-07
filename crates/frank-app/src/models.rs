use std::path::PathBuf;
use std::time::Instant;

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

/// Monotonic time used by the prepare/apply boundary. Production uses
/// `Instant::now`; tests can inject a deterministic clock to exercise expiry
/// without sleeping or weakening the one-shot-plan contract.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrankPaths {
    pub config_dir: PathBuf,
    pub data_root: PathBuf,
    pub user_config_dir: PathBuf,
    pub cwd: PathBuf,
    pub frank_bin: PathBuf,
}

impl FrankPaths {
    pub fn from_process() -> Self {
        let home = frank_safeio::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"));
        let data_root = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("share"))
            .join("frank");
        let user_config_dir = if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("frank")
        } else {
            #[cfg(windows)]
            if let Some(appdata) = std::env::var_os("APPDATA") {
                PathBuf::from(appdata).join("frank")
            } else {
                home.join(".config").join("frank")
            }
            #[cfg(not(windows))]
            {
                home.join(".config").join("frank")
            }
        };
        Self {
            config_dir,
            data_root,
            user_config_dir,
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            frank_bin: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("frank")),
        }
    }

    pub fn user_config_dir(&self) -> PathBuf {
        self.user_config_dir.clone()
    }

    pub fn user_config_path(&self) -> PathBuf {
        self.user_config_dir().join("config.toml")
    }

    /// The state files `frank-state`'s engine reads and writes: the active
    /// flag, its one-shot-restore backup, and the mode-transition log.
    pub fn flag_paths(&self) -> frank_state::FlagPaths {
        frank_state::FlagPaths::under(&self.config_dir)
    }

    /// The two ledger files `frank-ledger` reads and appends to: the
    /// per-turn injection log and the lifetime session history.
    pub fn ledger_paths(&self) -> frank_ledger::LedgerPaths {
        frank_ledger::LedgerPaths::under(&self.config_dir)
    }

    pub fn active_flag_path(&self) -> PathBuf {
        self.flag_paths().active
    }

    /// Override the executable used in generated target hook commands. The
    /// CLI uses `current_exe`; the desktop adapter points this at the `frank`
    /// binary bundled alongside it so installing from the GUI still leaves a
    /// hook that can run without the GUI process.
    pub fn with_frank_bin(mut self, frank_bin: PathBuf) -> Self {
        self.frank_bin = frank_bin;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("pack operation failed: {0}")]
    Pack(#[from] frank_pack::PackStoreError),
    #[error("target operation failed: {0}")]
    Target(#[from] frank_target::ApplyError),
    #[error("safe IO failed: {0}")]
    SafeIo(#[from] frank_safeio::SafeIoError),
    #[error("configuration at {path} is invalid: {reason}")]
    Config { path: PathBuf, reason: String },
    #[error("target '{0}' was not found")]
    UnknownTarget(String),
    #[error("level '{0}' is not valid for the active pack")]
    UnknownLevel(String),
    #[error("pack source must be a local directory")]
    InvalidPackSource,
    #[error("prepared plan is unknown, expired, already used, or stale")]
    StalePlan,
    #[error("prepared plan could not be applied: {0}")]
    Apply(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GuiSettings {
    pub launch_at_login: bool,
    pub close_to_tray: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            close_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UserSettings {
    pub default_level: Option<String>,
    pub gui: GuiSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UserSettingsPatch {
    /// `None` means the caller did not send this field; `Some(None)` means
    /// explicitly remove the user override; `Some(Some(level))` sets it.
    /// Serde's normal `Option<Option<T>>` handling collapses the first two
    /// cases for JSON `null`, so keep the tri-state contract explicit.
    #[serde(deserialize_with = "deserialize_double_option")]
    pub default_level: Option<Option<String>>,
    pub launch_at_login: Option<bool>,
    pub close_to_tray: Option<bool>,
}

fn deserialize_double_option<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LevelSummary {
    pub id: String,
    pub title: Option<String>,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackSummary {
    pub id: String,
    pub version: String,
    pub active: bool,
    pub builtin: bool,
    pub levels: Vec<LevelSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetSummary {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub verified: bool,
    pub soft: bool,
    pub detected: bool,
    pub source: String,
}

/// Target discovery result shared by CLI and desktop adapters. Parse errors
/// remain visible to the CLI while the GUI can still render valid targets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetDiscovery {
    pub targets: Vec<TargetSummary>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosisView {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DiagnosisView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardSnapshot {
    pub active_level: Option<String>,
    pub active_pack: String,
    pub active_pack_version: String,
    pub default_level: String,
    pub settings: UserSettings,
    pub packs: Vec<PackSummary>,
    pub targets: Vec<TargetSummary>,
    /// Target manifests that could not be parsed. Valid targets remain
    /// visible, but the GUI/CLI must not silently hide a broken integration
    /// declaration.
    pub target_errors: Vec<String>,
    pub diagnoses: Vec<DiagnosisView>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TargetOperation {
    Install,
    Uninstall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PackOperationKind {
    Add,
    Use,
    Remove,
}

/// A pack mutation is prepared before it is applied. The source and selector
/// stay inside the typed backend operation; the GUI only receives the opaque
/// preview id and cannot manufacture a different operation at apply time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PackOperation {
    Add {
        source: PathBuf,
        expected_sha256: Option<String>,
    },
    Use {
        selector: String,
    },
    Remove {
        selector: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanPreview {
    pub plan_id: String,
    pub target_id: String,
    pub operation: TargetOperation,
    pub actions: Vec<String>,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackPlanPreview {
    pub plan_id: String,
    pub operation: PackOperationKind,
    pub selector: String,
    pub actions: Vec<String>,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationResult {
    pub target_id: String,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackOperationResult {
    pub operation: PackOperationKind,
    pub selector: String,
    pub pack: Option<PackSummary>,
}
