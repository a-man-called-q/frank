//! Shared application services for Frank frontends.
//!
//! `frank-cli` and the desktop UI both call this crate.  Domain crates remain
//! responsible for their own invariants; this crate only supplies paths,
//! orchestration, serializable view models, and the prepare/apply boundary
//! needed by a confirmation-based UI.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod builtin;

pub use builtin::builtin_pack;

const PLAN_TTL: Duration = Duration::from_secs(5 * 60);

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

    pub fn active_flag_path(&self) -> PathBuf {
        self.config_dir.join(".frank-active")
    }

    /// Override the executable used in generated target hook commands. The
    /// CLI uses `current_exe`; the desktop adapter points this at the bundled
    /// sidecar so installing from the GUI still leaves a hook that can run
    /// without the GUI process.
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
    /// cases for JSON/Tauri `null`, so keep the tri-state contract explicit.
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

struct PreparedPlan {
    plan: frank_target::InstallPlan,
    state_fingerprint: String,
    created: Instant,
    operation: TargetOperation,
}

struct PreparedPackPlan {
    operation: PackOperation,
    state_fingerprint: String,
    created: Instant,
}

#[derive(Clone)]
pub struct FrankService {
    paths: FrankPaths,
    prepared: Arc<Mutex<HashMap<String, PreparedPlan>>>,
    prepared_packs: Arc<Mutex<HashMap<String, PreparedPackPlan>>>,
    clock: Arc<dyn Clock>,
    plan_nonce: Arc<AtomicU64>,
}

impl FrankService {
    pub fn new(paths: FrankPaths) -> Self {
        Self::with_clock(paths, Arc::new(SystemClock))
    }

    pub fn with_clock(paths: FrankPaths, clock: Arc<dyn Clock>) -> Self {
        Self {
            paths,
            prepared: Arc::new(Mutex::new(HashMap::new())),
            prepared_packs: Arc::new(Mutex::new(HashMap::new())),
            clock,
            plan_nonce: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn paths(&self) -> &FrankPaths {
        &self.paths
    }

    pub fn settings(&self) -> Result<UserSettings, AppError> {
        read_settings(&self.paths.user_config_path())
    }

    /// Resolve the default level once for every frontend. The state crate
    /// owns the precedence contract (environment, then repository, then user
    /// config, then pack default), while FrankPaths supplies the same cwd/config roots
    /// to CLI, GUI, and hooks.
    pub fn effective_default_level(&self) -> Result<String, AppError> {
        let pack = self.current_pack()?;
        Ok(frank_state::resolve_default_level_with_user_dir(
            &pack,
            &self.paths.cwd,
            "FRANK_DEFAULT_LEVEL",
            Some(&self.paths.user_config_dir),
        ))
    }

    /// Build and record the ledger report used by both the CLI stats command
    /// and the UserPromptSubmit hook. Keeping session discovery, mode-log
    /// joining, and history writes here prevents the desktop adapter from
    /// drifting away from hook/CLI accounting.
    pub fn build_and_record_stats(
        &self,
        session_override: Option<&Path>,
    ) -> frank_ledger::SessionReport {
        let compiled = self.current_pack().unwrap_or_else(|_| builtin_pack());
        let session_path = session_override
            .map(PathBuf::from)
            .or_else(|| frank_ledger::find_recent_session(&self.paths.config_dir));

        let Some(session_path) = session_path else {
            return frank_ledger::SessionReport {
                session_path: None,
                session_id: None,
                turns: 0,
                model: None,
                attribution: frank_ledger::attribute_by_mode(&[], &[], None, None),
                injection_activate_bytes: 0,
                injection_reinforce_bytes: 0,
            };
        };

        let mode_log_path = self.paths.config_dir.join(".frank-mode-log.jsonl");
        let ledger_path = self.paths.config_dir.join(".frank-ledger.jsonl");
        let valid = valid_values(&compiled);
        let current_mode = frank_safeio::read_flag(&self.paths.active_flag_path(), &valid);
        let flag_mtime_ms = std::fs::metadata(self.paths.active_flag_path())
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);

        let report = frank_ledger::build_session_report(
            &session_path,
            &mode_log_path,
            &ledger_path,
            &compiled,
            current_mode.as_deref(),
            flag_mtime_ms,
        );

        if report.turns > 0 {
            if let Some(session_id) = &report.session_id {
                frank_ledger::stats::append_history(
                    &self.paths.config_dir.join(".frank-history.jsonl"),
                    &frank_ledger::HistoryRow {
                        ts: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0),
                        session_id: session_id.clone(),
                        model: report.model.clone(),
                        output_tokens: frank_ledger::stats::measured_output_total(
                            &report.attribution,
                        ),
                        input_tokens: frank_ledger::stats::measured_input_total(
                            &report.attribution,
                        ),
                        turns: report.turns,
                    },
                );
            }
        }

        report
    }

    pub fn update_settings(&self, patch: UserSettingsPatch) -> Result<UserSettings, AppError> {
        let path = self.paths.user_config_path();
        let mut settings = read_settings(&path)?;
        if let Some(default_level) = patch.default_level {
            if let Some(level) = &default_level {
                let pack = self.current_pack()?;
                if level != "off" && pack.resolve_level(level).is_none() {
                    return Err(AppError::UnknownLevel(level.clone()));
                }
            }
            settings.default_level = default_level;
        }
        if let Some(value) = patch.launch_at_login {
            settings.gui.launch_at_login = value;
        }
        if let Some(value) = patch.close_to_tray {
            settings.gui.close_to_tray = value;
        }
        write_settings(&path, &settings)?;
        Ok(settings)
    }

    pub fn current_pack(&self) -> Result<frank_pack::CompiledPack, AppError> {
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        match store.active()? {
            Some((_, pack)) => Ok(pack),
            None => Ok(builtin_pack()),
        }
    }

