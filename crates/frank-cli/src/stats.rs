//! `frank stats` — and the shared report-building logic the
//! `/caveman-stats` hook interception (`hook.rs`) also calls, so the two
//! entry points can never drift into reporting different numbers.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frank_app::FrankService;
use frank_ledger::{
    MIN_SESSIONS_FOR_LIFETIME_VERDICT, MIN_TURNS_FOR_LIFETIME_VERDICT, aggregate_history,
    format_usd, lifetime_verdict_has_enough_data, measured_output_total, price_for_model,
    read_history, read_injections, savings_estimate,
};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
pub fn report_text(svc: &FrankService, args: &StatsArgs) -> String {
    if args.all {
        return lifetime_text(svc);
    }

    let report = svc.build_and_record_stats(args.session.as_deref());
    let compiled = svc.pack_or_builtin();

    if args.json {
        return frank_ledger::render_json(&report, &compiled).to_string();
    }
    if args.share {
        return share_line(&compiled, &report);
    }

    let mut text = frank_ledger::render_text(&report, &compiled);
    if args.explain {
        text.push_str(&explain_text());
    }
    text
}

fn share_line(compiled: &frank_pack::CompiledPack, report: &frank_ledger::SessionReport) -> String {
    let output_total = measured_output_total(&report.attribution);
    if report.turns == 0 || output_total == 0 {
        return "\u{1FAA8} frank armed but no turns yet\n".to_string();
    }
    let mut saved = 0u64;
    for (mode, bucket) in &report.attribution.by_mode {
        if let Some(stat) = compiled.benchmark.get(mode) {
            let est = savings_estimate(bucket.output_tokens, stat, report.model.as_deref());
            saved += est.mean_tokens;
        }
    }
    let usd = price_for_model(report.model.as_deref())
        .map(|p| (saved as f64 / 1_000_000.0) * p)
        .unwrap_or(0.0);
    format!(
        "\u{1FAA8} Saved ~{saved} output tokens (~{}) across {} turns this session\n",
        format_usd(usd),
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

fn lifetime_text(svc: &FrankService) -> String {
    let ledger_paths = svc.paths().ledger_paths();
    let rows = aggregate_history(&read_history(&ledger_paths.history));
    let sessions = rows.len();
    let turns: usize = rows.iter().map(|row| row.turns).sum();

    if !lifetime_verdict_has_enough_data(&rows) {
        return format!(
            "Frank ledger — lifetime\n  Not enough data yet ({sessions} session(s), {turns} turn(s) recorded; need {} sessions and {} turns). \
            Keep using Frank; `frank stats --all` will report a verdict once there's enough history \
            to be honest about.\n",
            MIN_SESSIONS_FOR_LIFETIME_VERDICT, MIN_TURNS_FOR_LIFETIME_VERDICT
        );
    }

    let output_total: u64 = rows.iter().map(|r| r.output_tokens).sum();
    let input_total: u64 = rows.iter().map(|r| r.input_tokens).sum();
    let injections = read_injections(&ledger_paths.ledger);
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
