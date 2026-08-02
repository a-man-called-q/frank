//! Shared application services for Frank frontends.
//!
//! `frank-cli` and the desktop UI both call this crate.  Domain crates remain
//! responsible for their own invariants; this crate only supplies paths,
//! orchestration, serializable view models, and the prepare/apply boundary
//! needed by a confirmation-based UI.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

mod builtin;
mod ledger;
mod models;
mod plan_store;
mod repository;
mod settings;

pub use builtin::builtin_pack;

pub use models::{
    AppError, Clock, DashboardSnapshot, DiagnosisView, DoctorReport, FrankPaths, GuiSettings,
    LevelSummary, OperationResult, PackOperation, PackOperationKind, PackOperationResult,
    PackPlanPreview, PackSummary, PlanPreview, SystemClock, TargetDiscovery, TargetOperation,
    TargetSummary, UserSettings, UserSettingsPatch,
};
use plan_store::PreparedStore;
use repository::{load_manifests, pack_summary, valid_values};
use settings::{read_settings, write_settings};

const PLAN_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct FrankService {
    paths: FrankPaths,
    prepared: Arc<PreparedStore<frank_target::InstallPlan>>,
    prepared_packs: Arc<PreparedStore<PackOperation>>,
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
            prepared: Arc::new(PreparedStore::new()),
            prepared_packs: Arc::new(PreparedStore::new()),
            clock,
            plan_nonce: Arc::new(AtomicU64::new(0)),
        }
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
        self.pack_for_selector(None)
    }

    /// Load a specific installed pack for presentation adapters. Built-in
    /// selection remains explicit and never falls back from a corrupt user
    /// pack, so CLI and GUI share the same resolution rules.
    pub fn pack_for_selector(
        &self,
        selector: Option<&str>,
    ) -> Result<frank_pack::CompiledPack, AppError> {
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        match selector {
            None => match store.active()? {
                Some((_, pack)) => Ok(pack),
                None => Ok(builtin_pack()),
            },
            Some(selector)
                if selector == builtin::PACK_ID
                    || selector == format!("{}@{}", builtin::PACK_ID, builtin::PACK_VERSION) =>
            {
                Ok(builtin_pack())
            }
            Some(selector) => {
                let installed = store.find(selector)?;
                store.compile_installed(&installed).map_err(AppError::from)
            }
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
        self.prepared_packs
            .insert(plan_id.clone(), operation, state_fingerprint, now)?;
        Ok(PackPlanPreview {
            plan_id,
            operation: kind,
            selector,
            actions: vec![action],
            expires_in_seconds: PLAN_TTL.as_secs(),
        })
    }

    pub fn apply_prepared_pack(&self, plan_id: &str) -> Result<PackOperationResult, AppError> {
        let prepared = self.prepared_packs.take(plan_id)?;
        let source = match &prepared.payload {
            PackOperation::Add { source, .. } => Some(source.as_path()),
            PackOperation::Use { .. } | PackOperation::Remove { .. } => None,
        };
        let current_fingerprint = self.pack_state_fingerprint(source).ok();
        if self.clock.now().saturating_duration_since(prepared.created) > PLAN_TTL
            || current_fingerprint.as_deref() != Some(prepared.state_fingerprint.as_str())
        {
            return Err(AppError::StalePlan);
        }
        let (kind, selector, summary) = match prepared.payload {
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
                let summary = self.list_packs()?.into_iter().find(|p| p.active);
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

    pub fn discover_targets(&self) -> TargetDiscovery {
        let env = frank_target::ProbeEnv::from_process();
        let mut rows = vec![TargetSummary {
            id: "claude-code".to_string(),
            label: "Claude Code".to_string(),
            kind: "native".to_string(),
            verified: true,
            soft: false,
            detected: frank_target::ClaudeCodeTarget::detect(&env)
                == frank_target::Detection::Detected,
            source: "built-in".to_string(),
        }];
        let mut errors = Vec::new();
        for (path, parsed) in load_manifests(&self.paths) {
            match parsed {
                Ok(manifest) => {
                    let detected =
                        frank_target::detect(&manifest, &env) == frank_target::Detection::Detected;
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
        let pack = self.current_pack().ok();
        self.doctor_with(pack.as_ref(), self.settings())
    }

    fn doctor_with(
        &self,
        pack: Option<&frank_pack::CompiledPack>,
        settings: Result<UserSettings, AppError>,
    ) -> DoctorReport {
        let ctx = self.install_ctx();
        let mut checks = frank_target::ClaudeCodeTarget::doctor(&ctx)
            .into_iter()
            .map(|d| DiagnosisView {
                ok: d.ok,
                message: d.message,
            })
            .collect::<Vec<_>>();
        let settings_check = match settings {
            Err(error) => DiagnosisView {
                ok: false,
                message: format!("Frank user config is invalid: {error}"),
            },
            Ok(settings) => {
                let valid = settings.default_level.as_deref().is_none_or(|level| {
                    level == "off" || pack.is_some_and(|pack| pack.resolve_level(level).is_some())
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
        self.prepared
            .insert(plan_id.clone(), plan, state_fingerprint, now)?;
        Ok(PlanPreview {
            plan_id,
            target_id: target_id.to_string(),
            operation,
            actions,
            expires_in_seconds: PLAN_TTL.as_secs(),
        })
    }

    pub fn apply_prepared_plan(&self, plan_id: &str) -> Result<OperationResult, AppError> {
        let prepared = self.prepared.take(plan_id)?;
        if self.clock.now().saturating_duration_since(prepared.created) > PLAN_TTL
            || self.state_fingerprint(&prepared.payload) != prepared.state_fingerprint
        {
            return Err(AppError::StalePlan);
        }
        let target_id = prepared.payload.target_id.clone();
        let log =
            frank_target::apply(&prepared.payload).map_err(|e| AppError::Apply(e.to_string()))?;
        Ok(OperationResult { target_id, log })
    }

    pub fn snapshot(&self) -> Result<DashboardSnapshot, AppError> {
        let pack = self.current_pack()?;
        // A damaged user config must not strand the desktop on its loading
        // screen. Keep the view model renderable with defaults and let the
        // diagnostics panel report the precise parse error. The strict
        // `settings()` API remains fallible for callers that need to refuse a
        // write, while a status snapshot is deliberately read-only/fail-soft.
        let settings_result = self.settings();
        let settings = match &settings_result {
            Ok(settings) => settings.clone(),
            Err(_) => UserSettings::default(),
        };
        let default_level = frank_state::resolve_default_level_with_user_dir(
            &pack,
            &self.paths.cwd,
            "FRANK_DEFAULT_LEVEL",
            Some(&self.paths.user_config_dir),
        );
        let active_level =
            frank_safeio::read_flag(&self.paths.active_flag_path(), &valid_values(&pack))
                .filter(|v| v != "off");
        let packs = self.list_packs()?;
        let active_pack = packs
            .iter()
            .find(|p| p.active)
            .cloned()
            .unwrap_or_else(|| pack_summary(&pack, true, true));
        let discovery = self.discover_targets();
        let diagnoses = self.doctor_with(Some(&pack), settings_result).checks;
        Ok(DashboardSnapshot {
            active_level,
            active_pack: active_pack.id,
            active_pack_version: active_pack.version,
            default_level,
            settings,
            packs,
            targets: discovery.targets,
            target_errors: discovery.errors,
            diagnoses,
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
                TargetOperation::Install => frank_target::ClaudeCodeTarget::plan_install(&ctx),
                TargetOperation::Uninstall => frank_target::ClaudeCodeTarget::plan_uninstall(&ctx),
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
                frank_target::build_install_plan(&manifest, &ctx, |reference| {
                    resolve_body_ref(reference, &pack)
                })
            }
            TargetOperation::Uninstall => frank_target::build_uninstall_plan(&manifest, &ctx),
        };
        plan.validate_scope()?;
        Ok(plan)
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
                h.update(fingerprint_file_contents(path).as_bytes());
            }
        }
        Err(_) => h.update([0]),
    }
    format!("{:x}", h.finalize())
}

fn fingerprint_file_contents(path: &Path) -> String {
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
    format!("{:x}", file_digest.finalize())
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
    use std::sync::{Arc, Mutex};
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
    fn active_level_reports_the_validated_flag_and_filters_off() {
        let tmp = tempdir().unwrap();
        let service = FrankService::new(paths(tmp.path()));
        assert_eq!(service.active_level().unwrap(), None);
        service.set_active_level(Some("lite")).unwrap();
        assert_eq!(service.active_level().unwrap().as_deref(), Some("lite"));
        service.set_active_level(None).unwrap();
        assert_eq!(service.active_level().unwrap(), None);
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
        let history = frank_ledger::read_history(&p.config_dir.join(".frank-history.jsonl"));
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].output_tokens, 12);
    }

    #[test]
    fn stats_does_not_record_a_history_row_for_a_zero_turn_session() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(&p.config_dir).unwrap();
        let session = tmp.path().join("empty.jsonl");
        fs::write(&session, "{\"type\":\"user\"}\n").unwrap();

        let report = FrankService::new(p.clone()).build_and_record_stats(Some(&session));
        assert_eq!(report.turns, 0);
        assert!(!p.config_dir.join(".frank-history.jsonl").exists());
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
    fn prepared_target_plan_is_valid_at_the_exact_ttl_boundary() {
        let tmp = tempdir().unwrap();
        let clock = Arc::new(TestClock {
            now: Mutex::new(Instant::now()),
        });
        let service =
            FrankService::with_clock(paths(tmp.path()), Arc::clone(&clock) as Arc<dyn Clock>);
        let preview = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        assert_eq!(preview.expires_in_seconds, 300);
        clock.advance(PLAN_TTL);
        assert!(service.apply_prepared_plan(&preview.plan_id).is_ok());
    }

    #[test]
    fn target_plan_at_the_ttl_boundary_is_not_dropped_when_a_new_plan_is_prepared() {
        let tmp = tempdir().unwrap();
        let clock = Arc::new(TestClock {
            now: Mutex::new(Instant::now()),
        });
        let service =
            FrankService::with_clock(paths(tmp.path()), Arc::clone(&clock) as Arc<dyn Clock>);
        let first = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        clock.advance(PLAN_TTL);
        let _second = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        assert!(service.apply_prepared_plan(&first.plan_id).is_ok());
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
    fn pack_plan_at_the_ttl_boundary_is_not_dropped_when_a_new_plan_is_prepared() {
        let tmp = tempdir().unwrap();
        let clock = Arc::new(TestClock {
            now: Mutex::new(Instant::now()),
        });
        let service =
            FrankService::with_clock(paths(tmp.path()), Arc::clone(&clock) as Arc<dyn Clock>);
        let first = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        clock.advance(PLAN_TTL);
        let _second = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        assert!(service.apply_prepared_pack(&first.plan_id).is_ok());
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

        let invalid_hex = service
            .prepare_pack_change(PackOperation::Add {
                source,
                expected_sha256: Some("g".repeat(64)),
            })
            .unwrap_err();
        assert!(matches!(invalid_hex, AppError::Config { .. }));
    }

    #[test]
    fn built_in_pack_versions_and_selectors_keep_their_fail_closed_contract() {
        let tmp = tempdir().unwrap();
        let service = FrankService::new(paths(tmp.path()));
        let versioned = service
            .prepare_pack_change(PackOperation::Use {
                selector: format!("caveman@{}", builtin::PACK_VERSION),
            })
            .unwrap();
        assert_eq!(versioned.operation, PackOperationKind::Use);
        assert!(matches!(
            service.remove_pack("caveman@9.9.9"),
            Err(AppError::Config { .. })
        ));
        assert!(matches!(
            service.prepare_pack_change(PackOperation::Remove {
                selector: "caveman".to_string(),
            }),
            Err(AppError::Config { .. })
        ));
        assert!(matches!(
            service.prepare_pack_change(PackOperation::Remove {
                selector: "caveman@9.9.9".to_string(),
            }),
            Err(AppError::Config { .. })
        ));
    }

    #[test]
    fn pack_selector_service_resolves_builtin_forms_and_rejects_unknowns() {
        let tmp = tempdir().unwrap();
        let service = FrankService::new(paths(tmp.path()));

        assert_eq!(
            service
                .pack_for_selector(Some(builtin::PACK_ID))
                .unwrap()
                .id,
            builtin::PACK_ID
        );
        assert_eq!(
            service
                .pack_for_selector(Some(&format!(
                    "{}@{}",
                    builtin::PACK_ID,
                    builtin::PACK_VERSION
                )))
                .unwrap()
                .version,
            builtin::PACK_VERSION
        );
        assert!(matches!(
            service.pack_for_selector(Some("missing-pack")),
            Err(AppError::Pack(_))
        ));
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
        assert!(
            service
                .list_packs()
                .unwrap()
                .into_iter()
                .any(|pack| pack.id == "local" && pack.active)
        );

        let use_preview = service
            .prepare_pack_change(PackOperation::Use {
                selector: "local@1.0.0".to_string(),
            })
            .unwrap();
        let use_result = service.apply_prepared_pack(&use_preview.plan_id).unwrap();
        assert_eq!(use_result.selector, "local@1.0.0");
        assert_eq!(
            use_result.pack.as_ref().map(|pack| pack.id.as_str()),
            Some("local")
        );

        service.set_active_level(Some("full")).unwrap();
        let snapshot = service.snapshot().unwrap();
        assert_eq!(snapshot.active_pack, "local");
        assert_eq!(snapshot.active_level.as_deref(), Some("full"));
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
[[detect]]
command = "sh"
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
        fs::write(tmp.path().join("AGENTS.md"), "user content\n").unwrap();
        let service = FrankService::new(p.clone());
        let discovery = service.discover_targets();
        let generic = discovery
            .targets
            .iter()
            .find(|target| target.id == "generic")
            .unwrap();
        assert!(generic.detected);
        assert!(matches!(
            service.prepare_target_change("missing", TargetOperation::Install),
            Err(AppError::UnknownTarget(target)) if target == "missing"
        ));
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
        assert_eq!(
            fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap(),
            "user content\n"
        );
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

    #[test]
    fn valid_default_level_is_reported_as_healthy_by_doctor() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(p.user_config_dir()).unwrap();
        fs::write(p.user_config_path(), "default_level = \"full\"\n").unwrap();
        let report = FrankService::new(p).doctor();
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.ok && check.message.contains("user config is valid"))
        );
    }

    #[test]
    fn invalid_utf8_settings_are_not_treated_as_missing() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        fs::create_dir_all(p.user_config_dir()).unwrap();
        fs::write(p.user_config_path(), [0xff, 0xfe]).unwrap();
        let service = FrankService::new(p.clone());
        assert!(matches!(service.settings(), Err(AppError::SafeIo(_))));
        assert!(matches!(
            service.update_settings(UserSettingsPatch {
                close_to_tray: Some(false),
                ..Default::default()
            }),
            Err(AppError::SafeIo(_))
        ));
        assert!(matches!(
            write_settings(&p.user_config_path(), &UserSettings::default()),
            Err(AppError::SafeIo(_))
        ));
        assert_eq!(fs::read(p.user_config_path()).unwrap(), [0xff, 0xfe]);
    }

    #[test]
    fn bounded_file_fingerprints_include_content_and_error_sentinels() {
        let tmp = tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        fs::write(&first, "same-size-a").unwrap();
        fs::write(&second, "same-size-b").unwrap();
        assert_ne!(
            fingerprint_file_contents(&first),
            fingerprint_file_contents(&second)
        );
        assert_ne!(
            fingerprint_file_contents(&first),
            fingerprint_file_contents(&tmp.path().join("missing"))
        );

        let exact = tmp.path().join("exact");
        let content = vec![b'x'; frank_safeio::MAX_CONFIG_BYTES];
        fs::write(&exact, &content).unwrap();
        let mut expected = Sha256::new();
        expected.update(&content);
        expected.update([0xfe]);
        assert_eq!(
            fingerprint_file_contents(&exact),
            format!("{:x}", expected.finalize())
        );
    }

    #[test]
    fn helper_values_are_nonempty_and_only_resolve_known_body_references() {
        let pack = builtin_pack();
        let values = valid_values(&pack);
        assert!(values.contains(&"full"));
        assert!(values.contains(&"commit"));
        assert!(values.contains(&"off"));
        assert!(!values.contains(&"xyzzy"));

        let expected = pack
            .resolve_level(&pack.default_level)
            .unwrap()
            .activation_prompt
            .clone();
        assert_eq!(
            resolve_body_ref("pack:static_digest", &pack),
            Some(expected)
        );
        assert_eq!(resolve_body_ref("pack:unknown", &pack), None);
    }

    #[test]
    fn plan_ids_are_stablely_prefixed_and_unique_per_preparation() {
        let tmp = tempdir().unwrap();
        let service = FrankService::new(paths(tmp.path()));
        let first = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        let second = service
            .prepare_target_change("claude-code", TargetOperation::Uninstall)
            .unwrap();
        assert!(first.plan_id.starts_with("frank-plan-"));
        assert_ne!(first.plan_id, second.plan_id);

        let first_pack = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        let second_pack = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        assert!(first_pack.plan_id.starts_with("frank-pack-plan-"));
        assert_ne!(first_pack.plan_id, second_pack.plan_id);

        let install = service
            .build_plan("claude-code", TargetOperation::Install)
            .unwrap();
        let uninstall = service
            .build_plan("claude-code", TargetOperation::Uninstall)
            .unwrap();
        assert_ne!(plan_fingerprint(&install), plan_fingerprint(&uninstall));
        let now = Instant::now();
        assert_ne!(
            new_plan_id(&install, "state", now, 0),
            new_plan_id(&uninstall, "state", now, 0)
        );
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
