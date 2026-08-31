use frank_app::{AppError, FrankPaths, FrankService, PackOperation, TargetOperation};
use std::fs;
use std::path::{Path, PathBuf};
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

fn write_pack(root: &Path, id: &str) -> PathBuf {
    let source = root.join(id);
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("pack.toml"),
        format!(
            "schema = 1\n[pack]\nid = \"{id}\"\nversion = \"1.0.0\"\ndefault_level = \"full\"\n[fragments]\ncore = {{ file = \"core.md\" }}\n[[level]]\nid = \"full\"\ncompose = [\"core\"]\nreinforce = \"active\"\n[activation]\non = []\noff = []\n"
        ),
    )
    .unwrap();
    fs::write(source.join("core.md"), "Keep output concise.").unwrap();
    source
}

#[test]
fn app_rejects_non_directories_reserved_packs_and_bad_digests() {
    let tmp = tempdir().unwrap();
    let service = FrankService::new(paths(tmp.path()));
    let not_a_directory = tmp.path().join("not-a-pack");
    fs::write(&not_a_directory, "not a directory").unwrap();

    assert!(matches!(
        service.add_local_pack(&not_a_directory, None),
        Err(AppError::InvalidPackSource)
    ));

    let builtin = write_pack(tmp.path(), "caveman");
    assert!(matches!(
        service.add_local_pack(&builtin, None),
        Err(AppError::Config { .. })
    ));

    let local = write_pack(tmp.path(), "local");
    assert!(matches!(
        service.prepare_pack_change(PackOperation::Add {
            source: local,
            expected_sha256: Some("not-a-digest".to_string()),
        }),
        Err(AppError::Config { .. })
    ));
}

#[test]
fn app_rejects_unknown_target_without_creating_state() {
    let tmp = tempdir().unwrap();
    let p = paths(tmp.path());
    let service = FrankService::new(p.clone());

    assert!(matches!(
        service.prepare_target_change("missing-target", TargetOperation::Install),
        Err(AppError::UnknownTarget(target)) if target == "missing-target"
    ));
    assert!(!p.config_dir.exists());
    assert!(!p.data_root.exists());
}
