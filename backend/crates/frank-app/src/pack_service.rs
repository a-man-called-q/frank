//! Pack operations: selection, install/use/remove, and the prepare/apply
//! flow for pack mutations. Shares the generic algorithm in `prepare.rs`
//! with target plans; the per-operation validators here (`validate_*`) are
//! the single place a pack source or selector is admitted, used by both
//! the direct methods and the preview path so they can't drift apart.

use std::path::Path;

use crate::fingerprint::Fingerprint;
use crate::prepare::PreparationFlow;
use crate::repository::pack_summary;
use crate::{
    AppError, FrankService, PLAN_TTL, PackOperation, PackOperationKind, PackOperationResult,
    PackPlanPreview, PackSummary, builtin, builtin_pack,
};

impl FrankService {
    pub fn current_pack(&self) -> Result<frank_pack::CompiledPack, AppError> {
        self.pack_for_selector(None)
    }

    /// The current pack, or the embedded built-in when it can't be loaded.
    /// Only for read-only reporting paths (ledger/stats) that must still
    /// produce output if a selected pack was since removed or corrupted;
    /// hooks and mutating commands use `current_pack()` and fail closed
    /// instead, since silently falling back there would change a user's
    /// explicit persona selection.
    pub fn pack_or_builtin(&self) -> frank_pack::CompiledPack {
        self.current_pack().unwrap_or_else(|_| builtin_pack())
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
            Some(selector) if builtin::selects_builtin(selector) => Ok(builtin_pack()),
            Some(selector) => {
                let installed = store.find(selector)?;
                store.compile_installed(&installed).map_err(AppError::from)
            }
        }
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

    /// Load, compile, and reject the reserved built-in id. The narrow half
    /// of pack-source admission shared by `add_local_pack` (which lets
    /// `PackStore::add_local` own the digest check at actual-install time)
    /// and `validate_add` (which layers the digest check on top for the
    /// read-only preview path).
    fn validate_pack_source(&self, source: &Path) -> Result<frank_pack::CompiledPack, AppError> {
        if !source.is_dir() {
            return Err(AppError::InvalidPackSource);
        }
        let loaded = frank_pack::PackSource::load(source).map_err(|e| AppError::Config {
            path: source.to_path_buf(),
            reason: e.to_string(),
        })?;
        let compiled = frank_pack::compile(&loaded).map_err(|e| AppError::Config {
            path: source.to_path_buf(),
            reason: e.to_string(),
        })?;
        if compiled.id == builtin::PACK_ID {
            return Err(AppError::Config {
                path: source.to_path_buf(),
                reason: "the built-in pack id is reserved".to_string(),
            });
        }
        Ok(compiled)
    }

