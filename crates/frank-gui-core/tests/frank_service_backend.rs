//! Confirms `impl Backend for FrankService` actually forwards to the real
//! service rather than merely type-checking. `RecordingBackend` (in
//! `backend_contract.rs`) exercises `reduce`'s wiring against a double; this
//! exercises the one real `Backend` implementation this crate ships against
//! a throwaway `FrankPaths` on a tempdir, the same pattern `frank-app`'s own
//! tests use.

use std::fs;
use std::path::Path;

use frank_app::{FrankPaths, FrankService, TargetOperation, UserSettingsPatch};
use frank_gui_core::Backend;
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
fn snapshot_and_settings_round_trip_through_the_backend_trait() {
    let tmp = tempdir().unwrap();
    let service = FrankService::new(paths(tmp.path()));

    let before = Backend::snapshot(&service).unwrap();
    assert!(before.active_level.is_none(), "off by default");

    Backend::update_settings(
        &service,
        UserSettingsPatch {
            close_to_tray: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    let after = Backend::snapshot(&service).unwrap();
    assert!(!after.settings.gui.close_to_tray);
}

#[test]
fn set_active_level_through_the_backend_trait_persists() {
    let tmp = tempdir().unwrap();
    let service = FrankService::new(paths(tmp.path()));

    let active = Backend::set_active_level(&service, Some("full")).unwrap();
    assert_eq!(active.as_deref(), Some("full"));
    assert_eq!(
        Backend::snapshot(&service).unwrap().active_level.as_deref(),
        Some("full")
    );
}

#[test]
fn unknown_target_prepare_through_the_backend_trait_is_an_error() {
    let tmp = tempdir().unwrap();
    let service = FrankService::new(paths(tmp.path()));

    let result =
        Backend::prepare_target_change(&service, "does-not-exist", TargetOperation::Install);
    assert!(result.is_err());
}

#[test]
fn apply_calls_through_the_backend_trait_reject_an_unknown_plan() {
    let tmp = tempdir().unwrap();
    let service = FrankService::new(paths(tmp.path()));

    assert!(Backend::apply_prepared_plan(&service, "no-such-plan").is_err());
    assert!(Backend::apply_prepared_pack(&service, "no-such-plan").is_err());
}

#[test]
fn pack_prepare_through_the_backend_trait_rejects_a_missing_source() {
    let tmp = tempdir().unwrap();
    let service = FrankService::new(paths(tmp.path()));
    fs::create_dir_all(paths(tmp.path()).user_config_dir()).unwrap();

    let result = Backend::prepare_pack_change(
        &service,
        frank_app::PackOperation::Add {
            source: tmp.path().join("does-not-exist"),
            expected_sha256: None,
        },
    );
    assert!(result.is_err());
}
