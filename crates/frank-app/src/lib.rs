//! Shared application services for Frank frontends.
//!
//! `frank-cli` and the desktop UI both call this crate.  Domain crates remain
//! responsible for their own invariants; this crate only supplies paths,
//! orchestration, serializable view models, and the prepare/apply boundary
//! needed by a confirmation-based UI.
//!
//! [`FrankService`] is a thin facade: settings and flag-state live here,
//! pack operations live in `pack_service.rs`, target operations live in
//! `target_service.rs`, and both share the generic prepare/apply algorithm
//! in `prepare.rs` and the fingerprint builder in `fingerprint.rs`.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

mod builtin;
mod fingerprint;
mod ledger;
mod models;
mod pack_service;
mod plan_store;
mod prepare;
mod repository;
mod settings;
mod target_service;

pub use builtin::{PACK_ID as BUILTIN_PACK_ID, builtin_pack};

pub use models::{
    AppError, Clock, DashboardSnapshot, DiagnosisView, DoctorReport, FrankPaths, GuiSettings,
    LevelSummary, OperationResult, PackOperation, PackOperationKind, PackOperationResult,
    PackPlanPreview, PackSummary, PlanPreview, SystemClock, TargetDiscovery, TargetOperation,
    TargetSummary, UserSettings, UserSettingsPatch,
};
use plan_store::PreparedStore;
use repository::pack_summary;
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

    /// The path-resolution roots this service was built with. For
    /// frontend code that needs a path `FrankService` doesn't already
    /// expose a method for (e.g. the CLI's lifetime ledger report) — using
    /// this instead of independently re-deriving `FrankPaths::from_process()`
    /// keeps it on the exact same roots the service itself uses, which
    /// matters once paths are injected in tests.
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
            frank_safeio::read_flag(&self.paths.active_flag_path(), &pack.valid_flag_values())
                .filter(|v| v != "off"),
        )
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
            frank_safeio::read_flag(&self.paths.active_flag_path(), &pack.valid_flag_values())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::fs;
    use std::path::Path;
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
        let history = frank_ledger::read_history(&p.ledger_paths().history);
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
        assert!(!p.ledger_paths().history.exists());
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