    pub fn set_active_level(&self, level: Option<&str>) -> Result<Option<String>, AppError> {
        let pack = self.current_pack()?;
        let canonical = match level {
            None | Some("off") => None,
            Some(raw) => Some(
                pack.resolve_level(raw)
                    .ok_or_else(|| AppError::UnknownLevel(raw.to_string()))?
                    .id
                    .clone(),
            ),
        };
        frank_safeio::write_text_atomic(
            &self.paths.active_flag_path(),
            canonical.as_deref().unwrap_or("off"),
            frank_safeio::MAX_FLAG_BYTES,
        )?;
        Ok(canonical)
    }

    /// Read only the validated active flag. This intentionally does not call
    /// `snapshot()` so a cheap status check is not coupled to target probing
    /// or config parsing.
    pub fn active_level(&self) -> Result<Option<String>, AppError> {
        let pack = self.current_pack()?;
        Ok(
            frank_safeio::read_flag(&self.paths.active_flag_path(), &valid_values(&pack))
                .filter(|v| v != "off"),
        )
    }

    pub fn list_packs(&self) -> Result<Vec<PackSummary>, AppError> {
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        let lock = store.load_lock()?;
        let active = lock.active.clone();
        let builtin = builtin_pack();
        let mut out = vec![pack_summary(&builtin, active.is_none(), true)];
        for installed in lock.packs {
            let pack = store.compile_installed(&installed)?;
            out.push(pack_summary(
                &pack,
                active.as_ref() == Some(&installed.pack_ref()),
                false,
            ));
        }
        Ok(out)
    }

    pub fn add_local_pack(
        &self,
        source: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<PackSummary, AppError> {
        if !source.is_dir() {
            return Err(AppError::InvalidPackSource);
        }
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        let loaded = frank_pack::PackSource::load(source).map_err(|e| AppError::Config {
            path: source.to_path_buf(),
            reason: e.to_string(),
        })?;
        let preview = frank_pack::compile(&loaded).map_err(|e| AppError::Config {
            path: source.to_path_buf(),
            reason: e.to_string(),
        })?;
        if preview.id == builtin::PACK_ID {
            return Err(AppError::Config {
                path: source.to_path_buf(),
                reason: "the built-in pack id is reserved".to_string(),
            });
        }
        store.add_local(source, expected_sha256)?;
        Ok(pack_summary(&preview, false, false))
    }

    pub fn use_pack(&self, selector: &str) -> Result<(), AppError> {
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        if selector == builtin::PACK_ID
            || selector == format!("{}@{}", builtin::PACK_ID, builtin::PACK_VERSION)
        {
            store.set_active(None)?;
            return Ok(());
        }
        let installed = store.find(selector)?;
        store.compile_installed(&installed)?;
        store.set_active(Some(installed.pack_ref()))?;
        Ok(())
    }

    pub fn remove_pack(&self, selector: &str) -> Result<(), AppError> {
        if selector == builtin::PACK_ID || selector.starts_with("caveman@") {
            return Err(AppError::Config {
                path: self.paths.data_root.clone(),
                reason: "the built-in pack cannot be removed".to_string(),
            });
        }
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        store.remove(selector)?;
        Ok(())
    }

    /// Prepare a pack mutation for a confirmation UI. This mirrors target
    /// preview/apply: the backend stores the real operation and rejects an
    /// expired, already-consumed, or externally changed plan rather than
    /// silently rebasing it on current state.
    pub fn prepare_pack_change(
        &self,
        operation: PackOperation,
    ) -> Result<PackPlanPreview, AppError> {
        let (kind, selector, action, source) = match &operation {
            PackOperation::Add {
                source,
                expected_sha256,
            } => {
                if !source.is_dir() {
                    return Err(AppError::InvalidPackSource);
                }
                let loaded =
                    frank_pack::PackSource::load(source).map_err(|e| AppError::Config {
                        path: source.clone(),
                        reason: e.to_string(),
                    })?;
                let compiled = frank_pack::compile(&loaded).map_err(|e| AppError::Config {
                    path: source.clone(),
                    reason: e.to_string(),
                })?;
                if compiled.id == builtin::PACK_ID {
                    return Err(AppError::Config {
                        path: source.clone(),
                        reason: "the built-in pack id is reserved".to_string(),
                    });
                }
                if let Some(expected) = expected_sha256 {
                    let expected = expected.trim().to_ascii_lowercase();
                    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err(AppError::Config {
                            path: source.clone(),
                            reason: "expected SHA-256 must be 64 hexadecimal characters"
                                .to_string(),
                        });
                    }
                    let actual = frank_pack::directory_sha256(source)?;
                    if expected != actual {
                        return Err(AppError::Pack(frank_pack::PackStoreError::DigestMismatch {
                            expected,
                            actual,
                        }));
                    }
                }
                (
                    PackOperationKind::Add,
                    format!("{}@{}", compiled.id, compiled.version),
                    format!(
                        "install pack {}@{} from {}",
                        compiled.id,
                        compiled.version,
                        source.display()
                    ),
                    Some(source.clone()),
                )
            }
            PackOperation::Use { selector } => {
                if selector == builtin::PACK_ID
                    || selector.as_str()
                        == format!("{}@{}", builtin::PACK_ID, builtin::PACK_VERSION)
                {
                    (
                        PackOperationKind::Use,
                        selector.clone(),
                        "use built-in pack".to_string(),
                        None,
                    )
                } else {
                    let store = frank_pack::PackStore::new(self.paths.data_root.clone());
                    let installed = store.find(selector)?;
                    store.compile_installed(&installed)?;
                    (
                        PackOperationKind::Use,
                        selector.clone(),
                        format!("use pack {selector}"),
                        None,
                    )
                }
            }
            PackOperation::Remove { selector } => {
                if selector == builtin::PACK_ID || selector.starts_with("caveman@") {
                    return Err(AppError::Config {
                        path: self.paths.data_root.clone(),
                        reason: "the built-in pack cannot be removed".to_string(),
                    });
                }
                let store = frank_pack::PackStore::new(self.paths.data_root.clone());
                store.find(selector)?;
                (
                    PackOperationKind::Remove,
                    selector.clone(),
                    format!("remove pack {selector}"),
                    None,
                )
            }
        };

        let now = self.clock.now();
        let state_fingerprint = self.pack_state_fingerprint(source.as_deref())?;
        let nonce = self.plan_nonce.fetch_add(1, Ordering::Relaxed);
        let plan_id = new_pack_plan_id(&operation, &state_fingerprint, now, nonce);
        let mut prepared = self
            .prepared_packs
            .lock()
            .map_err(|_| AppError::StalePlan)?;
        prepared.retain(|_, p| now.saturating_duration_since(p.created) <= PLAN_TTL);
        prepared.insert(
            plan_id.clone(),
            PreparedPackPlan {
                operation,
                state_fingerprint,
                created: now,
            },
        );
        Ok(PackPlanPreview {
            plan_id,
            operation: kind,
            selector,
            actions: vec![action],
            expires_in_seconds: PLAN_TTL.as_secs(),
        })
    }

