use frank_target::{Action, InstallCtx, TargetManifest, build_install_plan, build_uninstall_plan};
use std::path::PathBuf;
use tempfile::tempdir;

fn context(root: PathBuf) -> InstallCtx {
    InstallCtx {
        config_dir: root.join("config"),
        frank_bin: root.join("bin/frank"),
        cwd: root,
    }
}

fn manifest(raw: &str) -> TargetManifest {
    toml::from_str(raw).unwrap()
}

#[test]
fn unresolved_body_and_unimplemented_files_are_explicit_noops() {
    let tmp = tempdir().unwrap();
    let ctx = context(tmp.path().to_path_buf());
    let markdown = manifest(
        r#"
schema = 1
[target]
id = "generic"
label = "Generic"
kind = "generic"
[install]
strategy = "markdown-block"
[install.markdown]
path = "./AGENTS.md"
begin = "<!-- begin -->"
end = "<!-- end -->"
body = "pack:missing"
create_if_missing = true
"#,
    );
    let plan = build_install_plan(&markdown, &ctx, |_| None);
    assert!(matches!(&plan.actions[0], Action::Noop { reason } if reason.contains("pack:missing")));

    let files = manifest(
        r#"
schema = 1
[target]
id = "files"
label = "Files"
kind = "generic"
[install]
strategy = "files"
"#,
    );
    let plan = build_install_plan(&files, &ctx, |_| None);
    assert!(
        matches!(&plan.actions[0], Action::Noop { reason } if reason.contains("not yet implemented"))
    );
    let uninstall = build_uninstall_plan(&files, &ctx);
    assert!(
        matches!(&uninstall.actions[0], Action::Noop { reason } if reason.contains("not yet implemented"))
    );
}