    fn validate_expected_digest(
        &self,
        source: &Path,
        expected_sha256: &str,
    ) -> Result<(), AppError> {
        let expected = expected_sha256.trim().to_ascii_lowercase();
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(AppError::Config {
                path: source.to_path_buf(),
                reason: "expected SHA-256 must be 64 hexadecimal characters".to_string(),
            });
        }
        let actual = frank_pack::directory_sha256(source)?;
        if expected != actual {
            return Err(AppError::Pack(frank_pack::PackStoreError::DigestMismatch {
                expected,
                actual,
            }));
        }
        Ok(())
    }

    /// Load, compile, reject the reserved built-in id, and verify an
    /// expected digest if one was supplied — without installing anything.
    /// The only place an `Add` operation is admitted, used by both the
    /// preview path and `preview_pack_source`.
    fn validate_add(
        &self,
        source: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<frank_pack::CompiledPack, AppError> {
        let compiled = self.validate_pack_source(source)?;
        if let Some(expected) = expected_sha256 {
            self.validate_expected_digest(source, expected)?;
        }
        Ok(compiled)
    }

    /// Validate a `use` selector: either it selects the built-in pack, or
    /// it must resolve to an installed, compilable pack — returned as
    /// `Some` so `use_pack` doesn't have to look it up a second time.
    /// Shared by `use_pack` (which then flips the active pointer) and the
    /// preview path (which stops here — a preview must never mutate
    /// `packs.lock`).
    fn validate_use(&self, selector: &str) -> Result<Option<frank_pack::InstalledPack>, AppError> {
        if builtin::selects_builtin(selector) {
            return Ok(None);
        }
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        let installed = store.find(selector)?;
        store.compile_installed(&installed)?;
        Ok(Some(installed))
    }

    /// Validate a `remove` selector: it must not claim the built-in id at
    /// any version. Shared by `remove_pack` and the preview path.
    fn validate_remove(&self, selector: &str) -> Result<(), AppError> {
        if builtin::claims_builtin_id(selector) {
            return Err(AppError::Config {
                path: self.paths.data_root.clone(),
                reason: "the built-in pack cannot be removed".to_string(),
            });
        }
        Ok(())
    }

    /// Compile and preview a local pack source without installing it — the
    /// activation prompt, reserved-id check, and optional digest check a
    /// confirmation UI needs before asking the user to proceed.
    pub fn preview_pack_source(
        &self,
        source: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<frank_pack::CompiledPack, AppError> {
        self.validate_add(source, expected_sha256)
    }

    pub fn add_local_pack(
        &self,
        source: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<PackSummary, AppError> {
        let preview = self.validate_pack_source(source)?;
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        store.add_local(source, expected_sha256)?;
        Ok(pack_summary(&preview, false, false))
    }

    pub fn use_pack(&self, selector: &str) -> Result<(), AppError> {
        let installed = self.validate_use(selector)?;
        let store = frank_pack::PackStore::new(self.paths.data_root.clone());
        store.set_active(installed.map(|installed| installed.pack_ref()))?;
        Ok(())
    }

    pub fn remove_pack(&self, selector: &str) -> Result<(), AppError> {
        self.validate_remove(selector)?;
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
                let compiled = self.validate_add(source, expected_sha256.as_deref())?;
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
                self.validate_use(selector)?;
                if builtin::selects_builtin(selector) {
                    (
                        PackOperationKind::Use,
                        selector.clone(),
                        "use built-in pack".to_string(),
                        None,
                    )
                } else {
                    (
                        PackOperationKind::Use,
                        selector.clone(),
                        format!("use pack {selector}"),
                        None,
                    )
                }
            }
            PackOperation::Remove { selector } => {
                self.validate_remove(selector)?;
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

        let state_fingerprint = self.pack_state_fingerprint(source.as_deref())?;
        let identity = format!("{operation:?}");
        let plan_id = self
            .pack_flow()
            .prepare(operation, &identity, state_fingerprint)?;
        Ok(PackPlanPreview {
            plan_id,
            operation: kind,
            selector,
            actions: vec![action],
            expires_in_seconds: PLAN_TTL.as_secs(),
        })
    }

    pub fn apply_prepared_pack(&self, plan_id: &str) -> Result<PackOperationResult, AppError> {
        let operation = self.pack_flow().take_valid(plan_id, |op| {
            let source = match op {
                PackOperation::Add { source, .. } => Some(source.as_path()),
                PackOperation::Use { .. } | PackOperation::Remove { .. } => None,
            };
            self.pack_state_fingerprint(source).ok()
        })?;
        let (kind, selector, summary) = match operation {
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

    fn pack_state_fingerprint(&self, source: Option<&Path>) -> Result<String, AppError> {
        let lock = self.paths.data_root.join("packs.lock");
        let mut fp = Fingerprint::new()
            .path(&lock)
            .field(self.paths.data_root.to_string_lossy().as_bytes());
        // The lockfile alone is not enough for a one-shot `use`/`remove`
        // plan: an external editor can mutate an installed pack directory
        // without touching packs.lock. Include every locked copy's bounded,
        // symlink-rejecting content digest so apply refuses to operate on a
        // pack state different from the one the preview described.
        if let Ok(lock_data) = frank_pack::PackStore::new(self.paths.data_root.clone()).load_lock()
        {
            for installed in lock_data.packs {
                let path = self.paths.data_root.join(&installed.path);
                fp = fp.field(path.to_string_lossy().as_bytes());
                fp = match frank_pack::directory_sha256(&path) {
                    Ok(digest) => fp.field(digest.as_bytes()),
                    Err(error) => fp.field(format!("error:{error}").as_bytes()),
                };
            }
        }
        if let Some(source) = source {
            let digest = frank_pack::directory_sha256(source).map_err(AppError::from)?;
            fp = fp
                .field(source.to_string_lossy().as_bytes())
                .field(digest.as_bytes());
        }
        Ok(fp.finish())
    }

    fn pack_flow(&self) -> PreparationFlow<'_, PackOperation> {
        PreparationFlow {
            store: &self.prepared_packs,
            clock: self.clock.as_ref(),
            nonce: &self.plan_nonce,
            id_prefix: "frank-pack-plan-",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Clock, FrankPaths};
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

    fn write_pack(source: &std::path::Path, id: &str) {
        fs::create_dir_all(source.join("levels")).unwrap();
        fs::write(
            source.join("pack.toml"),
            format!(
                r#"schema = 1
[pack]
id = "{id}"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#
            ),
        )
        .unwrap();
        fs::write(source.join("levels/full.md"), "Be concise.").unwrap();
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
        write_pack(&source, "local");

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
    fn local_pack_can_be_added_selected_listed_and_removed() {
        let tmp = tempdir().unwrap();
        let p = paths(tmp.path());
        let source = tmp.path().join("my-pack");
        write_pack(&source, "local");
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
        write_pack(&source, "mutable");
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
    fn pack_plan_ids_are_stably_prefixed_and_unique_per_preparation() {
        let tmp = tempdir().unwrap();
        let service = FrankService::new(paths(tmp.path()));
        let first = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        let second = service
            .prepare_pack_change(PackOperation::Use {
                selector: "caveman".to_string(),
            })
            .unwrap();
        assert!(first.plan_id.starts_with("frank-pack-plan-"));
        assert_ne!(first.plan_id, second.plan_id);
    }
}
