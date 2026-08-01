//! `frank on|off|status|levels` — the minimal M0 slice of what becomes the
//! full state machine in `frank-state` at M1. These commands write/read the
//! flag file directly; M1 adds natural-language triggers, slash-command
//! parsing, and the one-shot commit/review/compress restore behavior ported
//! from `archive/src/hooks/caveman-mode-tracker.js`.

use std::path::PathBuf;

use frank_target::claude_code::ClaudeCodeTarget;

use crate::{flag, pack};

pub fn install_ctx() -> frank_target::InstallCtx {
    frank_target::InstallCtx {
        config_dir: flag::config_dir(),
        frank_bin: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("frank")),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

pub fn on(level: Option<&str>) -> i32 {
    let current = match pack::current() {
        Ok(pack) => pack,
        Err(e) => {
            eprintln!("frank: selected pack is unavailable: {e}");
            return 1;
        }
    };
    let requested = level.unwrap_or(&current.default_level);
    let Some(canonical) = pack::resolve_level(&current, requested) else {
        eprintln!("frank: unknown level '{requested}'. Try `frank levels`.");
        return 1;
    };
    match frank_safeio::write_flag_atomic(&flag::path(), &canonical) {
        Ok(()) => {
            println!("frank: on ({canonical})");
            0
        }
        Err(e) => {
            eprintln!("frank: failed to activate: {e}");
            1
        }
    }
}

pub fn off() -> i32 {
    match frank_safeio::write_flag_atomic(&flag::path(), "off") {
        Ok(()) => {
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
    let current = match pack::current() {
        Ok(pack) => pack,
        Err(e) => {
            eprintln!("frank: selected pack is unavailable: {e}");
            return 1;
        }
    };
    let valid = pack::valid_flag_values(&current);
    let valid = valid.iter().map(String::as_str).collect::<Vec<_>>();
    let level = frank_safeio::read_flag(&flag::path(), &valid);
    match level.as_deref() {
        None | Some("off") => println!("frank: off"),
        Some(id) => println!("frank: on ({id})"),
    }
    0
}

pub fn levels() -> i32 {
    let current = match pack::current() {
        Ok(pack) => pack,
        Err(e) => {
            eprintln!("frank: selected pack is unavailable: {e}");
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

/// Only `claude-code` exists as a target for now — M2's scope, per
/// AGENTS.md's milestone table. `frank targets` (M5) is where multi-target
/// selection/detection lands.
/// `"pack:static_digest"` resolves to the active pack's default level's
/// full activation prompt — a reasonable stand-in for "the one static blob
/// of rules a hooks-less target gets" until a pack ships a dedicated,
/// shorter digest output. See AGENTS.md's Antigravity/Codex fallback notes.
fn resolve_body_ref(reference: &str, current: &frank_pack::CompiledPack) -> Option<String> {
    match reference {
        "pack:static_digest" => pack::level_by_id(current, &current.default_level)
            .map(|l| l.activation_prompt.to_string()),
        _ => None,
    }
}

pub fn install(dry_run: bool, only: Option<&str>) -> i32 {
    let ctx = install_ctx();
    match only {
        None | Some("claude-code") => run_plan(
            ClaudeCodeTarget::plan_install(&ctx),
            "claude-code",
            "installed for",
            dry_run,
        ),
        Some(id) => {
            let Some(manifest) = crate::targets_cmd::find_manifest(id) else {
                eprintln!("frank: unknown target '{id}'. Try `frank targets`.");
                return 1;
            };
            if !manifest.target.verified {
                eprintln!(
                    "frank: warning — target '{id}' is unverified (not confirmed against a real install). Proceeding anyway."
                );
            }
            let current = match pack::current() {
                Ok(pack) => pack,
                Err(e) => {
                    eprintln!("frank: selected pack is unavailable: {e}");
                    return 1;
                }
            };
            let plan = frank_target::generic::build_install_plan(&manifest, &ctx, |reference| {
                resolve_body_ref(reference, &current)
            });
            run_plan(plan, id, "installed for", dry_run)
        }
    }
}

pub fn uninstall(dry_run: bool, only: Option<&str>) -> i32 {
    let ctx = install_ctx();
    match only {
        None | Some("claude-code") => run_plan(
            ClaudeCodeTarget::plan_uninstall(&ctx),
            "claude-code",
            "uninstalled from",
            dry_run,
        ),
        Some(id) => {
            let Some(manifest) = crate::targets_cmd::find_manifest(id) else {
                eprintln!("frank: unknown target '{id}'. Try `frank targets`.");
                return 1;
            };
            let plan = frank_target::generic::build_uninstall_plan(&manifest, &ctx);
            run_plan(plan, id, "uninstalled from", dry_run)
        }
    }
}

fn run_plan(plan: frank_target::InstallPlan, id: &str, verb: &str, dry_run: bool) -> i32 {
    if dry_run {
        println!("Would apply to {id}:");
        for line in plan.describe() {
            println!("  {line}");
        }
        return 0;
    }
    match frank_target::apply(&plan) {
        Ok(log) => {
            println!("frank: {verb} {id}");
            for line in log {
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
    let ctx = install_ctx();
    let diagnoses = ClaudeCodeTarget::doctor(&ctx);
    let mut all_ok = true;
    for d in &diagnoses {
        let mark = if d.ok { "\u{2713}" } else { "\u{2717}" };
        println!("{mark} {}", d.message);
        all_ok &= d.ok;
    }
    if all_ok { 0 } else { 1 }
}