    pub fn apply_prepared_pack(&self, plan_id: &str) -> Result<PackOperationResult, AppError> {
        let prepared = self
            .prepared_packs
            .lock()
            .map_err(|_| AppError::StalePlan)?
            .remove(plan_id)
            .ok_or(AppError::StalePlan)?;
        let source = match &prepared.operation {
            PackOperation::Add { source, .. } => Some(source.as_path()),
            PackOperation::Use { .. } | PackOperation::Remove { .. } => None,
        };
        let current_fingerprint = self.pack_state_fingerprint(source).ok();
        if self.clock.now().saturating_duration_since(prepared.created) > PLAN_TTL
            || current_fingerprint.as_deref() != Some(prepared.state_fingerprint.as_str())
        {
            return Err(AppError::StalePlan);
        }
        let (kind, selector, summary) = match prepared.operation {
            PackOperation::Add {
                source,
                expected_sha256,
            } => {
                let summary = self.add_local_pack(&source, expected_sha256.as_deref())?;
                (
                    PackOperationKind::Add,
                    format!("{}@{}", summary.id, summary.version),
                    Some(summary),
                )
            }
            PackOperation::Use { selector } => {
                self.use_pack(&selector)?;
                let summary = self
                    .list_packs()?
                    .into_iter()
                    .find(|p| p.active && format!("{}@{}", p.id, p.version) == selector)
                    .or_else(|| {
                        self.list_packs()
                            .ok()
                            .and_then(|packs| packs.into_iter().find(|p| p.active))
                    });
                (PackOperationKind::Use, selector, summary)
            }
            PackOperation::Remove { selector } => {
                self.remove_pack(&selector)?;
                (PackOperationKind::Remove, selector, None)
            }
        };
        Ok(PackOperationResult {
            operation: kind,
            selector,
            pack: summary,
        })
    }

    pub fn list_targets(&self) -> Vec<TargetSummary> {
        self.discover_targets().targets
    }

