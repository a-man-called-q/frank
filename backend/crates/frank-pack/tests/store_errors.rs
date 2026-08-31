use frank_pack::{InstalledPack, PackLock, PackStore, PackStoreError};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_pack(root: &Path, id: &str, version: &str) {
    fs::create_dir_all(root.join("levels")).unwrap();
    fs::write(
        root.join("pack.toml"),
        format!(
            "schema = 1\n[pack]\nid = \"{id}\"\nversion = \"{version}\"\ndefault_level = \"full\"\n[[level]]\nid = \"full\"\ncompose = [\"@rules\"]\nrules = \"levels/full.md\"\n"
        ),
    )
    .unwrap();
    fs::write(root.join("levels/full.md"), "Be concise.").unwrap();
}

#[test]
fn invalid_digest_and_duplicate_selector_are_rejected() {
    let tmp = tempdir().unwrap();
    let source = tmp.path().join("source");
    write_pack(&source, "demo", "1.0.0");
    let store = PackStore::new(tmp.path().join("data"));

    assert!(matches!(
        store.add_local(&source, Some("bad")),
        Err(PackStoreError::InvalidDigest(_))
    ));

    fs::create_dir_all(store.root()).unwrap();
    let lock = PackLock {
        schema: 1,
        active: None,
        packs: vec![
            InstalledPack {
                id: "demo".into(),
                version: "1.0.0".into(),
                path: "packs/demo@1.0.0".into(),
                sha256: "0".repeat(64),
                source: "one".into(),
            },
            InstalledPack {
                id: "demo".into(),
                version: "2.0.0".into(),
                path: "packs/demo@2.0.0".into(),
                sha256: "1".repeat(64),
                source: "two".into(),
            },
        ],
    };
    fs::write(store.lock_path(), toml::to_string(&lock).unwrap()).unwrap();
    assert!(matches!(
        store.find("demo"),
        Err(PackStoreError::Ambiguous(selector)) if selector == "demo"
    ));
}

#[test]
fn malformed_and_unsupported_lockfiles_fail_closed() {
    let tmp = tempdir().unwrap();
    let store = PackStore::new(tmp.path().join("data"));
    fs::create_dir_all(store.root()).unwrap();

    fs::write(store.lock_path(), "not = [valid\n").unwrap();
    assert!(matches!(
        store.load_lock(),
        Err(PackStoreError::LockToml(_, _))
    ));

    fs::write(store.lock_path(), "schema = 99\n").unwrap();
    assert!(matches!(
        store.load_lock(),
        Err(PackStoreError::UnsupportedLockSchema(99))
    ));
}
