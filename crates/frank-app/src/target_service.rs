//! Target discovery, install-plan building, doctor diagnostics, and the
//! staleness fingerprint for a target's install plan. Pack operations live
//! in `pack_service.rs`; the two share the generic prepare/apply algorithm
//! in `prepare.rs`.

use std::path::PathBuf;

use crate::fingerprint::Fingerprint;
use crate::prepare::PreparationFlow;
use crate::repository::load_manifests;
use crate::{
    AppError, DiagnosisView, DoctorReport, FrankService, OperationResult, PLAN_TTL, PlanPreview,
    TargetDiscovery, TargetOperation, TargetSummary, UserSettings,
};

impl FrankService {
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

    pub(crate) fn doctor_with(
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
        let identity = plan_fingerprint(&plan);
        let fingerprint = self.state_fingerprint(&plan);
        let plan_id = self.target_flow().prepare(plan, &identity, fingerprint)?;
        Ok(PlanPreview {
            plan_id,
            target_id: target_id.to_string(),
            operation,
            actions,
            expires_in_seconds: PLAN_TTL.as_secs(),
        })
    }

    pub fn apply_prepared_plan(&self, plan_id: &str) -> Result<OperationResult, AppError> {
        let plan = self
            .target_flow()
            .take_valid(plan_id, |plan| Some(self.state_fingerprint(plan)))?;
        let target_id = plan.target_id.clone();
        let log = frank_target::apply(&plan).map_err(|e| AppError::Apply(e.to_string()))?;
        Ok(OperationResult { target_id, log })
    }

    pub(crate) fn install_ctx(&self) -> frank_target::InstallCtx {
        frank_target::InstallCtx {
            config_dir: self.paths.config_dir.clone(),
            frank_bin: self.paths.frank_bin.clone(),
            cwd: self.paths.cwd.clone(),
        }
    }

    fn state_fingerprint(&self, plan: &frank_target::InstallPlan) -> String {
        let mut fp = Fingerprint::new().field(current_fingerprint(plan).as_bytes());
        for path in [
            self.paths.active_flag_path(),
            self.paths.data_root.join("packs.lock"),
            self.paths.user_config_path(),
        ] {
            fp = fp.path(&path);
        }
        if let Ok(pack) = self.current_pack() {
            fp = fp
                .field(pack.id.as_bytes())
                .field(pack.version.as_bytes())
                .field(pack.default_level.as_bytes());
            for (id, level) in &pack.levels {
                fp = fp
                    .field(id.as_bytes())
                    .field(level.activation_prompt.as_bytes())
                    .field(level.reinforce.as_bytes());
            }
        }
        fp.finish()
    }

    pub(crate) fn build_plan(
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

    fn target_flow(&self) -> PreparationFlow<'_, frank_target::InstallPlan> {
        PreparationFlow {
            store: &self.prepared,
            clock: self.clock.as_ref(),
            nonce: &self.plan_nonce,
            id_prefix: "frank-plan-",
        }
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
    let mut fp = Fingerprint::new();
    for path in paths {
        fp = fp.path(&path);
    }
    fp.finish()
}

fn plan_fingerprint(plan: &frank_target::InstallPlan) -> String {
    // The action description is stable and captures the exact plan identity;
    // current_fingerprint is used separately to detect external changes.
    let mut fp = Fingerprint::new().field(plan.target_id.as_bytes());
    for action in plan.describe() {
        fp = fp.field(action.as_bytes()).field([0]);
    }
    fp.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Clock, FrankPaths};
    use crate::{AppError, UserSettingsPatch, builtin_pack};
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn paths(root: &std::path::Path) -> FrankPaths {
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
            joins.push(std::thread::spawn(move || {
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
    fn helper_values_are_nonempty_and_only_resolve_known_body_references() {
        let pack = builtin_pack();
        let values = pack.valid_flag_values();
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
    fn target_plan_ids_are_stably_prefixed_and_unique_per_preparation() {
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

        let install = service
            .build_plan("claude-code", TargetOperation::Install)
            .unwrap();
        let uninstall = service
            .build_plan("claude-code", TargetOperation::Uninstall)
            .unwrap();
        assert_ne!(plan_fingerprint(&install), plan_fingerprint(&uninstall));
    }
}
