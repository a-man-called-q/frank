use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn binary_dispatches_real_workspace_tasks() {
    let output = run(&["version-check"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("all application metadata"));

    let output = run(&["lint-targets"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("lint-targets:"));
}
