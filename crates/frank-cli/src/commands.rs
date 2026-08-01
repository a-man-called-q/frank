//! `frank on|off|status|levels` — the minimal M0 slice of what becomes the
//! full state machine in `frank-state` at M1. These commands write/read the
//! flag file directly; M1 adds natural-language triggers, slash-command
//! parsing, and the one-shot commit/review/compress restore behavior ported
//! from `archive/src/hooks/caveman-mode-tracker.js`.

use frank_app::{FrankPaths, FrankService, TargetOperation};

pub fn on(level: Option<&str>) -> i32 {
    let service = FrankService::new(FrankPaths::from_process());
    let requested = match level {
        Some(value) => value.to_string(),
        None => match service.effective_default_level() {
            Ok(level) => level,
            Err(e) => {
                eprintln!("frank: {e}");
                return 1;
            }
        },
    };
    match service.set_active_level(Some(&requested)) {
        Ok(Some(canonical)) => {
            println!("frank: on ({canonical})");
            0
        }
        Ok(None) => {
            println!("frank: off");
            0
        }
        Err(e) => {
            eprintln!("frank: failed to activate: {e}");
            1
        }
    }
}

pub fn off() -> i32 {
    let service = FrankService::new(FrankPaths::from_process());
    match service.set_active_level(None) {
        Ok(_) => {
            println!("frank: off");
            0
        }
        Err(e) => {
            eprintln!("frank: failed to deactivate: {e}");
            1
        }
    }
}

pub fn status() -> i32 {
    let service = FrankService::new(FrankPaths::from_process());
    match service.active_level() {
        Ok(level) => match level.as_deref() {
            None => {
                println!("frank: off");
                0
            }
            Some(id) => {
                println!("frank: on ({id})");
                0
            }
        },
        Err(e) => {
            eprintln!("frank: {e}");
            1
        }
    }
}

pub fn levels() -> i32 {
    let service = FrankService::new(FrankPaths::from_process());
    let current = match service.current_pack() {
        Ok(pack) => pack,
        Err(e) => {
            eprintln!("frank: {e}");
            return 1;
        }
    };
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
    0
}

pub fn install(dry_run: bool, only: Option<&str>) -> i32 {
    run_service_plan(
        only.unwrap_or("claude-code"),
        TargetOperation::Install,
        dry_run,
    )
}

pub fn uninstall(dry_run: bool, only: Option<&str>) -> i32 {
    run_service_plan(
        only.unwrap_or("claude-code"),
        TargetOperation::Uninstall,
        dry_run,
    )
}

fn run_service_plan(id: &str, operation: TargetOperation, dry_run: bool) -> i32 {
    let service = FrankService::new(FrankPaths::from_process());
    let preview = match service.prepare_target_change(id, operation) {
        Ok(preview) => preview,
        Err(e) => {
            eprintln!("frank: {e}");
            return 1;
        }
    };
    if dry_run {
        println!("Would apply to {}:", preview.target_id);
        for action in preview.actions {
            println!("  {action}");
        }
        return 0;
    }
    match service.apply_prepared_plan(&preview.plan_id) {
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
            0
        }
        Err(e) => {
            eprintln!("frank: failed for {id}: {e}");
            1
        }
    }
}

pub fn doctor() -> i32 {
    let service = FrankService::new(FrankPaths::from_process());
    let report = service.doctor();
    for d in &report.checks {
        let mark = if d.ok { "\u{2713}" } else { "\u{2717}" };
        println!("{mark} {}", d.message);
    }
    if report.ok { 0 } else { 1 }
}
