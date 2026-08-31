//! `frank on|off|status|levels` — the minimal M0 slice of what becomes the
//! full state machine in `frank-state` at M1. These commands write/read the
//! flag file directly; M1 adds natural-language triggers, slash-command
//! parsing, and the one-shot commit/review/compress restore behavior ported
//! from `archive/src/hooks/caveman-mode-tracker.js`.

use frank_app::{FrankService, TargetOperation};

use crate::error::CliError;

pub fn on(svc: &FrankService, level: Option<&str>) -> Result<(), CliError> {
    let requested = match level {
        Some(value) => value.to_string(),
        None => svc.effective_default_level().map_err(CliError::general)?,
    };
    match svc.set_active_level(Some(&requested)) {
        Ok(Some(canonical)) => {
            println!("frank: on ({canonical})");
            Ok(())
        }
        Ok(None) => {
            println!("frank: off");
            Ok(())
        }
        Err(e) => Err(CliError::general(format!("failed to activate: {e}"))),
    }
}

pub fn off(svc: &FrankService) -> Result<(), CliError> {
    svc.set_active_level(None)
        .map(|_| println!("frank: off"))
        .map_err(|e| CliError::general(format!("failed to deactivate: {e}")))
}

pub fn status(svc: &FrankService) -> Result<(), CliError> {
    let level = svc.active_level().map_err(CliError::general)?;
    match level.as_deref() {
        None => println!("frank: off"),
        Some(id) => println!("frank: on ({id})"),
    }
    Ok(())
}

pub fn levels(svc: &FrankService) -> Result<(), CliError> {
    let current = svc.current_pack().map_err(CliError::general)?;
    println!("pack: {} v{}", current.id, current.version);
    for l in current.levels.values() {
        let default_marker = if l.id == current.default_level {
            " [default]"
        } else {
            ""
        };
        let aliases = if l.aliases.is_empty() {
            String::new()
        } else {
            format!(" (aliases: {})", l.aliases.join(", "))
        };
        println!("  {}{default_marker}{aliases}", l.id);
    }
    Ok(())
}

pub fn install(svc: &FrankService, dry_run: bool, only: Option<&str>) -> Result<(), CliError> {
    run_service_plan(
        svc,
        only.unwrap_or("claude-code"),
        TargetOperation::Install,
        dry_run,
    )
}

pub fn uninstall(svc: &FrankService, dry_run: bool, only: Option<&str>) -> Result<(), CliError> {
    run_service_plan(
        svc,
        only.unwrap_or("claude-code"),
        TargetOperation::Uninstall,
        dry_run,
    )
}

fn run_service_plan(
    svc: &FrankService,
    id: &str,
    operation: TargetOperation,
    dry_run: bool,
) -> Result<(), CliError> {
    let preview = svc
        .prepare_target_change(id, operation)
        .map_err(CliError::general)?;
    if dry_run {
        println!("Would apply to {}:", preview.target_id);
        for action in preview.actions {
            println!("  {action}");
        }
        return Ok(());
    }
    match svc.apply_prepared_plan(&preview.plan_id) {
        Ok(result) => {
            println!(
                "frank: {} {}",
                if operation == TargetOperation::Install {
                    "installed for"
                } else {
                    "uninstalled from"
                },
                result.target_id
            );
            for line in result.log {
                println!("  {line}");
            }
            Ok(())
        }
        Err(e) => Err(CliError::general(format!("failed for {id}: {e}"))),
    }
}

pub fn doctor(svc: &FrankService) -> i32 {
    let report = svc.doctor();
    for d in &report.checks {
        let mark = if d.ok { "\u{2713}" } else { "\u{2717}" };
        println!("{mark} {}", d.message);
    }
    if report.ok { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_app::FrankPaths;
    use tempfile::tempdir;

    fn service(root: &std::path::Path) -> FrankService {
        FrankService::new(FrankPaths {
            config_dir: root.join("claude"),
            data_root: root.join("data"),
            user_config_dir: root.join("config"),
            cwd: root.to_path_buf(),
            frank_bin: root.join("bin/frank"),
        })
    }

    // These exercise the command layer directly against a tempdir-backed
    // `FrankService` — the seam `main()`'s injection unlocked — instead of
    // spawning the `frank` binary as a subprocess the way
    // `tests/command_contract.rs` has to.

    #[test]
    fn on_off_status_round_trip_without_a_level_argument() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(status(&svc).is_ok());
        assert!(on(&svc, Some("lite")).is_ok());
        assert!(status(&svc).is_ok());
        assert!(off(&svc).is_ok());
    }

    #[test]
    fn on_with_no_level_argument_falls_back_to_the_effective_default() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(on(&svc, None).is_ok());
        let level = svc.active_level().unwrap();
        assert!(level.is_some());
    }

    #[test]
    fn on_with_off_deactivates_instead_of_activating() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(on(&svc, Some("lite")).is_ok());
        assert!(on(&svc, Some("off")).is_ok());
        assert_eq!(svc.active_level().unwrap(), None);
    }

    #[test]
    fn on_rejects_an_unknown_level() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        let err = on(&svc, Some("not-a-level")).unwrap_err();
        assert_eq!(err.code, 1);
        assert!(format!("{err}").starts_with("frank:"));
    }

    #[test]
    fn levels_succeeds_against_the_builtin_pack() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(levels(&svc).is_ok());
    }

    #[test]
    fn install_dry_run_succeeds_and_uninstall_dry_run_of_an_unknown_target_fails() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(install(&svc, true, None).is_ok());
        let err = uninstall(&svc, true, Some("missing")).unwrap_err();
        assert_eq!(err.code, 1);
    }

    #[test]
    fn doctor_reports_failure_before_install_and_success_after() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert_eq!(doctor(&svc), 1);
        assert!(install(&svc, false, None).is_ok());
        assert_eq!(doctor(&svc), 0);
    }
}
