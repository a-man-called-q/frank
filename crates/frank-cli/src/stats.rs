//! `frank stats` — and the shared report-building logic the
//! `/caveman-stats` hook interception (`hook.rs`) also calls, so the two
//! entry points can never drift into reporting different numbers.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use frank_ledger::stats::{HistoryRow, aggregate_history, append_history, read_history};
use frank_ledger::{find_recent_session, pricing};

use crate::{flag, pack};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn flag_mtime_ms() -> Option<i64> {
    std::fs::metadata(flag::path())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
}

/// Build the current-session report and, if it has any turns, append a
/// lifetime history row. Shared by `frank stats` and the
/// `/caveman-stats` hook interception.
pub fn build_and_record(session_override: Option<&Path>) -> frank_ledger::SessionReport {
    let config_dir = flag::config_dir();
    let compiled = pack::current_or_builtin();

    let session_path = session_override
        .map(PathBuf::from)
        .or_else(|| find_recent_session(&config_dir));

    let Some(session_path) = session_path else {
        // No session found at all: report zero turns rather than reading a
        // path we know doesn't exist.
        return frank_ledger::SessionReport {
            session_path: None,
            session_id: None,
            turns: 0,
            model: None,
            attribution: frank_ledger::attribute_by_mode(&[], &[], None, None),
            injection_activate_bytes: 0,
            injection_reinforce_bytes: 0,
        };
    };

    let mode_log_path = config_dir.join(".frank-mode-log.jsonl");
    let ledger_path = config_dir.join(".frank-ledger.jsonl");
    let valid = pack::valid_flag_values(&compiled);
    let valid = valid.iter().map(String::as_str).collect::<Vec<_>>();
    let current_mode = frank_safeio::read_flag(&flag::path(), &valid);

    let report = frank_ledger::build_session_report(
        &session_path,
        &mode_log_path,
        &ledger_path,
        &compiled,
        current_mode.as_deref(),
        flag_mtime_ms(),
    );

    if report.turns > 0 {
        if let Some(sid) = &report.session_id {
            let output_total = frank_ledger::stats::measured_output_total(&report.attribution);
            let input_total = frank_ledger::stats::measured_input_total(&report.attribution);
            append_history(
                &config_dir.join(".frank-history.jsonl"),
                &HistoryRow {
                    ts: now_ms(),
                    session_id: sid.clone(),
                    model: report.model.clone(),
                    output_tokens: output_total,
                    input_tokens: input_total,
                    turns: report.turns,
                },
            );
        }
    }

    report
}

pub struct StatsArgs {
    pub session: Option<PathBuf>,
    pub json: bool,
    pub all: bool,
    pub share: bool,
    pub explain: bool,
}

/// Returns the report text (used by both the CLI command's stdout and the
/// hook's `{"decision":"block","reason":...}` payload).
pub fn report_text(args: &StatsArgs) -> String {
    if args.all {
        return lifetime_text();
    }

    let report = build_and_record(args.session.as_deref());
    let compiled = pack::current_or_builtin();

    if args.json {
        return frank_ledger::render_json(&report, &compiled).to_string();
    }
    if args.share {
        return share_line(&report);
    }

    let mut text = frank_ledger::render_text(&report, &compiled);
    if args.explain {
        text.push_str(&explain_text());
    }
    text
}

fn share_line(report: &frank_ledger::SessionReport) -> String {
    let output_total = frank_ledger::stats::measured_output_total(&report.attribution);
    if report.turns == 0 || output_total == 0 {
        return "\u{1FAA8} frank armed but no turns yet\n".to_string();
    }
    let compiled = pack::current_or_builtin();
    let mut saved = 0u64;
    for (mode, bucket) in &report.attribution.by_mode {
        if let Some(stat) = compiled.benchmark.get(mode) {
            let est = frank_ledger::stats::savings_estimate(
                bucket.output_tokens,
                stat,
                report.model.as_deref(),
            );
            saved += est.mean_tokens;
        }
    }
    let usd = pricing::price_for_model(report.model.as_deref())
        .map(|p| (saved as f64 / 1_000_000.0) * p)
        .unwrap_or(0.0);
    format!(
        "\u{1FAA8} Saved ~{saved} output tokens (~{}) across {} turns this session\n",
        pricing::format_usd(usd),
        report.turns
    )
}

fn explain_text() -> String {
    "\nExplain: est. saved = round(output_tokens / (1 - reduction)) - output_tokens, \
    per mode, using the pack's [benchmark.reduction] table. Modes without a benchmark \
    entry report no estimate. Frank's own injected bytes (from .frank-ledger.jsonl) are \
    shown separately, measured, never folded into the same number as the estimate. \
    See AGENTS.md for the full accounting rules.\n"
        .to_string()
}

fn lifetime_text() -> String {
    let config_dir = flag::config_dir();
    let rows = aggregate_history(&read_history(&config_dir.join(".frank-history.jsonl")));
    let sessions = rows.len();
    let turns: usize = rows.iter().map(|row| row.turns).sum();

    if !frank_ledger::stats::lifetime_verdict_has_enough_data(&rows) {
        return format!(
            "Frank ledger — lifetime\n  Not enough data yet ({sessions} session(s), {turns} turn(s) recorded; need {} sessions and {} turns). \
            Keep using Frank; `frank stats --all` will report a verdict once there's enough history \
            to be honest about.\n",
            frank_ledger::stats::MIN_SESSIONS_FOR_LIFETIME_VERDICT,
            frank_ledger::stats::MIN_TURNS_FOR_LIFETIME_VERDICT
        );
    }

    let output_total: u64 = rows.iter().map(|r| r.output_tokens).sum();
    let input_total: u64 = rows.iter().map(|r| r.input_tokens).sum();
    let ledger_path = config_dir.join(".frank-ledger.jsonl");
    let injections = frank_ledger::injection_ledger::read_all(&ledger_path);
    let frank_bytes: usize = injections.iter().map(|e| e.inject_bytes).sum();

    format!(
        "Frank ledger — lifetime ({sessions} sessions)\n\
         \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
         Output tokens (measured):  {output_total}\n\
         Input tokens (measured):   {input_total}\n\
         Frank injected (measured): {frank_bytes} bytes across {} log entries\n\
         \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\
         Per-session and per-mode savings estimates: run `frank stats` without --all \
         inside a live session, or `frank stats --json` for machine-readable per-mode detail.\n\
         Run `frank ab start` to measure a real control/treatment comparison instead of \
         extrapolating from the pack benchmark alone.\n",
        injections.len()
    )
}