    pub fn discover_targets(&self) -> TargetDiscovery {
        let env = frank_target::ProbeEnv::from_process();
        let mut rows = vec![TargetSummary {
            id: "claude-code".to_string(),
            label: "Claude Code".to_string(),
            kind: "native".to_string(),
            verified: true,
            soft: false,
            detected: frank_target::claude_code::ClaudeCodeTarget::detect(&env)
                == frank_target::Detection::Detected,
            source: "built-in".to_string(),
        }];
        let mut errors = Vec::new();
        for (path, parsed) in load_manifests(&self.paths) {
            match parsed {
                Ok(manifest) => {
                    let detected = frank_target::detect::detect(&manifest, &env)
                        == frank_target::Detection::Detected;
                    rows.push(TargetSummary {
                        id: manifest.target.id.clone(),
                        label: manifest.target.label.clone(),
                        kind: manifest.target.kind.clone(),
                        verified: manifest.target.verified,
                        soft: manifest.target.soft,
                        detected,
                        source: path.display().to_string(),
                    });
                }
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        TargetDiscovery {
            targets: rows,
            errors,
        }
    }

    pub fn doctor(&self) -> DoctorReport {
        let ctx = self.install_ctx();
        let mut checks = frank_target::claude_code::ClaudeCodeTarget::doctor(&ctx)
            .into_iter()
            .map(|d| DiagnosisView {
                ok: d.ok,
                message: d.message,
            })
            .collect::<Vec<_>>();
        let settings_check = match self.settings() {
            Err(error) => DiagnosisView {
                ok: false,
                message: format!("Frank user config is invalid: {error}"),
            },
            Ok(settings) => {
                let valid = settings.default_level.as_deref().is_none_or(|level| {
                    level == "off"
                        || self
                            .current_pack()
                            .ok()
                            .is_some_and(|pack| pack.resolve_level(level).is_some())
                });
                DiagnosisView {
                    ok: valid,
                    message: if valid {
                        "Frank user config is valid".to_string()
                    } else {
                        format!(
                            "Frank user config names an unknown default level: {}",
                            settings.default_level.as_deref().unwrap_or_default()
                        )
                    },
                }
            }
        };
        checks.push(settings_check);
        DoctorReport {
            ok: checks.iter().all(|check| check.ok),
            checks,
        }
    }

    pub fn prepare_target_change(
        &self,
        target_id: &str,
        operation: TargetOperation,
    ) -> Result<PlanPreview, AppError> {
        let plan = self.build_plan(target_id, operation)?;
        let actions = plan.describe();
        let state_fingerprint = self.state_fingerprint(&plan);
        let now = self.clock.now();
        let nonce = self.plan_nonce.fetch_add(1, Ordering::Relaxed);
        let plan_id = new_plan_id(&plan, &state_fingerprint, now, nonce);
        let mut prepared = self.prepared.lock().map_err(|_| AppError::StalePlan)?;
        prepared.retain(|_, p| now.saturating_duration_since(p.created) <= PLAN_TTL);
        prepared.insert(
            plan_id.clone(),
            PreparedPlan {
                plan,
                state_fingerprint,
                created: now,
                operation,
            },
        );
        Ok(PlanPreview {
            plan_id,
            target_id: target_id.to_string(),
            operation,
            actions,
            expires_in_seconds: PLAN_TTL.as_secs(),
        })
    }

    pub fn apply_prepared_plan(&self, plan_id: &str) -> Result<OperationResult, AppError> {
        let prepared = self
            .prepared
            .lock()
            .map_err(|_| AppError::StalePlan)?
            .remove(plan_id)
            .ok_or(AppError::StalePlan)?;
        if self.clock.now().saturating_duration_since(prepared.created) > PLAN_TTL
            || self.state_fingerprint(&prepared.plan) != prepared.state_fingerprint
        {
            return Err(AppError::StalePlan);
        }
        let target_id = prepared.plan.target_id.clone();
        let log =
            frank_target::apply(&prepared.plan).map_err(|e| AppError::Apply(e.to_string()))?;
        let _ = prepared.operation;
        Ok(OperationResult { target_id, log })
    }

    pub fn snapshot(&self) -> Result<DashboardSnapshot, AppError> {
        let pack = self.current_pack()?;
        // A damaged user config must not strand the desktop on its loading
        // screen. Keep the view model renderable with defaults and let the
        // diagnostics panel report the precise parse error. The strict
        // `settings()` API remains fallible for callers that need to refuse a
        // write, while a status snapshot is deliberately read-only/fail-soft.
        let settings = self.settings().unwrap_or_default();
        let default_level = frank_state::resolve_default_level_with_user_dir(
            &pack,
            &self.paths.cwd,
            "FRANK_DEFAULT_LEVEL",
            Some(&self.paths.user_config_dir),
        );
        let active_level =
            frank_safeio::read_flag(&self.paths.active_flag_path(), &valid_values(&pack))
                .filter(|v| v != "off");
        let active_pack = self
            .list_packs()?
            .into_iter()
            .find(|p| p.active)
            .unwrap_or_else(|| pack_summary(&pack, true, true));
        let discovery = self.discover_targets();
        Ok(DashboardSnapshot {
            active_level,
            active_pack: active_pack.id,
            active_pack_version: active_pack.version,
            default_level,
            settings,
            packs: self.list_packs()?,
            targets: discovery.targets,
            target_errors: discovery.errors,
            diagnoses: self.doctor().checks,
        })
    }

    fn install_ctx(&self) -> frank_target::InstallCtx {
        frank_target::InstallCtx {
            config_dir: self.paths.config_dir.clone(),
            frank_bin: self.paths.frank_bin.clone(),
            cwd: self.paths.cwd.clone(),
        }
    }

    fn state_fingerprint(&self, plan: &frank_target::InstallPlan) -> String {
        let mut h = Sha256::new();
        h.update(current_fingerprint(plan).as_bytes());
        for path in [
            self.paths.active_flag_path(),
            self.paths.data_root.join("packs.lock"),
            self.paths.user_config_path(),
        ] {
            h.update(path.to_string_lossy().as_bytes());
            h.update(fingerprint_path(&path).as_bytes());
        }
        if let Ok(pack) = self.current_pack() {
            h.update(pack.id.as_bytes());
            h.update(pack.version.as_bytes());
            h.update(pack.default_level.as_bytes());
            for (id, level) in &pack.levels {
                h.update(id.as_bytes());
                h.update(level.activation_prompt.as_bytes());
                h.update(level.reinforce.as_bytes());
            }
        }
        format!("{:x}", h.finalize())
    }

    fn pack_state_fingerprint(&self, source: Option<&Path>) -> Result<String, AppError> {
        let mut h = Sha256::new();
        let lock = self.paths.data_root.join("packs.lock");
        h.update(fingerprint_path(&lock).as_bytes());
        h.update(self.paths.data_root.to_string_lossy().as_bytes());
        // The lockfile alone is not enough for a one-shot `use`/`remove`
        // plan: an external editor can mutate an installed pack directory
        // without touching packs.lock. Include every locked copy's bounded,
        // symlink-rejecting content digest so apply refuses to operate on a
        // pack state different from the one the preview described.
        if let Ok(lock_data) = frank_pack::PackStore::new(self.paths.data_root.clone()).load_lock()
        {
            for installed in lock_data.packs {
                let path = self.paths.data_root.join(&installed.path);
                h.update(path.to_string_lossy().as_bytes());
                match frank_pack::directory_sha256(&path) {
                    Ok(digest) => h.update(digest.as_bytes()),
                    Err(error) => h.update(format!("error:{error}").as_bytes()),
                }
            }
        }
        if let Some(source) = source {
            let digest = frank_pack::directory_sha256(source).map_err(AppError::from)?;
            h.update(source.to_string_lossy().as_bytes());
            h.update(digest.as_bytes());
        }
        Ok(format!("{:x}", h.finalize()))
    }

    fn build_plan(
        &self,
        target_id: &str,
        operation: TargetOperation,
    ) -> Result<frank_target::InstallPlan, AppError> {
        let ctx = self.install_ctx();
        if target_id == "claude-code" {
            let plan = match operation {
                TargetOperation::Install => {
                    frank_target::claude_code::ClaudeCodeTarget::plan_install(&ctx)
                }
                TargetOperation::Uninstall => {
                    frank_target::claude_code::ClaudeCodeTarget::plan_uninstall(&ctx)
                }
            };
            plan.validate_scope()?;
            return Ok(plan);
        }
        let manifest = load_manifests(&self.paths)
            .into_iter()
            .find_map(|(_, parsed)| match parsed {
                Ok(m) if m.target.id == target_id => Some(m),
                _ => None,
            })
            .ok_or_else(|| AppError::UnknownTarget(target_id.to_string()))?;
        let pack = self.current_pack()?;
        let plan = match operation {
            TargetOperation::Install => {
                frank_target::generic::build_install_plan(&manifest, &ctx, |reference| {
                    resolve_body_ref(reference, &pack)
                })
            }
            TargetOperation::Uninstall => {
                frank_target::generic::build_uninstall_plan(&manifest, &ctx)
            }
        };
        plan.validate_scope()?;
        Ok(plan)
    }
}

fn valid_values(pack: &frank_pack::CompiledPack) -> Vec<&str> {
    let mut values = pack.levels.keys().map(String::as_str).collect::<Vec<_>>();
    values.extend(pack.oneshots.keys().map(String::as_str));
    values.push("off");
    values
}

fn pack_summary(pack: &frank_pack::CompiledPack, active: bool, builtin: bool) -> PackSummary {
    PackSummary {
        id: pack.id.clone(),
        version: pack.version.clone(),
        active,
        builtin,
        levels: pack
            .levels
            .values()
            .map(|l| LevelSummary {
                id: l.id.clone(),
                title: l.title.clone(),
                aliases: l.aliases.clone(),
            })
            .collect(),
    }
}

fn resolve_body_ref(reference: &str, pack: &frank_pack::CompiledPack) -> Option<String> {
    (reference == "pack:static_digest")
        .then(|| {
            pack.resolve_level(&pack.default_level)
                .map(|l| l.activation_prompt.clone())
        })
        .flatten()
}

fn load_manifests(
    paths: &FrankPaths,
) -> Vec<(
    PathBuf,
    Result<frank_target::manifest::TargetManifest, String>,
)> {
    let mut dirs = vec![paths.user_config_dir().join("targets")];
    dirs.push(paths.cwd.join("targets"));
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let parsed = frank_safeio::read_text_capped(&path, frank_safeio::MAX_CONFIG_BYTES)
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    toml::from_str::<frank_target::manifest::TargetManifest>(&raw)
                        .map_err(|e| e.to_string())
                });
            if let Ok(m) = &parsed {
                if !seen.insert(m.target.id.clone()) {
                    continue;
                }
            }
            out.push((path, parsed));
        }
    }
    out
}

