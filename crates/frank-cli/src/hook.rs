//! Agent lifecycle hook entry points.
//!
//! Ported from `archive/src/hooks/caveman-activate.js` (SessionStart) and
//! `caveman-mode-tracker.js` (UserPromptSubmit). Every function here must
//! return `0` — hooks that exit non-zero, or panic, can break the host
//! agent's turn. `main.rs` wraps the call into this module in
//! `catch_unwind` as a backstop, but the functions here are also written to
//! never `unwrap`/`expect` on anything that depends on the environment or
//! stdin, matching the archive's #538 fix (an unhandled stdin `error` event
//! made the Node hook exit non-zero on a broken pipe).

use std::io::{Read, Write};
use std::path::PathBuf;

use frank_app::FrankService;
use frank_ledger::{InjectionEntry, append_injection};
use frank_state::AppliedState;

use crate::stats;

pub fn dispatch(svc: &FrankService, name: &str) -> i32 {
    match name {
        "session-start" => session_start(svc),
        "user-prompt-submit" => user_prompt_submit(svc),
        "statusline" => statusline(svc),
        _ => 0,
    }
}

fn session_start(svc: &FrankService) -> i32 {
    let current = match svc.current_pack() {
        Ok(pack) => pack,
        Err(_) => return 0,
    };
    let flag_path = svc.paths().flag_paths().active;
    let level_id = frank_safeio::read_flag(&flag_path, &current.valid_flag_values());

    match level_id.as_deref() {
        None | Some("off") => {}
        Some(id) => {
            if let Some(level) = current.levels.get(id) {
                // SessionStart stdout is injected as hidden context by the
                // host agent, not shown to the user directly.
                let _ = std::io::stdout().write_all(level.activation_prompt.as_bytes());

                // Best-effort: SessionStart doesn't carry a session id on
                // stdin the way UserPromptSubmit does (the archive's
                // equivalent hook never reads stdin at all — reading it
                // unconditionally here would risk blocking on EOF if the
                // host never pipes/closes it for this event, which would
                // violate "a hook must always return fast"). The
                // most-recently-modified session file is a reasonable
                // stand-in; if none exists yet (a session's JSONL often
                // isn't created until the first turn), the entry is
                // written with no session id and simply won't be counted
                // toward any session's report — a safe degradation.
                let session_id = frank_ledger::find_recent_session(&svc.paths().config_dir)
                    .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
                append_injection(
                    &svc.paths().ledger_paths().ledger,
                    &InjectionEntry {
                        ts: stats::now_ms(),
                        kind: "activate".to_string(),
                        session: session_id,
                        level: Some(id.to_string()),
                        inject_bytes: level.activation_prompt.len(),
                    },
                );
            }
        }
    }
    0
}

fn user_prompt_submit(svc: &FrankService) -> i32 {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return 0;
    }
    let Ok(input) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return 0;
    };
    let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let transcript_path = input.get("transcript_path").and_then(|v| v.as_str());
    let session_id = transcript_path
        .map(PathBuf::from)
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));

    let compiled = match svc.current_pack() {
        Ok(pack) => pack,
        Err(_) => return 0,
    };
    let resolved_default = svc
        .effective_default_level()
        .unwrap_or_else(|_| compiled.default_level.clone());
    let intent = frank_state::classify(prompt, &compiled, &resolved_default);

    let flag_paths = svc.paths().flag_paths();
    let applied = frank_state::apply(&intent, &compiled, &flag_paths);

    match applied {
        AppliedState::StatsRequested(tail_args) => {
            let args = stats::StatsArgs {
                session: transcript_path.map(PathBuf::from),
                json: false,
                all: tail_args.iter().any(|a| a == "--all"),
                share: tail_args.iter().any(|a| a == "--share"),
                explain: false,
            };
            let reason = stats::report_text(svc, &args);
            let out = serde_json::json!({ "decision": "block", "reason": reason.trim() });
            let _ = std::io::stdout().write_all(out.to_string().as_bytes());
        }
        AppliedState::Level(id) => {
            if let Some(level) = compiled.levels.get(&id) {
                let out = serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "UserPromptSubmit",
                        "additionalContext": level.reinforce,
                    }
                });
                let _ = std::io::stdout().write_all(out.to_string().as_bytes());

                // The expensive half of the injection (see AGENTS.md): this
                // line lands at the end of every prompt and is therefore
                // never cached, unlike the activation block. Recorded on
                // every turn it fires so `frank stats` can show it as its
                // own line, not folded into the activation cost.
                append_injection(
                    &svc.paths().ledger_paths().ledger,
                    &InjectionEntry {
                        ts: stats::now_ms(),
                        kind: "reinforce".to_string(),
                        session: session_id,
                        level: Some(id),
                        inject_bytes: level.reinforce.len(),
                    },
                );
            }
        }
        // Independent/oneshot modes have their own skill behavior; the base
        // pack's reinforcement would conflict with it, matching the
        // archive's INDEPENDENT_MODES skip.
        AppliedState::Oneshot(_) | AppliedState::Off => {}
    }

    0
}

fn statusline(svc: &FrankService) -> i32 {
    let current = match svc.current_pack() {
        Ok(pack) => pack,
        Err(_) => return 0,
    };
    let level_id = frank_safeio::read_flag(
        &svc.paths().flag_paths().active,
        &current.valid_flag_values(),
    );
    let pack_label = current.id.to_uppercase();

    let badge = match level_id.as_deref() {
        None | Some("off") => return 0,
        Some(id) if id == current.default_level => format!("[{pack_label}]"),
        Some(id) => format!("[{pack_label}:{}]", id.to_uppercase()),
    };
    // Same orange (256-color 172) as the archive's statusline for visual
    // continuity. Lifetime-savings suffix (the archive's `⛏ 2.8k` badge)
    // is deliberately omitted here: it would need a fast, cached read of
    // the lifetime ledger on every statusline render, which is out of
    // scope for M3's "does the ledger exist and is it honest" goal — see
    // `frank stats --all` for the equivalent number today.
    let _ = std::io::stdout().write_all(format!("\x1b[38;5;172m{badge}\x1b[0m").as_bytes());
    0
}