fn read_settings(path: &Path) -> Result<UserSettings, AppError> {
    let raw = match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
        Ok(raw) => raw,
        Err(frank_safeio::SafeIoError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UserSettings::default());
        }
        Err(e) => return Err(AppError::SafeIo(e)),
    };
    toml::from_str(&raw).map_err(|e| AppError::Config {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

fn write_settings(path: &Path, settings: &UserSettings) -> Result<(), AppError> {
    let mut doc = match frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES) {
        Ok(raw) => raw
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| AppError::Config {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?,
        Err(frank_safeio::SafeIoError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            toml_edit::DocumentMut::new()
        }
        Err(e) => return Err(AppError::SafeIo(e)),
    };
    match &settings.default_level {
        Some(level) => doc["default_level"] = toml_edit::value(level.clone()),
        None => {
            doc.remove("default_level");
        }
    }
    doc["gui"]["launch_at_login"] = toml_edit::value(settings.gui.launch_at_login);
    doc["gui"]["close_to_tray"] = toml_edit::value(settings.gui.close_to_tray);
    frank_safeio::write_text_atomic(path, &doc.to_string(), frank_safeio::MAX_CONFIG_BYTES)?;
    Ok(())
}

fn action_paths(plan: &frank_target::InstallPlan) -> Vec<PathBuf> {
    plan.actions
        .iter()
        .flat_map(|action| match action {
            frank_target::Action::EnsureDir(p) => vec![p.clone()],
            frank_target::Action::BackupIfAbsent { path, backup_path } => {
                vec![path.clone(), backup_path.clone()]
            }
            frank_target::Action::MergeSettingsHooks { settings_path, .. } => {
                vec![settings_path.clone()]
            }
            frank_target::Action::RemoveSettingsHooks { settings_path, .. } => {
                vec![settings_path.clone()]
            }
            frank_target::Action::RemoveFileIfManaged { path, .. } => vec![path.clone()],
            frank_target::Action::MarkdownBlockAppend { path, .. } => vec![path.clone()],
            frank_target::Action::MarkdownBlockRemove { path, .. } => vec![path.clone()],
            frank_target::Action::SpawnSteps { .. } | frank_target::Action::Noop { .. } => {
                Vec::new()
            }
        })
        .collect()
}

fn current_fingerprint(plan: &frank_target::InstallPlan) -> String {
    let paths = action_paths(plan);
    let mut h = Sha256::new();
    for path in paths {
        h.update(path.to_string_lossy().as_bytes());
        h.update(fingerprint_path(&path).as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn fingerprint_path(path: &Path) -> String {
    let mut h = Sha256::new();
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            h.update([1, m.file_type().is_symlink() as u8]);
            h.update(m.len().to_le_bytes());
            h.update(
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
                    .to_le_bytes(),
            );
            if m.file_type().is_symlink() {
                if let Ok(target) = std::fs::read_link(path) {
                    h.update(target.as_os_str().to_string_lossy().as_bytes());
                }
            } else if m.is_file() {
                // Metadata alone is not enough: an external edit can
                // preserve size and, on coarse filesystems, timestamps.
                // Hash bounded file contents as part of the stale-plan
                // guard, while avoiding an unbounded read of a hostile path.
                let mut file_digest = Sha256::new();
                if let Ok(mut file) = std::fs::File::open(path) {
                    let mut buf = [0_u8; 8192];
                    let mut remaining = frank_safeio::MAX_CONFIG_BYTES;
                    while remaining > 0 {
                        let want = remaining.min(buf.len());
                        match file.read(&mut buf[..want]) {
                            Ok(0) => break,
                            Ok(n) => {
                                file_digest.update(&buf[..n]);
                                remaining -= n;
                            }
                            Err(_) => {
                                file_digest.update([0xff]);
                                break;
                            }
                        }
                    }
                    if remaining == 0 {
                        file_digest.update([0xfe]);
                    }
                } else {
                    file_digest.update([0xfd]);
                }
                h.update(file_digest.finalize());
            }
        }
        Err(_) => h.update([0]),
    }
    format!("{:x}", h.finalize())
}

fn plan_fingerprint(plan: &frank_target::InstallPlan) -> String {
    // The action description is stable and captures the exact plan identity;
    // current_fingerprint is used separately to detect external changes.
    let mut h = Sha256::new();
    h.update(plan.target_id.as_bytes());
    for action in plan.describe() {
        h.update(action.as_bytes());
        h.update([0]);
    }
    format!("{:x}", h.finalize())
}

fn new_plan_id(
    plan: &frank_target::InstallPlan,
    state_fingerprint: &str,
    now: Instant,
    nonce: u64,
) -> String {
    let mut h = Sha256::new();
    h.update(plan_fingerprint(plan).as_bytes());
    h.update(state_fingerprint.as_bytes());
    h.update(format!("{:?}", now).as_bytes());
    h.update(nonce.to_le_bytes());
    format!("frank-plan-{:x}", h.finalize())
}

fn new_pack_plan_id(
    operation: &PackOperation,
    state_fingerprint: &str,
    now: Instant,
    nonce: u64,
) -> String {
    let mut h = Sha256::new();
    h.update(format!("{operation:?}").as_bytes());
    h.update(state_fingerprint.as_bytes());
    h.update(format!("{:?}", now).as_bytes());
    h.update(nonce.to_le_bytes());
    format!("frank-pack-plan-{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::fs;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    fn paths(root: &Path) -> FrankPaths {
        FrankPaths {
            config_dir: root.join("claude"),
            data_root: root.join("data"),
            user_config_dir: root.join("config"),
            cwd: root.to_path_buf(),
            frank_bin: root.join("bin/frank"),
        }
    }

    struct TestClock {
        now: Mutex<Instant>,
    }

    impl TestClock {
        fn advance(&self, by: Duration) {
            let mut now = self.now.lock().unwrap();
            *now += by;
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }
    }

    #[test]
    fn settings_round_trip_preserves_unknown_fields_and_comments() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(p.user_config_dir()).unwrap();
        fs::write(p.user_config_path(), "# keep\nfuture = true\n").unwrap();
        let service = FrankService::new(p.clone());
        service
            .update_settings(UserSettingsPatch {
                close_to_tray: Some(false),
                ..Default::default()
            })
            .unwrap();
        let raw = fs::read_to_string(p.user_config_path()).unwrap();
        assert!(raw.contains("# keep"));
        assert!(raw.contains("future = true"));
        assert!(raw.contains("close_to_tray = false"));
    }

    #[test]
    fn malformed_gui_table_is_rejected_without_panicking() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(p.user_config_dir()).unwrap();
        fs::write(p.user_config_path(), "gui = \"not-a-table\"\n").unwrap();
        let result = FrankService::new(p).update_settings(UserSettingsPatch {
            close_to_tray: Some(false),
            ..Default::default()
        });
        assert!(matches!(result, Err(AppError::Config { .. })));
    }

    #[test]
    fn doctor_surfaces_an_unknown_user_default_level() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(p.user_config_dir()).unwrap();
        fs::write(p.user_config_path(), "default_level = \"does-not-exist\"\n").unwrap();

        let report = FrankService::new(p).doctor();
        assert!(!report.ok);
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.message.contains("unknown default level"))
        );
    }

    #[test]
    fn snapshot_stays_renderable_when_user_config_is_malformed() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(&p.user_config_dir).unwrap();
        fs::write(p.user_config_path(), "gui = \"not-a-table\"\n").unwrap();

        let snapshot = FrankService::new(p).snapshot().unwrap();
        assert_eq!(snapshot.settings, UserSettings::default());
        assert!(
            snapshot
                .diagnoses
                .iter()
                .any(|check| check.message.contains("user config is invalid"))
        );
    }

    #[test]
    fn active_level_is_canonical_and_off_is_explicit() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let service = FrankService::new(p.clone());
        assert_eq!(
            service.set_active_level(Some("lite")).unwrap(),
            Some("lite".into())
        );
        assert_eq!(service.set_active_level(None).unwrap(), None);
        assert_eq!(fs::read_to_string(p.active_flag_path()).unwrap(), "off");
    }

    #[test]
    fn default_level_patch_accepts_valid_levels_and_rejects_unknown_levels() {
        let tmp = tempdir().unwrap();
        let service = FrankService::new(paths(tmp.path()));
        let updated = service
            .update_settings(UserSettingsPatch {
                default_level: Some(Some("lite".into())),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(updated.default_level.as_deref(), Some("lite"));
        assert!(matches!(
            service.update_settings(UserSettingsPatch {
                default_level: Some(Some("does-not-exist".into())),
                ..Default::default()
            }),
            Err(AppError::UnknownLevel(_))
        ));
    }

    #[test]
    fn settings_patch_preserves_absent_and_explicit_null_as_distinct_states() {
        let absent: UserSettingsPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(absent.default_level, None);

        let clear: UserSettingsPatch = serde_json::from_str(r#"{"default_level":null}"#).unwrap();
        assert_eq!(clear.default_level, Some(None));

        let set: UserSettingsPatch = serde_json::from_str(r#"{"default_level":"full"}"#).unwrap();
        assert_eq!(set.default_level, Some(Some("full".to_string())));
    }

    #[test]
    fn stats_orchestration_uses_shared_paths_and_records_history() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(&p.config_dir).unwrap();
        let session = tmp.path().join("session.jsonl");
        fs::write(
            &session,
            r#"{"type":"assistant","timestamp":"2026-08-01T00:00:00.000Z","message":{"model":"m","usage":{"output_tokens":12,"input_tokens":8}}}"#,
        )
        .unwrap();

        let report = FrankService::new(p.clone()).build_and_record_stats(Some(&session));
        assert_eq!(report.turns, 1);
        assert_eq!(report.session_id.as_deref(), Some("session"));
        let history = frank_ledger::stats::read_history(&p.config_dir.join(".frank-history.jsonl"));
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].output_tokens, 12);
    }

    #[test]
    fn prepared_plan_cannot_be_applied_twice() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(&p.config_dir).unwrap();
        let service = FrankService::new(p);
        let preview = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        let _ = service.apply_prepared_plan(&preview.plan_id).unwrap();
        assert!(matches!(
            service.apply_prepared_plan(&preview.plan_id),
            Err(AppError::StalePlan)
        ));
    }

    #[test]
    fn prepared_plan_becomes_stale_after_external_state_change() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let service = FrankService::new(p.clone());
        let preview = service
            .prepare_target_change("claude-code", TargetOperation::Install)
            .unwrap();
        fs::create_dir_all(&p.config_dir).unwrap();
        fs::write(p.config_dir.join("settings.json"), "{}\n").unwrap();
        assert!(matches!(
            service.apply_prepared_plan(&preview.plan_id),
            Err(AppError::StalePlan)
        ));
    }

    #[test]
    fn prepared_plan_expires_without_sleeping() {
        let tmp = tempdir().unwrap();
        let clock = Arc::new(TestClock {
            now: Mutex::new(Instant::now()),
        });
        let service =
            FrankService::with_clock(paths(tmp.path()), Arc::clone(&clock) as Arc<dyn Clock>);
        let preview = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        clock.advance(PLAN_TTL + Duration::from_nanos(1));
        assert!(matches!(
            service.apply_prepared_plan(&preview.plan_id),
            Err(AppError::StalePlan)
        ));
    }

    #[test]
    fn concurrent_apply_has_exactly_one_winner() {
        let tmp = tempdir().unwrap();
        let service = Arc::new(FrankService::new(paths(tmp.path())));
        let preview = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        let mut joins = Vec::new();
        for _ in 0..8 {
            let service = Arc::clone(&service);
            let id = preview.plan_id.clone();
            joins.push(thread::spawn(move || {
                service.apply_prepared_plan(&id).is_ok()
            }));
        }
        let winners = joins
            .into_iter()
            .filter_map(|j| j.join().ok())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
    }

    #[test]
    fn pack_preview_is_one_shot_and_does_not_rebase_after_switch() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let service = FrankService::new(p.clone());
        let preview = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        let result = service.apply_prepared_pack(&preview.plan_id).unwrap();
        assert_eq!(result.operation, PackOperationKind::Use);
        assert!(matches!(
            service.apply_prepared_pack(&preview.plan_id),
            Err(AppError::StalePlan)
        ));
    }

    #[test]
    fn pack_add_preview_rejects_digest_mismatch_without_installing() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let service = FrankService::new(p.clone());
        let source = tmp.path().join("pack");
        fs::create_dir_all(source.join("levels")).unwrap();
        fs::write(
            source.join("pack.toml"),
            r#"schema = 1
[pack]
id = "local"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#,
        )
        .unwrap();
        fs::write(source.join("levels/full.md"), "Be concise.").unwrap();

        let error = service
            .prepare_pack_change(PackOperation::Add {
                source: source.clone(),
                expected_sha256: Some("0".repeat(64)),
            })
            .unwrap_err();
        assert!(matches!(error, AppError::Pack(_)));
        assert!(!p.data_root.join("packs.lock").exists());
    }

    #[test]
    fn snapshot_exposes_doctor_and_malformed_target_manifests() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(tmp.path().join("targets")).unwrap();
        fs::write(tmp.path().join("targets/broken.toml"), "[not valid").unwrap();
        let snapshot = FrankService::new(p).snapshot().unwrap();
        assert_eq!(snapshot.active_pack, "caveman");
        assert_eq!(snapshot.default_level, "full");
        assert!(
            snapshot
                .targets
                .iter()
                .any(|target| target.id == "claude-code")
        );
        assert!(!snapshot.target_errors.is_empty());
        assert!(snapshot.diagnoses.len() >= 3);
        assert!(!snapshot.diagnoses.iter().all(|check| check.ok));
    }

    #[test]
    fn explicit_user_config_participates_in_default_level_precedence() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(p.user_config_dir()).unwrap();
        fs::write(
            p.user_config_path(),
            "# user preference\ndefault_level = \"lite\"\n",
        )
        .unwrap();
        assert_eq!(
            FrankService::new(p).effective_default_level().unwrap(),
            "lite"
        );
    }

    #[test]
    fn local_pack_can_be_added_selected_listed_and_removed() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let source = tmp.path().join("my-pack");
        fs::create_dir_all(source.join("levels")).unwrap();
        fs::write(
            source.join("pack.toml"),
            r#"schema = 1
[pack]
id = "local"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#,
        )
        .unwrap();
        fs::write(source.join("levels/full.md"), "Respond with short answers.").unwrap();
        let service = FrankService::new(p.clone());
        let added = service.add_local_pack(&source, None).unwrap();
        assert_eq!(added.id, "local");
        assert!(
            service
                .list_packs()
                .unwrap()
                .iter()
                .any(|pack| pack.id == "local")
        );
        service.use_pack("local").unwrap();
        assert_eq!(service.current_pack().unwrap().id, "local");
        service.remove_pack("local").unwrap();
        assert!(
            service
                .list_packs()
                .unwrap()
                .iter()
                .all(|pack| pack.id != "local")
        );
    }

    #[test]
    fn pack_plan_rejects_an_external_edit_to_the_locked_copy() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let source = tmp.path().join("pack");
        fs::create_dir_all(source.join("levels")).unwrap();
        fs::write(
            source.join("pack.toml"),
            r#"schema = 1
[pack]
id = "mutable"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#,
        )
        .unwrap();
        fs::write(source.join("levels/full.md"), "Original rules.").unwrap();
        let service = FrankService::new(p.clone());
        service.add_local_pack(&source, None).unwrap();

        let preview = service
            .prepare_pack_change(PackOperation::Use {
                selector: "mutable".into(),
            })
            .unwrap();
        fs::write(
            p.data_root.join("packs/mutable@1.0.0/levels/full.md"),
            "Externally changed rules.",
        )
        .unwrap();

        assert!(matches!(
            service.apply_prepared_pack(&preview.plan_id),
            Err(AppError::StalePlan)
        ));
    }

    #[test]
    fn generic_target_plan_uses_the_same_preview_actions_as_apply() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(tmp.path().join("targets")).unwrap();
        fs::write(
            tmp.path().join("targets/generic.toml"),
            r#"schema = 1
[target]
id = "generic"
label = "Generic"
kind = "generic"
verified = true
[install]
strategy = "markdown-block"
[install.markdown]
path = "./AGENTS.md"
begin = "<!-- frank:begin -->"
end = "<!-- frank:end -->"
body = "pack:static_digest"
create_if_missing = true
"#,
        )
        .unwrap();
        let service = FrankService::new(p.clone());
        let preview = service
            .prepare_target_change("generic", TargetOperation::Install)
            .unwrap();
        assert_eq!(preview.actions.len(), 1);
        let result = service.apply_prepared_plan(&preview.plan_id).unwrap();
        assert_eq!(result.target_id, "generic");
        let text = fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap();
        assert!(text.contains("<!-- frank:begin -->"));

        let uninstall = service
            .prepare_target_change("generic", TargetOperation::Uninstall)
            .unwrap();
        assert_eq!(uninstall.actions.len(), 1);
        service.apply_prepared_plan(&uninstall.plan_id).unwrap();
        assert!(!tmp.path().join("AGENTS.md").exists());
    }

    #[test]
    fn changing_settings_makes_a_prepared_target_plan_stale() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let service = FrankService::new(p.clone());
        let preview = service
            .prepare_target_change("claude-code", TargetOperation::Install)
            .unwrap();
        service
            .update_settings(UserSettingsPatch {
                close_to_tray: Some(false),
                ..Default::default()
            })
            .unwrap();
        assert!(matches!(
            service.apply_prepared_plan(&preview.plan_id),
            Err(AppError::StalePlan)
        ));
    }

    proptest! {
        #[test]
        fn settings_patch_preserves_boolean_invariants(launch in any::<bool>(), tray in any::<bool>()) {
            let tmp = tempdir().unwrap();
            let service = FrankService::new(paths(tmp.path()));
            let settings = service.update_settings(UserSettingsPatch {
                launch_at_login: Some(launch),
                close_to_tray: Some(tray),
                ..Default::default()
            }).unwrap();
            prop_assert_eq!(settings.gui.launch_at_login, launch);
            prop_assert_eq!(settings.gui.close_to_tray, tray);
            prop_assert_eq!(service.settings().unwrap(), settings);
        }

        #[test]
        fn randomized_cli_gui_restart_sequences_never_panic(actions in prop::collection::vec((0u8..8, any::<bool>(), any::<bool>()), 1..48)) {
            let tmp = tempdir().unwrap();
            let p = paths(tmp.path());
            let mut service = FrankService::new(p.clone());
            let mut pending = Vec::new();
            let mut applied = std::collections::HashSet::new();

            for (action, launch, tray) in actions {
                match action {
                    // GUI level toggle / CLI `frank on`.
                    0 => { let _ = service.set_active_level(Some("full")); }
                    1 => { let _ = service.set_active_level(None); }
                    2 => {
                        let _ = service.update_settings(UserSettingsPatch {
                            launch_at_login: Some(launch),
                            close_to_tray: Some(tray),
                            ..Default::default()
                        });
                    }
                    // Preview/apply is deliberately separated; a stale or
                    // already-consumed plan is a normal rejected outcome.
                    3 => {
                        if let Ok(preview) = service.prepare_target_change("claude-code", TargetOperation::Uninstall) {
                            pending.push(preview.plan_id);
                        }
                    }
                    4 => {
                        if let Some(plan_id) = pending.first().cloned() {
                            let _ = service.apply_prepared_plan(&plan_id);
                            applied.insert(plan_id);
                            pending.remove(0);
                        }
                    }
                    // Pack switch and restart are both state transitions that
                    // must keep using the same path/config precedence.
                    5 => { let _ = service.use_pack("caveman"); }
                    6 => { let _ = service.snapshot(); }
                    7 => { service = FrankService::new(p.clone()); }
                    _ => unreachable!(),
                }
            }

            prop_assert!(applied.len() <= 48);
            prop_assert!(service.settings().is_ok());
        }
    }
}
